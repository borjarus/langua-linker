//! Composition of the backend: the SQLite collection, the job runtime, bundled content (Phase 8)
//! and the Tauri event sink, plus the mapping from application types to contract DTOs.

use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, SecondsFormat};
use langualinker_application::content::ContentService;
use langualinker_application::jobs::{
    JobDetail, JobEvent, JobKind, JobRecord, JobService, JobStatus,
};
use langualinker_application::ports::{Clock, CollectionScope, EventSink};
use langualinker_application::{AppError, DetailValue};
use langualinker_contracts::dto::{self, JobDto, JobEventDto, JobProgressDto, JobResultDto};
use langualinker_contracts::{ErrorCode, ErrorDto};
use langualinker_domain::TimestampMs;
use langualinker_infrastructure::sqlite::{DATABASE_FILE, SqliteDatabase};
use langualinker_infrastructure::system::{OsRng, SystemClock};
use serde_json::{Map, Value};
use tauri::{AppHandle, Emitter, Runtime};

pub const JOB_PROGRESS_EVENT: &str = "job_progress";
pub const COLLECTION_CHANGED_EVENT: &str = "collection_changed";

/// Backend state managed by Tauri. A collection that cannot be opened (for example a newer
/// schema) leaves `collection` as the error, which every data command then returns.
pub struct Backend {
    pub collection: Result<Collection, AppError>,
    pub log_dir: Option<PathBuf>,
}

pub struct Collection {
    pub db: Arc<SqliteDatabase>,
    pub jobs: JobService<SqliteDatabase>,
    pub content: ContentService<SqliteDatabase>,
    /// Signals after committed mutations (read signals for the UI, never truth).
    pub events: Arc<dyn EventSink>,
    pub clock: Arc<dyn Clock>,
}

impl Backend {
    /// Opens `<data_dir>/collection.sqlite`, migrating it, and marks jobs a previous process left
    /// unfinished as failed.
    pub fn open<R: Runtime>(
        app: &AppHandle<R>,
        data_dir: PathBuf,
        log_dir: Option<PathBuf>,
    ) -> Self {
        let clock = Arc::new(SystemClock);
        let collection = SqliteDatabase::open(&data_dir.join(DATABASE_FILE), clock.as_ref())
            .and_then(|opened| {
                tracing::info!(
                    schema_version = opened.migration.to_version,
                    status = if opened.migration.from_version == opened.migration.to_version {
                        "current"
                    } else {
                        "migrated"
                    },
                    "collection opened"
                );
                Collection::start(
                    Arc::new(opened.database),
                    Arc::new(TauriEvents(app.clone())),
                    clock,
                )
            });
        match &collection {
            Err(error) => tracing::error!(code = error.code(), "collection could not be opened"),
            // A preparation cut short by a kill, or a newer bundled version, continues now in
            // the background; the first screen does not wait for it.
            Ok(collection) => match collection.content.resume() {
                Ok(Some(job)) => {
                    tracing::info!(job_id = %job.id, "bundled content preparation resumed")
                }
                Ok(None) => {}
                Err(error) => tracing::error!(code = error.code(), "bundled content not resumed"),
            },
        }
        Self {
            collection,
            log_dir,
        }
    }

    pub fn collection(&self) -> Result<&Collection, ErrorDto> {
        self.collection.as_ref().map_err(error_dto)
    }
}

impl Collection {
    pub fn start(
        db: Arc<SqliteDatabase>,
        events: Arc<dyn EventSink>,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, AppError> {
        let jobs = JobService::new(db.clone(), events.clone(), clock.clone());
        let interrupted = jobs.recover_interrupted()?;
        if interrupted > 0 {
            tracing::warn!(count = interrupted, "jobs interrupted by the previous run");
        }
        let content = ContentService::new(
            db.clone(),
            Arc::new(crate::content::BUNDLED),
            jobs.clone(),
            events.clone(),
            clock.clone(),
            Arc::new(OsRng),
            langualinker_application::content::ResultEncoders {
                prepare: crate::content::encode_outcome,
                import: crate::content::encode_import,
            },
        );
        Ok(Self {
            db,
            jobs,
            content,
            events,
            clock,
        })
    }
}

/// Emits backend signals to every window. A failed emit is logged; it never fails the mutation
/// that already committed.
pub struct TauriEvents<R: Runtime>(pub AppHandle<R>);

impl<R: Runtime> EventSink for TauriEvents<R> {
    fn job_progress(&self, event: &JobEvent) {
        if self
            .0
            .emit(JOB_PROGRESS_EVENT, job_event_dto(event))
            .is_err()
        {
            tracing::warn!(job_id = %event.job_id, "job event not delivered");
        }
    }

    fn collection_changed(&self, scopes: &[CollectionScope], at: TimestampMs) {
        let payload = dto::CollectionChangedEventDto {
            scopes: scopes.iter().map(|scope| scope_dto(*scope)).collect(),
            changed_at: iso(at),
        };
        if self.0.emit(COLLECTION_CHANGED_EVENT, payload).is_err() {
            tracing::warn!("collection event not delivered");
        }
    }
}

/// UTC ISO-8601 with milliseconds, the only timestamp form that crosses IPC.
pub fn iso(at: TimestampMs) -> String {
    DateTime::from_timestamp_millis(at.0)
        .unwrap_or_default()
        .to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Maps an application error to the contract. `Internal` keeps its text in the log (redacted by
/// field policy) and sends only the code.
pub fn error_dto(error: &AppError) -> ErrorDto {
    let code = ErrorCode::parse(error.code()).unwrap_or(ErrorCode::Internal);
    if code == ErrorCode::Internal {
        tracing::error!(code = code.as_str(), "internal error");
    }
    let mut dto = ErrorDto::new(code);
    for (key, value) in error.details() {
        dto = match value {
            DetailValue::Text(text) => dto.with_detail(key, text),
            DetailValue::Number(number) => dto.with_detail(key, number),
        };
    }
    dto
}

fn kind_dto(kind: JobKind) -> dto::JobKind {
    match kind {
        JobKind::PrepareContent => dto::JobKind::PrepareContent,
        JobKind::Import => dto::JobKind::Import,
        JobKind::Export => dto::JobKind::Export,
        JobKind::Backup => dto::JobKind::Backup,
        JobKind::Restore => dto::JobKind::Restore,
        JobKind::Reset => dto::JobKind::Reset,
        JobKind::Llm => dto::JobKind::Llm,
    }
}

fn status_dto(status: JobStatus) -> dto::JobStatus {
    match status {
        JobStatus::Queued => dto::JobStatus::Queued,
        JobStatus::Running => dto::JobStatus::Running,
        JobStatus::Succeeded => dto::JobStatus::Succeeded,
        JobStatus::Failed => dto::JobStatus::Failed,
        JobStatus::Cancelled => dto::JobStatus::Cancelled,
    }
}

fn scope_dto(scope: CollectionScope) -> dto::CollectionScope {
    match scope {
        CollectionScope::Decks => dto::CollectionScope::Decks,
        CollectionScope::Cards => dto::CollectionScope::Cards,
        CollectionScope::Notes => dto::CollectionScope::Notes,
        CollectionScope::Settings => dto::CollectionScope::Settings,
        CollectionScope::Stats => dto::CollectionScope::Stats,
        CollectionScope::Backups => dto::CollectionScope::Backups,
    }
}

fn clamp_u32(value: impl TryInto<u32>) -> u32 {
    value.try_into().unwrap_or(u32::MAX)
}

fn progress_dto(
    progress: Option<langualinker_application::jobs::JobProgress>,
) -> Option<JobProgressDto> {
    progress.map(|progress| JobProgressDto {
        done: clamp_u32(progress.done),
        total: clamp_u32(progress.total),
    })
}

pub fn job_event_dto(event: &JobEvent) -> JobEventDto {
    JobEventDto {
        job_id: event.job_id.0.clone(),
        kind: kind_dto(event.kind),
        sequence: clamp_u32(event.sequence),
        status: status_dto(event.status),
        progress: progress_dto(event.progress),
    }
}

pub fn job_dto(job: &JobRecord) -> Result<JobDto, ErrorDto> {
    let error = job.error.as_ref().map(|failure| {
        let mut dto = ErrorDto::new(ErrorCode::parse(&failure.code).unwrap_or(ErrorCode::Internal));
        if !failure.details.is_empty() {
            let details: Map<String, Value> = failure
                .details
                .iter()
                .map(|(key, value)| {
                    let value = match value {
                        JobDetail::Text(text) => Value::from(text.as_str()),
                        JobDetail::Number(number) => Value::from(*number),
                    };
                    (key.clone(), value)
                })
                .collect();
            dto.details = Some(details);
        }
        dto
    });
    let result = job
        .result_json
        .as_deref()
        .map(serde_json::from_str::<JobResultDto>)
        .transpose()
        .map_err(|_| {
            error_dto(&AppError::Internal(
                "stored job result does not match the contract".into(),
            ))
        })?;
    Ok(JobDto {
        job_id: job.id.0.clone(),
        kind: kind_dto(job.kind),
        status: status_dto(job.status),
        sequence: clamp_u32(job.sequence),
        progress: progress_dto(job.progress),
        started_at: iso(job.started_at),
        finished_at: job.finished_at.map(iso),
        error,
        result,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use langualinker_application::jobs::{JobFailure, JobId, JobProgress};
    use langualinker_domain::ValidationError;

    #[test]
    fn every_application_error_maps_to_a_known_contract_code() {
        let errors = [
            AppError::Validation(ValidationError::new("front", "required")),
            AppError::VersionConflict {
                entity: "note",
                current: 3,
            },
            AppError::NotFound { entity: "job" },
            AppError::Deleted { entity: "deck" },
            AppError::DbBusy,
            AppError::StorageFull,
            AppError::SchemaTooNew {
                found: 3,
                supported: 1,
            },
            AppError::PermissionDenied {
                reason: "secretStore",
            },
            AppError::PreviewStale,
            AppError::UnsupportedFormat { format: "apkg" },
            AppError::Cancelled,
            AppError::NotImplemented {
                operation: "getDashboard",
            },
            AppError::Internal("sqlite: no such table".into()),
        ];
        for error in errors {
            let dto = error_dto(&error);
            assert_eq!(dto.code.as_str(), error.code());
            assert_eq!(dto.message_key, format!("errors.{}", error.code()));
        }
    }

    #[test]
    fn internal_errors_cross_ipc_without_their_text() {
        let dto = error_dto(&AppError::Internal(
            "UNIQUE constraint failed: notes.guid".into(),
        ));
        let json = serde_json::to_string(&dto).unwrap();
        assert!(!json.contains("notes.guid"), "{json}");
        assert_eq!(dto.details, None);
    }

    #[test]
    fn validation_details_name_field_and_reason() {
        let dto = error_dto(&AppError::Validation(ValidationError::new(
            "name", "tooLong",
        )));
        let details = dto.details.unwrap();
        assert_eq!(details["field"], "name");
        assert_eq!(details["reason"], "tooLong");
    }

    #[test]
    fn job_record_maps_to_the_contract_dto() {
        let record = JobRecord {
            id: JobId("42".into()),
            kind: JobKind::Import,
            status: JobStatus::Failed,
            sequence: 4,
            progress: Some(JobProgress { done: 2, total: 4 }),
            operation_id: Some("op".into()),
            started_at: TimestampMs(1_772_438_400_000),
            finished_at: Some(TimestampMs(1_772_438_401_250)),
            error: Some(JobFailure {
                code: "STORAGE_FULL".into(),
                details: vec![("bytes".into(), JobDetail::Number(10))],
            }),
            result_json: Some(r#"{"type":"export","fileName":"a.csv","cardCount":3}"#.into()),
        };
        let dto = job_dto(&record).unwrap();
        assert_eq!(dto.started_at, "2026-03-02T08:00:00.000Z");
        assert_eq!(dto.finished_at.as_deref(), Some("2026-03-02T08:00:01.250Z"));
        let error = dto.error.unwrap();
        assert_eq!(error.code, ErrorCode::StorageFull);
        assert_eq!(error.details.unwrap()["bytes"], 10);
        assert!(matches!(
            dto.result,
            Some(JobResultDto::Export { card_count: 3, .. })
        ));
        let mut broken = record.clone();
        broken.result_json = Some(r#"{"type":"unknown"}"#.into());
        assert_eq!(job_dto(&broken).unwrap_err().code, ErrorCode::Internal);
    }
}
