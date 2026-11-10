//! Backups, restore and reset (plan Phase 12 steps 8 and 10–16) on real collection files:
//! a backup written outside the app folder restores into a clean install with identical notes,
//! GUIDs, schedules, memory states and history; corrupt archives and failed swaps leave the
//! collection as it was; resets need a fresh verified backup or the typed phrase; deleting
//! everything keeps the backups and removes the stored AI key.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use langualinker_application::jobs::{JobEvent, JobRecord, JobService, JobStatus};
use langualinker_application::ports::{Clock, CollectionScope, EventSink, Rng, SecretStore};
use langualinker_application::repositories::UnitOfWork;
use langualinker_application::settings::{self, SettingsChange};
use langualinker_application::study::{self, RateCard, StartStudy, StudyView};
use langualinker_application::{AppError, AppResult};
use langualinker_domain::deck::{DeckType, NativeLanguage, NewDeck, TargetLanguage};
use langualinker_domain::note::{NewNote, NoteFields, NoteTypeKind};
use langualinker_domain::review::Rating;
use langualinker_domain::{DeckId, Guid, TimestampMs, ValidationError};
use langualinker_infrastructure::backup::{
    BackupDirs, BackupKind, BackupLocation, BackupService, ResetScope, read_manifest,
};
use langualinker_infrastructure::sqlite::{DATABASE_FILE, SqliteDatabase};
use rusqlite::Connection;

/// 2026-03-02 12:00 UTC.
const NOON: i64 = 1_772_452_800_000;
const DAY: i64 = 86_400_000;

struct TestClock(AtomicI64);

impl Clock for TestClock {
    fn now(&self) -> TimestampMs {
        TimestampMs(self.0.load(Ordering::SeqCst))
    }
}

impl TestClock {
    fn advance(&self, ms: i64) {
        self.0.fetch_add(ms, Ordering::SeqCst);
    }
}

struct Counter(AtomicI64);

impl Rng for Counter {
    fn fill(&self, bytes: &mut [u8]) {
        let next = self.0.fetch_add(1, Ordering::SeqCst).to_le_bytes();
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = next[index % 8] ^ (index as u8);
        }
    }
}

#[derive(Default)]
struct Events(Mutex<Vec<Vec<CollectionScope>>>);

impl EventSink for Events {
    fn job_progress(&self, _: &JobEvent) {}
    fn collection_changed(&self, scopes: &[CollectionScope], _: TimestampMs) {
        self.0.lock().unwrap().push(scopes.to_vec());
    }
}

#[derive(Default)]
struct Secrets(Mutex<HashMap<String, String>>);

impl SecretStore for Secrets {
    fn save(&self, name: &str, value: &str) -> AppResult<()> {
        self.0.lock().unwrap().insert(name.into(), value.into());
        Ok(())
    }
    fn remove(&self, name: &str) -> AppResult<bool> {
        Ok(self.0.lock().unwrap().remove(name).is_some())
    }
    fn has(&self, name: &str) -> AppResult<bool> {
        Ok(self.0.lock().unwrap().contains_key(name))
    }
    fn read(&self, name: &str) -> AppResult<Option<String>> {
        Ok(self.0.lock().unwrap().get(name).cloned())
    }
}

/// One installation: a collection file in `dir` and its backup service.
struct Install {
    dir: PathBuf,
    clock: Arc<TestClock>,
    db: Arc<SqliteDatabase>,
    jobs: JobService<SqliteDatabase>,
    backups: BackupService,
    secrets: Arc<Secrets>,
}

fn install(dir: &Path, seed: i64) -> Install {
    let clock = Arc::new(TestClock(AtomicI64::new(NOON)));
    let db = Arc::new(
        SqliteDatabase::open(&dir.join(DATABASE_FILE), clock.as_ref())
            .unwrap()
            .database,
    );
    let events: Arc<dyn EventSink> = Arc::new(Events::default());
    let jobs = JobService::new(db.clone(), events.clone(), clock.clone());
    let secrets = Arc::new(Secrets::default());
    let backups = BackupService::new(
        db.clone(),
        jobs.clone(),
        events,
        clock.clone(),
        Arc::new(Counter(AtomicI64::new(seed))),
        Some(secrets.clone()),
        BackupDirs::under(dir),
        "0.0.0-test",
    );
    Install {
        dir: dir.to_owned(),
        clock,
        db,
        jobs,
        backups,
        secrets,
    }
}

fn guid(seed: u64) -> Guid {
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&seed.to_le_bytes());
    bytes[8] = 0x5c;
    Guid::from_bytes(bytes)
}

/// A deck with `notes` reversed notes (two cards each).
fn content(db: &Arc<SqliteDatabase>, notes: u64) -> DeckId {
    db.write(move |tx| {
        let config = tx.default_config()?.id;
        let deck = tx.insert_deck(
            &NewDeck {
                guid: guid(1),
                name: "Niemiecki".into(),
                parent_id: None,
                description: String::new(),
                native_language: NativeLanguage::Pl,
                target_language: TargetLanguage::De,
                deck_type: DeckType::Linguistic,
                config_id: config,
                content_version: None,
                bundled: false,
            },
            TimestampMs(NOON - DAY),
        )?;
        let note_type = tx.note_type(NoteTypeKind::BasicAndReversed)?;
        for seed in 0..notes {
            let note = tx.insert_note(
                &NewNote {
                    guid: guid(100 + seed),
                    note_type_id: note_type.id,
                    fields: NoteFields {
                        front: format!("das Wort {seed}"),
                        back: format!("słowo {seed}"),
                    },
                    tags: vec!["test".into()],
                },
                TimestampMs(NOON - DAY),
            )?;
            for template in tx.templates(note_type.id)? {
                let position = tx.next_position()?;
                tx.insert_card(
                    &langualinker_domain::card::NewCard {
                        guid: guid(10_000 + seed * 10 + u64::from(template.ordinal)),
                        note_id: note.id,
                        deck_id: deck.id,
                        template_ordinal: template.ordinal,
                        position,
                    },
                    TimestampMs(NOON - DAY),
                )?;
            }
        }
        Ok(deck.id)
    })
    .unwrap()
}

fn start(db: &Arc<SqliteDatabase>, op: &str, at: i64) -> StudyView {
    let input = StartStudy {
        operation_id: op.into(),
        deck_id: None,
        card_ids: None,
    };
    db.write(move |tx| study::start_study(tx, &input, TimestampMs(at)))
        .unwrap()
}

/// Rates `count` cards of a new session at `at`, cycling through the four buttons, then undoes
/// the last rating (an invalidated log must survive the round trip too).
fn study_day(db: &Arc<SqliteDatabase>, day: &str, count: usize, at: i64) {
    let mut view = start(db, &format!("start-{day}"), at);
    let ratings = [Rating::Good, Rating::Again, Rating::Easy, Rating::Hard];
    for index in 0..count {
        let Some(card) = view.current.clone() else {
            break;
        };
        let session = view.session.id;
        let preview = db
            .read(move |tx| study::preview(tx, session, card.id, card.version, TimestampMs(at)))
            .unwrap();
        let input = RateCard {
            operation_id: format!("rate-{day}-{index}"),
            session_id: session,
            card_id: preview.card_id,
            card_version: preview.card_version,
            preview_token: preview.token,
            rating: ratings[index % ratings.len()],
            answer_ms: 2_000 + index as i64 * 100,
        };
        view = db
            .write(move |tx| study::rate_card(tx, &input, TimestampMs(at)))
            .unwrap()
            .view;
    }
    let session = view.session.id;
    let op = format!("undo-{day}");
    db.write(move |tx| study::undo_review(tx, &op, session, TimestampMs(at)))
        .unwrap();
}

type Row = Vec<rusqlite::types::Value>;

/// Rows a restore must bring back unchanged.
fn dump(path: &Path) -> HashMap<&'static str, Vec<Row>> {
    let conn = Connection::open(path).unwrap();
    let queries = [
        ("notes", "SELECT guid, fields, sort_field, deleted_at, version FROM notes ORDER BY guid"),
        (
            "cards",
            "SELECT guid, state, queue, due_at, stability, difficulty, reps, lapses, position,
                    scheduled_days, last_review_at, learning_step, version FROM cards ORDER BY guid",
        ),
        (
            "review_logs",
            "SELECT c.guid, l.operation_id, l.rating, l.kind, l.reviewed_at, l.duration_ms,
                    l.card_before, l.card_after, l.parameters_hash, l.invalidated_at
               FROM review_logs l JOIN cards c ON c.id = l.card_id ORDER BY l.operation_id",
        ),
        ("decks", "SELECT guid, name, config_id, version FROM decks ORDER BY guid"),
        ("tags", "SELECT n.guid, t.name FROM note_tags nt JOIN notes n ON n.id = nt.note_id JOIN tags t ON t.id = nt.tag_id ORDER BY 1, 2"),
    ];
    queries
        .into_iter()
        .map(|(name, sql)| {
            let mut statement = conn.prepare(sql).unwrap();
            let columns = statement.column_count();
            let rows = statement
                .query_map([], |row| {
                    (0..columns)
                        .map(|index| row.get::<_, rusqlite::types::Value>(index))
                        .collect()
                })
                .unwrap()
                .collect::<rusqlite::Result<Vec<Row>>>()
                .unwrap();
            (name, rows)
        })
        .collect()
}

fn wait(jobs: &JobService<SqliteDatabase>, job: &JobRecord) -> JobRecord {
    for _ in 0..1_000 {
        let current = jobs.get(&job.id).unwrap();
        if current.status.is_finished() {
            return current;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("job {} did not finish", job.id);
}

fn result(job: &JobRecord) -> serde_json::Value {
    serde_json::from_str(job.result_json.as_deref().expect("a result")).unwrap()
}

fn use_external_folder(install: &Install, folder: &Path) {
    let path = folder.to_str().unwrap().to_owned();
    install
        .db
        .write(move |tx| {
            let version = settings::load_doc(tx)?.version;
            settings::update(
                tx,
                version,
                SettingsChange {
                    backup_folder: Some(("Pendrive".into(), path)),
                    ..SettingsChange::default()
                },
                TimestampMs(NOON),
            )
        })
        .unwrap();
}

/// Phase 12 DoD: backup outside the sandbox → clean install → restore: notes, GUIDs, due dates,
/// S/D, reps/lapses and logs identical.
#[test]
fn a_backup_outside_the_sandbox_restores_into_a_clean_install_identically() {
    let root = tempfile::tempdir().unwrap();
    let (first_dir, outside, second_dir) = (
        root.path().join("first"),
        root.path().join("outside"),
        root.path().join("second"),
    );
    let first = install(&first_dir, 1);
    content(&first.db, 30);
    study_day(&first.db, "1", 25, NOON);
    first.clock.advance(3 * DAY);
    study_day(&first.db, "2", 25, NOON + 3 * DAY);
    use_external_folder(&first, &outside);

    let job = first
        .backups
        .create(Some("backup-1".into()), BackupKind::Manual)
        .unwrap();
    let job = wait(&first.jobs, &job);
    assert_eq!(job.status, JobStatus::Succeeded, "{job:?}");
    let id = result(&job)["backupId"].as_str().unwrap().to_owned();
    let listed = first.backups.find(&id).unwrap();
    assert_eq!(listed.location, BackupLocation::External);
    assert!(
        listed.path.starts_with(&outside),
        "written to the chosen folder"
    );
    let (manifest, _) = read_manifest(&listed.path).unwrap();
    assert_eq!(manifest.counts.cards, 60);
    assert_eq!(manifest.schema_version, first.db.schema_version());
    let expected = dump(&first_dir.join(DATABASE_FILE));
    assert!(expected["review_logs"].len() >= 40);
    drop(first);

    // A clean install on another device: only the copied archive survives.
    let copied = root.path().join("copied.llbackup");
    std::fs::copy(&listed.path, &copied).unwrap();
    let second = install(&second_dir, 2);
    let preview = second
        .backups
        .validate_file("copied.llbackup", &copied)
        .unwrap();
    assert_eq!(preview.counts.cards, 60);
    assert_eq!(preview.current.cards, 0);
    assert_eq!(preview.counts.reviews, manifest.counts.reviews);
    let job = second
        .backups
        .restore("restore-1".into(), &preview.token)
        .unwrap();
    let job = wait(&second.jobs, &job);
    assert_eq!(job.status, JobStatus::Succeeded, "{job:?}");
    assert_eq!(job.progress.map(|p| (p.done, p.total)), Some((5, 5)));
    let safety = result(&job)["safetyBackupId"].as_str().unwrap().to_owned();
    assert_eq!(
        second.backups.find(&safety).unwrap().manifest.kind,
        BackupKind::Safety
    );
    // A retried request returns the same job instead of restoring twice.
    let again = second
        .backups
        .restore("restore-1".into(), &preview.token)
        .unwrap();
    assert_eq!(again.id, job.id);

    let restored = dump(&second_dir.join(DATABASE_FILE));
    for table in ["notes", "cards", "review_logs", "decks", "tags"] {
        assert_eq!(restored[table], expected[table], "{table}");
    }
    let journal: String = second
        .db
        .with_connection(|conn| {
            conn.query_row("PRAGMA journal_mode", [], |row| row.get(0))
                .map_err(|error| AppError::Internal(error.to_string()))
        })
        .unwrap();
    assert_eq!(journal, "wal", "the restored collection keeps WAL");
    // The restored collection keeps working.
    let view = start(&second.db, "after-restore", NOON + 4 * DAY);
    assert!(view.current.is_some());
}

/// Phase 12 DoD: a corrupt archive, an interrupted write and a failed swap preserve the original.
#[test]
fn corrupt_archives_and_failed_swaps_leave_the_collection_as_it_was() {
    let root = tempfile::tempdir().unwrap();
    let app = install(root.path(), 3);
    content(&app.db, 10);
    study_day(&app.db, "1", 8, NOON);
    let before = dump(&root.path().join(DATABASE_FILE));
    let backup = app
        .backups
        .write_backup(BackupKind::Manual, &|_| Ok(()))
        .unwrap();
    let bytes = std::fs::read(&backup.path).unwrap();

    let flipped = root.path().join("flipped.llbackup");
    let mut damaged = bytes.clone();
    let last = damaged.len() - 100;
    damaged[last] ^= 0xff;
    std::fs::write(&flipped, &damaged).unwrap();
    assert_eq!(
        app.backups.validate_file("flipped", &flipped).unwrap_err(),
        AppError::BackupCorrupt { reason: "checksum" }
    );
    let truncated = root.path().join("truncated.llbackup");
    std::fs::write(&truncated, &bytes[..bytes.len() / 2]).unwrap();
    assert_eq!(
        app.backups
            .validate_file("truncated", &truncated)
            .unwrap_err(),
        AppError::BackupCorrupt { reason: "size" }
    );
    let newer = root.path().join("newer.llbackup");
    let text = String::from_utf8_lossy(&bytes[..bytes.iter().position(|b| *b == b'\n').unwrap()])
        .replace(
            &format!("\"schemaVersion\":{}", backup.manifest.schema_version),
            "\"schemaVersion\":99",
        );
    std::fs::write(&newer, format!("{text}\n")).unwrap();
    assert!(matches!(
        app.backups.validate_file("newer", &newer).unwrap_err(),
        AppError::SchemaTooNew { found: 99, .. }
    ));

    // A write killed before its rename leaves a hidden temporary file, never a listed backup;
    // a half-written archive under a backup name is skipped by the list and refused on restore.
    let dir = backup.path.parent().unwrap();
    std::fs::write(dir.join(".20990101-000000-000-manual.tmp"), &bytes[..10]).unwrap();
    let partial = dir.join("20990101-000000-001-manual.llbackup");
    std::fs::write(&partial, &bytes[..bytes.len() - 1]).unwrap();
    let ids: Vec<String> = app
        .backups
        .list()
        .unwrap()
        .into_iter()
        .map(|b| b.id)
        .collect();
    assert!(ids.contains(&backup.id));
    assert_eq!(
        app.backups
            .validate_backup("20990101-000000-001-manual")
            .unwrap_err(),
        AppError::BackupCorrupt { reason: "size" }
    );

    // A swap that fails part way (here: the source is not a database) rolls back.
    let garbage = root.path().join("garbage.sqlite");
    std::fs::write(&garbage, vec![7u8; 8192]).unwrap();
    assert!(app.db.replace_with(&garbage).is_err());
    // A stale preview is refused, and the collection is unchanged by all of the above.
    let preview = app.backups.validate_backup(&backup.id).unwrap();
    study_day(&app.db, "2", 1, NOON + 60_000);
    assert_eq!(
        app.backups
            .restore("stale".into(), &preview.token)
            .unwrap_err(),
        AppError::PreviewStale
    );
    let after = dump(&root.path().join(DATABASE_FILE));
    assert_eq!(after["notes"], before["notes"]);
    assert_eq!(after["decks"], before["decks"]);
    assert!(
        after["review_logs"].len() > before["review_logs"].len(),
        "only the new study day"
    );
}

/// Phase 12 steps 12 and 16: retention keeps the newest regular backups, safety copies apart;
/// automatic backups follow studying and the daily interval.
#[test]
fn rotation_keeps_the_newest_backups_and_automatic_backups_follow_studying() {
    let root = tempfile::tempdir().unwrap();
    let app = install(root.path(), 4);
    content(&app.db, 3);
    assert!(!app.backups.automatic_due().unwrap(), "nothing studied yet");
    study_day(&app.db, "1", 2, NOON);
    assert!(app.backups.automatic_due().unwrap());
    let job = app.backups.start_automatic().unwrap().expect("started");
    assert_eq!(wait(&app.jobs, &job).status, JobStatus::Succeeded);
    assert!(!app.backups.automatic_due().unwrap(), "made today");
    app.clock.advance(DAY + 1);
    assert!(
        !app.backups.automatic_due().unwrap(),
        "no study since the last one"
    );
    study_day(&app.db, "2", 2, NOON + DAY + 2);
    assert!(app.backups.automatic_due().unwrap());

    for _ in 0..4 {
        app.clock.advance(1_000);
        app.backups
            .write_backup(BackupKind::Manual, &|_| Ok(()))
            .unwrap();
    }
    app.backups
        .write_backup(BackupKind::Safety, &|_| Ok(()))
        .unwrap();
    let list = app.backups.list().unwrap();
    let regular = list
        .iter()
        .filter(|b| b.manifest.kind != BackupKind::Safety)
        .count();
    assert_eq!(regular, settings::DEFAULT_BACKUP_RETENTION as usize);
    assert_eq!(
        list.iter()
            .filter(|b| b.manifest.kind == BackupKind::Safety)
            .count(),
        1
    );
    assert!(
        list.windows(2)
            .all(|pair| pair[0].manifest.created_at >= pair[1].manifest.created_at),
        "newest first"
    );
    assert!(
        app.dir.join("staging").read_dir().unwrap().next().is_none(),
        "no leftovers"
    );
}

fn phrase_error() -> AppError {
    ValidationError::new("confirmation", "mismatch").into()
}

/// Phase 12 step 15: progress is reset only after a fresh verified backup or the phrase; notes
/// stay, the history goes.
#[test]
fn resetting_progress_needs_a_fresh_verified_backup_or_the_phrase() {
    let root = tempfile::tempdir().unwrap();
    let app = install(root.path(), 5);
    content(&app.db, 6);
    study_day(&app.db, "1", 6, NOON);
    let old = app
        .backups
        .write_backup(BackupKind::Manual, &|_| Ok(()))
        .unwrap();

    let preview = app.backups.preview_reset(ResetScope::Progress).unwrap();
    assert_eq!(preview.phrase, "RESETUJ POSTĘP");
    assert_eq!(preview.counts.cards, 12);
    assert_eq!(
        preview.latest_backup_at.map(|at| at.0),
        Some(old.manifest.created_at)
    );
    let reset = |op: &str, backup: Option<String>, phrase: Option<&str>| {
        app.backups
            .reset(op.into(), &preview.token, backup, phrase.map(str::to_owned))
    };
    assert_eq!(
        reset("r1", None, Some("reset")).unwrap_err(),
        phrase_error()
    );
    assert_eq!(
        reset("r2", Some(old.id.clone()), None).unwrap_err(),
        ValidationError::new("safetyBackupId", "unverified").into(),
        "a backup older than the preview is not a safety copy"
    );
    // A fresh backup whose archive was damaged after writing does not verify either.
    app.clock.advance(1_000);
    let damaged = app
        .backups
        .write_backup(BackupKind::Manual, &|_| Ok(()))
        .unwrap();
    let mut bytes = std::fs::read(&damaged.path).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    std::fs::write(&damaged.path, bytes).unwrap();
    assert_eq!(
        reset("r3", Some(damaged.id.clone()), None).unwrap_err(),
        ValidationError::new("safetyBackupId", "unverified").into()
    );
    app.clock.advance(1_000);
    let fresh = app
        .backups
        .write_backup(BackupKind::Manual, &|_| Ok(()))
        .unwrap();
    let job = reset("r4", Some(fresh.id.clone()), None).unwrap();
    let job = wait(&app.jobs, &job);
    assert_eq!(job.status, JobStatus::Succeeded, "{job:?}");
    assert_eq!(result(&job)["safetyBackupId"], fresh.id.as_str());

    let (counts, studied, logs) = app
        .db
        .read(|tx| {
            let counts = tx.collection_counts()?;
            Ok((counts, tx.newest_review_at()?, tx.open_session()?))
        })
        .unwrap();
    assert_eq!((counts.notes, counts.cards, counts.reviews), (6, 12, 0));
    assert_eq!(studied, None);
    assert!(logs.is_none(), "sessions are gone");
    let conn = Connection::open(root.path().join(DATABASE_FILE)).unwrap();
    let scheduled: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM cards WHERE state <> 0 OR due_at IS NOT NULL OR stability IS NOT NULL OR reps > 0",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(scheduled, 0);
    let positions: Vec<i64> = conn
        .prepare("SELECT position FROM cards ORDER BY position")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(positions, (1..=12).collect::<Vec<_>>());
    // The token is spent.
    assert_eq!(
        reset("r5", None, Some("RESETUJ POSTĘP")).unwrap_err(),
        AppError::PreviewStale
    );
}

/// Phase 12 step 8: delete everything empties the collection, removes media and the AI key,
/// keeps the backups (the way back) and the UI language.
#[test]
fn deleting_everything_keeps_only_backups_and_the_ui_language() {
    let root = tempfile::tempdir().unwrap();
    let app = install(root.path(), 6);
    content(&app.db, 4);
    study_day(&app.db, "1", 4, NOON);
    app.db
        .write(|tx| {
            let version = settings::load_doc(tx)?.version;
            settings::update(
                tx,
                version,
                SettingsChange {
                    ui_language: Some(settings::UiLanguage::En),
                    ..SettingsChange::default()
                },
                TimestampMs(NOON),
            )
        })
        .unwrap();
    app.secrets
        .save(settings::AI_KEY_SECRET, "sk-test-value")
        .unwrap();
    std::fs::create_dir_all(root.path().join("media")).unwrap();
    std::fs::write(root.path().join("media").join("a.mp3"), b"x").unwrap();
    let backup = app
        .backups
        .write_backup(BackupKind::Manual, &|_| Ok(()))
        .unwrap();

    let preview = app.backups.preview_reset(ResetScope::Everything).unwrap();
    assert_eq!(preview.phrase, "DELETE EVERYTHING");
    let job = app
        .backups
        .reset(
            "wipe".into(),
            &preview.token,
            None,
            Some(" DELETE EVERYTHING ".into()),
        )
        .unwrap();
    let job = wait(&app.jobs, &job);
    assert_eq!(job.status, JobStatus::Succeeded, "{job:?}");
    assert_eq!(result(&job)["safetyBackupId"], serde_json::Value::Null);

    let (counts, doc, onboarding) = app
        .db
        .read(|tx| {
            Ok((
                tx.collection_counts()?,
                settings::load_doc(tx)?,
                tx.setting("onboarding")?,
            ))
        })
        .unwrap();
    assert_eq!(counts, Default::default());
    assert_eq!(doc.ui_language, settings::UiLanguage::En);
    assert_eq!(onboarding, None, "onboarding starts again");
    assert!(!app.secrets.has(settings::AI_KEY_SECRET).unwrap());
    assert!(!root.path().join("media").exists());
    assert!(app.backups.find(&backup.id).is_ok());
    // The job that did it is still readable after the swap.
    assert_eq!(app.jobs.get(&job.id).unwrap().status, JobStatus::Succeeded);
    // And the backup brings everything back.
    let restore = app.backups.validate_backup(&backup.id).unwrap();
    assert_eq!(restore.counts.cards, 8);
    let job = wait(
        &app.jobs,
        &app.backups.restore("back".into(), &restore.token).unwrap(),
    );
    assert_eq!(job.status, JobStatus::Succeeded, "{job:?}");
    assert_eq!(app.db.read(|tx| tx.collection_counts()).unwrap().cards, 8);
}

/// The archive carries no secret: the AI key lives in the platform store only.
#[test]
fn archives_never_contain_the_ai_key() {
    let root = tempfile::tempdir().unwrap();
    let app = install(root.path(), 7);
    content(&app.db, 2);
    app.secrets
        .save(settings::AI_KEY_SECRET, "sk-live-very-secret-key")
        .unwrap();
    let backup = app
        .backups
        .write_backup(BackupKind::Manual, &|_| Ok(()))
        .unwrap();
    let bytes = std::fs::read(&backup.path).unwrap();
    let needle = b"sk-live-very-secret-key";
    assert!(!bytes.windows(needle.len()).any(|window| window == needle));
}
