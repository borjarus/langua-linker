//! Bundled-content importer on the real SQLite unit of work (plan Phase 8 DoD):
//! fresh install, idempotent second run, update +50 with progress and overrides, a failure in the
//! middle of a deck, sentence links resolved once their word deck arrives, invalid content.

use std::io::{self, BufRead, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use langualinker_application::content::{
    ContentService, ImportOutcome, OnboardingInput, PENDING_KEY, PrepareOutcome, ResultEncoders,
};
use langualinker_application::jobs::{JobEvent, JobRecord, JobService, JobStatus};
use langualinker_application::ports::{Clock, CollectionScope, EventSink, Rng};
use langualinker_application::repositories::UnitOfWork;
use langualinker_content::reader::sha256_hex;
use langualinker_content::{ContentSource, DirSource};
use langualinker_domain::card::{CardState, QueueState};
use langualinker_domain::content::WordLinkResolution;
use langualinker_domain::deck::TargetLanguage;
use langualinker_domain::{Guid, TimestampMs};
use langualinker_infrastructure::sqlite::SqliteDatabase;
use serde_json::Value;

const T0: i64 = 1_772_438_400_000;

struct Clocked(Mutex<i64>);

impl Clock for Clocked {
    fn now(&self) -> TimestampMs {
        let mut now = self.0.lock().unwrap();
        *now += 1;
        TimestampMs(*now)
    }
}

/// Deterministic but unique across every service of the test process, like real GUIDs.
struct CountingRng;

static RNG_COUNTER: AtomicUsize = AtomicUsize::new(1);

impl Rng for CountingRng {
    fn fill(&self, bytes: &mut [u8]) {
        let n = RNG_COUNTER.fetch_add(1, Ordering::SeqCst) as u64;
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = (n >> ((index % 8) * 8)) as u8 ^ index as u8;
        }
    }
}

#[derive(Default)]
struct Events {
    changed: Mutex<Vec<Vec<CollectionScope>>>,
}

impl EventSink for Events {
    fn job_progress(&self, _: &JobEvent) {}

    fn collection_changed(&self, scopes: &[CollectionScope], _: TimestampMs) {
        self.changed.lock().unwrap().push(scopes.to_vec());
    }
}

fn encode_import(outcome: &ImportOutcome) -> String {
    serde_json::json!({
        "deckId": outcome.deck_id.0,
        "created": outcome.created,
        "updated": outcome.updated,
        "unchanged": outcome.unchanged,
        "scheduled": outcome.scheduled,
        "reviews": outcome.reviews,
    })
    .to_string()
}

fn encode(outcome: &PrepareOutcome) -> String {
    serde_json::json!({
        "created": outcome.created,
        "updated": outcome.updated,
        "unchanged": outcome.unchanged,
        "kept": outcome.kept,
        "archived": outcome.archived,
        "decks": outcome.deck_ids.len(),
        "warnings": outcome.warnings.iter().map(|w| format!("{}:{}", w.code, w.field.clone().unwrap_or_default())).collect::<Vec<_>>(),
    })
    .to_string()
}

struct Harness {
    db: Arc<SqliteDatabase>,
    jobs: JobService<SqliteDatabase>,
    events: Arc<Events>,
}

impl Harness {
    fn new() -> Self {
        let db = Arc::new(
            SqliteDatabase::open_in_memory(&Clocked(Mutex::new(T0)))
                .unwrap()
                .database,
        );
        Self::on(db)
    }

    fn on(db: Arc<SqliteDatabase>) -> Self {
        let events = Arc::new(Events::default());
        let jobs = JobService::new(
            db.clone(),
            events.clone(),
            Arc::new(Clocked(Mutex::new(T0))),
        );
        Self { db, jobs, events }
    }

    fn service(&self, source: Arc<dyn ContentSource>) -> ContentService<SqliteDatabase> {
        ContentService::new(
            self.db.clone(),
            source,
            self.jobs.clone(),
            self.events.clone(),
            Arc::new(Clocked(Mutex::new(T0 + 1_000))),
            Arc::new(CountingRng),
            ResultEncoders {
                prepare: encode,
                import: encode_import,
            },
        )
    }

    fn wait(&self, job: &JobRecord) -> JobRecord {
        for _ in 0..4_000 {
            let current = self.jobs.get(&job.id).unwrap();
            if current.status.is_finished() {
                return current;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("job did not finish");
    }

    fn count(&self, sql: &'static str) -> i64 {
        self.db
            .with_connection(move |conn| {
                conn.query_row(sql, [], |row| row.get(0)).map_err(|error| {
                    langualinker_application::AppError::Internal(error.to_string())
                })
            })
            .unwrap()
    }

    fn pending(&self) -> Vec<String> {
        self.db
            .read(|tx| tx.setting(PENDING_KEY))
            .unwrap()
            .map(|json| serde_json::from_str(&json).unwrap())
            .unwrap_or_default()
    }
}

/// The bundled content, decrypted once per test binary into a plain directory (the committed
/// `content/decks` holds only the encrypted archive).
fn content_dir() -> PathBuf {
    static DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let password = langualinker_content::archive::find_password(&root)
            .expect("LANGUA_DECKS_PASSWORD (environment or .env) decrypts content/decks/decks.7z");
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(concat!(env!("CARGO_CRATE_NAME"), "-bundled-content"));
        let _ = std::fs::remove_dir_all(&dir);
        langualinker_content::archive::unpack_to_dir(
            &root.join("content/decks/decks.7z"),
            &dir,
            &password,
        )
        .unwrap();
        dir
    })
    .clone()
}

fn result(job: &JobRecord) -> Value {
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    serde_json::from_str(job.result_json.as_deref().unwrap()).unwrap()
}

const ALL: [&str; 4] = [
    "bundled-de-words-a1",
    "bundled-de-sentences-a1",
    "bundled-en-words-a1",
    "bundled-en-sentences-a1",
];

fn onboarding(operation: &str, decks: &[&str]) -> OnboardingInput {
    OnboardingInput {
        operation_id: operation.into(),
        native_language: langualinker_domain::deck::NativeLanguage::Pl,
        target_languages: vec![TargetLanguage::De, TargetLanguage::En],
        daily_goal: 25,
        deck_guids: decks.iter().map(|guid| (*guid).to_owned()).collect(),
    }
}

/// Copy of the bundled content that a test may edit.
fn copy_content() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for entry in std::fs::read_dir(content_dir()).unwrap() {
        let entry = entry.unwrap();
        std::fs::copy(entry.path(), dir.path().join(entry.file_name())).unwrap();
    }
    dir
}

fn rewrite(dir: &Path, stem: &str, version: &str, change: impl FnOnce(&mut Vec<Value>)) {
    let notes = dir.join(format!("{stem}.ndjson"));
    let mut records: Vec<Value> = std::fs::read_to_string(&notes)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    change(&mut records);
    let text: String = records
        .iter()
        .map(|record| format!("{}\n", serde_json::to_string(record).unwrap()))
        .collect();
    std::fs::write(&notes, &text).unwrap();
    let path = dir.join(format!("{stem}.json"));
    let mut manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let templates = if manifest["noteType"] == "BasicAndReversed" {
        2
    } else {
        1
    };
    manifest["sha256"] = sha256_hex(text.as_bytes()).into();
    manifest["expectedNotes"] = records.len().into();
    manifest["expectedCards"] = (records.len() * templates).into();
    manifest["contentVersion"] = version.into();
    std::fs::write(&path, serde_json::to_string_pretty(&manifest).unwrap()).unwrap();
}

#[test]
fn fresh_install_contains_the_four_decks_with_content_and_links() {
    let harness = Harness::new();
    let content = harness.service(Arc::new(DirSource::new(content_dir())));
    assert!(!content.onboarding_state().unwrap().completed);
    let job = content
        .complete_onboarding(onboarding("op-1", &ALL))
        .unwrap();
    let outcome = result(&harness.wait(&job));
    assert_eq!(outcome["created"], 3112);
    assert_eq!(outcome["decks"], 4);
    assert_eq!(
        harness.count("SELECT COUNT(*) FROM decks WHERE bundled = 1"),
        4
    );
    assert_eq!(harness.count("SELECT COUNT(*) FROM notes"), 3112);
    // 486 words × 2 templates + 2626 sentences × 1.
    assert_eq!(harness.count("SELECT COUNT(*) FROM cards"), 3598);
    assert_eq!(
        harness.count("SELECT COUNT(*) FROM cards WHERE state <> 0 OR due_at IS NOT NULL"),
        0
    );
    assert_eq!(
        harness.count("SELECT COUNT(*) FROM associations WHERE provenance = 1"),
        3112
    );
    assert_eq!(harness.count("SELECT COUNT(*) FROM bundled_notes"), 3112);
    assert_eq!(
        harness.count("SELECT COUNT(*) FROM sentence_word_links WHERE resolution <> 1"),
        0
    );
    assert!(harness.count("SELECT COUNT(*) FROM sentence_word_links") > 50);
    assert_eq!(
        harness.count("SELECT COUNT(DISTINCT guid) FROM cards"),
        3598,
        "card GUIDs are generated"
    );
    assert_eq!(
        harness.count(
            "SELECT COUNT(*) FROM cards c JOIN notes n ON n.id = c.note_id WHERE c.guid = n.guid"
        ),
        0
    );
    assert!(harness.pending().is_empty());
    let state = content.onboarding_state().unwrap();
    assert!(state.completed);
    assert_eq!(state.installed.len(), 4);
    assert_eq!(state.daily_goal, 25);
    // A repeated operation id returns the first job instead of starting another one.
    assert_eq!(
        content
            .complete_onboarding(onboarding("op-1", &ALL))
            .unwrap()
            .id,
        job.id
    );
    assert!(
        harness
            .events
            .changed
            .lock()
            .unwrap()
            .iter()
            .any(|scopes| scopes.contains(&CollectionScope::Cards))
    );
}

#[test]
fn second_import_is_idempotent() {
    let harness = Harness::new();
    let content = harness.service(Arc::new(DirSource::new(content_dir())));
    harness.wait(
        &content
            .complete_onboarding(onboarding("op-1", &ALL))
            .unwrap(),
    );
    let before = harness.count("SELECT COUNT(*) FROM notes")
        + harness.count("SELECT COUNT(*) FROM cards") * 1_000;
    // Force every deck through the importer again.
    let at = TimestampMs(T0 + 50);
    harness
        .db
        .write(move |tx| tx.put_setting(PENDING_KEY, &serde_json::to_string(&ALL).unwrap(), at))
        .unwrap();
    let job = content
        .resume()
        .unwrap()
        .expect("pending decks start a job");
    let outcome = result(&harness.wait(&job));
    assert_eq!(
        (outcome["created"].as_u64(), outcome["updated"].as_u64()),
        (Some(0), Some(0))
    );
    assert_eq!(outcome["unchanged"], 3112);
    assert_eq!(
        harness.count("SELECT COUNT(*) FROM notes")
            + harness.count("SELECT COUNT(*) FROM cards") * 1_000,
        before
    );
    assert_eq!(harness.count("SELECT COUNT(*) FROM associations"), 3112);
    assert_eq!(harness.count("SELECT COUNT(*) FROM review_logs"), 0);
    // Nothing is pending afterwards, so the next start does nothing.
    assert!(content.resume().unwrap().is_none());
}

#[test]
fn update_with_fifty_new_notes_preserves_progress_and_overrides() {
    let harness = Harness::new();
    let original = harness.service(Arc::new(DirSource::new(content_dir())));
    harness.wait(
        &original
            .complete_onboarding(onboarding("op-1", &["bundled-de-words-a1"]))
            .unwrap(),
    );

    let guid_at = |index: usize| -> Guid {
        let text = std::fs::read_to_string(content_dir().join("de_words_a1.ndjson")).unwrap();
        let record: Value = serde_json::from_str(text.lines().nth(index).unwrap()).unwrap();
        Guid::parse(record["guid"].as_str().unwrap()).unwrap()
    };
    let studied = guid_at(0);
    let edited = guid_at(1);
    let words = std::fs::read_to_string(content_dir().join("de_words_a1.ndjson"))
        .unwrap()
        .lines()
        .count();
    let removed_studied = guid_at(words - 2);
    let removed_new = guid_at(words - 1);

    // The learner studied two notes and edited the back of another.
    let (studied_card, studied_removed_card) = {
        let (studied, edited, removed_studied) =
            (studied.clone(), edited.clone(), removed_studied.clone());
        harness
            .db
            .write(move |tx| {
                let mut cards = Vec::new();
                for guid in [&studied, &removed_studied] {
                    let note = tx.note_by_guid(guid)?.unwrap();
                    let mut card = tx.cards_of_note(note.id)?.remove(0);
                    card.state = CardState::Review;
                    card.queue = QueueState::Review;
                    card.due_at = Some(TimestampMs(T0 + 86_400_000 * 3));
                    card.fsrs.stability = Some(3.2);
                    card.fsrs.difficulty = Some(5.1);
                    card.fsrs.last_review_at = Some(TimestampMs(T0));
                    card.fsrs.scheduled_days = 3;
                    card.reps = 4;
                    card.lapses = 1;
                    cards.push(tx.update_card(&card, TimestampMs(T0 + 10))?);
                }
                let mut note = tx.note_by_guid(&edited)?.unwrap();
                note.fields.back = "moja wersja".into();
                tx.update_note(&note, TimestampMs(T0 + 10))?;
                tx.put_content_override(note.id, "back", "2026.11.09", TimestampMs(T0 + 10))?;
                Ok((cards[0].clone(), cards[1].clone()))
            })
            .unwrap()
    };

    // Next content version: +50 words, a changed back on both edited and studied notes, the
    // last two notes removed.
    let dir = copy_content();
    rewrite(dir.path(), "de_words_a1", "2026.12.01", |records| {
        records[0]["back"] = "ja (zaimek)".into();
        records[1]["back"] = "ty (nowe tłumaczenie)".into();
        records.truncate(records.len() - 2);
        for n in 0..50 {
            records.push(serde_json::json!({
                "guid": format!("dew-new-{n:03}"),
                "front": format!("Wort {n}"),
                "back": format!("słowo {n}"),
                "tags": ["noun", "A1"],
                "associations": [{"guid": format!("dew-new-{n:03}-a1"), "text": format!("Skojarzenie {n}"), "promptVersion": null, "model": null}],
            }));
        }
    });
    let updated = harness.service(Arc::new(DirSource::new(dir.path())));
    let job = updated
        .resume()
        .unwrap()
        .expect("a newer version queues the deck");
    let outcome = result(&harness.wait(&job));
    assert_eq!(outcome["created"], 50);
    assert_eq!(
        outcome["updated"], 1,
        "only the studied note changed (its back)"
    );
    assert_eq!(outcome["kept"], 1);
    assert_eq!(outcome["archived"], 1);
    assert_eq!(
        outcome["warnings"],
        serde_json::json!(["overrideConflict:back"])
    );

    let (studied_after, edited_back, removed_cards, kept_cards, version) = harness
        .db
        .read(move |tx| {
            let note = tx.note_by_guid(&studied)?.unwrap();
            let studied_after = tx.cards_of_note(note.id)?.remove(0);
            let edited_back = tx.note_by_guid(&edited)?.unwrap().fields.back;
            let removed = tx.note_by_guid(&removed_new)?.unwrap();
            let removed_cards = tx.cards_of_note(removed.id)?;
            let kept = tx.note_by_guid(&removed_studied)?.unwrap();
            let kept_cards = tx.cards_of_note(kept.id)?;
            let deck = tx
                .deck_by_guid(&Guid::parse("bundled-de-words-a1").unwrap())?
                .unwrap();
            Ok((
                studied_after,
                edited_back,
                removed_cards,
                kept_cards,
                deck.content_version,
            ))
        })
        .unwrap();
    // Scheduling is untouched by the content update.
    let (before, after) = (&studied_card, &studied_after);
    assert_eq!(
        (
            after.due_at,
            after.fsrs,
            after.reps,
            after.lapses,
            after.state,
            after.queue
        ),
        (
            before.due_at,
            before.fsrs,
            before.reps,
            before.lapses,
            before.state,
            before.queue
        )
    );
    assert_eq!(edited_back, "moja wersja", "the user's edit wins");
    assert!(
        removed_cards.iter().all(|card| card.archived_at.is_some()),
        "removed without progress: archived"
    );
    assert!(
        kept_cards.iter().all(|card| card.archived_at.is_none()),
        "removed with progress: kept"
    );
    assert_eq!(kept_cards[0].reps, studied_removed_card.reps);
    assert_eq!(version.as_deref(), Some("2026.12.01"));
    assert_eq!(
        harness.count("SELECT COUNT(*) FROM notes"),
        words as i64 + 50
    );
}

/// Serves a notes file normally on the first open (validation) and fails half-way through on
/// the second (the install transaction) — the importer's view of a process killed mid-deck.
struct FailingSecondRead {
    inner: DirSource,
    file: &'static str,
    opens: AtomicUsize,
}

struct FailAfter<R> {
    inner: R,
    left: usize,
}

impl<R: BufRead> Read for FailAfter<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let available = self.fill_buf()?;
        let n = available.len().min(buf.len());
        buf[..n].copy_from_slice(&available[..n]);
        self.consume(n);
        Ok(n)
    }
}

impl<R: BufRead> BufRead for FailAfter<R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        if self.left == 0 {
            return Err(io::Error::other("killed"));
        }
        let buf = self.inner.fill_buf()?;
        Ok(&buf[..buf.len().min(self.left)])
    }

    fn consume(&mut self, amount: usize) {
        self.left -= amount;
        self.inner.consume(amount);
    }
}

impl ContentSource for FailingSecondRead {
    fn open(&self, name: &str) -> io::Result<Box<dyn BufRead + Send + '_>> {
        let reader = self.inner.open(name)?;
        if name == self.file && self.opens.fetch_add(1, Ordering::SeqCst) == 1 {
            return Ok(Box::new(FailAfter {
                inner: reader,
                left: 2_000,
            }));
        }
        Ok(reader)
    }
}

#[test]
fn failure_inside_a_deck_leaves_no_partial_deck_and_restart_completes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("collection.sqlite");
    let db = Arc::new(
        SqliteDatabase::open(&path, &Clocked(Mutex::new(T0)))
            .unwrap()
            .database,
    );
    let harness = Harness::on(db);
    let failing = harness.service(Arc::new(FailingSecondRead {
        inner: DirSource::new(content_dir()),
        file: "en_words_a1.ndjson",
        opens: AtomicUsize::new(0),
    }));
    let job = harness.wait(
        &failing
            .complete_onboarding(onboarding(
                "op-1",
                &["bundled-de-words-a1", "bundled-en-words-a1"],
            ))
            .unwrap(),
    );
    assert_eq!(job.status, JobStatus::Failed);
    // The German deck committed; the English one left nothing behind and is still pending.
    assert_eq!(harness.count("SELECT COUNT(*) FROM decks"), 1);
    assert_eq!(harness.count("SELECT COUNT(*) FROM notes"), 151);
    assert_eq!(harness.pending(), ["bundled-en-words-a1"]);

    // "Restart": reopen the file and resume with an intact source.
    drop(failing);
    let Harness { db, jobs, events } = harness;
    drop((jobs, events));
    drop(db);
    let reopened = Arc::new(
        SqliteDatabase::open(&path, &Clocked(Mutex::new(T0 + 5)))
            .unwrap()
            .database,
    );
    let harness = Harness::on(reopened);
    let content = harness.service(Arc::new(DirSource::new(content_dir())));
    let job = content
        .resume()
        .unwrap()
        .expect("pending deck resumes at startup");
    let outcome = result(&harness.wait(&job));
    assert_eq!(outcome["created"], 335);
    assert_eq!(harness.count("SELECT COUNT(*) FROM decks"), 2);
    assert!(harness.pending().is_empty());
}

#[test]
fn sentence_links_resolve_when_the_word_deck_arrives_later() {
    let harness = Harness::new();
    let content = harness.service(Arc::new(DirSource::new(content_dir())));
    harness.wait(
        &content
            .complete_onboarding(onboarding("op-1", &["bundled-de-sentences-a1"]))
            .unwrap(),
    );
    let links = harness.count("SELECT COUNT(*) FROM sentence_word_links");
    assert!(links > 0);
    assert_eq!(
        harness.count("SELECT COUNT(*) FROM sentence_word_links WHERE word_note_id IS NULL AND lemma IS NOT NULL"),
        links,
        "unresolved, lemma kept"
    );
    let _ = WordLinkResolution::Unresolved;
    let job = content
        .complete_onboarding(onboarding("op-2", &["bundled-de-words-a1"]))
        .unwrap();
    let outcome = result(&harness.wait(&job));
    assert_eq!(outcome["created"], 151);
    assert_eq!(
        harness.count("SELECT COUNT(*) FROM sentence_word_links WHERE resolution = 1"),
        links
    );
}

#[test]
fn invalid_content_fails_the_job_before_writing() {
    let dir = copy_content();
    rewrite(dir.path(), "de_sentences_a1", "2026.11.04", |records| {
        records[0]["wordLinks"][0]["wordGuid"] = "dew-missing".into();
    });
    let harness = Harness::new();
    let content = harness.service(Arc::new(DirSource::new(dir.path())));
    let job = harness.wait(
        &content
            .complete_onboarding(onboarding(
                "op-1",
                &["bundled-de-words-a1", "bundled-de-sentences-a1"],
            ))
            .unwrap(),
    );
    assert_eq!(job.status, JobStatus::Failed);
    let error = job.error.unwrap();
    assert_eq!(error.code, "VALIDATION_FAILED");
    assert!(
        format!("{:?}", error.details).contains("link.unresolved"),
        "{:?}",
        error.details
    );
    assert_eq!(
        harness.count("SELECT COUNT(*) FROM notes"),
        0,
        "validated before any deck was written"
    );
}

#[test]
fn onboarding_input_is_validated() {
    let harness = Harness::new();
    let content = harness.service(Arc::new(DirSource::new(content_dir())));
    let mut input = onboarding("op-1", &["bundled-en-words-a1"]);
    input.target_languages = vec![TargetLanguage::De];
    assert!(
        content.complete_onboarding(input).is_err(),
        "deck language not chosen"
    );
    let mut input = onboarding("op-2", &["nope"]);
    input.daily_goal = 25;
    assert!(content.complete_onboarding(input).is_err());
    let mut input = onboarding("op-3", &[]);
    input.daily_goal = 0;
    assert!(content.complete_onboarding(input).is_err());
    assert_eq!(harness.count("SELECT COUNT(*) FROM jobs"), 0);
}

/// Plan step 17: 5 000 notes in under 5 s. Run with
/// `cargo test --release -p langualinker-infrastructure --test content_import -- --ignored`.
#[test]
#[ignore = "performance budget; run in release"]
fn imports_five_thousand_notes_within_budget() {
    let dir = copy_content();
    rewrite(dir.path(), "de_words_a1", "2026.11.04", |records| {
        let template = records[0].clone();
        records.clear();
        for n in 0..5_000 {
            let mut record = template.clone();
            record["guid"] = format!("dew-perf-{n:05}").into();
            record["front"] = format!("Wort {n}").into();
            record["associations"][0]["guid"] = format!("dew-perf-{n:05}-a1").into();
            records.push(record);
        }
    });
    rewrite(dir.path(), "de_sentences_a1", "2026.11.04", |records| {
        for record in records.iter_mut() {
            for link in record["wordLinks"].as_array_mut().unwrap() {
                link["wordGuid"] = "dew-perf-00000".into();
            }
        }
    });
    let harness = Harness::new();
    let content = harness.service(Arc::new(DirSource::new(dir.path())));
    let started = Instant::now();
    let job = harness.wait(
        &content
            .complete_onboarding(onboarding("op-1", &["bundled-de-words-a1"]))
            .unwrap(),
    );
    let elapsed = started.elapsed();
    assert_eq!(result(&job)["created"], 5_000);
    println!("imported 5000 notes / 10000 cards in {elapsed:?}");
    assert!(elapsed < Duration::from_secs(5), "{elapsed:?}");
}

// ---------------------------------------------------------------- import packages (Anki)

/// The real Anki export of the fixtures, converted with sentence + reverse cards and the German
/// side as the front.
fn anki_package() -> Vec<u8> {
    let apkg = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/kmp/apkg/sample_german_a1_sentences.apkg");
    let collection = langualinker_anki::read_apkg(&apkg).unwrap();
    let options = langualinker_anki::Options {
        name: Some("Niemiecki — zdania z Anki".into()),
        description: None,
        language: TargetLanguage::De,
        kind: langualinker_content::format::ContentKind::Sentences,
        reverse: true,
        deck_type: langualinker_domain::deck::DeckType::Linguistic,
        level: "A1".into(),
        anki_deck: None,
        front_field: langualinker_anki::FieldRef::Name("Back".into()),
        back_field: langualinker_anki::FieldRef::Name("Front".into()),
        scheduling: true,
        reviews: true,
        deck_guid: None,
        file_name: "sample_german_a1_sentences.apkg".into(),
        today: chrono::NaiveDate::from_ymd_opt(2026, 11, 4).unwrap(),
        strict: true,
    };
    langualinker_anki::convert(&collection, &options)
        .unwrap()
        .package
}

#[test]
fn anki_package_imports_with_its_scheduling_and_history() {
    let harness = Harness::new();
    let content = harness.service(Arc::new(DirSource::new(content_dir())));
    let package = anki_package();
    let preview = content.preview_package("deck.ndjson", &package).unwrap();
    assert_eq!(
        (preview.notes, preview.cards, preview.reviews),
        (432, 864, 5_329)
    );
    assert_eq!(
        (
            preview.new_notes,
            preview.existing_notes,
            preview.existing_deck
        ),
        (432, 0, None)
    );
    assert!(preview.scheduled > 800);
    assert_eq!(
        harness.count("SELECT COUNT(*) FROM notes"),
        0,
        "a preview writes nothing"
    );

    let job = harness.wait(
        &content
            .commit_package("op-import".into(), &preview.token)
            .unwrap(),
    );
    let outcome = result(&job);
    assert_eq!(outcome["created"], 432);
    assert_eq!(outcome["reviews"], 5_329);
    assert_eq!(outcome["scheduled"], preview.scheduled);
    assert_eq!(
        harness.count("SELECT COUNT(*) FROM decks WHERE bundled = 0"),
        1,
        "a user deck, not bundled"
    );
    assert_eq!(harness.count("SELECT COUNT(*) FROM cards"), 864);
    assert_eq!(
        harness.count("SELECT COUNT(*) FROM cards WHERE state <> 0"),
        i64::from(preview.scheduled)
    );
    assert_eq!(
        harness.count(
            "SELECT COUNT(*) FROM cards WHERE state = 2 AND (due_at IS NULL OR stability IS NULL)"
        ),
        0
    );
    assert_eq!(
        harness.count("SELECT COUNT(*) FROM review_logs WHERE fsrs_version = 'import'"),
        5_329
    );
    // The converted "Nie." card: review state, due 2027-01-15 05:00 UTC, FSRS state kept.
    let due: i64 = harness.count(
        "SELECT c.due_at FROM cards c JOIN notes n ON n.id = c.note_id
         WHERE json_extract(n.fields, '$.front') = 'Nein. /najn/' AND c.template_ordinal = 0",
    );
    assert_eq!(due, 1_800_036_000_000);
    // A repeated operation id returns the first job.
    assert_eq!(
        content
            .commit_package("op-import".into(), &preview.token)
            .unwrap()
            .id,
        job.id
    );
}

#[test]
fn importing_the_package_again_keeps_progress_made_in_the_app() {
    let harness = Harness::new();
    let content = harness.service(Arc::new(DirSource::new(content_dir())));
    let package = anki_package();
    let preview = content.preview_package("deck.ndjson", &package).unwrap();
    harness.wait(
        &content
            .commit_package("op-1".into(), &preview.token)
            .unwrap(),
    );
    // The learner studies a card in the app afterwards.
    harness
        .db
        .with_connection(|conn| {
            conn.execute("UPDATE cards SET reps = 99, due_at = 1900000000000, state = 2, queue = 2, stability = 1.5, difficulty = 5 WHERE id = (SELECT MIN(id) FROM cards)", [])
                .map(drop)
                .map_err(|error| langualinker_application::AppError::Internal(error.to_string()))
        })
        .unwrap();
    let again = content.preview_package("deck.ndjson", &package).unwrap();
    assert_eq!((again.new_notes, again.existing_notes), (0, 432));
    assert!(again.existing_deck.is_some());
    let outcome =
        result(&harness.wait(&content.commit_package("op-2".into(), &again.token).unwrap()));
    assert_eq!(
        (outcome["created"].as_u64(), outcome["reviews"].as_u64()),
        (Some(0), Some(0))
    );
    assert_eq!(outcome["unchanged"], 432);
    assert_eq!(
        harness.count("SELECT reps FROM cards WHERE id = (SELECT MIN(id) FROM cards)"),
        99
    );
    assert_eq!(
        harness.count("SELECT COUNT(*) FROM review_logs"),
        5_329,
        "no duplicated history"
    );
}

#[test]
fn package_previews_go_stale_and_foreign_files_are_rejected() {
    let harness = Harness::new();
    let content = harness.service(Arc::new(DirSource::new(content_dir())));
    let package = anki_package();
    let first = content.preview_package("deck.ndjson", &package).unwrap();
    let second = content.preview_package("deck.ndjson", &package).unwrap();
    assert!(
        matches!(
            content.commit_package("op-old".into(), &first.token),
            Err(langualinker_application::AppError::PreviewStale)
        ),
        "only the latest preview can be imported"
    );
    // The collection changes between preview and import.
    harness.wait(
        &content
            .complete_onboarding(onboarding("op-onb", &["bundled-de-words-a1"]))
            .unwrap(),
    );
    assert!(matches!(
        content.commit_package("op-new".into(), &second.token),
        Err(langualinker_application::AppError::PreviewStale)
    ));
    let apkg = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/kmp/apkg/sample_german_a1_sentences.apkg"),
    )
    .unwrap();
    assert_eq!(
        content.preview_package("deck.apkg", &apkg).unwrap_err(),
        langualinker_application::AppError::UnsupportedFormat { format: "apkg" }
    );
    assert_eq!(
        content
            .preview_package("deck.csv", b"front;back\nHund;pies\n")
            .unwrap_err(),
        langualinker_application::AppError::UnsupportedFormat { format: "unknown" }
    );
    // A package whose notes were edited after conversion fails the checksum.
    let mut tampered = package.clone();
    let last = tampered.len() - 3;
    tampered[last] = b'X';
    assert_eq!(
        content
            .preview_package("deck.ndjson", &tampered)
            .unwrap_err()
            .code(),
        "VALIDATION_FAILED"
    );
}
