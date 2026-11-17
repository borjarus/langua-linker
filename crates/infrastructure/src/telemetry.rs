//! Local store of the opt-in usage metrics (plan Phase 16 step 1, ADR-13).
//!
//! A separate SQLite file next to the collection (`telemetry.sqlite`), so metrics never travel
//! inside a backup, a restore never brings back someone's old metrics, and withdrawing consent
//! deletes them without touching the collection.
//!
//! - Nothing is written while the learner has not opted in; [`LocalTelemetry::set_enabled`]
//!   follows the stored consent.
//! - Opting in creates a random pseudonymous id and the cohort start. It identifies this
//!   installation's metrics, not the learner: pseudonymous, not anonymous (plan step 2).
//! - Opting out deletes the id and every event and compacts the file.
//! - Events older than [`RETENTION_DAYS`] and anything above [`MAX_EVENTS`] are pruned.
//! - No network: there is no upload until the vendor/hosting decision (D-08) is made; the
//!   learner can copy the summary from the Privacy screen.
//!
//! Failures are logged and swallowed: metrics must never break the action that produced them.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use langualinker_application::ports::{Clock, Rng, Telemetry};
use langualinker_application::telemetry::{MetricEvent, MetricsSummary, RecordedEvent, summarize};
use langualinker_application::{AppError, AppResult};
use langualinker_domain::TimestampMs;
use rusqlite::{Connection, OptionalExtension, params};

use crate::sqlite::map_sqlite;

/// File name inside the app data directory.
pub const TELEMETRY_FILE: &str = "telemetry.sqlite";
/// Events older than this are deleted.
pub const RETENTION_DAYS: i64 = 180;
/// Upper bound of stored events; the oldest go first.
pub const MAX_EVENTS: i64 = 50_000;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS telemetry_meta (
    id          INTEGER PRIMARY KEY CHECK (id = 1),
    install_id  TEXT    NOT NULL,
    since       INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS telemetry_events (
    id    INTEGER PRIMARY KEY,
    at    INTEGER NOT NULL,
    name  TEXT    NOT NULL,
    a     INTEGER,
    b     INTEGER
);
CREATE INDEX IF NOT EXISTS telemetry_events_at ON telemetry_events (at);
";

/// The pseudonymous id and the summary while the learner is opted in.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalMetrics {
    pub install_id: String,
    pub summary: MetricsSummary,
}

struct State {
    conn: Connection,
    /// Opt-in time while enabled.
    since: Option<TimestampMs>,
}

/// The opt-in metrics store. Cheap to share behind an `Arc`.
pub struct LocalTelemetry {
    state: Mutex<Option<State>>,
    path: Option<PathBuf>,
    clock: Arc<dyn Clock>,
    rng: Arc<dyn Rng>,
}

impl LocalTelemetry {
    /// Opens (creating if needed) `<dir>/telemetry.sqlite`. A store that cannot be opened turns
    /// into one that records nothing.
    pub fn open(dir: &Path, clock: Arc<dyn Clock>, rng: Arc<dyn Rng>) -> Self {
        let path = dir.join(TELEMETRY_FILE);
        let state = std::fs::create_dir_all(dir)
            .map_err(|error| AppError::Internal(error.to_string()))
            .and_then(|()| Connection::open(&path).map_err(map_sqlite))
            .and_then(|conn| Self::prepare(conn, clock.now()));
        if let Err(error) = &state {
            tracing::warn!(code = error.code(), "metrics store unavailable");
        }
        Self {
            state: Mutex::new(state.ok()),
            path: Some(path),
            clock,
            rng,
        }
    }

    /// An in-memory store (tests).
    pub fn in_memory(clock: Arc<dyn Clock>, rng: Arc<dyn Rng>) -> Self {
        let state = Connection::open_in_memory()
            .map_err(map_sqlite)
            .and_then(|conn| Self::prepare(conn, clock.now()))
            .ok();
        Self {
            state: Mutex::new(state),
            path: None,
            clock,
            rng,
        }
    }

    fn prepare(conn: Connection, now: TimestampMs) -> AppResult<State> {
        conn.execute_batch(SCHEMA).map_err(map_sqlite)?;
        let since = conn
            .query_row("SELECT since FROM telemetry_meta WHERE id = 1", [], |row| {
                row.get(0)
            })
            .optional()
            .map_err(map_sqlite)?
            .map(TimestampMs);
        let cutoff = now.0 - RETENTION_DAYS * 86_400_000;
        conn.execute("DELETE FROM telemetry_events WHERE at < ?1", [cutoff])
            .map_err(map_sqlite)?;
        Ok(State { conn, since })
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    fn with_state<T>(&self, work: impl FnOnce(&mut State) -> AppResult<T>) -> AppResult<T> {
        let mut guard = self
            .state
            .lock()
            .map_err(|_| AppError::Internal("metrics store poisoned".into()))?;
        match guard.as_mut() {
            Some(state) => work(state),
            None => Err(AppError::Internal("metrics store unavailable".into())),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.with_state(|state| Ok(state.since.is_some()))
            .unwrap_or(false)
    }

    /// Follows the stored consent. Enabling starts a new cohort with a new id; disabling
    /// deletes the id and every event. Returns whether anything changed.
    pub fn set_enabled(&self, enabled: bool) -> AppResult<bool> {
        let now = self.clock.now();
        let mut bytes = [0u8; 16];
        self.rng.fill(&mut bytes);
        let install_id: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        let changed = self.with_state(move |state| {
            match (enabled, state.since) {
                (true, None) => {
                    let tx = state.conn.transaction().map_err(map_sqlite)?;
                    // Leftovers of an interrupted opt-out never join the new cohort.
                    tx.execute("DELETE FROM telemetry_events", [])
                        .map_err(map_sqlite)?;
                    tx.execute(
                        "INSERT OR REPLACE INTO telemetry_meta (id, install_id, since) VALUES (1, ?1, ?2)",
                        params![install_id, now.0],
                    )
                    .map_err(map_sqlite)?;
                    tx.commit().map_err(map_sqlite)?;
                    state.since = Some(now);
                    Ok(true)
                }
                (false, Some(_)) => {
                    state
                        .conn
                        .execute_batch(
                            "BEGIN IMMEDIATE;
                             DELETE FROM telemetry_events;
                             DELETE FROM telemetry_meta;
                             COMMIT;",
                        )
                        .map_err(map_sqlite)?;
                    state.since = None;
                    // Deleted rows must not linger in free pages of the file.
                    if state.conn.execute_batch("VACUUM").is_err() {
                        tracing::warn!("metrics store not compacted");
                    }
                    Ok(true)
                }
                _ => Ok(false),
            }
        })?;
        if changed {
            tracing::info!(
                status = if enabled { "enabled" } else { "disabled" },
                "usage metrics"
            );
        }
        Ok(changed)
    }

    fn insert(&self, event: MetricEvent, at: TimestampMs) -> AppResult<()> {
        self.with_state(|state| {
            if state.since.is_none() {
                return Ok(());
            }
            let encoded = event.encode();
            state
                .conn
                .execute(
                    "INSERT INTO telemetry_events (at, name, a, b) VALUES (?1, ?2, ?3, ?4)",
                    params![at.0, encoded.name, encoded.a, encoded.b],
                )
                .map_err(map_sqlite)?;
            let id = state.conn.last_insert_rowid();
            // Cheap bound check: ids grow by one per event.
            if id % 1_000 == 0 {
                state
                    .conn
                    .execute(
                        "DELETE FROM telemetry_events WHERE id <= ?1 - ?2",
                        params![id, MAX_EVENTS],
                    )
                    .map_err(map_sqlite)?;
            }
            Ok(())
        })
    }

    /// The pseudonymous id and summary, or `None` while not opted in.
    pub fn metrics(&self) -> AppResult<Option<LocalMetrics>> {
        let now = self.clock.now();
        self.with_state(|state| {
            let Some(since) = state.since else {
                return Ok(None);
            };
            let install_id: String = state
                .conn
                .query_row(
                    "SELECT install_id FROM telemetry_meta WHERE id = 1",
                    [],
                    |row| row.get(0),
                )
                .map_err(map_sqlite)?;
            let mut statement = state
                .conn
                .prepare("SELECT at, name, a, b FROM telemetry_events ORDER BY id")
                .map_err(map_sqlite)?;
            let events: Vec<RecordedEvent> = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<i64>>(2)?,
                        row.get::<_, Option<i64>>(3)?,
                    ))
                })
                .map_err(map_sqlite)?
                .filter_map(Result::ok)
                .filter_map(|(at, name, a, b)| {
                    MetricEvent::decode(&name, a, b).map(|event| RecordedEvent {
                        at: TimestampMs(at),
                        event,
                    })
                })
                .collect();
            Ok(Some(LocalMetrics {
                install_id,
                summary: summarize(&events, since, now),
            }))
        })
    }

    /// Number of stored events (diagnostics and tests).
    pub fn event_count(&self) -> AppResult<i64> {
        self.with_state(|state| {
            state
                .conn
                .query_row("SELECT COUNT(*) FROM telemetry_events", [], |row| {
                    row.get(0)
                })
                .map_err(map_sqlite)
        })
    }
}

impl Telemetry for LocalTelemetry {
    fn record(&self, event: MetricEvent) {
        if let Err(error) = self.insert(event, self.clock.now()) {
            tracing::warn!(code = error.code(), "metric not recorded");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicI64, Ordering};

    use super::*;
    use crate::system::OsRng;

    struct Manual(AtomicI64);

    impl Clock for Manual {
        fn now(&self) -> TimestampMs {
            TimestampMs(self.0.load(Ordering::SeqCst))
        }
    }

    const START: i64 = 1_772_438_400_000;

    fn store() -> (LocalTelemetry, Arc<Manual>) {
        let clock = Arc::new(Manual(AtomicI64::new(START)));
        (
            LocalTelemetry::in_memory(clock.clone(), Arc::new(OsRng)),
            clock,
        )
    }

    #[test]
    fn nothing_is_recorded_without_consent() {
        let (telemetry, _) = store();
        telemetry.record(MetricEvent::AppStarted { cold_start_ms: 900 });
        assert_eq!(telemetry.event_count().unwrap(), 0);
        assert_eq!(telemetry.metrics().unwrap(), None);
    }

    #[test]
    fn opting_in_records_and_summarizes_opting_out_deletes_everything() {
        let (telemetry, clock) = store();
        assert!(telemetry.set_enabled(true).unwrap());
        assert!(!telemetry.set_enabled(true).unwrap(), "idempotent");
        telemetry.record(MetricEvent::AppStarted { cold_start_ms: 900 });
        telemetry.record(MetricEvent::Rated { rating: 3 });
        clock.0.fetch_add(86_400_000, Ordering::SeqCst);
        let metrics = telemetry.metrics().unwrap().unwrap();
        assert_eq!(metrics.install_id.len(), 32);
        assert_eq!(metrics.summary.app_starts, 1);
        assert_eq!(metrics.summary.ratings, [0, 0, 1, 0]);
        assert_eq!(metrics.summary.days_observed, 1);

        assert!(telemetry.set_enabled(false).unwrap());
        assert_eq!(telemetry.event_count().unwrap(), 0);
        assert_eq!(telemetry.metrics().unwrap(), None);
        // A new opt-in is a new cohort with a new id.
        telemetry.set_enabled(true).unwrap();
        let again = telemetry.metrics().unwrap().unwrap();
        assert_ne!(again.install_id, metrics.install_id);
        assert_eq!(again.summary.app_starts, 0);
    }

    #[test]
    fn consent_and_events_survive_a_restart_and_old_events_are_pruned() {
        let dir = tempfile::tempdir().unwrap();
        let clock = Arc::new(Manual(AtomicI64::new(START)));
        {
            let telemetry = LocalTelemetry::open(dir.path(), clock.clone(), Arc::new(OsRng));
            telemetry.set_enabled(true).unwrap();
            telemetry.record(MetricEvent::NativeCrash);
            clock
                .0
                .fetch_add(RETENTION_DAYS * 86_400_000, Ordering::SeqCst);
            telemetry.record(MetricEvent::ClientError);
        }
        clock.0.fetch_add(1, Ordering::SeqCst);
        let reopened = LocalTelemetry::open(dir.path(), clock, Arc::new(OsRng));
        assert!(reopened.is_enabled());
        let summary = reopened.metrics().unwrap().unwrap().summary;
        assert_eq!((summary.native_crashes, summary.client_errors), (0, 1));
    }

    #[test]
    fn opting_out_leaves_no_trace_in_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let clock = Arc::new(Manual(AtomicI64::new(START)));
        let telemetry = LocalTelemetry::open(dir.path(), clock, Arc::new(OsRng));
        telemetry.set_enabled(true).unwrap();
        let id = telemetry.metrics().unwrap().unwrap().install_id;
        for _ in 0..100 {
            telemetry.record(MetricEvent::Rated { rating: 2 });
        }
        telemetry.set_enabled(false).unwrap();
        let bytes = std::fs::read(telemetry.path().unwrap()).unwrap();
        let needle = id.as_bytes();
        assert!(
            !bytes.windows(needle.len()).any(|window| window == needle),
            "pseudonymous id still in the file"
        );
    }

    #[test]
    fn an_unopenable_store_records_nothing_and_does_not_panic() {
        let dir = tempfile::tempdir().unwrap();
        // A directory where the database file should be.
        std::fs::create_dir(dir.path().join(TELEMETRY_FILE)).unwrap();
        let clock = Arc::new(Manual(AtomicI64::new(START)));
        let telemetry = LocalTelemetry::open(dir.path(), clock, Arc::new(OsRng));
        assert!(telemetry.set_enabled(true).is_err());
        telemetry.record(MetricEvent::Undone);
        assert!(!telemetry.is_enabled());
    }
}
