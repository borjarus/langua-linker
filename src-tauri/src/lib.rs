//! Tauri composition root. Desktop (`main.rs`) and mobile (`mobile_entry_point`) both call `run()`.

mod diagnostics;

use diagnostics::SecretService;
use langualinker_contracts::{API_VERSION, AppInfoDto};
use langualinker_infrastructure::logging;
use tauri::{AppHandle, Builder, Emitter, Manager, Runtime};

pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Build variant (`dev` | `beta` | `release`), validated by `build.rs`.
pub const APP_VARIANT: &str = env!("LL_APP_VARIANT");

/// Event emitted once the shell is ready; the frontend treats it as a signal only.
pub const APP_READY_EVENT: &str = "app-ready";

#[tauri::command]
fn get_app_info() -> AppInfoDto {
    app_info()
}

fn app_info() -> AppInfoDto {
    AppInfoDto {
        api_version: API_VERSION.to_owned(),
        app_name: "LanguaLinker".to_owned(),
        app_version: APP_VERSION.to_owned(),
        mode: APP_VARIANT.to_owned(),
    }
}

fn announce_ready<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    app.emit(APP_READY_EVENT, "ready")
}

/// File logs and crash reports live in the app log directory. Failure is reported on stderr and
/// never prevents the app from starting.
fn start_diagnostics<R: Runtime>(app: &tauri::App<R>) {
    let dir = match app.path().app_log_dir() {
        Ok(dir) => dir,
        Err(error) => {
            eprintln!("diagnostics disabled: {error}");
            return;
        }
    };
    logging::install_crash_hook(dir.clone(), APP_VERSION);
    match logging::init_file_logging(&dir) {
        Ok(guard) => {
            app.manage(guard);
            tracing::info!(
                app_version = APP_VERSION,
                app_variant = APP_VARIANT,
                platform = std::env::consts::OS,
                "app started"
            );
        }
        Err(error) => eprintln!("file logging disabled: {error}"),
    }
}

/// Registers commands and setup on any runtime, so tests can use the mock runtime.
fn configure<R: Runtime>(builder: Builder<R>, diagnostics: bool) -> Builder<R> {
    builder
        .setup(move |app| {
            if diagnostics {
                start_diagnostics(app);
            }
            app.manage(SecretService(app.config().identifier.clone()));
            announce_ready(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_app_info,
            diagnostics::probe_secret_store
        ])
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    configure(Builder::default(), true)
        .run(tauri::generate_context!())
        .expect("error while running LanguaLinker");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;
    use tauri::ipc::{CallbackFn, InvokeBody};
    use tauri::test::{INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder};
    use tauri::webview::InvokeRequest;
    use tauri::{Listener, Manager, WebviewWindow};

    /// Origin of bundled assets as seen by the ACL ("local" URLs).
    const LOCAL_URL: &str = if cfg!(any(windows, target_os = "android")) {
        "http://tauri.localhost"
    } else {
        "tauri://localhost"
    };

    fn app() -> tauri::App<MockRuntime> {
        configure(mock_builder(), false)
            .build(tauri::generate_context!(test = true))
            .expect("mock app builds")
    }

    fn main_window(app: &tauri::App<MockRuntime>) -> WebviewWindow<MockRuntime> {
        app.get_webview_window("main")
            .or_else(|| {
                tauri::WebviewWindowBuilder::new(app, "main", Default::default())
                    .build()
                    .ok()
            })
            .expect("main window")
    }

    fn invoke(
        window: &WebviewWindow<MockRuntime>,
        cmd: &str,
        body: serde_json::Value,
    ) -> Result<serde_json::Value, serde_json::Value> {
        get_ipc_response(
            window,
            InvokeRequest {
                cmd: cmd.into(),
                callback: CallbackFn(0),
                error: CallbackFn(1),
                url: LOCAL_URL.parse().unwrap(),
                body: InvokeBody::Json(body),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_string(),
            },
        )
        .map(|response| response.deserialize::<serde_json::Value>().unwrap())
    }

    #[test]
    fn app_info_uses_the_shared_contract() {
        let value = serde_json::to_value(app_info()).unwrap();
        assert_eq!(value["apiVersion"], API_VERSION);
        assert_eq!(value["appName"], "LanguaLinker");
        assert_eq!(value["appVersion"], APP_VERSION);
        assert!(["dev", "beta", "release"].contains(&value["mode"].as_str().unwrap()));
    }

    fn config(json: &str) -> serde_json::Value {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn variants_install_side_by_side() {
        let release = config(include_str!("../tauri.conf.json"));
        let dev = config(include_str!("../tauri.dev.conf.json"));
        let beta = config(include_str!("../tauri.beta.conf.json"));
        let mobile = config(include_str!("../tauri.mobile.conf.json"));
        let base = release["identifier"].as_str().unwrap();
        assert_eq!(base, "com.mila.langualinker");
        // Mobile projects are generated for the base identifier; Gradle adds the variant suffix.
        assert_eq!(mobile["identifier"], base);
        for (overlay, variant) in [(&dev, "dev"), (&beta, "beta")] {
            assert_eq!(overlay["identifier"], format!("{base}.{variant}"));
            assert_ne!(overlay["productName"], release["productName"]);
        }
    }

    #[test]
    fn only_dev_bundles_the_demo_frontend() {
        let frontend = |json: &str| config(json)["build"]["frontendDist"].clone();
        assert_eq!(
            frontend(include_str!("../tauri.conf.json")),
            "../frontend/dist-release"
        );
        assert_eq!(
            frontend(include_str!("../tauri.beta.conf.json")),
            "../frontend/dist-beta"
        );
        assert_eq!(
            frontend(include_str!("../tauri.dev.conf.json")),
            "../frontend/dist"
        );
    }

    #[test]
    fn get_app_info_is_reachable_over_ipc() {
        let app = app();
        let window = main_window(&app);
        let response = invoke(&window, "get_app_info", serde_json::json!({})).expect("allowed");
        assert_eq!(response["apiVersion"], API_VERSION);
        assert_eq!(response["appName"], "LanguaLinker");
    }

    #[test]
    fn commands_outside_the_capability_are_rejected() {
        let app = app();
        let window = main_window(&app);
        // core:window is not granted to the main window.
        let denied = invoke(
            &window,
            "plugin:window|close",
            serde_json::json!({ "label": "main" }),
        );
        let message = denied
            .expect_err("window plugin must be denied")
            .to_string();
        assert!(message.contains("not allowed"), "{message}");
        let unknown = invoke(&window, "does_not_exist", serde_json::json!({}));
        assert!(unknown.is_err());
    }

    #[test]
    fn ready_event_reaches_listeners() {
        let app = app();
        let (sender, receiver) = mpsc::channel();
        app.listen_any(APP_READY_EVENT, move |event| {
            sender.send(event.payload().to_owned()).unwrap();
        });
        announce_ready(app.handle()).unwrap();
        let payload = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(payload, "\"ready\"");
    }

    #[test]
    fn capability_grants_only_the_minimal_permission_set() {
        let capability: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/default.json")).unwrap();
        let permissions: Vec<&str> = capability["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|permission| permission.as_str().unwrap())
            .collect();
        assert_eq!(permissions, ["core:event:default", "allow-get-app-info"]);
        assert_eq!(capability["windows"], serde_json::json!(["main"]));
    }

    #[test]
    fn diagnostics_capability_is_enabled_only_in_dev() {
        let enabled = |json: &str| config(json)["app"]["security"]["capabilities"].clone();
        let release = enabled(include_str!("../tauri.conf.json"));
        assert_eq!(release, serde_json::json!(["default"]));
        // Beta inherits the release list; only the dev overlay adds diagnostics.
        assert!(enabled(include_str!("../tauri.beta.conf.json")).is_null());
        assert_eq!(
            enabled(include_str!("../tauri.dev.conf.json")),
            serde_json::json!(["default", "diagnostics"])
        );
        let diagnostics = config(include_str!("../capabilities/diagnostics.json"));
        assert_eq!(
            diagnostics["permissions"],
            serde_json::json!(["allow-probe-secret-store"])
        );
    }

    #[test]
    fn diagnostics_commands_are_denied_without_the_dev_capability() {
        // The test context is the release configuration, which enables `default` only.
        let app = app();
        let window = main_window(&app);
        let denied = invoke(&window, "probe_secret_store", serde_json::json!({}));
        let message = denied.expect_err("probe must be denied").to_string();
        assert!(message.contains("not allowed"), "{message}");
    }
}
