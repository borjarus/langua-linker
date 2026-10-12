//! Tauri composition root. Desktop (`main.rs`) and mobile (`mobile_entry_point`) both call `run()`.

use langualinker_contracts::{API_VERSION, AppInfoDto};
use langualinker_infrastructure::logging;
use tauri::{AppHandle, Builder, Emitter, Manager, Runtime};

pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

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
        mode: if cfg!(debug_assertions) {
            "development"
        } else {
            "release"
        }
        .to_owned(),
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
            announce_ready(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_app_info])
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
        assert!(value["mode"].is_string());
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
}
