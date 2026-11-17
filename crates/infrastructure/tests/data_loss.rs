//! Data-loss drill (plan Phase 16 step 20 and DoD "four-operation data-loss drill / fault
//! injection without loss of committed ratings").
//!
//! Each drill runs the operation in a **child process** (this test binary started again with
//! `LL_DRILL_ROLE`) and ends it the hard way: the parent kills it (`TerminateProcess` /
//! `SIGKILL`) at several moments, or the child aborts itself at an exact point inside a
//! transaction. Nothing is flushed or closed. The parent then reopens the collection file and
//! checks: SQLite integrity, every rating the child reported as committed is still there, card
//! `reps` match the valid log, and the interrupted operation left all-or-nothing.
//!
//! - session: ratings in a loop (15 ms apart), killed after 1–8 reported commits;
//! - import: a CSV import job, killed at 0–1.5 s;
//! - migration: the v4 migration aborts after its statements, before COMMIT;
//! - restore: validate + restore of a larger backup, killed at 0–3 s;
//! - disk full: an import into a capped database fails with `STORAGE_FULL` and changes nothing.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use langualinker_application::jobs::{JobEvent, JobRecord, JobService, JobStatus};
use langualinker_application::ports::{Clock, CollectionScope, EventSink, Rng};
use langualinker_application::repositories::UnitOfWork;
use langualinker_application::study::{self, RateCard, StartStudy};
use langualinker_application::transfer::{CsvOptions, CsvOutcome, CsvTarget, DuplicatePolicy};
use langualinker_application::{AppError, AppResult};
use langualinker_domain::card::NewCard;
use langualinker_domain::deck::{DeckType, NativeLanguage, NewDeck, TargetLanguage};
use langualinker_domain::note::{NewNote, NoteFields, NoteTypeKind};
use langualinker_domain::review::Rating;
use langualinker_domain::{DeckConfigId, DeckId, Guid, TimestampMs};
use langualinker_infrastructure::backup::{
    BackupDirs, BackupKind, BackupService, DatabasePart, FORMAT, FORMAT_VERSION, Manifest,
    ManifestCounts, MediaPart,
};
use langualinker_infrastructure::sqlite::seed::{SeedSize, seed_collection};
use langualinker_infrastructure::sqlite::{
    DATABASE_FILE, MIGRATIONS, Migration, SqliteDatabase, migrations,
};
use langualinker_infrastructure::transfer::{ExportOutcome, TransferEncoders, TransferService};
use langualinker_interop::csv::Delimiter;
use rusqlite::Connection;
use sha2::{Digest, Sha256};

/// 2026-03-02 12:00 UTC.
const NOON: i64 = 1_772_452_800_000;
const ROLE: &str = "LL_DRILL_ROLE";
const DIR: &str = "LL_DRILL_DIR";
const NOTES: u64 = 30;

struct TestClock(AtomicI64);

impl Clock for TestClock {
    fn now(&self) -> TimestampMs {
        TimestampMs(self.0.load(Ordering::SeqCst))
    }
}

struct Counter(AtomicI64);

impl Rng for Counter {
    fn fill(&self, bytes: &mut [u8]) {
        let next = self.0.fetch_add(1, Ordering::SeqCst).to_le_bytes();
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = next[index % 8] ^ (index as u8).wrapping_mul(31);
        }
    }
}

struct Silent;

impl EventSink for Silent {
    fn job_progress(&self, _: &JobEvent) {}
    fn collection_changed(&self, _: &[CollectionScope], _: TimestampMs) {}
}

fn guid(seed: u64) -> Guid {
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&seed.to_le_bytes());
    bytes[8] = 0xd1;
    Guid::from_bytes(bytes)
}

fn clock() -> Arc<TestClock> {
    Arc::new(TestClock(AtomicI64::new(NOON)))
}

fn open(path: &Path) -> Arc<SqliteDatabase> {
    Arc::new(
        SqliteDatabase::open(path, clock().as_ref())
            .unwrap()
            .database,
    )
}

/// A deck of `NOTES` reversed notes.
fn fill(db: &SqliteDatabase) -> DeckId {
    db.write(|tx| {
        let deck = tx
            .insert_deck(
                &NewDeck {
                    guid: guid(1),
                    name: "Drill".into(),
                    parent_id: None,
                    description: String::new(),
                    native_language: NativeLanguage::Pl,
                    target_language: TargetLanguage::De,
                    deck_type: DeckType::Linguistic,
                    config_id: DeckConfigId(1),
                    content_version: None,
                    bundled: false,
                },
                TimestampMs(0),
            )?
            .id;
        let note_type = tx.note_type(NoteTypeKind::BasicAndReversed)?;
        for seed in 0..NOTES {
            let note = tx.insert_note(
                &NewNote {
                    guid: guid(100 + seed),
                    note_type_id: note_type.id,
                    fields: NoteFields {
                        front: format!("Wort {seed}"),
                        back: format!("słowo {seed}"),
                    },
                    tags: Vec::new(),
                },
                TimestampMs(0),
            )?;
            for template in tx.templates(note_type.id)? {
                let position = tx.next_position()?;
                tx.insert_card(
                    &NewCard {
                        guid: guid(10_000 + seed * 10 + u64::from(template.ordinal)),
                        note_id: note.id,
                        deck_id: deck,
                        template_ordinal: template.ordinal,
                        position,
                    },
                    TimestampMs(0),
                )?;
            }
        }
        Ok(deck)
    })
    .unwrap()
}

/// Rates the current card of a session on `deck`; `on_commit` sees the valid-log count after
/// each committed rating. Stops after `limit` ratings or when the queue is empty.
fn rate_loop(
    db: &SqliteDatabase,
    deck: DeckId,
    prefix: &str,
    limit: usize,
    mut on_commit: impl FnMut(i64),
) -> usize {
    let input = StartStudy {
        operation_id: format!("{prefix}-start"),
        deck_id: Some(deck),
        card_ids: None,
    };
    let mut view = db
        .write(move |tx| study::start_study(tx, &input, TimestampMs(NOON)))
        .unwrap();
    let session = view.session.id;
    let mut done = 0;
    while done < limit {
        let Some(current) = view.current.clone() else {
            break;
        };
        let at = TimestampMs(NOON + done as i64 * 1_000);
        let preview = db
            .write(move |tx| study::preview(tx, session, current.id, current.version, at))
            .unwrap();
        let rate = RateCard {
            operation_id: format!("{prefix}-rate-{done}"),
            session_id: session,
            card_id: preview.card_id,
            card_version: preview.card_version,
            preview_token: preview.token,
            rating: if done % 4 == 0 {
                Rating::Again
            } else {
                Rating::Good
            },
            answer_ms: 2_000,
        };
        view = db
            .write(move |tx| study::rate_card(tx, &rate, at))
            .unwrap()
            .view;
        done += 1;
        on_commit(valid_logs(db));
    }
    done
}

fn valid_logs(db: &SqliteDatabase) -> i64 {
    db.with_connection(|conn| {
        conn.query_row(
            "SELECT COUNT(*) FROM review_logs WHERE invalidated_at IS NULL",
            [],
            |row| row.get(0),
        )
        .map_err(|error| AppError::Internal(error.to_string()))
    })
    .unwrap()
}

fn drill_deck(db: &SqliteDatabase) -> DeckId {
    db.with_connection(|conn| {
        conn.query_row("SELECT id FROM decks WHERE name = 'Drill'", [], |row| {
            row.get(0)
        })
        .map(DeckId)
        .map_err(|error| AppError::Internal(error.to_string()))
    })
    .unwrap()
}

/// What a recovered collection file looks like, read with a plain connection.
#[derive(Debug, Clone, PartialEq, Eq)]
struct State {
    notes: i64,
    valid_logs: i64,
    /// Valid review-log ids, to show committed ratings are the same rows.
    log_ids: Vec<i64>,
    /// Cards whose `reps` disagree with their valid log (a half-applied rating). Zero for every
    /// collection built through the use cases; the seeded restore source is synthetic.
    reps_mismatch: i64,
}

fn inspect(path: &Path) -> State {
    let conn = Connection::open(path).unwrap();
    let integrity: String = conn
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .unwrap();
    assert_eq!(integrity, "ok", "{}", path.display());
    let notes = conn
        .query_row("SELECT COUNT(*) FROM notes", [], |row| row.get(0))
        .unwrap();
    let mut statement = conn
        .prepare("SELECT id FROM review_logs WHERE invalidated_at IS NULL ORDER BY id")
        .unwrap();
    let log_ids: Vec<i64> = statement
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    // Every card's repetition count should match its valid log: no half-applied rating.
    let mismatched: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM cards c WHERE c.reps <> (SELECT COUNT(*) FROM review_logs l
                 WHERE l.card_id = c.id AND l.invalidated_at IS NULL)",
            [],
            |row| row.get(0),
        )
        .unwrap();
    State {
        notes,
        valid_logs: log_ids.len() as i64,
        log_ids,
        reps_mismatch: mismatched,
    }
}

// ---------------------------------------------------------------- child process

/// Starts this test binary again running only [`drill_child`] with the given role.
fn spawn(role: &str, dir: &Path) -> (Child, mpsc::Receiver<String>) {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "drill_child",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(ROLE, role)
        .env(DIR, dir)
        .stdout(Stdio::piped())
        .stderr(if std::env::var_os("LL_DRILL_VERBOSE").is_some() {
            Stdio::inherit()
        } else {
            Stdio::null()
        })
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let (send, receive) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let _ = send.send(line);
        }
    });
    (child, receive)
}

/// Waits for a `drill:<prefix>…` message from the child; `None` once it exited. Other output
/// (the test harness, a panic) is printed when the child exits without the message.
fn expect(receive: &mpsc::Receiver<String>, prefix: &str) -> Option<String> {
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut other = Vec::new();
    while Instant::now() < deadline {
        match receive.recv_timeout(Duration::from_millis(50)) {
            // The harness may print `test drill_child ... ` on the same line.
            Ok(line) => match line.find("drill:").map(|at| &line[at + "drill:".len()..]) {
                Some(message) if message.starts_with(prefix) => return Some(message.to_owned()),
                _ => other.push(line),
            },
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                println!("child exited before {prefix}: {other:#?}");
                return None;
            }
        }
    }
    panic!("child never reported {prefix}");
}

fn kill(mut child: Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn say(message: impl std::fmt::Display) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "drill:{message}");
    let _ = out.flush();
}

/// The child side; does nothing unless started by a drill.
#[test]
#[ignore = "started as a child process by the drills"]
fn drill_child() {
    let (Ok(role), Ok(dir)) = (std::env::var(ROLE), std::env::var(DIR)) else {
        return;
    };
    let dir = PathBuf::from(dir);
    match role.as_str() {
        "session" => {
            let db = open(&dir.join(DATABASE_FILE));
            say("ready");
            let deck = drill_deck(&db);
            // A short pause per rating, like a learner, so kills land mid-session.
            let rated = rate_loop(&db, deck, "child", usize::MAX, |logs| {
                say(format!("committed {logs}"));
                std::thread::sleep(Duration::from_millis(15));
            });
            say(format!("finished {rated}"));
        }
        "import" => {
            let (transfer, jobs) = transfer(&dir);
            let preview = transfer
                .preview_csv("drill.csv", csv(import_rows()).as_bytes(), csv_options())
                .unwrap();
            let job = transfer
                .commit_csv("drill-import".into(), &preview.token)
                .unwrap();
            say("started");
            let job = wait(&jobs, &job);
            say(format!("finished {:?}", job.status));
        }
        "migration" => {
            // The last migration aborts the process after its statements, inside the
            // migration's transaction, before COMMIT.
            let mut conn = Connection::open(dir.join(DATABASE_FILE)).unwrap();
            conn.create_scalar_function(
                "drill_abort",
                0,
                rusqlite::functions::FunctionFlags::SQLITE_UTF8,
                |_| -> rusqlite::Result<i64> { std::process::abort() },
            )
            .unwrap();
            let last = MIGRATIONS[MIGRATIONS.len() - 1];
            let sql: &'static str =
                Box::leak(format!("{}\nSELECT drill_abort();", last.sql).into_boxed_str());
            let mut list: Vec<Migration> = MIGRATIONS.to_vec();
            *list.last_mut().unwrap() = Migration { sql, ..last };
            say("migrating");
            let path = dir.join(DATABASE_FILE);
            let _ = migrations::migrate(&mut conn, Some(&path), &list, NOON);
            say("migration returned");
        }
        "restore" => {
            let db = open(&dir.join(DATABASE_FILE));
            let clock: Arc<dyn Clock> = clock();
            let events: Arc<dyn EventSink> = Arc::new(Silent);
            let jobs = JobService::new(db.clone(), events.clone(), clock.clone());
            let backups = BackupService::new(
                db,
                jobs.clone(),
                events,
                clock,
                Arc::new(Counter(AtomicI64::new(7))),
                None,
                BackupDirs::under(&dir),
                "0.0.0-drill",
            );
            say("validating");
            let archive = dir.join("other.llbackup");
            let preview = backups.validate_file("other.llbackup", &archive).unwrap();
            let job = backups
                .restore("drill-restore".into(), &preview.token)
                .unwrap();
            say("started");
            let job = wait(&jobs, &job);
            say(format!("finished {:?}", job.status));
        }
        other => panic!("unknown drill role {other}"),
    }
}

// ---------------------------------------------------------------- import helpers

fn import_rows() -> usize {
    if cfg!(debug_assertions) {
        3_000
    } else {
        20_000
    }
}

fn csv(rows: usize) -> String {
    let mut csv = String::from("front,back,tags\n");
    for index in 0..rows {
        csv.push_str(&format!("Import {index},import {index},drill\n"));
    }
    csv
}

fn csv_options() -> CsvOptions {
    CsvOptions {
        delimiter: Some(Delimiter::Comma),
        encoding: None,
        has_header: true,
        mapping: Some(vec![CsvTarget::Front, CsvTarget::Back, CsvTarget::Tags]),
        duplicate_policy: DuplicatePolicy::Skip,
        target_deck: None,
        formula_guard: false,
    }
}

fn encode_import(outcome: &CsvOutcome) -> String {
    serde_json::json!({ "deckId": outcome.deck_id.0.to_string() }).to_string()
}

fn encode_export(outcome: &ExportOutcome) -> String {
    serde_json::json!({ "fileName": outcome.file_name }).to_string()
}

fn transfer(dir: &Path) -> (TransferService, JobService<SqliteDatabase>) {
    transfer_on(open(&dir.join(DATABASE_FILE)), dir)
}

fn transfer_on(
    db: Arc<SqliteDatabase>,
    dir: &Path,
) -> (TransferService, JobService<SqliteDatabase>) {
    let clock: Arc<dyn Clock> = clock();
    let events: Arc<dyn EventSink> = Arc::new(Silent);
    let jobs = JobService::new(db.clone(), events.clone(), clock.clone());
    let transfer = TransferService::new(
        db,
        jobs.clone(),
        events,
        clock,
        Arc::new(Counter(AtomicI64::new(3))),
        dir.join("exports"),
        TransferEncoders {
            import: encode_import,
            export: encode_export,
        },
    );
    (transfer, jobs)
}

fn wait(jobs: &JobService<SqliteDatabase>, job: &JobRecord) -> JobRecord {
    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        let current = jobs.get(&job.id).unwrap();
        if matches!(
            current.status,
            JobStatus::Succeeded | JobStatus::Failed | JobStatus::Cancelled
        ) {
            return current;
        }
        assert!(Instant::now() < deadline, "job did not finish");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// A collection with the drill deck and `ratings` committed ratings.
fn prepared(dir: &Path, ratings: usize) -> State {
    let db = open(&dir.join(DATABASE_FILE));
    let deck = fill(&db);
    rate_loop(&db, deck, "setup", ratings, |_| {});
    drop(db);
    inspect(&dir.join(DATABASE_FILE))
}

/// After a kill the app opens the file again: migrations check, interrupted jobs fail.
fn reopen_like_the_app(dir: &Path) -> AppResult<usize> {
    let db = open(&dir.join(DATABASE_FILE));
    let jobs = JobService::new(db, Arc::new(Silent), clock());
    jobs.recover_interrupted()
}

// ---------------------------------------------------------------- drills

#[test]
fn killed_study_sessions_keep_every_reported_rating() {
    for (round, kill_after) in [1usize, 2, 3, 5, 8].into_iter().enumerate() {
        let dir = tempfile::tempdir().unwrap();
        let before = prepared(dir.path(), 4);
        let (child, receive) = spawn("session", dir.path());
        expect(&receive, "ready");
        let mut reported = before.valid_logs;
        for _ in 0..kill_after {
            let message = expect(&receive, "committed").expect("child stopped early");
            reported = message["committed ".len()..].parse().unwrap();
        }
        kill(child);
        reopen_like_the_app(dir.path()).unwrap();
        let after = inspect(&dir.path().join(DATABASE_FILE));
        // Every reported commit survived (ratings after the last report may exist as well).
        assert!(
            after.valid_logs >= reported,
            "round {round}: reported {reported}, found {}",
            after.valid_logs
        );
        assert!(after.log_ids.starts_with(&before.log_ids), "round {round}");
        assert_eq!(after.notes, before.notes);
        assert_eq!(after.reps_mismatch, 0, "round {round}");
    }
}

#[test]
fn killed_imports_are_all_or_nothing_and_keep_ratings() {
    let rows = import_rows() as i64;
    let mut outcomes = Vec::new();
    for delay_ms in [0u64, 30, 120, 400, 1_500] {
        let dir = tempfile::tempdir().unwrap();
        let before = prepared(dir.path(), 6);
        let (child, receive) = spawn("import", dir.path());
        expect(&receive, "started");
        std::thread::sleep(Duration::from_millis(delay_ms));
        kill(child);
        reopen_like_the_app(dir.path()).unwrap();
        let after = inspect(&dir.path().join(DATABASE_FILE));
        assert_eq!(after.log_ids, before.log_ids, "kill at {delay_ms} ms");
        assert_eq!(after.reps_mismatch, 0, "kill at {delay_ms} ms");
        let imported = after.notes - before.notes;
        assert!(
            imported == 0 || imported == rows,
            "kill at {delay_ms} ms left {imported} of {rows} notes"
        );
        outcomes.push((delay_ms, imported));
    }
    println!("import kills (delay ms, notes imported): {outcomes:?}");
}

#[test]
fn a_migration_killed_before_commit_leaves_the_previous_version() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(DATABASE_FILE);
    let previous = migrations::latest_version(MIGRATIONS) - 1;
    {
        // A collection at the previous schema version.
        let mut conn = Connection::open(&path).unwrap();
        conn.pragma_update(None, "journal_mode", "WAL").unwrap();
        migrations::migrate(
            &mut conn,
            Some(&path),
            &MIGRATIONS[..previous as usize],
            NOON,
        )
        .unwrap();
    }
    {
        // Rows through the real use cases need the latest schema, so ratings are written
        // with SQL that is valid in every version since v3.
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "INSERT INTO decks (guid, name, native_language, target_language, config_id, created_at, updated_at)
                 VALUES ('d1', 'Drill', 1, 1, 1, 0, 0);
             INSERT INTO notes (id, guid, note_type_id, fields, sort_field, created_at, updated_at)
                 VALUES (7, 'n7', 1, '{\"front\":\"Hund\",\"back\":\"pies\"}', 'hund', 0, 0);
             INSERT INTO cards (id, guid, note_id, deck_id, template_ordinal, reps, created_at, updated_at)
                 SELECT 9, 'c9', 7, id, 0, 2, 0, 0 FROM decks WHERE guid = 'd1';
             INSERT INTO study_sessions (id, status, started_at, created_at, updated_at)
                 VALUES (3, 'finished', 0, 0, 0);
             INSERT INTO review_logs (id, card_id, session_id, operation_id, rating, kind,
                     reviewed_at, duration_ms, card_before, card_after, fsrs_version, parameters_hash)
                 VALUES (5, 9, 3, 'op1', 3, 0, 0, 0, '{}', '{}', 'fsrs-6', 'h'),
                        (6, 9, 3, 'op2', 3, 1, 1, 0, '{}', '{}', 'fsrs-6', 'h');",
        )
        .unwrap();
    }
    let before = inspect(&path);
    assert_eq!(before.valid_logs, 2);

    let (child, receive) = spawn("migration", dir.path());
    expect(&receive, "migrating");
    // The child aborts itself; it never reports a return.
    assert_eq!(expect(&receive, "migration returned"), None);
    kill(child);

    let conn = Connection::open(&path).unwrap();
    assert_eq!(migrations::applied_version(&conn).unwrap(), previous);
    drop(conn);
    assert_eq!(inspect(&path), before);
    // The safety copy was written before the migration began.
    assert!(
        dir.path()
            .join(format!("collection.pre-v{}.sqlite", previous + 1))
            .exists()
    );
    // The next start migrates for real and keeps the ratings.
    let db = open(&path);
    assert_eq!(db.schema_version(), previous + 1);
    drop(db);
    assert_eq!(inspect(&path).log_ids, before.log_ids);
}

/// A backup archive of another collection with `cards` seeded cards.
fn other_archive(dir: &Path, cards: i64) -> i64 {
    let source = dir.join("other.sqlite");
    let db = SqliteDatabase::open(&source, clock().as_ref())
        .unwrap()
        .database;
    db.with_connection(move |conn| {
        seed_collection(
            conn,
            SeedSize {
                decks: 2,
                cards,
                logs_per_card: 3,
            },
        )
    })
    .unwrap();
    let snapshot = dir.join("other-snapshot.sqlite");
    db.snapshot_to(&snapshot).unwrap();
    drop(db);
    let notes: i64 = Connection::open(&snapshot)
        .unwrap()
        .query_row("SELECT COUNT(*) FROM notes", [], |row| row.get(0))
        .unwrap();
    let database = std::fs::read(&snapshot).unwrap();
    let manifest = Manifest {
        format: FORMAT.to_owned(),
        format_version: FORMAT_VERSION,
        created_at: NOON,
        kind: BackupKind::Manual,
        app_version: "0.0.0-drill".to_owned(),
        schema_version: migrations::latest_version(MIGRATIONS),
        counts: ManifestCounts::default(),
        database: DatabasePart {
            bytes: database.len() as u64,
            sha256: Sha256::digest(&database)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        },
        media: MediaPart::default(),
    };
    let mut bytes = serde_json::to_vec(&manifest).unwrap();
    bytes.push(b'\n');
    bytes.extend_from_slice(&database);
    std::fs::write(dir.join("other.llbackup"), bytes).unwrap();
    let _ = std::fs::remove_file(snapshot);
    let _ = std::fs::remove_file(source);
    notes
}

#[test]
fn killed_restores_leave_either_the_old_or_the_new_collection() {
    let cards = if cfg!(debug_assertions) {
        8_000
    } else {
        40_000
    };
    let mut outcomes = Vec::new();
    for delay_ms in [0u64, 100, 400, 1_000, 3_000] {
        let dir = tempfile::tempdir().unwrap();
        let before = prepared(dir.path(), 6);
        let restored_notes = other_archive(dir.path(), cards);
        let (child, receive) = spawn("restore", dir.path());
        expect(&receive, "started");
        std::thread::sleep(Duration::from_millis(delay_ms));
        kill(child);
        reopen_like_the_app(dir.path()).unwrap();
        let after = inspect(&dir.path().join(DATABASE_FILE));
        let outcome = if after.notes == before.notes {
            assert_eq!(after.log_ids, before.log_ids, "kill at {delay_ms} ms");
            assert_eq!(after.reps_mismatch, 0, "kill at {delay_ms} ms");
            "old"
        } else {
            assert_eq!(
                after.notes, restored_notes,
                "kill at {delay_ms} ms: mixed collection"
            );
            "restored"
        };
        outcomes.push((delay_ms, outcome));
    }
    println!("restore kills (delay ms, collection): {outcomes:?}");
}

#[test]
fn a_full_disk_during_an_import_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let before = prepared(dir.path(), 6);
    let db = open(&dir.path().join(DATABASE_FILE));
    let (transfer, jobs) = transfer_on(db.clone(), dir.path());
    let preview = transfer
        .preview_csv("full.csv", csv(import_rows()).as_bytes(), csv_options())
        .unwrap();
    // The collection may grow by a few pages only (the limit is per connection: this one is
    // the worker every service of this install shares).
    db.with_connection(|conn| {
        let pages: i64 = conn
            .query_row("PRAGMA page_count", [], |row| row.get(0))
            .unwrap();
        conn.pragma_update(None, "max_page_count", pages + 4)
            .unwrap();
        Ok(())
    })
    .unwrap();
    let job = transfer
        .commit_csv("full-import".into(), &preview.token)
        .unwrap();
    let job = wait(&jobs, &job);
    assert_eq!(job.status, JobStatus::Failed);
    assert_eq!(
        job.error.as_ref().map(|e| e.code.as_str()),
        Some("STORAGE_FULL")
    );
    drop((transfer, jobs, db));
    let after = inspect(&dir.path().join(DATABASE_FILE));
    assert_eq!(after, before);
}
