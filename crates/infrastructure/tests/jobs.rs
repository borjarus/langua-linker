//! Durable job runtime on the real SQLite unit of work (plan step 30).

use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use langualinker_application::AppError;
use langualinker_application::jobs::{
    JobContext, JobEvent, JobId, JobKind, JobRecord, JobService, JobStatus, JobWork, NewJob,
};
use langualinker_application::ports::{Clock, CollectionScope, EventSink};
use langualinker_application::repositories::UnitOfWork;
use langualinker_domain::TimestampMs;
use langualinker_infrastructure::sqlite::SqliteDatabase;

struct Fixed;

impl Clock for Fixed {
    fn now(&self) -> TimestampMs {
        TimestampMs(1_772_438_400_000)
    }
}

#[derive(Default)]
struct Recorder {
    events: Mutex<Vec<JobEvent>>,
}

impl EventSink for Recorder {
    fn job_progress(&self, event: &JobEvent) {
        self.events.lock().unwrap().push(event.clone());
    }

    fn collection_changed(&self, _: &[CollectionScope], _: TimestampMs) {}
}

fn service() -> (
    JobService<SqliteDatabase>,
    Arc<SqliteDatabase>,
    Arc<Recorder>,
) {
    let db = Arc::new(SqliteDatabase::open_in_memory(&Fixed).unwrap().database);
    let recorder = Arc::new(Recorder::default());
    let jobs = JobService::new(db.clone(), recorder.clone(), Arc::new(Fixed));
    (jobs, db, recorder)
}

fn job(kind: JobKind, operation: &str) -> NewJob {
    NewJob {
        kind,
        operation_id: Some(operation.into()),
        total: Some(3),
    }
}

/// Polls the durable status until the job finished (the worker runs on its own thread).
fn wait_finished(jobs: &JobService<SqliteDatabase>, id: &JobId) -> JobRecord {
    for _ in 0..500 {
        let job = jobs.get(id).unwrap();
        if job.status.is_finished() {
            return job;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("job {id} did not finish");
}

#[test]
fn progress_is_persisted_before_each_event_and_the_result_is_kept() {
    let (jobs, _, recorder) = service();
    let started = jobs
        .start(
            job(JobKind::Backup, "op-backup"),
            Box::new(|ctx: &JobContext<SqliteDatabase>| {
                for done in 1..=3 {
                    ctx.progress(done, 3)?;
                }
                ctx.begin_commit()?;
                Ok(Some(r#"{"type":"backup","backupId":"b-1"}"#.into()))
            }),
        )
        .unwrap();
    assert_eq!((started.status, started.sequence), (JobStatus::Queued, 0));
    let finished = wait_finished(&jobs, &started.id);
    assert_eq!(finished.status, JobStatus::Succeeded);
    assert_eq!(
        finished.result_json.as_deref(),
        Some(r#"{"type":"backup","backupId":"b-1"}"#)
    );
    assert!(finished.finished_at.is_some());

    let events = recorder.events.lock().unwrap().clone();
    let sequences: Vec<i64> = events.iter().map(|event| event.sequence).collect();
    assert_eq!(
        sequences,
        (0..=5).collect::<Vec<_>>(),
        "queued, running, 3× progress, done"
    );
    let last = events.last().unwrap();
    assert_eq!(
        (last.status, last.sequence),
        (finished.status, finished.sequence)
    );
}

#[test]
fn a_repeated_operation_id_returns_the_same_job() {
    let (jobs, _, _) = service();
    let work = || -> JobWork<SqliteDatabase> { Box::new(|_| Ok(None)) };
    let first = jobs.start(job(JobKind::Export, "op-same"), work()).unwrap();
    wait_finished(&jobs, &first.id);
    let second = jobs.start(job(JobKind::Export, "op-same"), work()).unwrap();
    assert_eq!(second.id, first.id);
    assert_eq!(second.status, JobStatus::Succeeded);
}

/// Work that blocks until the test lets it continue, then reports progress.
fn gated(gate: Receiver<()>, commit_first: bool) -> JobWork<SqliteDatabase> {
    Box::new(move |ctx| {
        ctx.progress(1, 3)?;
        if commit_first {
            ctx.begin_commit()?;
        }
        gate.recv().unwrap();
        ctx.progress(2, 3)?;
        ctx.progress(3, 3)?;
        Ok(None)
    })
}

fn wait_for_progress(jobs: &JobService<SqliteDatabase>, id: &JobId) {
    for _ in 0..500 {
        if jobs.get(id).unwrap().progress.is_some_and(|p| p.done == 1) {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("job {id} made no progress");
}

#[test]
fn cancel_stops_a_running_job_before_its_commit() {
    let (jobs, _, _) = service();
    let (open, gate) = mpsc::channel();
    let started = jobs
        .start(job(JobKind::Import, "op-cancel"), gated(gate, false))
        .unwrap();
    wait_for_progress(&jobs, &started.id);
    let requested = jobs.cancel(&started.id).unwrap();
    assert_eq!(
        requested.status,
        JobStatus::Running,
        "cancel is a request; the job confirms it"
    );
    open.send(()).unwrap();
    let finished = wait_finished(&jobs, &started.id);
    assert_eq!(finished.status, JobStatus::Cancelled);
    assert_eq!(finished.progress.map(|p| p.done), Some(1));
    assert!(finished.error.is_none());
}

#[test]
fn cancel_during_the_commit_phase_is_ignored() {
    let (jobs, _, _) = service();
    let (open, gate) = mpsc::channel();
    let started = jobs
        .start(job(JobKind::Restore, "op-commit"), gated(gate, true))
        .unwrap();
    wait_for_progress(&jobs, &started.id);
    jobs.cancel(&started.id).unwrap();
    open.send(()).unwrap();
    assert_eq!(
        wait_finished(&jobs, &started.id).status,
        JobStatus::Succeeded
    );
}

#[test]
fn failures_are_stored_as_contract_errors() {
    let (jobs, _, _) = service();
    let started = jobs
        .start(
            job(JobKind::Import, "op-fail"),
            Box::new(|_| Err(AppError::StorageFull)),
        )
        .unwrap();
    let finished = wait_finished(&jobs, &started.id);
    assert_eq!(finished.status, JobStatus::Failed);
    assert_eq!(finished.error.unwrap().code, "STORAGE_FULL");
    // A finished job is returned unchanged by cancel.
    assert_eq!(jobs.cancel(&started.id).unwrap().status, JobStatus::Failed);
    let missing = jobs.get(&JobId("12345".into())).unwrap_err();
    assert_eq!(missing, AppError::NotFound { entity: "job" });
}

#[test]
fn jobs_left_running_by_a_previous_process_are_marked_failed() {
    let (jobs, db, _) = service();
    // A job row as a killed process would leave it.
    let orphan = db
        .write(|tx| {
            let mut job = tx.insert_job(&job(JobKind::Import, "op-orphan"), TimestampMs(1))?;
            job.status = JobStatus::Running;
            job.sequence = 1;
            tx.update_job(&job)?;
            Ok(job)
        })
        .unwrap();
    assert_eq!(jobs.recover_interrupted().unwrap(), 1);
    let recovered = jobs.get(&orphan.id).unwrap();
    assert_eq!(
        (recovered.status, recovered.sequence),
        (JobStatus::Failed, 2)
    );
    let error = recovered.error.unwrap();
    assert_eq!(error.code, "INTERNAL");
    assert_eq!(jobs.recover_interrupted().unwrap(), 0);
}
