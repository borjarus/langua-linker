//! AppApi commands (contract v1). Every command of `crates/contracts/v1/catalogue.json` is
//! registered (integration checklist rule 1): the ones implemented in Phases 7–8 are below, the
//! learning commands of Phase 10 are in `study.rs`, management (Phase 11) in `manage.rs` and
//! statistics, settings, backup and restore (Phase 12) in `settings.rs`; every
//! other one answers `NOT_IMPLEMENTED` until its phase arrives — never demo data and never
//! "command not found" (which would reach the UI as `INTERNAL`).

use std::collections::HashSet;
use std::sync::OnceLock;

use langualinker_application::AppError;
use langualinker_application::content::OnboardingInput;
use langualinker_application::jobs::JobId;
use langualinker_application::repositories::UnitOfWork;
use langualinker_contracts::dto::{
    AboutDto, AttributionDto, CommitDeckImportInput, CompleteOnboardingInput, DashboardDto,
    DeckImportPreviewDto, DiagnosticsReportDto, FileHandleDto, FilePurpose, JobDto, LogLevel,
    LogLineDto, OnboardingStateDto, PickFileInput, PreviewDeckImportInput,
};
use langualinker_contracts::{API_VERSION, ErrorCode, ErrorDto};
use langualinker_domain::ValidationError;
use langualinker_domain::validation::MAX_PAYLOAD_BYTES;
use langualinker_infrastructure::diagnostics;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tauri::ipc::Invoke;
use tauri::{AppHandle, Runtime, State};

use crate::APP_VERSION;
use crate::backend::{Backend, error_dto, iso, job_dto};
use crate::files::PickedFiles;

const CATALOGUE: &str = include_str!("../../crates/contracts/v1/catalogue.json");

/// FSRS generation the scheduler implements (Phase 9 ports it; the version is fixed by the plan).
pub const FSRS_VERSION: &str = "FSRS-6";

/// Commands with a real implementation; also listed in `generate_handler!` in `lib.rs`.
pub const IMPLEMENTED: &[&str] = &[
    "get_app_info",
    "get_about_info",
    "get_diagnostics_report",
    "get_job_status",
    "cancel_job",
    "get_attributions",
    "get_onboarding_state",
    "complete_onboarding",
    "get_dashboard",
    "pick_file",
    "preview_deck_import",
    "commit_deck_import",
    "start_study",
    "resume_study",
    "update_study_session",
    "get_rating_preview",
    "rate_card",
    "undo_review",
    "apply_card_action",
    "get_session_summary",
    "get_note_content",
    "set_favorite_association",
    "resolve_word_link",
    "get_deck",
    "create_deck",
    "update_deck",
    "delete_deck",
    "list_presets",
    "save_preset",
    "get_note_details",
    "get_editor_data",
    "check_duplicate",
    "save_note",
    "delete_note",
    "update_note_content",
    "search_words",
    "search_cards",
    "preview_bulk_action",
    "apply_bulk_action",
    "get_stats",
    "get_settings",
    "update_settings",
    "preview_reset",
    "reset_collection",
    "list_backups",
    "create_backup",
    "validate_restore",
    "restore_backup",
    "share_file",
];

/// Lines returned by the diagnostic report.
const REPORT_LINES: usize = 200;

/// Catalogue commands (`command` → operation name) without a real implementation yet.
pub fn pending() -> &'static [(String, String)] {
    static PENDING: OnceLock<Vec<(String, String)>> = OnceLock::new();
    PENDING.get_or_init(|| {
        let catalogue: Value = serde_json::from_str(CATALOGUE).expect("catalogue.json is JSON");
        let operations = catalogue["operations"].as_object().expect("operations");
        let implemented: HashSet<&str> = IMPLEMENTED.iter().copied().collect();
        operations
            .iter()
            .filter(|(_, spec)| spec["kind"] != "subscription")
            .filter_map(|(operation, spec)| {
                let command = spec["command"].as_str()?;
                (!implemented.contains(command)).then(|| (command.to_owned(), operation.clone()))
            })
            .collect()
    })
}

/// Answers a pending command with `NOT_IMPLEMENTED { operation }` and returns `None`; any other
/// command is handed back for the generated handler. ACL checks already ran: a pending command
/// still needs its `allow-*` permission in the capability.
pub fn reject_pending<R: Runtime>(invoke: Invoke<R>) -> Option<Invoke<R>> {
    let command = invoke.message.command();
    let Some((_, operation)) = pending().iter().find(|(pending, _)| pending == command) else {
        return Some(invoke);
    };
    let dto = ErrorDto::new(ErrorCode::NotImplemented).with_detail("operation", operation.as_str());
    invoke.resolver.reject(dto);
    None
}

/// Deserializes a command's `input` ourselves, so a malformed payload is `VALIDATION_FAILED`
/// instead of a transport error string the UI can only show as `INTERNAL`.
pub(crate) fn parse_input<T: DeserializeOwned>(input: Value) -> Result<T, ErrorDto> {
    let too_large =
        serde_json::to_vec(&input).map_or(true, |bytes| bytes.len() > MAX_PAYLOAD_BYTES);
    if too_large {
        return Err(error_dto(&ValidationError::new("input", "tooLarge").into()));
    }
    serde_json::from_value(input)
        .map_err(|_| error_dto(&ValidationError::new("input", "format").into()))
}

fn job_id(input: Value) -> Result<JobId, ErrorDto> {
    let id: String = parse_input(input)?;
    if id.is_empty() || id.len() > 64 {
        return Err(error_dto(&ValidationError::new("jobId", "format").into()));
    }
    Ok(JobId(id))
}

pub(crate) async fn blocking<T: Send + 'static>(
    task: impl FnOnce() -> Result<T, ErrorDto> + Send + 'static,
) -> Result<T, ErrorDto> {
    tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|_| error_dto(&AppError::Internal("blocking task failed".into())))?
}

#[tauri::command]
pub async fn get_job_status(backend: State<'_, Backend>, input: Value) -> Result<JobDto, ErrorDto> {
    let id = job_id(input)?;
    let jobs = backend.collection()?.jobs.clone();
    blocking(move || {
        jobs.get(&id)
            .map_err(|error| error_dto(&error))
            .and_then(|job| job_dto(&job))
    })
    .await
}

#[tauri::command]
pub async fn cancel_job(backend: State<'_, Backend>, input: Value) -> Result<JobDto, ErrorDto> {
    let id = job_id(input)?;
    let jobs = backend.collection()?.jobs.clone();
    blocking(move || {
        jobs.cancel(&id)
            .map_err(|error| error_dto(&error))
            .and_then(|job| job_dto(&job))
    })
    .await
}

pub(crate) fn operation_id(value: &str) -> Result<(), ErrorDto> {
    if value.is_empty() || value.len() > 128 {
        return Err(error_dto(
            &ValidationError::new("operationId", "format").into(),
        ));
    }
    Ok(())
}

#[tauri::command]
pub async fn get_attributions(
    backend: State<'_, Backend>,
) -> Result<Vec<AttributionDto>, ErrorDto> {
    // Dependencies are listed even when the collection cannot be opened.
    let content = match &backend.collection {
        Ok(collection) => collection
            .content
            .content_licences()
            .map_err(|error| error_dto(&error))?,
        Err(_) => Vec::new(),
    };
    Ok(crate::content::attributions(content))
}

#[tauri::command]
pub async fn get_onboarding_state(
    backend: State<'_, Backend>,
) -> Result<OnboardingStateDto, ErrorDto> {
    let content = backend.collection()?.content.clone();
    blocking(move || {
        content
            .onboarding_state()
            .map(crate::content::onboarding_dto)
            .map_err(|error| error_dto(&error))
    })
    .await
}

#[tauri::command]
pub async fn complete_onboarding(
    backend: State<'_, Backend>,
    input: Value,
) -> Result<JobDto, ErrorDto> {
    let input: CompleteOnboardingInput = parse_input(input)?;
    operation_id(&input.operation_id)?;
    if input.deck_guids.len() > 64 {
        return Err(error_dto(
            &ValidationError::new("deckGuids", "tooMany").into(),
        ));
    }
    let content = backend.collection()?.content.clone();
    blocking(move || {
        let input = OnboardingInput {
            operation_id: input.operation_id,
            native_language: langualinker_domain::deck::NativeLanguage::Pl,
            target_languages: input
                .target_languages
                .into_iter()
                .map(crate::content::domain_target_language)
                .collect(),
            daily_goal: input.daily_goal,
            deck_guids: input.deck_guids,
        };
        content
            .complete_onboarding(input)
            .map_err(|error| error_dto(&error))
            .and_then(|job| job_dto(&job))
    })
    .await
}

#[tauri::command]
pub async fn get_dashboard(backend: State<'_, Backend>) -> Result<DashboardDto, ErrorDto> {
    let collection = backend.collection()?;
    let (db, content) = (collection.db.clone(), collection.content.clone());
    let clock = collection.clock.clone();
    blocking(move || {
        let pending = content.pending().map_err(|error| error_dto(&error))?;
        let now = clock.now();
        // A write: expired burials return to their queues before counting (Phase 10).
        db.write(move |tx| langualinker_application::dashboard::dashboard(tx, now, &pending))
            .map(crate::content::dashboard_dto)
            .map_err(|error| error_dto(&error))
    })
    .await
}

/// System file picker: deck packages to import, backup archives to restore (copied into the
/// staging folder right away) and, on desktop, a folder for external backups.
#[tauri::command]
pub async fn pick_file<R: Runtime>(
    app: AppHandle<R>,
    backend: State<'_, Backend>,
    files: State<'_, PickedFiles>,
    input: Value,
) -> Result<Option<FileHandleDto>, ErrorDto> {
    let input: PickFileInput = parse_input(input)?;
    match input.purpose {
        FilePurpose::Import => {}
        FilePurpose::Restore => {
            let staging = backend
                .collection()?
                .backups
                .staging_dir()
                .map_err(|error| error_dto(&error))?;
            let picked = blocking(move || {
                crate::files::pick_restore_file(&app, &staging).map_err(|error| error_dto(&error))
            })
            .await?;
            return Ok(picked.map(|(name, path, size)| {
                let file = files.register_path(name, path);
                FileHandleDto {
                    token: file.token,
                    name: file.name,
                    size_bytes: u32::try_from(size).unwrap_or(u32::MAX),
                }
            }));
        }
        FilePurpose::BackupFolder => {
            let picked = blocking(move || {
                crate::files::pick_backup_folder(&app).map_err(|error| error_dto(&error))
            })
            .await?;
            return Ok(picked.map(|(name, path)| {
                let folder = files.register_path(name, path);
                FileHandleDto {
                    token: folder.token,
                    name: folder.name,
                    size_bytes: 0,
                }
            }));
        }
    }
    let picked =
        blocking(move || crate::files::pick_import_file(&app).map_err(|error| error_dto(&error)))
            .await?;
    Ok(picked.map(|(name, bytes)| {
        let size = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
        let file = files.register(name, bytes);
        FileHandleDto {
            token: file.token,
            name: file.name,
            size_bytes: size,
        }
    }))
}

#[tauri::command]
pub async fn preview_deck_import(
    backend: State<'_, Backend>,
    files: State<'_, PickedFiles>,
    input: Value,
) -> Result<DeckImportPreviewDto, ErrorDto> {
    let input: PreviewDeckImportInput = parse_input(input)?;
    let file = files
        .get(&input.file_token)
        .ok_or_else(|| error_dto(&AppError::NotFound { entity: "file" }))?;
    let content = backend.collection()?.content.clone();
    blocking(move || {
        content
            .preview_package(&file.name, &file.bytes)
            .map(crate::content::deck_import_preview_dto)
            .map_err(|error| error_dto(&error))
    })
    .await
}

#[tauri::command]
pub async fn commit_deck_import(
    backend: State<'_, Backend>,
    input: Value,
) -> Result<JobDto, ErrorDto> {
    let input: CommitDeckImportInput = parse_input(input)?;
    operation_id(&input.operation_id)?;
    let content = backend.collection()?.content.clone();
    blocking(move || {
        content
            .commit_package(input.operation_id, &input.preview_token)
            .map_err(|error| error_dto(&error))
            .and_then(|job| job_dto(&job))
    })
    .await
}

#[tauri::command]
pub async fn get_about_info(backend: State<'_, Backend>) -> Result<AboutDto, ErrorDto> {
    let (schema_version, content_version) = match &backend.collection {
        Ok(collection) => {
            let db = collection.db.clone();
            let schema = u32::try_from(db.schema_version()).ok();
            let content = blocking(move || {
                db.read(|tx| tx.installed_content_version())
                    .map_err(|error| error_dto(&error))
            })
            .await?;
            (schema, content)
        }
        Err(_) => (None, None),
    };
    Ok(AboutDto {
        app_version: APP_VERSION.to_owned(),
        api_version: API_VERSION.to_owned(),
        schema_version,
        fsrs_version: FSRS_VERSION.to_owned(),
        content_version: content_version.unwrap_or_else(|| "none".to_owned()),
        platform: std::env::consts::OS.to_owned(),
    })
}

#[tauri::command]
pub async fn get_diagnostics_report(
    backend: State<'_, Backend>,
) -> Result<DiagnosticsReportDto, ErrorDto> {
    let dir = backend.log_dir.clone();
    blocking(move || {
        let report = match dir {
            Some(dir) => diagnostics::read_recent(&dir, REPORT_LINES)
                .map_err(|error| error_dto(&AppError::Internal(error.to_string())))?,
            None => diagnostics::LogReport::default(),
        };
        Ok(DiagnosticsReportDto {
            generated_at: iso(langualinker_application::ports::Clock::now(
                &langualinker_infrastructure::system::SystemClock,
            )),
            redactions: u32::try_from(report.redactions).unwrap_or(u32::MAX),
            lines: report
                .lines
                .into_iter()
                .map(|line| LogLineDto {
                    at: line.at,
                    level: match line.level {
                        diagnostics::LogLevel::Error => LogLevel::Error,
                        diagnostics::LogLevel::Warn => LogLevel::Warn,
                        diagnostics::LogLevel::Info => LogLevel::Info,
                        diagnostics::LogLevel::Debug => LogLevel::Debug,
                    },
                    target: line.target,
                    message: line.message,
                })
                .collect(),
        })
    })
    .await
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use langualinker_application::jobs::{JobKind, NewJob};
    use langualinker_infrastructure::sqlite::SqliteDatabase;
    use langualinker_infrastructure::system::SystemClock;
    use serde_json::json;
    use tauri::Manager;
    use tauri::test::{MockRuntime, mock_app};

    use super::*;
    use crate::backend::{Collection, TauriEvents};

    const EXPECTED_PANIC: &str = "expected panic from the blocking test";

    fn code<T: std::fmt::Debug>(result: Result<T, ErrorDto>) -> ErrorCode {
        result.unwrap_err().code
    }

    fn app_with(
        collection: Result<Collection, AppError>,
        log_dir: Option<std::path::PathBuf>,
    ) -> tauri::App<MockRuntime> {
        let app = mock_app();
        app.manage(Backend {
            collection,
            log_dir,
        });
        app
    }

    fn open_collection(app: &tauri::App<MockRuntime>) -> Collection {
        let clock = Arc::new(SystemClock);
        let db = SqliteDatabase::open_in_memory(clock.as_ref())
            .unwrap()
            .database;
        let events = Arc::new(TauriEvents(app.handle().clone()));
        Collection::start(Arc::new(db), events, clock).unwrap()
    }

    fn scratch_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ll-commands-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    // ---------------------------------------------------------------- catalogue

    #[test]
    fn pending_lists_exactly_the_catalogue_commands_that_are_not_implemented() {
        let catalogue: Value = serde_json::from_str(CATALOGUE).unwrap();
        let commands: HashSet<&str> = catalogue["operations"]
            .as_object()
            .unwrap()
            .values()
            .filter(|spec| spec["kind"] != "subscription")
            .map(|spec| spec["command"].as_str().unwrap())
            .collect();
        let pending: HashSet<&str> = pending()
            .iter()
            .map(|(command, _)| command.as_str())
            .collect();
        let implemented: HashSet<&str> = IMPLEMENTED.iter().copied().collect();
        assert!(pending.is_disjoint(&implemented));
        assert!(
            implemented.is_subset(&commands),
            "every implemented command is in the catalogue"
        );
        assert_eq!(pending.len() + implemented.len(), commands.len());
    }

    #[test]
    fn pending_pairs_a_command_with_its_operation_name() {
        let pending = pending();
        assert!(!pending.is_empty());
        let catalogue: Value = serde_json::from_str(CATALOGUE).unwrap();
        for (command, operation) in pending {
            assert_eq!(
                catalogue["operations"][operation]["command"],
                command.as_str()
            );
        }
    }

    #[test]
    fn implemented_commands_are_unique_snake_case_names() {
        let unique: HashSet<_> = IMPLEMENTED.iter().collect();
        assert_eq!(unique.len(), IMPLEMENTED.len());
        for name in IMPLEMENTED {
            assert!(
                name.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "{name}"
            );
        }
    }

    // ---------------------------------------------------------------- input parsing

    #[test]
    fn parse_input_reads_well_formed_values() {
        assert_eq!(parse_input::<String>(json!("abc")).unwrap(), "abc");
        assert_eq!(parse_input::<Vec<u32>>(json!([1, 2])).unwrap(), [1, 2]);
    }

    #[test]
    fn parse_input_rejects_wrong_shapes_as_validation_errors() {
        let error = parse_input::<String>(json!(7)).unwrap_err();
        assert_eq!(error.code, ErrorCode::ValidationFailed);
        let details = error.details.unwrap();
        assert_eq!(
            (details["field"].as_str(), details["reason"].as_str()),
            (Some("input"), Some("format"))
        );
        assert_eq!(
            code(parse_input::<String>(Value::Null)),
            ErrorCode::ValidationFailed
        );
    }

    #[test]
    fn parse_input_rejects_oversized_payloads_before_deserializing() {
        let huge = json!("x".repeat(MAX_PAYLOAD_BYTES));
        let error = parse_input::<String>(huge).unwrap_err();
        assert_eq!(error.details.unwrap()["reason"], "tooLarge");
        let just_fits = json!("x".repeat(MAX_PAYLOAD_BYTES - 2));
        assert!(parse_input::<String>(just_fits).is_ok());
    }

    #[test]
    fn job_id_accepts_one_to_sixty_four_characters() {
        assert_eq!(job_id(json!("1")).unwrap(), JobId("1".into()));
        let longest = "9".repeat(64);
        assert_eq!(job_id(json!(longest)).unwrap().0, longest);
    }

    #[test]
    fn job_id_rejects_empty_long_and_non_string_values() {
        for bad in [
            json!(""),
            json!("x".repeat(65)),
            json!(12),
            json!(null),
            json!({}),
        ] {
            let error = job_id(bad.clone()).unwrap_err();
            assert_eq!(error.code, ErrorCode::ValidationFailed, "{bad}");
        }
        let empty = job_id(json!("")).unwrap_err();
        assert_eq!(empty.details.unwrap()["field"], "jobId");
    }

    // ---------------------------------------------------------------- blocking

    #[test]
    fn blocking_returns_the_task_result() {
        let value = tauri::async_runtime::block_on(blocking(|| Ok(40 + 2))).unwrap();
        assert_eq!(value, 42);
    }

    #[test]
    fn blocking_passes_task_errors_through() {
        let result =
            tauri::async_runtime::block_on(blocking::<()>(|| Err(error_dto(&AppError::DbBusy))));
        assert_eq!(code(result), ErrorCode::DbBusy);
    }

    #[test]
    fn a_panicking_task_becomes_an_internal_error() {
        // Mute only this test's expected panic; any other panic still reaches the default hook.
        let previous = std::sync::Arc::new(std::panic::take_hook());
        let chained = previous.clone();
        std::panic::set_hook(Box::new(move |info| {
            let expected = info.payload().downcast_ref::<&str>() == Some(&EXPECTED_PANIC);
            if !expected {
                chained(info);
            }
        }));
        let result = tauri::async_runtime::block_on(blocking::<()>(|| {
            std::panic::panic_any(EXPECTED_PANIC)
        }));
        let _ = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| previous(info)));
        assert_eq!(code(result), ErrorCode::Internal);
    }

    // ---------------------------------------------------------------- commands

    #[test]
    fn job_commands_return_the_stored_job() {
        let app = mock_app();
        let collection = open_collection(&app);
        let job = collection
            .jobs
            .start(
                NewJob {
                    kind: JobKind::Export,
                    operation_id: Some("op-cmd".into()),
                    total: None,
                },
                Box::new(|_| {
                    Ok(Some(
                        r#"{"type":"export","fileName":"a.csv","cardCount":2}"#.into(),
                    ))
                }),
            )
            .unwrap();
        for _ in 0..500 {
            if collection.jobs.get(&job.id).unwrap().status.is_finished() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        app.manage(Backend {
            collection: Ok(collection),
            log_dir: None,
        });
        let state = || app.state::<Backend>();

        let status =
            tauri::async_runtime::block_on(get_job_status(state(), json!(job.id.0))).unwrap();
        assert_eq!(status.job_id, job.id.0);
        assert!(matches!(
            status.result,
            Some(langualinker_contracts::dto::JobResultDto::Export { card_count: 2, .. })
        ));
        // Cancelling a finished job returns it unchanged.
        let cancelled =
            tauri::async_runtime::block_on(cancel_job(state(), json!(job.id.0))).unwrap();
        assert_eq!(cancelled.status, status.status);
        assert_eq!(cancelled.sequence, status.sequence);
    }

    #[test]
    fn job_commands_validate_input_before_touching_the_backend() {
        let app = app_with(Err(AppError::DbBusy), None);
        let state = app.state::<Backend>();
        let status = tauri::async_runtime::block_on(get_job_status(state.clone(), json!(5)));
        assert_eq!(code(status), ErrorCode::ValidationFailed);
        let cancel = tauri::async_runtime::block_on(cancel_job(state, json!("")));
        assert_eq!(code(cancel), ErrorCode::ValidationFailed);
    }

    #[test]
    fn job_commands_report_an_unopenable_collection() {
        let app = app_with(
            Err(AppError::SchemaTooNew {
                found: 9,
                supported: 1,
            }),
            None,
        );
        let state = app.state::<Backend>();
        let status = tauri::async_runtime::block_on(get_job_status(state.clone(), json!("1")));
        let error = status.unwrap_err();
        assert_eq!(error.code, ErrorCode::SchemaTooNew);
        assert_eq!(error.details.unwrap()["found"], 9);
        let cancel = tauri::async_runtime::block_on(cancel_job(state, json!("1")));
        assert_eq!(code(cancel), ErrorCode::SchemaTooNew);
    }

    #[test]
    fn unknown_job_is_not_found() {
        let app = mock_app();
        let collection = open_collection(&app);
        app.manage(Backend {
            collection: Ok(collection),
            log_dir: None,
        });
        let state = app.state::<Backend>();
        let status = tauri::async_runtime::block_on(get_job_status(state.clone(), json!("424242")));
        assert_eq!(code(status), ErrorCode::NotFound);
        let cancel = tauri::async_runtime::block_on(cancel_job(state, json!("not-a-number")));
        assert_eq!(code(cancel), ErrorCode::NotFound);
    }

    #[test]
    fn about_info_without_a_collection_has_no_schema_version() {
        let app = app_with(Err(AppError::DbBusy), None);
        let about = tauri::async_runtime::block_on(get_about_info(app.state::<Backend>())).unwrap();
        assert_eq!(about.schema_version, None);
        assert_eq!(about.content_version, "none");
        assert_eq!(about.app_version, APP_VERSION);
        assert_eq!(about.api_version, API_VERSION);
        assert_eq!(about.fsrs_version, FSRS_VERSION);
        assert_eq!(about.platform, std::env::consts::OS);
    }

    #[test]
    fn about_info_reports_schema_and_installed_content_versions() {
        let app = mock_app();
        let collection = open_collection(&app);
        let schema = collection.db.schema_version();
        app.manage(Backend {
            collection: Ok(collection),
            log_dir: None,
        });
        let about = tauri::async_runtime::block_on(get_about_info(app.state::<Backend>())).unwrap();
        assert_eq!(about.schema_version, u32::try_from(schema).ok());
        assert_eq!(
            about.content_version, "none",
            "no bundled deck is installed yet"
        );
    }

    #[test]
    fn diagnostics_without_a_log_dir_is_an_empty_report() {
        let app = app_with(Err(AppError::DbBusy), None);
        let report =
            tauri::async_runtime::block_on(get_diagnostics_report(app.state::<Backend>())).unwrap();
        assert!(report.lines.is_empty());
        assert_eq!(report.redactions, 0);
        assert_eq!(report.generated_at.len(), 24);
        assert!(report.generated_at.ends_with('Z'));
    }

    #[test]
    fn diagnostics_with_a_missing_log_dir_is_an_empty_report() {
        let missing = std::env::temp_dir().join(format!("ll-missing-{}", std::process::id()));
        let app = app_with(Err(AppError::DbBusy), Some(missing));
        let report =
            tauri::async_runtime::block_on(get_diagnostics_report(app.state::<Backend>())).unwrap();
        assert!(report.lines.is_empty());
    }

    #[test]
    fn diagnostics_maps_levels_and_redacts_unlisted_fields() {
        let dir = scratch_dir("report");
        let log = [
            "2026-03-02T08:00:00.000000Z ERROR langualinker::jobs: failed code=\"DB_BUSY\"",
            "2026-03-02T08:00:01.000000Z  WARN langualinker::jobs: slow duration_ms=900",
            "2026-03-02T08:00:02.000000Z  INFO langualinker::app: saved front=\"Kühlschrank\"",
            "2026-03-02T08:00:03.000000Z DEBUG langualinker::app: tick",
            "not a log line",
        ]
        .join("\n");
        std::fs::write(dir.join("langualinker.2026-03-02.log"), log).unwrap();
        let app = app_with(Err(AppError::DbBusy), Some(dir.clone()));
        let report =
            tauri::async_runtime::block_on(get_diagnostics_report(app.state::<Backend>())).unwrap();

        let levels: Vec<LogLevel> = report.lines.iter().map(|line| line.level).collect();
        assert_eq!(
            levels,
            [
                LogLevel::Error,
                LogLevel::Warn,
                LogLevel::Info,
                LogLevel::Debug
            ]
        );
        assert_eq!(report.lines[0].target, "langualinker::jobs");
        assert_eq!(report.lines[0].at, "2026-03-02T08:00:00.000Z");
        assert_eq!(report.redactions, 1);
        let json = serde_json::to_string(&report).unwrap();
        assert!(!json.contains("Kühlschrank"), "{json}");
        assert!(json.contains("DB_BUSY"), "{json}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn diagnostics_returns_at_most_the_newest_report_lines() {
        let dir = scratch_dir("limit");
        let log: String = (0..REPORT_LINES + 25)
            .map(|n| format!("2026-03-02T08:00:00.000000Z  INFO langualinker::app: line {n}\n"))
            .collect();
        std::fs::write(dir.join("langualinker.2026-03-02.log"), log).unwrap();
        let app = app_with(Err(AppError::DbBusy), Some(dir.clone()));
        let report =
            tauri::async_runtime::block_on(get_diagnostics_report(app.state::<Backend>())).unwrap();
        assert_eq!(report.lines.len(), REPORT_LINES);
        assert_eq!(report.lines.first().unwrap().message, "line 25");
        assert_eq!(
            report.lines.last().unwrap().message,
            format!("line {}", REPORT_LINES + 24)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unreadable_log_dir_is_an_internal_error_without_the_path() {
        let dir = scratch_dir("file");
        let not_a_dir = dir.join("plain-file");
        std::fs::write(&not_a_dir, "x").unwrap();
        let app = app_with(Err(AppError::DbBusy), Some(not_a_dir));
        let result = tauri::async_runtime::block_on(get_diagnostics_report(app.state::<Backend>()));
        let error = result.unwrap_err();
        assert_eq!(error.code, ErrorCode::Internal);
        let json = serde_json::to_string(&error).unwrap();
        assert!(!json.contains("plain-file"), "{json}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
