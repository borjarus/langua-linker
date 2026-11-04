//! Tauri composition root. Desktop (`main.rs`) and mobile (`mobile_entry_point`) both call `run()`.

mod backend;
mod commands;
mod content;
mod diagnostics;
mod files;

use std::path::PathBuf;

use backend::Backend;
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
    let Some(dir) = log_dir(app) else {
        return;
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

fn log_dir<R: Runtime>(app: &tauri::App<R>) -> Option<PathBuf> {
    app.path()
        .app_log_dir()
        .inspect_err(|error| eprintln!("diagnostics disabled: {error}"))
        .ok()
}

/// The collection lives in the variant's app data directory, so dev/beta/release never share it.
fn open_backend<R: Runtime>(app: &tauri::App<R>) -> Backend {
    let log_dir = log_dir(app);
    match app.path().app_data_dir() {
        Ok(data_dir) => Backend::open(app.handle(), data_dir, log_dir),
        Err(error) => {
            tracing::error!("app data directory unavailable");
            Backend {
                collection: Err(langualinker_application::AppError::Internal(
                    error.to_string(),
                )),
                log_dir,
            }
        }
    }
}

type BackendFactory<R> = fn(&tauri::App<R>) -> Backend;

/// Registers commands and setup on any runtime, so tests can use the mock runtime.
fn configure<R: Runtime>(
    builder: Builder<R>,
    diagnostics: bool,
    backend: BackendFactory<R>,
) -> Builder<R> {
    let generated: fn(tauri::ipc::Invoke<R>) -> bool = tauri::generate_handler![
        get_app_info,
        commands::get_about_info,
        commands::get_diagnostics_report,
        commands::get_job_status,
        commands::cancel_job,
        commands::get_attributions,
        commands::get_onboarding_state,
        commands::complete_onboarding,
        commands::get_dashboard,
        commands::pick_file,
        commands::preview_deck_import,
        commands::commit_deck_import,
        diagnostics::probe_secret_store,
        diagnostics::probe_file_open,
        diagnostics::probe_file_save
    ];
    builder
        .setup(move |app| {
            if diagnostics {
                start_diagnostics(app);
            }
            app.manage(SecretService(app.config().identifier.clone()));
            app.manage(backend(app));
            app.manage(files::PickedFiles::default());
            announce_ready(app.handle())?;
            Ok(())
        })
        .invoke_handler(move |invoke| match commands::reject_pending(invoke) {
            Some(invoke) => generated(invoke),
            None => true,
        })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Plugins are registered in every variant, but their commands are reachable only through
    // capabilities: today just the dev-only `diagnostics` one (ADR-12 spikes). Rust code may call
    // them directly (the file probes do). Not part of `configure`, so mock-runtime tests stay lean.
    let builder = Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init());
    configure(builder, true, open_backend)
        .run(tauri::generate_context!())
        .expect("error while running LanguaLinker");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, mpsc};
    use std::time::Duration;

    use langualinker_application::jobs::{JobKind, NewJob};
    use langualinker_contracts::dto::{
        AboutDto, DiagnosticsReportDto, JobDto, JobEventDto, JobResultDto, JobStatus,
    };
    use langualinker_contracts::{ErrorCode, ErrorDto};
    use langualinker_infrastructure::sqlite::SqliteDatabase;
    use langualinker_infrastructure::system::SystemClock;
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

    fn in_memory_backend(app: &tauri::App<MockRuntime>) -> Backend {
        let clock = Arc::new(SystemClock);
        let db = SqliteDatabase::open_in_memory(clock.as_ref())
            .expect("in-memory collection")
            .database;
        let events = Arc::new(backend::TauriEvents(app.handle().clone()));
        Backend {
            collection: backend::Collection::start(Arc::new(db), events, clock),
            log_dir: None,
        }
    }

    fn app() -> tauri::App<MockRuntime> {
        let app = configure(mock_builder(), false, in_memory_backend)
            .build(tauri::generate_context!(test = true))
            .expect("mock app builds");
        // The mock runtime does not run `setup` before the first event-loop iteration.
        app.manage(in_memory_backend(&app));
        app.manage(files::PickedFiles::default());
        app
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

    /// The real command answers in the shape frozen in contract v1, which the frontend's
    /// adapter conformance test replays (`frontend/tests/contractV1.test.ts`).
    #[test]
    fn get_app_info_matches_the_frozen_v1_sample() {
        let samples = config(include_str!("../../crates/contracts/v1/samples.json"));
        let sample = samples["calls"]
            .as_array()
            .unwrap()
            .iter()
            .find(|call| call["operation"] == "getAppInfo")
            .expect("getAppInfo sample");
        assert_eq!(sample["command"], "get_app_info");
        let app = app();
        let response =
            invoke(&main_window(&app), "get_app_info", serde_json::json!({})).expect("allowed");
        let keys = |value: &serde_json::Value| {
            value
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect::<Vec<_>>()
        };
        assert_eq!(keys(&response), keys(&sample["output"]));
        assert!(
            response
                .as_object()
                .unwrap()
                .values()
                .all(serde_json::Value::is_string)
        );
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

    /// Every callable operation of the frozen catalogue as `(operation, command)`.
    fn catalogue_commands() -> Vec<(String, String)> {
        let catalogue = config(include_str!("../../crates/contracts/v1/catalogue.json"));
        catalogue["operations"]
            .as_object()
            .unwrap()
            .iter()
            .filter(|(_, spec)| spec["kind"] != "subscription")
            .map(|(operation, spec)| {
                let command = spec["command"].as_str().unwrap().to_owned();
                (operation.clone(), command)
            })
            .collect()
    }

    #[test]
    fn capability_grants_exactly_the_contract_commands() {
        let capability = config(include_str!("../capabilities/default.json"));
        let permissions: Vec<&str> = capability["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|permission| permission.as_str().unwrap())
            .collect();
        let mut commands: Vec<String> = catalogue_commands()
            .into_iter()
            .map(|(_, command)| format!("allow-{}", command.replace('_', "-")))
            .collect();
        commands.sort();
        let mut granted: Vec<String> = permissions[1..].iter().map(|p| (*p).to_owned()).collect();
        granted.sort();
        assert_eq!(permissions[0], "core:event:default");
        assert_eq!(granted, commands);
        assert_eq!(capability["windows"], serde_json::json!(["main"]));
    }

    /// The `input` of the first frozen sample of `command`, as the Tauri adapter sends it.
    fn sample_body(command: &str) -> serde_json::Value {
        let samples = config(include_str!("../../crates/contracts/v1/samples.json"));
        samples["calls"]
            .as_array()
            .unwrap()
            .iter()
            .find(|call| call["command"] == command)
            .and_then(|call| call["args"].as_array()?.first().cloned())
            .map_or_else(
                || serde_json::json!({}),
                |input| serde_json::json!({ "input": input }),
            )
    }

    fn error_code(error: serde_json::Value) -> ErrorCode {
        serde_json::from_value::<ErrorDto>(error.clone())
            .unwrap_or_else(|_| panic!("not an ErrorDto: {error}"))
            .code
    }

    /// Integration checklist rule 1: every v1 command is registered and answers with a payload
    /// or an `ErrorDto`; pending ones answer `NOT_IMPLEMENTED` naming their operation.
    #[test]
    fn every_contract_command_answers_with_a_payload_or_an_error_dto() {
        let app = app();
        let window = main_window(&app);
        for (operation, command) in catalogue_commands() {
            let pending = commands::pending().iter().any(|(name, _)| *name == command);
            match invoke(&window, &command, sample_body(&command)) {
                Ok(_) => assert!(!pending, "{command} answered although it is pending"),
                Err(error) => {
                    let dto: ErrorDto = serde_json::from_value(error.clone())
                        .unwrap_or_else(|_| panic!("{command}: not an ErrorDto: {error}"));
                    if pending {
                        assert_eq!(dto.code, ErrorCode::NotImplemented, "{command}");
                        assert_eq!(dto.details.unwrap()["operation"], operation.as_str());
                    } else {
                        assert_ne!(dto.code, ErrorCode::NotImplemented, "{command}");
                    }
                }
            }
        }
        assert_eq!(
            commands::pending().len() + commands::IMPLEMENTED.len(),
            catalogue_commands().len(),
            "every implemented command is a catalogue command"
        );
    }

    #[test]
    fn about_info_and_diagnostics_answer_in_the_frozen_shape() {
        let app = app();
        let window = main_window(&app);
        let about = invoke(&window, "get_about_info", serde_json::json!({})).unwrap();
        let about: AboutDto = serde_json::from_value(about).unwrap();
        assert_eq!(about.schema_version, Some(2));
        assert_eq!(about.api_version, API_VERSION);
        assert_eq!(
            (about.fsrs_version.as_str(), about.content_version.as_str()),
            ("FSRS-6", "none")
        );
        let report = invoke(&window, "get_diagnostics_report", serde_json::json!({})).unwrap();
        let report: DiagnosticsReportDto = serde_json::from_value(report).unwrap();
        assert!(report.lines.is_empty());
        assert_eq!(report.generated_at.len(), 24);
    }

    /// Phase 8 over IPC: onboarding installs the bundled decks through the prepare job, and the
    /// dashboard and licences read them back in the contract shapes.
    #[test]
    fn onboarding_installs_bundled_decks_and_the_dashboard_lists_them() {
        use langualinker_contracts::dto::{AttributionDto, DashboardDto, OnboardingStateDto};
        let app = app();
        let window = main_window(&app);
        let state: OnboardingStateDto = serde_json::from_value(
            invoke(&window, "get_onboarding_state", serde_json::json!({})).unwrap(),
        )
        .unwrap();
        assert!(!state.completed);
        assert_eq!(state.available_decks.len(), 4);
        let guids: Vec<String> = state
            .available_decks
            .iter()
            .map(|deck| deck.guid.clone())
            .collect();
        let input = serde_json::json!({ "input": {
            "operationId": "op-onboarding", "nativeLanguage": "pl", "targetLanguages": ["de", "en"],
            "dailyGoal": 30, "deckGuids": guids,
        }});
        let job: JobDto =
            serde_json::from_value(invoke(&window, "complete_onboarding", input).unwrap()).unwrap();
        let mut status = job.clone();
        for _ in 0..400 {
            status = serde_json::from_value(
                invoke(
                    &window,
                    "get_job_status",
                    serde_json::json!({ "input": job.job_id }),
                )
                .unwrap(),
            )
            .unwrap();
            if matches!(status.status, JobStatus::Succeeded | JobStatus::Failed) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            matches!(status.result, Some(JobResultDto::PrepareContent { created: 113, ref deck_ids, .. }) if deck_ids.len() == 4),
            "{status:?}"
        );
        let dashboard: DashboardDto = serde_json::from_value(
            invoke(&window, "get_dashboard", serde_json::json!({})).unwrap(),
        )
        .unwrap();
        assert_eq!(dashboard.decks.len(), 4);
        assert_eq!(dashboard.daily_goal, 30);
        assert_eq!(
            dashboard
                .decks
                .iter()
                .map(|deck| deck.counts.new)
                .sum::<u32>(),
            202
        );
        let state: OnboardingStateDto = serde_json::from_value(
            invoke(&window, "get_onboarding_state", serde_json::json!({})).unwrap(),
        )
        .unwrap();
        assert!(state.completed);
        assert_eq!(state.installed_deck_guids.len(), 4);
        let licences: Vec<AttributionDto> = serde_json::from_value(
            invoke(&window, "get_attributions", serde_json::json!({})).unwrap(),
        )
        .unwrap();
        assert!(
            licences
                .iter()
                .any(|item| item.license.contains("rights audit pending"))
        );
        let error = invoke(
            &window,
            "complete_onboarding",
            serde_json::json!({ "input": { "operationId": "op-bad", "nativeLanguage": "pl",
                "targetLanguages": ["de"], "dailyGoal": 0, "deckGuids": [] } }),
        )
        .unwrap_err();
        assert_eq!(error_code(error), ErrorCode::ValidationFailed);
    }

    /// A package as `apkg-convert` writes it: one note with review progress and history.
    fn tiny_package() -> Vec<u8> {
        let body = concat!(
            r#"{"guid":"anki-tiny-1","front":"das Fenster","back":"okno","#,
            r#""cards":[{"ordinal":0,"state":"review","queue":"review","dueAtMs":1800000000000,"position":null,"scheduledDays":6,"stability":6.5,"difficulty":4.0,"ease":2500,"reps":3,"lapses":0,"lastReviewAtMs":1770000000000,"flag":0}],"#,
            r#""reviews":[{"ordinal":0,"reviewedAtMs":1770000000000,"rating":3,"kind":"review","scheduledDays":6,"ease":2500,"durationMs":4000}]}"#,
            "\n"
        );
        let manifest = serde_json::json!({
            "schemaVersion": 2, "source": "import", "deckGuid": "anki-deck-tiny", "contentVersion": "2026.11.04",
            "name": "Tiny", "description": "Import z Anki: tiny.apkg", "level": "A1", "nativeLanguage": "pl",
            "targetLanguage": "de", "deckType": "Linguistic", "noteType": "Basic", "contentKind": "words",
            "notesFile": "notes.ndjson", "sha256": langualinker_content::reader::sha256_hex(body.as_bytes()),
            "expectedNotes": 1, "expectedCards": 1,
            "license": {"name": "Personal", "attribution": null, "url": null, "rightsConfirmed": false},
            "review": {"status": "pending", "reviewer": null, "reviewedAt": null}
        });
        let mut package =
            serde_json::to_vec(&serde_json::json!({ "package": 1, "manifest": manifest })).unwrap();
        package.push(b'\n');
        package.extend_from_slice(body.as_bytes());
        package
    }

    /// Contract 1.2 over IPC: a picked package is previewed, imported with its progress and
    /// history, and a stale or foreign file is refused.
    #[test]
    fn deck_packages_are_previewed_and_imported_with_progress() {
        use langualinker_contracts::dto::{DashboardDto, DeckImportPreviewDto};
        let app = app();
        let window = main_window(&app);
        let files = app.state::<files::PickedFiles>();
        let picked = files.register("tiny.ndjson".into(), tiny_package());
        let preview: DeckImportPreviewDto = serde_json::from_value(
            invoke(
                &window,
                "preview_deck_import",
                serde_json::json!({ "input": { "fileToken": picked.token } }),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            (preview.notes, preview.scheduled_cards, preview.reviews),
            (1, 1, 1)
        );
        assert_eq!(preview.existing_deck_id, None);
        let job: JobDto = serde_json::from_value(
            invoke(
                &window,
                "commit_deck_import",
                serde_json::json!({ "input": { "operationId": "op-deck", "previewToken": preview.preview_token } }),
            )
            .unwrap(),
        )
        .unwrap();
        let mut status = job.clone();
        for _ in 0..400 {
            status = serde_json::from_value(
                invoke(
                    &window,
                    "get_job_status",
                    serde_json::json!({ "input": job.job_id }),
                )
                .unwrap(),
            )
            .unwrap();
            if matches!(status.status, JobStatus::Succeeded | JobStatus::Failed) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            matches!(status.result, Some(JobResultDto::Import { create: 1, .. })),
            "{status:?}"
        );
        let dashboard: DashboardDto = serde_json::from_value(
            invoke(&window, "get_dashboard", serde_json::json!({})).unwrap(),
        )
        .unwrap();
        let deck = dashboard
            .decks
            .iter()
            .find(|deck| deck.name == "Tiny")
            .expect("imported deck");
        assert!(!deck.bundled);
        assert_eq!(
            (deck.counts.total, deck.counts.new),
            (1, 0),
            "the card arrives with its progress"
        );

        let stale = invoke(
            &window,
            "commit_deck_import",
            serde_json::json!({ "input": { "operationId": "op-other", "previewToken": "nope" } }),
        )
        .unwrap_err();
        assert_eq!(error_code(stale), ErrorCode::PreviewStale);
        let zip = files.register("deck.apkg".into(), b"PK\x03\x04rest".to_vec());
        let unsupported = invoke(
            &window,
            "preview_deck_import",
            serde_json::json!({ "input": { "fileToken": zip.token } }),
        )
        .unwrap_err();
        assert_eq!(
            error_code(unsupported.clone()),
            ErrorCode::UnsupportedFormat
        );
        assert_eq!(unsupported["details"]["format"], "apkg");
        let missing = invoke(
            &window,
            "preview_deck_import",
            serde_json::json!({ "input": { "fileToken": "gone" } }),
        )
        .unwrap_err();
        assert_eq!(error_code(missing), ErrorCode::NotFound);
    }

    #[test]
    fn job_status_is_read_from_the_database_and_events_reach_listeners() {
        let app = app();
        let window = main_window(&app);
        let (sender, receiver) = mpsc::channel();
        app.listen_any(backend::JOB_PROGRESS_EVENT, move |event| {
            sender.send(event.payload().to_owned()).unwrap();
        });
        let jobs = app.state::<Backend>().collection().unwrap().jobs.clone();
        let job = jobs
            .start(
                NewJob {
                    kind: JobKind::Export,
                    operation_id: Some("op-ipc".into()),
                    total: Some(1),
                },
                Box::new(|ctx| {
                    ctx.progress(1, 1)?;
                    Ok(Some(
                        r#"{"type":"export","fileName":"x.csv","cardCount":1}"#.into(),
                    ))
                }),
            )
            .unwrap();
        let next = || -> JobEventDto {
            let payload = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
            serde_json::from_str(&payload).unwrap()
        };
        let first = next();
        assert_eq!(
            (first.job_id.as_str(), first.sequence),
            (job.id.0.as_str(), 0)
        );
        let mut last = first;
        while last.status != JobStatus::Succeeded {
            last = next();
        }
        let status = invoke(
            &window,
            "get_job_status",
            serde_json::json!({ "input": job.id.0 }),
        );
        let status: JobDto = serde_json::from_value(status.unwrap()).unwrap();
        assert_eq!(
            status.sequence, last.sequence,
            "the stored state equals the last event"
        );
        assert!(matches!(
            status.result,
            Some(JobResultDto::Export { card_count: 1, .. })
        ));
    }

    #[test]
    fn malformed_and_unknown_job_ids_are_error_dtos() {
        let app = app();
        let window = main_window(&app);
        let code = |input: serde_json::Value| {
            let body = serde_json::json!({ "input": input });
            error_code(invoke(&window, "get_job_status", body).unwrap_err())
        };
        assert_eq!(code(serde_json::json!("123456")), ErrorCode::NotFound);
        assert_eq!(code(serde_json::json!(7)), ErrorCode::ValidationFailed);
        assert_eq!(code(serde_json::json!("")), ErrorCode::ValidationFailed);
        assert_eq!(
            code(serde_json::json!("x".repeat(65))),
            ErrorCode::ValidationFailed
        );
        let cancel = invoke(&window, "cancel_job", serde_json::json!({ "input": "99" }));
        assert_eq!(error_code(cancel.unwrap_err()), ErrorCode::NotFound);
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
            serde_json::json!([
                "allow-probe-secret-store",
                "allow-probe-file-open",
                "allow-probe-file-save",
                "notification:allow-is-permission-granted",
                "notification:allow-request-permission",
                "notification:allow-notify",
                "notification:allow-get-pending",
                "notification:allow-cancel",
                "notification:allow-register-listener",
                "notification:allow-remove-listener"
            ])
        );
    }

    #[test]
    fn diagnostics_commands_are_denied_without_the_dev_capability() {
        // The test context is the release configuration, which enables `default` only.
        let app = app();
        let window = main_window(&app);
        for (command, body) in [
            ("probe_secret_store", serde_json::json!({})),
            ("probe_file_open", serde_json::json!({ "filtered": true })),
            ("probe_file_save", serde_json::json!({})),
            (
                "plugin:notification|notify",
                serde_json::json!({ "options": { "title": "x" } }),
            ),
        ] {
            let denied = invoke(&window, command, body);
            let message = denied.expect_err(command).to_string();
            assert!(message.contains("not allowed"), "{command}: {message}");
        }
    }
}
