//! Backups, restore and reset (plan Phase 12 steps 8 and 10–16, ADR-17).
//!
//! **Archive** (`.llbackup`): one line of manifest JSON, then the SQLite database bytes. The
//! manifest names the format and its version, the app and schema versions, the kind, the counts
//! and the SHA-256 and size of the database part; media are listed (none exist yet). Secrets
//! never enter the database, so an archive never holds the AI key (ADR-07). The content is
//! sensitive but not encrypted: passphrase encryption is a separate decision (D-14).
//!
//! **Snapshot**: `VACUUM INTO` on the database worker (a consistent read, WAL included), never a
//! raw copy of the live file. The copy is checked (`quick_check`, schema, counts), hashed, written
//! to a hidden temporary file and renamed into place, so a kill leaves either no archive or a
//! complete one.
//!
//! **Restore**: the archive is validated in a staging file (format, size, checksum, integrity,
//! schema, media) before anything changes; the restore job writes a safety backup of the current
//! collection, migrates the staged copy to this build's schema, records the running job in it,
//! and swaps it in with the SQLite backup API in one transaction ([`SqliteDatabase::replace_with`]).
//!
//! **Reset**: "progress" returns every card to New and deletes the review history in one
//! transaction; "everything" swaps in an empty collection (keeping the UI language, zone and
//! cutoff), deletes media and the stored AI key. Both need a fresh, verified backup made after the
//! preview, or the typed confirmation phrase. Backups themselves are separate files and stay.

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::DateTime;
use langualinker_application::jobs::{JobId, JobKind, JobRecord, JobService, NewJob};
use langualinker_application::ports::{Clock, CollectionScope, EventSink, Rng, SecretStore};
use langualinker_application::repositories::{CollectionCounts, UnitOfWork};
use langualinker_application::settings::{self, BackupTarget, SettingsDoc};
use langualinker_application::{AppError, AppResult};
use langualinker_domain::TimestampMs;
use rusqlite::{Connection, OpenFlags, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::sqlite::{MIGRATIONS, SqliteDatabase, collection_counts, map_sqlite, migrations};

pub const BACKUP_EXTENSION: &str = "llbackup";
pub const FORMAT: &str = "langualinker-backup";
pub const FORMAT_VERSION: u32 = 1;
/// Largest archive a restore reads (a 50k-card collection with years of history is far below).
pub const MAX_BACKUP_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
/// Safety copies kept besides the regular rotation.
pub const SAFETY_KEPT: usize = 5;
/// An automatic backup is made at most once per this interval, and only after studying.
pub const AUTOMATIC_INTERVAL_MS: i64 = 24 * 60 * 60 * 1000;
const ALL_SCOPES: [CollectionScope; 6] = [
    CollectionScope::Decks,
    CollectionScope::Cards,
    CollectionScope::Notes,
    CollectionScope::Settings,
    CollectionScope::Stats,
    CollectionScope::Backups,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BackupKind {
    Manual,
    Automatic,
    Safety,
}

impl BackupKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Automatic => "automatic",
            Self::Safety => "safety",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupLocation {
    /// Inside the app's data directory: removed when the app is uninstalled.
    App,
    /// A folder the user picked (desktop).
    External,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestCounts {
    pub decks: u32,
    pub notes: u32,
    pub cards: u32,
    pub reviews: u32,
}

impl From<CollectionCounts> for ManifestCounts {
    fn from(counts: CollectionCounts) -> Self {
        Self {
            decks: counts.decks,
            notes: counts.notes,
            cards: counts.cards,
            reviews: counts.reviews,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabasePart {
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaPart {
    pub files: u32,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub format: String,
    pub format_version: u32,
    pub created_at: i64,
    pub kind: BackupKind,
    pub app_version: String,
    pub schema_version: i64,
    pub counts: ManifestCounts,
    pub database: DatabasePart,
    pub media: MediaPart,
}

/// A backup file found in a backup folder.
#[derive(Debug, Clone, PartialEq)]
pub struct BackupInfo {
    pub id: String,
    pub path: PathBuf,
    pub location: BackupLocation,
    pub manifest: Manifest,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetScope {
    Progress,
    Everything,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResetPreview {
    pub scope: ResetScope,
    pub counts: CollectionCounts,
    pub phrase: &'static str,
    pub latest_backup_at: Option<TimestampMs>,
    pub token: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RestorePreview {
    pub file_name: String,
    pub created_at: TimestampMs,
    pub schema_version: i64,
    pub counts: CollectionCounts,
    pub current: CollectionCounts,
    pub token: String,
}

/// Maps a file-system failure; `reason` names what was being accessed (never a path).
fn io_error(error: &io::Error, reason: &'static str) -> AppError {
    match error.kind() {
        io::ErrorKind::StorageFull | io::ErrorKind::QuotaExceeded => AppError::StorageFull,
        io::ErrorKind::PermissionDenied | io::ErrorKind::ReadOnlyFilesystem => {
            AppError::PermissionDenied { reason }
        }
        io::ErrorKind::NotFound => AppError::NotFound { entity: "backup" },
        kind => AppError::Internal(format!("{reason}: {kind}")),
    }
}

fn corrupt(reason: &'static str) -> AppError {
    AppError::BackupCorrupt { reason }
}

/// Files removed when the guard drops, unless kept.
struct TempFiles(Vec<PathBuf>);

impl TempFiles {
    fn keep(mut self) {
        self.0.clear();
    }
}

impl Drop for TempFiles {
    fn drop(&mut self) {
        for path in &self.0 {
            let _ = fs::remove_file(path);
        }
    }
}

/// Backup ids are file stems we wrote: `YYYYMMDD-HHMMSS-mmm-<kind>[-n]`.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && id.as_bytes()[0].is_ascii_digit()
}

fn stem(at: TimestampMs, kind: BackupKind) -> String {
    let time = DateTime::from_timestamp_millis(at.0).unwrap_or_default();
    format!("{}-{}", time.format("%Y%m%d-%H%M%S-%3f"), kind.as_str())
}

fn hash_file(path: &Path) -> AppResult<(u64, String)> {
    let mut file = File::open(path).map_err(|error| io_error(&error, "backupRead"))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 256 * 1024];
    let mut total = 0u64;
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| io_error(&error, "backupRead"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        total += read as u64;
    }
    Ok((total, hex(&hasher.finalize())))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Reads and checks the manifest line; returns it with the offset of the database part.
pub fn read_manifest(path: &Path) -> AppResult<(Manifest, u64)> {
    let file = File::open(path).map_err(|error| io_error(&error, "backupRead"))?;
    let mut reader = BufReader::new(file);
    let head = reader
        .fill_buf()
        .map_err(|error| io_error(&error, "backupRead"))?;
    if head.starts_with(b"SQLite format 3\0") {
        return Err(AppError::UnsupportedFormat { format: "sqlite" });
    }
    if head.starts_with(b"PK\x03\x04") {
        return Err(AppError::UnsupportedFormat { format: "zip" });
    }
    if !head.starts_with(b"{") {
        return Err(corrupt("format"));
    }
    let mut line = Vec::new();
    reader
        .by_ref()
        .take(MAX_MANIFEST_BYTES)
        .read_until(b'\n', &mut line)
        .map_err(|error| io_error(&error, "backupRead"))?;
    if line.last() != Some(&b'\n') {
        return Err(corrupt("manifest"));
    }
    let header: serde_json::Value =
        serde_json::from_slice(&line).map_err(|_| corrupt("manifest"))?;
    if header["format"] != FORMAT {
        return Err(corrupt("format"));
    }
    if header["formatVersion"]
        .as_u64()
        .is_some_and(|version| version > u64::from(FORMAT_VERSION))
    {
        return Err(AppError::UnsupportedFormat {
            format: "newerBackup",
        });
    }
    let manifest: Manifest = serde_json::from_value(header).map_err(|_| corrupt("manifest"))?;
    Ok((manifest, line.len() as u64))
}

/// Copies the database part of an archive to `target`, checking its size and checksum.
fn extract_database(
    archive: &Path,
    offset: u64,
    manifest: &Manifest,
    target: &Path,
) -> AppResult<()> {
    let mut file = File::open(archive).map_err(|error| io_error(&error, "backupRead"))?;
    let length = file
        .metadata()
        .map_err(|error| io_error(&error, "backupRead"))?
        .len();
    if length.checked_sub(offset) != Some(manifest.database.bytes) {
        return Err(corrupt("size"));
    }
    file.seek(SeekFrom::Start(offset))
        .map_err(|error| io_error(&error, "backupRead"))?;
    let mut out = File::create(target).map_err(|error| io_error(&error, "staging"))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 256 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| io_error(&error, "backupRead"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        out.write_all(&buffer[..read])
            .map_err(|error| io_error(&error, "staging"))?;
    }
    out.sync_all()
        .map_err(|error| io_error(&error, "staging"))?;
    if hex(&hasher.finalize()) != manifest.database.sha256 {
        return Err(corrupt("checksum"));
    }
    Ok(())
}

/// Integrity, schema version and counts of a staged database.
fn inspect_database(path: &Path) -> AppResult<(i64, CollectionCounts)> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|_| corrupt("database"))?;
    let check: String = conn
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .map_err(|_| corrupt("database"))?;
    if check != "ok" {
        return Err(corrupt("integrity"));
    }
    let version = migrations::applied_version(&conn).map_err(|_| corrupt("database"))?;
    let latest = migrations::latest_version(MIGRATIONS);
    if version > latest {
        return Err(AppError::SchemaTooNew {
            found: version,
            supported: latest,
        });
    }
    if version == 0 {
        return Err(corrupt("database"));
    }
    let counts = collection_counts(&conn).map_err(|_| corrupt("database"))?;
    Ok((version, counts))
}

/// Jobs a snapshot caught mid-flight (the backup job itself, for one) are closed as interrupted,
/// like a restart would.
fn close_unfinished_jobs(conn: &Connection, at: TimestampMs) -> AppResult<()> {
    conn.execute(
        "UPDATE jobs SET status = 'failed', error_code = 'INTERNAL',
             error_details = '{\"reason\":\"interrupted\"}', finished_at = ?1,
             sequence = sequence + 1
         WHERE status IN ('queued', 'running')",
        [at.0],
    )
    .map(drop)
    .map_err(map_sqlite)
}

/// Records the job doing a swap (and its operation id) in the database that replaces the
/// collection, so the job's state and a retried request survive the swap.
fn stamp_job(conn: &Connection, job: &JobRecord, at: TimestampMs) -> AppResult<()> {
    close_unfinished_jobs(conn, at)?;
    conn.execute(
        "INSERT OR REPLACE INTO jobs (id, kind, status, sequence, progress_done, progress_total,
             operation_id, started_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            job.id
                .0
                .parse::<i64>()
                .map_err(|_| AppError::Internal("job id".into()))?,
            job.kind.as_str(),
            job.status.as_str(),
            job.sequence,
            job.progress.map(|progress| progress.done as i64),
            job.progress.map(|progress| progress.total as i64),
            job.operation_id,
            job.started_at.0,
        ],
    )
    .map_err(map_sqlite)?;
    if let Some(operation_id) = &job.operation_id {
        conn.execute(
            "INSERT OR REPLACE INTO operations (operation_id, operation, result, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                operation_id,
                format!("job:{}", job.kind.as_str()),
                job.id.0,
                at.0
            ],
        )
        .map_err(map_sqlite)?;
    }
    Ok(())
}

struct RestorePlan {
    token: String,
    staging: PathBuf,
    manifest: Manifest,
    fingerprint: String,
}

struct ResetPlan {
    token: String,
    scope: ResetScope,
    phrase: &'static str,
    fingerprint: String,
    backups_before: HashSet<String>,
}

#[derive(Default)]
struct Plans {
    restore: Option<RestorePlan>,
    reset: Option<ResetPlan>,
}

/// Paths of one installation's backup data.
#[derive(Debug, Clone)]
pub struct BackupDirs {
    /// App-location backups.
    pub backups: PathBuf,
    /// Temporary files of snapshots, validations and picked restore files.
    pub staging: PathBuf,
    /// Media files of the collection (none yet; deleted with everything).
    pub media: PathBuf,
}

impl BackupDirs {
    pub fn under(data_dir: &Path) -> Self {
        Self {
            backups: data_dir.join("backups"),
            staging: data_dir.join("staging"),
            media: data_dir.join("media"),
        }
    }
}

/// Creates, lists, restores and verifies backups; resets the collection.
#[derive(Clone)]
pub struct BackupService {
    db: Arc<SqliteDatabase>,
    jobs: JobService<SqliteDatabase>,
    events: Arc<dyn EventSink>,
    clock: Arc<dyn Clock>,
    rng: Arc<dyn Rng>,
    secrets: Option<Arc<dyn SecretStore>>,
    dirs: Arc<BackupDirs>,
    app_version: Arc<str>,
    plans: Arc<Mutex<Plans>>,
}

impl BackupService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        db: Arc<SqliteDatabase>,
        jobs: JobService<SqliteDatabase>,
        events: Arc<dyn EventSink>,
        clock: Arc<dyn Clock>,
        rng: Arc<dyn Rng>,
        secrets: Option<Arc<dyn SecretStore>>,
        dirs: BackupDirs,
        app_version: &str,
    ) -> Self {
        // Staged files of a previous run are never resumed.
        let _ = fs::remove_dir_all(&dirs.staging);
        Self {
            db,
            jobs,
            events,
            clock,
            rng,
            secrets,
            dirs: Arc::new(dirs),
            app_version: app_version.into(),
            plans: Arc::default(),
        }
    }

    pub fn staging_dir(&self) -> AppResult<PathBuf> {
        fs::create_dir_all(&self.dirs.staging).map_err(|error| io_error(&error, "staging"))?;
        Ok(self.dirs.staging.clone())
    }

    fn settings(&self) -> AppResult<SettingsDoc> {
        self.db.read(|tx| settings::load_doc(tx))
    }

    fn external_dir(doc: &SettingsDoc) -> Option<PathBuf> {
        doc.backup.external_path.as_ref().map(PathBuf::from)
    }

    /// Backups in the app folder and the external folder, newest first. Unreadable or foreign
    /// files are skipped (and logged by kind only).
    pub fn list(&self) -> AppResult<Vec<BackupInfo>> {
        let doc = self.settings()?;
        let mut found = scan(&self.dirs.backups, BackupLocation::App)?;
        if let Some(external) = Self::external_dir(&doc) {
            match scan(&external, BackupLocation::External) {
                Ok(more) => found.extend(more),
                Err(error) => {
                    tracing::warn!(code = error.code(), "external backup folder not readable")
                }
            }
        }
        found.sort_by(|a, b| {
            b.manifest
                .created_at
                .cmp(&a.manifest.created_at)
                .then_with(|| b.id.cmp(&a.id))
        });
        Ok(found)
    }

    pub fn find(&self, id: &str) -> AppResult<BackupInfo> {
        if !valid_id(id) {
            return Err(AppError::NotFound { entity: "backup" });
        }
        self.list()?
            .into_iter()
            .find(|backup| backup.id == id)
            .ok_or(AppError::NotFound { entity: "backup" })
    }

    /// Writes a backup now (the job body; also the safety copy of restore and migration).
    /// `progress(step)` is called after each of four steps.
    pub fn write_backup(
        &self,
        kind: BackupKind,
        progress: &dyn Fn(u64) -> AppResult<()>,
    ) -> AppResult<BackupInfo> {
        let now = self.clock.now();
        let doc = self.settings()?;
        let (dir, location) = match (kind, doc.backup.target, Self::external_dir(&doc)) {
            (BackupKind::Safety, ..) | (_, BackupTarget::App, _) | (_, _, None) => {
                (self.dirs.backups.clone(), BackupLocation::App)
            }
            (_, BackupTarget::External, Some(path)) => (path, BackupLocation::External),
        };
        let folder_reason = match location {
            BackupLocation::App => "backupStorage",
            BackupLocation::External => "backupFolder",
        };
        fs::create_dir_all(&dir).map_err(|error| io_error(&error, folder_reason))?;
        let staging = self.staging_dir()?;

        let snapshot = staging.join(format!("snapshot-{}.sqlite", self.rng.guid()));
        let temp = TempFiles(vec![snapshot.clone()]);
        self.db.snapshot_to(&snapshot)?;
        {
            let conn = Connection::open(&snapshot).map_err(map_sqlite)?;
            close_unfinished_jobs(&conn, now)?;
        }
        progress(1)?;
        let (schema_version, counts) = inspect_database(&snapshot)?;
        let (bytes, sha256) = hash_file(&snapshot)?;
        progress(2)?;

        let mut id = stem(now, kind);
        let mut suffix = 2;
        while dir.join(format!("{id}.{BACKUP_EXTENSION}")).exists() {
            id = format!("{}-{suffix}", stem(now, kind));
            suffix += 1;
        }
        let manifest = Manifest {
            format: FORMAT.to_owned(),
            format_version: FORMAT_VERSION,
            created_at: now.0,
            kind,
            app_version: self.app_version.to_string(),
            schema_version,
            counts: counts.into(),
            database: DatabasePart { bytes, sha256 },
            media: MediaPart::default(),
        };
        let path = dir.join(format!("{id}.{BACKUP_EXTENSION}"));
        let partial = dir.join(format!(".{id}.tmp"));
        let partial_guard = TempFiles(vec![partial.clone()]);
        write_archive(&partial, &manifest, &snapshot).map_err(|error| match error {
            AppError::PermissionDenied { .. } => AppError::PermissionDenied {
                reason: folder_reason,
            },
            other => other,
        })?;
        fs::rename(&partial, &path).map_err(|error| io_error(&error, folder_reason))?;
        partial_guard.keep();
        drop(temp);
        progress(3)?;

        let retention = doc.backup.retention.max(1) as usize;
        self.prune(&dir, location, kind, retention, &id);
        progress(4)?;
        tracing::info!(
            kind = kind.as_str(),
            cards = counts.cards,
            bytes,
            "backup written"
        );
        Ok(BackupInfo {
            size_bytes: bytes
                + serde_json::to_vec(&manifest).map_or(0, |line| line.len() as u64 + 1),
            id,
            path,
            location,
            manifest,
        })
    }

    /// Keeps the newest `retention` regular backups of the folder, and [`SAFETY_KEPT`] safety
    /// copies. Failures are logged; a backup that was written stays written.
    fn prune(
        &self,
        dir: &Path,
        location: BackupLocation,
        kind: BackupKind,
        retention: usize,
        keep: &str,
    ) {
        let Ok(mut found) = scan(dir, location) else {
            return;
        };
        found.sort_by(|a, b| b.manifest.created_at.cmp(&a.manifest.created_at));
        let safety = kind == BackupKind::Safety;
        let limit = if safety { SAFETY_KEPT } else { retention };
        let old = found
            .into_iter()
            .filter(|backup| (backup.manifest.kind == BackupKind::Safety) == safety)
            .skip(limit)
            .filter(|backup| backup.id != keep);
        for backup in old {
            if let Err(error) = fs::remove_file(&backup.path) {
                tracing::warn!(kind = %error.kind(), "old backup not removed");
            }
        }
    }

    /// Starts a backup job. A repeated `operation_id` returns the same job.
    pub fn create(&self, operation_id: Option<String>, kind: BackupKind) -> AppResult<JobRecord> {
        let service = self.clone();
        let events = self.events.clone();
        let clock = self.clock.clone();
        self.jobs.start(
            NewJob {
                kind: JobKind::Backup,
                operation_id,
                total: Some(4),
            },
            Box::new(move |ctx| {
                let backup = service.write_backup(kind, &|step| ctx.progress(step, 4))?;
                events.collection_changed(&[CollectionScope::Backups], clock.now());
                Ok(Some(
                    serde_json::json!({ "type": "backup", "backupId": backup.id }).to_string(),
                ))
            }),
        )
    }

    /// Whether an automatic backup is due: enabled, studied since the newest regular backup, and
    /// that backup older than a day. Checked when the app comes to the foreground; nothing runs
    /// after the app is killed (no native scheduler).
    pub fn automatic_due(&self) -> AppResult<bool> {
        let doc = self.settings()?;
        if !doc.backup.automatic {
            return Ok(false);
        }
        let Some(studied) = self.db.read(|tx| tx.newest_review_at())? else {
            return Ok(false);
        };
        let newest = self
            .list()?
            .into_iter()
            .filter(|backup| backup.manifest.kind != BackupKind::Safety)
            .map(|backup| backup.manifest.created_at)
            .max();
        let now = self.clock.now().0;
        Ok(newest.is_none_or(|at| studied.0 > at && now - at >= AUTOMATIC_INTERVAL_MS))
    }

    /// Starts the automatic backup when it is due and no other job runs.
    pub fn start_automatic(&self) -> AppResult<Option<JobRecord>> {
        if !self.jobs.running().is_empty() || !self.automatic_due()? {
            return Ok(None);
        }
        self.create(None, BackupKind::Automatic).map(Some)
    }

    // ------------------------------------------------------------ restore

    pub fn validate_backup(&self, id: &str) -> AppResult<RestorePreview> {
        let backup = self.find(id)?;
        self.validate(backup.id.clone(), &backup.path)
    }

    /// Validates an archive the user picked (already copied into the staging folder).
    pub fn validate_file(&self, name: &str, path: &Path) -> AppResult<RestorePreview> {
        self.validate(name.to_owned(), path)
    }

    fn validate(&self, file_name: String, archive: &Path) -> AppResult<RestorePreview> {
        let (manifest, offset) = read_manifest(archive)?;
        let latest = migrations::latest_version(MIGRATIONS);
        if manifest.schema_version > latest {
            return Err(AppError::SchemaTooNew {
                found: manifest.schema_version,
                supported: latest,
            });
        }
        if manifest.media.files > 0 {
            return Err(AppError::UnsupportedFormat { format: "media" });
        }
        let staging = self
            .staging_dir()?
            .join(format!("restore-{}.sqlite", self.rng.guid()));
        let temp = TempFiles(vec![staging.clone()]);
        extract_database(archive, offset, &manifest, &staging)?;
        let (schema_version, counts) = inspect_database(&staging)?;
        if schema_version != manifest.schema_version {
            return Err(corrupt("schema"));
        }
        let (current, fingerprint) = self
            .db
            .read(|tx| Ok((tx.collection_counts()?, tx.collection_fingerprint()?)))?;
        let token = self.rng.guid().as_str().to_owned();
        temp.keep();
        let previous = self
            .plans
            .lock()
            .expect("plans")
            .restore
            .replace(RestorePlan {
                token: token.clone(),
                staging,
                manifest: manifest.clone(),
                fingerprint,
            });
        if let Some(previous) = previous {
            let _ = fs::remove_file(previous.staging);
        }
        Ok(RestorePreview {
            file_name,
            created_at: TimestampMs(manifest.created_at),
            schema_version,
            counts,
            current,
            token,
        })
    }

    fn replayed(&self, operation_id: &str) -> AppResult<bool> {
        let operation_id = operation_id.to_owned();
        Ok(self
            .db
            .read(move |tx| tx.recorded_operation(&operation_id))?
            .is_some())
    }

    fn others_running(&self, own: Option<&JobId>) -> bool {
        self.jobs.running().iter().any(|id| Some(id) != own)
    }

    /// Starts the restore of the validated archive. The collection is untouched until the swap.
    pub fn restore(&self, operation_id: String, token: &str) -> AppResult<JobRecord> {
        let job = NewJob {
            kind: JobKind::Restore,
            operation_id: Some(operation_id.clone()),
            total: Some(5),
        };
        if self.replayed(&operation_id)? {
            return self.jobs.start(job, Box::new(|_| Ok(None)));
        }
        let plan = {
            let mut plans = self.plans.lock().expect("plans");
            match plans.restore.take() {
                Some(plan) if plan.token == token => plan,
                other => {
                    plans.restore = other;
                    return Err(AppError::PreviewStale);
                }
            }
        };
        let temp = TempFiles(vec![plan.staging.clone()]);
        if self.db.read(|tx| tx.collection_fingerprint())? != plan.fingerprint {
            return Err(AppError::PreviewStale);
        }
        if self.others_running(None) {
            return Err(AppError::DbBusy);
        }
        let service = self.clone();
        temp.keep();
        self.jobs.start(
            job,
            Box::new(move |ctx| {
                let _temp = TempFiles(vec![plan.staging.clone()]);
                ctx.progress(1, 5)?;
                // The staged copy is ours, but check it again: it is what replaces everything.
                let (bytes, sha256) = hash_file(&plan.staging)?;
                if bytes != plan.manifest.database.bytes || sha256 != plan.manifest.database.sha256
                {
                    return Err(corrupt("checksum"));
                }
                let safety =
                    service.write_backup(BackupKind::Safety, &|_| ctx.check_cancelled())?;
                ctx.progress(2, 5)?;
                let now = service.clock.now();
                {
                    let mut conn = Connection::open(&plan.staging).map_err(map_sqlite)?;
                    migrations::migrate(&mut conn, None, MIGRATIONS, now.0)?;
                }
                ctx.progress(3, 5)?;
                ctx.begin_commit()?;
                if service.others_running(Some(ctx.id())) {
                    return Err(AppError::DbBusy);
                }
                let record = service.jobs.get(ctx.id())?;
                {
                    let conn = Connection::open(&plan.staging).map_err(map_sqlite)?;
                    stamp_job(&conn, &record, now)?;
                }
                service.db.replace_with(&plan.staging)?;
                ctx.progress(4, 5)?;
                service
                    .events
                    .collection_changed(&ALL_SCOPES, service.clock.now());
                ctx.progress(5, 5)?;
                tracing::info!(
                    schema_version = plan.manifest.schema_version,
                    "collection restored"
                );
                Ok(Some(
                    serde_json::json!({ "type": "restore", "safetyBackupId": safety.id })
                        .to_string(),
                ))
            }),
        )
    }

    // ------------------------------------------------------------ reset

    pub fn preview_reset(&self, scope: ResetScope) -> AppResult<ResetPreview> {
        let doc = self.settings()?;
        let (counts, fingerprint) = self
            .db
            .read(|tx| Ok((tx.collection_counts()?, tx.collection_fingerprint()?)))?;
        let backups = self.list()?;
        let phrase =
            settings::confirmation_phrase(doc.ui_language, scope == ResetScope::Everything);
        let token = self.rng.guid().as_str().to_owned();
        self.plans.lock().expect("plans").reset = Some(ResetPlan {
            token: token.clone(),
            scope,
            phrase,
            fingerprint,
            backups_before: backups.iter().map(|backup| backup.id.clone()).collect(),
        });
        Ok(ResetPreview {
            scope,
            counts,
            phrase,
            latest_backup_at: backups
                .first()
                .map(|backup| TimestampMs(backup.manifest.created_at)),
            token,
        })
    }

    /// Checks that a backup was made after the preview and that its archive verifies in full.
    fn verified_fresh(&self, id: &str, before: &HashSet<String>) -> AppResult<()> {
        let unverified = || {
            AppError::from(langualinker_domain::ValidationError::new(
                "safetyBackupId",
                "unverified",
            ))
        };
        if before.contains(id) {
            return Err(unverified());
        }
        let backup = self.find(id).map_err(|_| unverified())?;
        let (manifest, offset) = read_manifest(&backup.path).map_err(|_| unverified())?;
        let length = fs::metadata(&backup.path)
            .map_err(|error| io_error(&error, "backupRead"))?
            .len();
        if length.checked_sub(offset) != Some(manifest.database.bytes) {
            return Err(unverified());
        }
        let mut file = File::open(&backup.path).map_err(|error| io_error(&error, "backupRead"))?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|error| io_error(&error, "backupRead"))?;
        let mut hasher = Sha256::new();
        io::copy(&mut file, &mut hasher).map_err(|error| io_error(&error, "backupRead"))?;
        if hex(&hasher.finalize()) != manifest.database.sha256 {
            return Err(unverified());
        }
        Ok(())
    }

    /// Starts a reset confirmed by a fresh verified backup or the typed phrase.
    pub fn reset(
        &self,
        operation_id: String,
        token: &str,
        safety_backup_id: Option<String>,
        phrase: Option<String>,
    ) -> AppResult<JobRecord> {
        let job = NewJob {
            kind: JobKind::Reset,
            operation_id: Some(operation_id.clone()),
            total: Some(3),
        };
        if self.replayed(&operation_id)? {
            return self.jobs.start(job, Box::new(|_| Ok(None)));
        }
        // The preview stays usable after a wrong phrase or an unverified backup; it is spent
        // only when the job starts.
        let (scope, expected_phrase, fingerprint, before) = {
            let plans = self.plans.lock().expect("plans");
            match &plans.reset {
                Some(plan) if plan.token == token => (
                    plan.scope,
                    plan.phrase,
                    plan.fingerprint.clone(),
                    plan.backups_before.clone(),
                ),
                _ => return Err(AppError::PreviewStale),
            }
        };
        if self.db.read(|tx| tx.collection_fingerprint())? != fingerprint {
            return Err(AppError::PreviewStale);
        }
        let safety = match safety_backup_id {
            Some(id) => {
                self.verified_fresh(&id, &before)?;
                Some(id)
            }
            None => {
                if phrase.as_deref().map(str::trim) != Some(expected_phrase) {
                    return Err(langualinker_domain::ValidationError::new(
                        "confirmation",
                        "mismatch",
                    )
                    .into());
                }
                None
            }
        };
        if self.others_running(None) {
            return Err(AppError::DbBusy);
        }
        {
            let mut plans = self.plans.lock().expect("plans");
            match &plans.reset {
                Some(plan) if plan.token == token => plans.reset = None,
                _ => return Err(AppError::PreviewStale),
            }
        }
        let service = self.clone();
        self.jobs.start(
            job,
            Box::new(move |ctx| {
                ctx.progress(1, 3)?;
                ctx.begin_commit()?;
                if service.others_running(Some(ctx.id())) {
                    return Err(AppError::DbBusy);
                }
                let now = service.clock.now();
                match scope {
                    ResetScope::Progress => {
                        let cards = service.db.write(move |tx| tx.reset_progress(now))?;
                        tracing::info!(cards, "progress reset");
                    }
                    ResetScope::Everything => service.delete_everything(ctx.id(), now)?,
                }
                ctx.progress(2, 3)?;
                service
                    .events
                    .collection_changed(&ALL_SCOPES, service.clock.now());
                ctx.progress(3, 3)?;
                Ok(Some(
                    serde_json::json!({ "type": "reset", "safetyBackupId": safety }).to_string(),
                ))
            }),
        )
    }

    /// Swaps in an empty collection and removes what lives beside it (media, staged files, the AI
    /// key). Backup files stay: they are the way back, and the user deletes them separately.
    fn delete_everything(&self, job: &JobId, now: TimestampMs) -> AppResult<()> {
        let staging = self
            .staging_dir()?
            .join(format!("empty-{}.sqlite", self.rng.guid()));
        let _temp = TempFiles(vec![staging.clone()]);
        let (doc, (zone, cutoff)) = self
            .db
            .read(|tx| Ok((settings::load_doc(tx)?, tx.collection_calendar()?)))?;
        let record = self.jobs.get(job)?;
        {
            let mut conn = Connection::open(&staging).map_err(map_sqlite)?;
            migrations::migrate(&mut conn, None, MIGRATIONS, now.0)?;
            conn.execute(
                "INSERT OR REPLACE INTO collection (id, created_at, time_zone, day_cutoff_minutes)
                 VALUES (1, ?1, ?2, ?3)",
                params![now.0, zone, cutoff],
            )
            .map_err(map_sqlite)?;
            // The UI keeps its language; the version moves on so an open form gets a conflict.
            let kept = SettingsDoc {
                ui_language: doc.ui_language,
                theme: doc.theme,
                version: doc.version + 1,
                ..SettingsDoc::default()
            };
            let json = serde_json::to_string(&kept)
                .map_err(|_| AppError::Internal("settings document".into()))?;
            conn.execute(
                "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)",
                params![settings::SETTINGS_KEY, json, now.0],
            )
            .map_err(map_sqlite)?;
            stamp_job(&conn, &record, now)?;
        }
        self.db.replace_with(&staging)?;
        if self.dirs.media.exists() {
            fs::remove_dir_all(&self.dirs.media).map_err(|error| io_error(&error, "media"))?;
        }
        if let Some(secrets) = &self.secrets
            && let Err(error) = secrets.remove(settings::AI_KEY_SECRET)
        {
            tracing::warn!(
                code = error.code(),
                "AI key not removed from the secret store"
            );
        }
        // Validated restores and picked files are copies of collection data too.
        self.plans.lock().expect("plans").restore = None;
        if let Ok(entries) = fs::read_dir(&self.dirs.staging) {
            for entry in entries.flatten() {
                if entry.path() != staging {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
        tracing::info!("collection deleted");
        Ok(())
    }
}

fn write_archive(target: &Path, manifest: &Manifest, database: &Path) -> AppResult<()> {
    let mut out = File::create(target).map_err(|error| io_error(&error, "backupFolder"))?;
    let mut line =
        serde_json::to_vec(manifest).map_err(|_| AppError::Internal("backup manifest".into()))?;
    line.push(b'\n');
    out.write_all(&line)
        .map_err(|error| io_error(&error, "backupFolder"))?;
    let mut source = File::open(database).map_err(|error| io_error(&error, "staging"))?;
    io::copy(&mut source, &mut out).map_err(|error| io_error(&error, "backupFolder"))?;
    out.sync_all()
        .map_err(|error| io_error(&error, "backupFolder"))?;
    Ok(())
}

/// Backups in one folder; a missing folder has none.
fn scan(dir: &Path, location: BackupLocation) -> AppResult<Vec<BackupInfo>> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(io_error(&error, "backupFolder")),
    };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some(BACKUP_EXTENSION) {
            continue;
        }
        let Some(id) = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(str::to_owned)
        else {
            continue;
        };
        if !valid_id(&id) {
            continue;
        }
        match read_manifest(&path) {
            Ok((manifest, _)) => {
                let size_bytes = entry.metadata().map_or(0, |meta| meta.len());
                found.push(BackupInfo {
                    id,
                    path,
                    location,
                    manifest,
                    size_bytes,
                });
            }
            Err(error) => tracing::warn!(code = error.code(), "unreadable backup skipped"),
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_our_file_stems_only() {
        assert!(valid_id("20261110-081500-123-manual"));
        assert!(valid_id("20261110-081500-123-safety-2"));
        for bad in ["", "../x", "a.b", "Backup", "x/y", "manual"] {
            assert!(!valid_id(bad), "{bad}");
        }
        assert_eq!(
            stem(TimestampMs(1_772_438_400_123), BackupKind::Manual),
            "20260302-080000-123-manual"
        );
    }

    #[test]
    fn foreign_files_are_unsupported_or_corrupt() {
        let dir = tempfile::tempdir().unwrap();
        let sqlite = dir.path().join("a.llbackup");
        fs::write(&sqlite, b"SQLite format 3\0rest").unwrap();
        assert_eq!(
            read_manifest(&sqlite).unwrap_err(),
            AppError::UnsupportedFormat { format: "sqlite" }
        );
        let zip = dir.path().join("b.llbackup");
        fs::write(&zip, b"PK\x03\x04rest").unwrap();
        assert_eq!(
            read_manifest(&zip).unwrap_err(),
            AppError::UnsupportedFormat { format: "zip" }
        );
        let text = dir.path().join("c.llbackup");
        fs::write(&text, b"hello").unwrap();
        assert_eq!(read_manifest(&text).unwrap_err(), corrupt("format"));
        let newer = dir.path().join("d.llbackup");
        fs::write(
            &newer,
            format!("{{\"format\":\"{FORMAT}\",\"formatVersion\":9}}\n"),
        )
        .unwrap();
        assert_eq!(
            read_manifest(&newer).unwrap_err(),
            AppError::UnsupportedFormat {
                format: "newerBackup"
            }
        );
        let truncated = dir.path().join("e.llbackup");
        fs::write(&truncated, format!("{{\"format\":\"{FORMAT}\"")).unwrap();
        assert_eq!(read_manifest(&truncated).unwrap_err(), corrupt("manifest"));
    }
}
