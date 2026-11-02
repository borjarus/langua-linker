//! Durable background jobs (plan step 30). The `jobs` table is the truth: every status change is
//! persisted with a higher `sequence` before the matching event is emitted, so a screen that
//! returns later reads the same state with `get_job_status`.
//!
//! Only `cancel` stops a job — a UI listener going away never does. Once a job entered its commit
//! phase ([`JobContext::begin_commit`]) a cancel request is ignored, so a commit is never torn.

use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use langualinker_domain::TimestampMs;

use crate::ports::{Clock, EventSink};
use crate::repositories::UnitOfWork;
use crate::{AppError, AppResult};

/// Job id as sent over IPC.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct JobId(pub String);

impl fmt::Display for JobId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JobKind {
    PrepareContent,
    Import,
    Export,
    Backup,
    Restore,
    Reset,
    Llm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl JobStatus {
    pub fn is_finished(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

/// Persisted names; explicit so renaming a variant never changes stored rows.
impl JobKind {
    pub const ALL: [JobKind; 7] = [
        Self::PrepareContent,
        Self::Import,
        Self::Export,
        Self::Backup,
        Self::Restore,
        Self::Reset,
        Self::Llm,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::PrepareContent => "prepareContent",
            Self::Import => "import",
            Self::Export => "export",
            Self::Backup => "backup",
            Self::Restore => "restore",
            Self::Reset => "reset",
            Self::Llm => "llm",
        }
    }

    pub fn parse(value: &str) -> AppResult<Self> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.as_str() == value)
            .ok_or_else(|| AppError::Internal(format!("unknown job kind {value}")))
    }
}

impl JobStatus {
    pub const ALL: [JobStatus; 5] = [
        Self::Queued,
        Self::Running,
        Self::Succeeded,
        Self::Failed,
        Self::Cancelled,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(value: &str) -> AppResult<Self> {
        Self::ALL
            .into_iter()
            .find(|status| status.as_str() == value)
            .ok_or_else(|| AppError::Internal(format!("unknown job status {value}")))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JobProgress {
    pub done: u64,
    pub total: u64,
}

/// Why a job failed, in contract terms (`ErrorDto.code` and `details`), as stored.
#[derive(Debug, Clone, PartialEq)]
pub struct JobFailure {
    pub code: String,
    pub details: Vec<(String, JobDetail)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum JobDetail {
    Text(String),
    Number(i64),
}

impl From<&AppError> for JobFailure {
    fn from(error: &AppError) -> Self {
        Self {
            code: error.code().to_owned(),
            details: error
                .details()
                .into_iter()
                .map(|(key, value)| {
                    let value = match value {
                        crate::DetailValue::Text(text) => JobDetail::Text(text.to_owned()),
                        crate::DetailValue::Number(number) => JobDetail::Number(number),
                    };
                    (key.to_owned(), value)
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewJob {
    pub kind: JobKind,
    pub operation_id: Option<String>,
    pub total: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JobRecord {
    pub id: JobId,
    pub kind: JobKind,
    pub status: JobStatus,
    pub sequence: i64,
    pub progress: Option<JobProgress>,
    pub operation_id: Option<String>,
    pub started_at: TimestampMs,
    pub finished_at: Option<TimestampMs>,
    pub error: Option<JobFailure>,
    /// Kind-specific outcome as contract JSON (`JobResultDto`), set when the job finished.
    pub result_json: Option<String>,
}

/// Signal sent with every persisted change.
#[derive(Debug, Clone, PartialEq)]
pub struct JobEvent {
    pub job_id: JobId,
    pub kind: JobKind,
    pub sequence: i64,
    pub status: JobStatus,
    pub progress: Option<JobProgress>,
}

impl JobEvent {
    fn of(job: &JobRecord) -> Self {
        Self {
            job_id: job.id.clone(),
            kind: job.kind,
            sequence: job.sequence,
            status: job.status,
            progress: job.progress,
        }
    }
}

#[derive(Default)]
struct CancelState {
    requested: AtomicBool,
    committing: AtomicBool,
}

/// Handle given to the work of a running job.
pub struct JobContext<U: UnitOfWork> {
    jobs: JobService<U>,
    id: JobId,
    cancel: Arc<CancelState>,
}

impl<U: UnitOfWork> JobContext<U> {
    /// Persists progress and emits it. Fails with `Cancelled` when a cancel was requested before
    /// the commit phase; the work should return that error unchanged.
    pub fn progress(&self, done: u64, total: u64) -> AppResult<()> {
        self.check_cancelled()?;
        self.jobs
            .transition(&self.id, |job| {
                job.progress = Some(JobProgress { done, total })
            })
            .map(drop)
    }

    pub fn check_cancelled(&self) -> AppResult<()> {
        let cancelled = self.cancel.requested.load(Ordering::SeqCst)
            && !self.cancel.committing.load(Ordering::SeqCst);
        if cancelled {
            Err(AppError::Cancelled)
        } else {
            Ok(())
        }
    }

    /// From here on the job finishes even if a cancel arrives (the commit must not be torn).
    /// Fails with `Cancelled` if a cancel already arrived.
    pub fn begin_commit(&self) -> AppResult<()> {
        self.check_cancelled()?;
        self.cancel.committing.store(true, Ordering::SeqCst);
        Ok(())
    }
}

/// Work of a job: returns the kind-specific result JSON (or `None`).
pub type JobWork<U> = Box<dyn FnOnce(&JobContext<U>) -> AppResult<Option<String>> + Send>;

/// Starts, tracks and cancels jobs.
pub struct JobService<U: UnitOfWork> {
    db: Arc<U>,
    events: Arc<dyn EventSink>,
    clock: Arc<dyn Clock>,
    running: Arc<Mutex<HashMap<JobId, Arc<CancelState>>>>,
}

impl<U: UnitOfWork> Clone for JobService<U> {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            events: self.events.clone(),
            clock: self.clock.clone(),
            running: self.running.clone(),
        }
    }
}

impl<U: UnitOfWork> JobService<U> {
    pub fn new(db: Arc<U>, events: Arc<dyn EventSink>, clock: Arc<dyn Clock>) -> Self {
        Self {
            db,
            events,
            clock,
            running: Arc::default(),
        }
    }

    /// Marks jobs left unfinished by a previous process as failed (`INTERNAL`, `interrupted`).
    /// Called once at startup, before any job starts. Returns how many were found.
    pub fn recover_interrupted(&self) -> AppResult<usize> {
        let at = self.clock.now();
        self.db.write(move |tx| {
            let unfinished = tx.unfinished_jobs()?;
            for mut job in unfinished.iter().cloned() {
                job.status = JobStatus::Failed;
                job.sequence += 1;
                job.finished_at = Some(at);
                job.error = Some(JobFailure {
                    code: "INTERNAL".to_owned(),
                    details: vec![(
                        "reason".to_owned(),
                        JobDetail::Text("interrupted".to_owned()),
                    )],
                });
                tx.update_job(&job)?;
            }
            Ok(unfinished.len())
        })
    }

    pub fn get(&self, id: &JobId) -> AppResult<JobRecord> {
        let id = id.clone();
        self.db
            .read(move |tx| tx.job(&id)?.ok_or(AppError::NotFound { entity: "job" }))
    }

    /// Requests cancellation. Returns the job as stored; a finished job is returned unchanged,
    /// a running one finishes as `cancelled` unless it is already committing.
    pub fn cancel(&self, id: &JobId) -> AppResult<JobRecord> {
        let job = self.get(id)?;
        if !job.status.is_finished()
            && let Some(state) = self.running.lock().expect("job registry").get(id)
        {
            state.requested.store(true, Ordering::SeqCst);
        }
        Ok(job)
    }

    /// Starts a job on a background thread. A repeated `operation_id` returns the existing job.
    pub fn start(&self, job: NewJob, work: JobWork<U>) -> AppResult<JobRecord> {
        let at = self.clock.now();
        let new = job.clone();
        let (record, created) = self.db.write(move |tx| {
            if let Some(operation_id) = &new.operation_id
                && let Some(existing) = tx.recorded_operation(operation_id)?
            {
                let id = JobId(existing.result_json);
                let job = tx.job(&id)?.ok_or(AppError::NotFound { entity: "job" })?;
                return Ok((job, false));
            }
            let record = tx.insert_job(&new, at)?;
            if let Some(operation_id) = &new.operation_id {
                let recorded = crate::repositories::RecordedOperation {
                    operation: format!("job:{}", new.kind.as_str()),
                    result_json: record.id.0.clone(),
                };
                tx.record_operation(operation_id, &recorded, at)?;
            }
            Ok((record, true))
        })?;
        if !created {
            return Ok(record);
        }
        let cancel = Arc::new(CancelState::default());
        self.running
            .lock()
            .expect("job registry")
            .insert(record.id.clone(), cancel.clone());
        self.events.job_progress(&JobEvent::of(&record));

        let context = JobContext {
            jobs: self.clone(),
            id: record.id.clone(),
            cancel,
        };
        std::thread::Builder::new()
            .name(format!("job-{}", record.id))
            .spawn(move || context.jobs.clone().run(context, work))
            .map_err(|error| AppError::Internal(format!("cannot start job thread: {error}")))?;
        Ok(record)
    }

    fn run(self, context: JobContext<U>, work: JobWork<U>) {
        let id = context.id.clone();
        let outcome = context
            .check_cancelled()
            .and_then(|()| {
                self.transition(&id, |job| job.status = JobStatus::Running)
                    .map(drop)
            })
            .and_then(|()| work(&context));
        let finished_at = self.clock.now();
        let finish = self.transition(&id, |job| {
            job.finished_at = Some(finished_at);
            match &outcome {
                Ok(result) => {
                    job.status = JobStatus::Succeeded;
                    job.result_json = result.clone();
                }
                Err(AppError::Cancelled) => job.status = JobStatus::Cancelled,
                Err(error) => {
                    job.status = JobStatus::Failed;
                    job.error = Some(JobFailure::from(error));
                }
            }
        });
        if let Err(error) = finish {
            tracing::error!(job_id = %id, code = error.code(), "job state could not be saved");
        }
        self.running.lock().expect("job registry").remove(&id);
    }

    /// Applies a change, bumps the sequence, persists it, then emits the event.
    fn transition(&self, id: &JobId, change: impl FnOnce(&mut JobRecord)) -> AppResult<JobRecord> {
        let mut job = self.get(id)?;
        change(&mut job);
        job.sequence += 1;
        let stored = job.clone();
        self.db.write(move |tx| tx.update_job(&stored))?;
        self.events.job_progress(&JobEvent::of(&job));
        Ok(job)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::CollectionScope;

    /// Database that is never reached: the tests below only exercise in-memory logic.
    struct NoDb;

    impl UnitOfWork for NoDb {
        fn read<T, F>(&self, _: F) -> AppResult<T>
        where
            T: Send + 'static,
            F: FnOnce(&mut dyn crate::repositories::Repositories) -> AppResult<T> + Send + 'static,
        {
            Err(AppError::Internal("no database".into()))
        }

        fn write<T, F>(&self, _: F) -> AppResult<T>
        where
            T: Send + 'static,
            F: FnOnce(&mut dyn crate::repositories::Repositories) -> AppResult<T> + Send + 'static,
        {
            Err(AppError::Internal("no database".into()))
        }

        fn schema_version(&self) -> i64 {
            0
        }
    }

    struct Fixed;

    impl Clock for Fixed {
        fn now(&self) -> TimestampMs {
            TimestampMs(1)
        }
    }

    struct Silent;

    impl EventSink for Silent {
        fn job_progress(&self, _: &JobEvent) {}
        fn collection_changed(&self, _: &[CollectionScope], _: TimestampMs) {}
    }

    fn context() -> JobContext<NoDb> {
        JobContext {
            jobs: JobService::new(Arc::new(NoDb), Arc::new(Silent), Arc::new(Fixed)),
            id: JobId("1".into()),
            cancel: Arc::default(),
        }
    }

    fn record(status: JobStatus) -> JobRecord {
        JobRecord {
            id: JobId("7".into()),
            kind: JobKind::Import,
            status,
            sequence: 4,
            progress: Some(JobProgress { done: 2, total: 5 }),
            operation_id: None,
            started_at: TimestampMs(1),
            finished_at: None,
            error: None,
            result_json: None,
        }
    }

    #[test]
    fn kinds_round_trip_through_their_stored_names() {
        for kind in JobKind::ALL {
            assert_eq!(JobKind::parse(kind.as_str()), Ok(kind));
        }
        assert_eq!(JobKind::PrepareContent.as_str(), "prepareContent");
    }

    #[test]
    fn statuses_round_trip_through_their_stored_names() {
        for status in JobStatus::ALL {
            assert_eq!(JobStatus::parse(status.as_str()), Ok(status));
        }
    }

    #[test]
    fn unknown_names_are_internal_errors() {
        assert_eq!(JobKind::parse("nope").unwrap_err().code(), "INTERNAL");
        assert_eq!(JobStatus::parse("Running").unwrap_err().code(), "INTERNAL");
    }

    #[test]
    fn only_terminal_statuses_are_finished() {
        let finished: Vec<bool> = JobStatus::ALL.iter().map(|s| s.is_finished()).collect();
        assert_eq!(finished, [false, false, true, true, true]);
    }

    #[test]
    fn job_id_displays_its_raw_value() {
        assert_eq!(JobId("abc".into()).to_string(), "abc");
    }

    #[test]
    fn failure_keeps_code_and_details_of_the_error() {
        let failure = JobFailure::from(&AppError::VersionConflict {
            entity: "deck",
            current: 3,
        });
        assert_eq!(failure.code, "VERSION_CONFLICT");
        assert_eq!(
            failure.details,
            [
                ("entity".to_owned(), JobDetail::Text("deck".into())),
                ("currentVersion".to_owned(), JobDetail::Number(3)),
            ]
        );
    }

    #[test]
    fn failure_of_an_internal_error_has_no_details() {
        let failure = JobFailure::from(&AppError::Internal("secret path".into()));
        assert_eq!(failure.code, "INTERNAL");
        assert!(failure.details.is_empty());
    }

    #[test]
    fn event_mirrors_the_stored_record() {
        let job = record(JobStatus::Running);
        let event = JobEvent::of(&job);
        assert_eq!(
            event,
            JobEvent {
                job_id: job.id,
                kind: JobKind::Import,
                sequence: 4,
                status: JobStatus::Running,
                progress: Some(JobProgress { done: 2, total: 5 }),
            }
        );
    }

    #[test]
    fn work_is_not_cancelled_until_requested() {
        assert_eq!(context().check_cancelled(), Ok(()));
    }

    #[test]
    fn a_cancel_request_fails_the_next_check() {
        let ctx = context();
        ctx.cancel.requested.store(true, Ordering::SeqCst);
        assert_eq!(ctx.check_cancelled(), Err(AppError::Cancelled));
        assert_eq!(ctx.begin_commit(), Err(AppError::Cancelled));
        assert!(!ctx.cancel.committing.load(Ordering::SeqCst));
    }

    #[test]
    fn a_cancel_after_begin_commit_is_ignored() {
        let ctx = context();
        assert_eq!(ctx.begin_commit(), Ok(()));
        ctx.cancel.requested.store(true, Ordering::SeqCst);
        assert_eq!(ctx.check_cancelled(), Ok(()));
    }

    #[test]
    fn progress_stops_on_cancel_before_touching_the_database() {
        let ctx = context();
        ctx.cancel.requested.store(true, Ordering::SeqCst);
        assert_eq!(ctx.progress(1, 2), Err(AppError::Cancelled));
    }

    #[test]
    fn progress_surfaces_storage_errors() {
        assert_eq!(context().progress(1, 2).unwrap_err().code(), "INTERNAL");
    }
}
