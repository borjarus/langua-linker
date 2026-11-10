//! SQLite persistence (plan Phase 7 step 3).
//!
//! One connection lives on a dedicated worker thread; callers send closures and block on the
//! reply, so SQL never runs on the UI or Tokio threads and writes are serialized. The Tauri shell
//! calls [`SqliteDatabase`] from blocking tasks. Reads share the worker for now; a read-only
//! connection pool can be added behind the same `UnitOfWork` port if profiling asks for it.
//!
//! Connection settings: `foreign_keys=ON`, WAL with `synchronous=NORMAL`, `busy_timeout` 5 s,
//! bundled SQLite with FTS5. WAL is tested on desktop only so far; the Android/iOS check is part of
//! the Phase 7 device run (plan step 20, ADR-09).

mod codec;
pub mod migrations;
mod repos;
pub mod seed;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use langualinker_application::ports::Clock;
use langualinker_application::repositories::{Repositories, UnitOfWork};
use langualinker_application::{AppError, AppResult};
use rusqlite::{Connection, ErrorCode, TransactionBehavior};

pub use migrations::{MIGRATIONS, Migration, MigrationReport};
use repos::SqliteRepos;
pub(crate) use repos::collection_counts;

/// File name of the collection inside the app data directory.
pub const DATABASE_FILE: &str = "collection.sqlite";

const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// Maps SQLite failures to application errors. Messages stay in `Internal` for logs only.
pub(crate) fn map_sqlite(error: rusqlite::Error) -> AppError {
    if let rusqlite::Error::SqliteFailure(failure, _) = &error {
        match failure.code {
            ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked => return AppError::DbBusy,
            ErrorCode::DiskFull => return AppError::StorageFull,
            _ => {}
        }
    }
    AppError::Internal(format!("sqlite: {error}"))
}

type Task = Box<dyn FnOnce(&mut Connection) + Send>;

/// Handle to the database worker. Cheap to clone; the worker stops when the last handle drops.
#[derive(Clone)]
pub struct SqliteDatabase {
    sender: mpsc::Sender<Task>,
    schema_version: i64,
    path: Option<Arc<PathBuf>>,
}

/// Outcome of opening the collection.
pub struct OpenedDatabase {
    pub database: SqliteDatabase,
    pub migration: MigrationReport,
}

impl SqliteDatabase {
    /// Opens (creating if needed) and migrates the collection at `path`.
    pub fn open(path: &Path, clock: &dyn Clock) -> AppResult<OpenedDatabase> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|error| AppError::Internal(error.to_string()))?;
        }
        let conn = Connection::open(path).map_err(map_sqlite)?;
        Self::start(conn, Some(path.to_owned()), MIGRATIONS, clock)
    }

    /// In-memory collection for tests and the benchmark.
    pub fn open_in_memory(clock: &dyn Clock) -> AppResult<OpenedDatabase> {
        Self::open_in_memory_with(MIGRATIONS, clock)
    }

    pub fn open_in_memory_with(
        migrations: &[Migration],
        clock: &dyn Clock,
    ) -> AppResult<OpenedDatabase> {
        let conn = Connection::open_in_memory().map_err(map_sqlite)?;
        Self::start(conn, None, migrations, clock)
    }

    fn start(
        mut conn: Connection,
        path: Option<PathBuf>,
        migrations: &[Migration],
        clock: &dyn Clock,
    ) -> AppResult<OpenedDatabase> {
        configure(&conn, path.is_some())?;
        let now = clock.now().0;
        let migration = migrations::migrate(&mut conn, path.as_deref(), migrations, now)?;
        conn.execute(
            "INSERT OR IGNORE INTO collection (id, created_at) VALUES (1, ?1)",
            [now],
        )
        .map_err(map_sqlite)?;

        let (sender, receiver) = mpsc::channel::<Task>();
        thread::Builder::new()
            .name("sqlite".into())
            .spawn(move || {
                let mut conn = conn;
                for task in receiver {
                    task(&mut conn);
                }
                // Last handle dropped: checkpoint WAL and close cleanly.
                let _ = conn.close();
            })
            .map_err(|error| {
                AppError::Internal(format!("cannot start database thread: {error}"))
            })?;
        Ok(OpenedDatabase {
            database: Self {
                sender,
                schema_version: migration.to_version,
                path: path.map(Arc::new),
            },
            migration,
        })
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref().map(PathBuf::as_path)
    }

    /// Runs `task` on the worker thread with the raw connection (maintenance, benchmarks).
    pub fn with_connection<T, F>(&self, task: F) -> AppResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection) -> AppResult<T> + Send + 'static,
    {
        let (reply, result) = mpsc::sync_channel(1);
        self.sender
            .send(Box::new(move |conn| {
                let _ = reply.send(task(conn));
            }))
            .map_err(|_| AppError::Internal("database thread stopped".into()))?;
        result
            .recv()
            .map_err(|_| AppError::Internal("database thread stopped".into()))?
    }

    /// Writes a consistent copy of the collection to `target` with `VACUUM INTO` (plan Phase 12
    /// step 10): one read transaction on the worker, WAL content included, never a raw file copy
    /// of a live database. `target` must not exist.
    pub fn snapshot_to(&self, target: &Path) -> AppResult<()> {
        let target = target
            .to_str()
            .ok_or_else(|| AppError::Internal("snapshot path is not UTF-8".into()))?
            .to_owned();
        self.with_connection(move |conn| {
            conn.execute("VACUUM INTO ?1", [target])
                .map(drop)
                .map_err(map_sqlite)
        })
    }

    /// Replaces the whole collection with the database at `source` (restore, delete everything).
    /// The SQLite backup API copies every page into the open connection in one step, i.e. one
    /// write transaction: a crash or kill during the copy leaves the previous collection intact
    /// (the WAL/journal rolls it back), and no other request runs in between because they share
    /// the worker. `source` must already be at this build's schema version.
    pub fn replace_with(&self, source: &Path) -> AppResult<()> {
        let source = source.to_owned();
        self.with_connection(move |conn| {
            let from = Connection::open_with_flags(
                &source,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                    | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .map_err(map_sqlite)?;
            {
                let backup = rusqlite::backup::Backup::new(&from, conn).map_err(map_sqlite)?;
                // -1 copies every page in one step.
                match backup.step(-1).map_err(map_sqlite)? {
                    rusqlite::backup::StepResult::Done => {}
                    _ => return Err(AppError::DbBusy),
                }
            }
            // Old pages must not linger in the WAL after a privacy reset.
            if conn.is_autocommit() {
                let _ = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()));
            }
            Ok(())
        })
    }

    fn transaction<T, F>(&self, behavior: TransactionBehavior, work: F) -> AppResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut dyn Repositories) -> AppResult<T> + Send + 'static,
    {
        self.with_connection(move |conn| {
            let tx = conn
                .transaction_with_behavior(behavior)
                .map_err(map_sqlite)?;
            let value = work(&mut SqliteRepos::new(&tx))?;
            tx.commit().map_err(map_sqlite)?;
            Ok(value)
        })
    }
}

impl UnitOfWork for SqliteDatabase {
    fn read<T, F>(&self, work: F) -> AppResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut dyn Repositories) -> AppResult<T> + Send + 'static,
    {
        self.transaction(TransactionBehavior::Deferred, work)
    }

    fn write<T, F>(&self, work: F) -> AppResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut dyn Repositories) -> AppResult<T> + Send + 'static,
    {
        self.transaction(TransactionBehavior::Immediate, work)
    }

    fn schema_version(&self) -> i64 {
        self.schema_version
    }
}

fn configure(conn: &Connection, file: bool) -> AppResult<()> {
    conn.busy_timeout(BUSY_TIMEOUT).map_err(map_sqlite)?;
    conn.pragma_update(None, "foreign_keys", true)
        .map_err(map_sqlite)?;
    if file {
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(map_sqlite)?;
        conn.pragma_update(None, "synchronous", "NORMAL")
            .map_err(map_sqlite)?;
    }
    let foreign_keys: bool = conn
        .pragma_query_value(None, "foreign_keys", |row| row.get(0))
        .map_err(map_sqlite)?;
    if !foreign_keys {
        return Err(AppError::Internal(
            "foreign keys could not be enabled".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests;
