//! Property-based fuzzing (plan Phase 16 step 10): the search language, backup archives, the
//! bundled-content archive and the study-session state machine get arbitrary or mutated input.
//! Each property says what must hold for *every* input — no panic, no partial effect, no state
//! that contradicts the review log. Failing cases are shrunk and printed by proptest; the seeds
//! of past failures live in `fuzz.proptest-regressions` next to this file and run first.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use langualinker_application::browser::{fts_query, parse_query};
use langualinker_application::jobs::{JobEvent, JobService};
use langualinker_application::ports::{Clock, CollectionScope, EventSink, Rng};
use langualinker_application::repositories::UnitOfWork;
use langualinker_application::study::{self, RateCard, SessionAction, StartStudy, StudyView};
use langualinker_application::{AppError, AppResult};
use langualinker_domain::card::{Card, NewCard};
use langualinker_domain::deck::{DeckType, NativeLanguage, NewDeck, TargetLanguage};
use langualinker_domain::note::{NewNote, NoteFields, NoteTypeKind};
use langualinker_domain::review::Rating;
use langualinker_domain::{CardId, DeckConfigId, DeckId, Guid, TimestampMs};
use langualinker_infrastructure::backup::{
    BackupDirs, BackupKind, BackupService, DatabasePart, FORMAT, FORMAT_VERSION, Manifest,
    ManifestCounts, MediaPart,
};
use langualinker_infrastructure::sqlite::{DATABASE_FILE, SqliteDatabase, migrations};
use proptest::prelude::*;
use sha2::{Digest, Sha256};

/// 2026-03-02 12:00 UTC.
const NOON: i64 = 1_772_452_800_000;
const MINUTE: i64 = 60_000;

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
            *byte = next[index % 8] ^ (index as u8);
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
    bytes[8] = 0xf0;
    Guid::from_bytes(bytes)
}

// ---------------------------------------------------------------- search language

/// Characters the parser treats specially, plus ordinary and non-ASCII text.
fn query_text() -> impl Strategy<Value = String> {
    proptest::collection::vec(
        prop_oneof![
            Just("\"".to_owned()),
            Just(":".to_owned()),
            Just(" ".to_owned()),
            Just("deck:".to_owned()),
            Just("tag:".to_owned()),
            Just("is:".to_owned()),
            Just("added:".to_owned()),
            Just("*".to_owned()),
            Just("'".to_owned()),
            "[a-zA-Z0-9äöüßżó_-]{1,6}",
            any::<char>().prop_map(String::from),
        ],
        0..12,
    )
    .prop_map(|parts| parts.concat())
}

/// `"phrase"* "phrase"* …`: every phrase quoted with inner quotes doubled, a prefix star after
/// each, one space between them. (A string split on `"* ` is not enough: a phrase may itself
/// start with `* `, found by this test with the query `"*\0deck:"`.)
fn well_formed_fts(fts: &str) -> bool {
    let mut chars = fts.chars().peekable();
    loop {
        if chars.next() != Some('"') {
            return false;
        }
        loop {
            match chars.next() {
                None => return false,
                Some('"') if chars.peek() == Some(&'"') => {
                    chars.next();
                }
                Some('"') => break,
                Some(c) if c.is_control() => return false,
                Some(_) => {}
            }
        }
        if chars.next() != Some('*') {
            return false;
        }
        match chars.next() {
            None => return true,
            Some(' ') => {}
            Some(_) => return false,
        }
    }
}

#[test]
fn the_fts_oracle_accepts_only_quoted_prefix_phrases() {
    assert!(well_formed_fts("\"a\"*"));
    assert!(well_formed_fts("\"* deck:\"* \"a\"\"b\"*"));
    assert!(!well_formed_fts("\"a\"* "));
    assert!(!well_formed_fts("\"a\"b\"*"));
    assert!(!well_formed_fts("a*"));
    assert!(!well_formed_fts("\"a\""));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]

    /// Any query parses or is refused with a validation error; the FTS expression built from
    /// the words is always well-formed (every phrase quoted, inner quotes doubled).
    #[test]
    fn any_search_query_parses_or_is_refused(query in query_text()) {
        match parse_query(&query) {
            Ok(parsed) => {
                if let Some(fts) = fts_query(&parsed.words) {
                    prop_assert!(well_formed_fts(&fts), "{}", fts);
                }
            }
            Err(error) => prop_assert!(matches!(error, AppError::Validation(_)), "{error:?}"),
        }
    }
}

/// The FTS expression of arbitrary words runs on the real index without an SQL or FTS error.
#[test]
fn fts_expressions_of_odd_words_run_on_the_index() {
    let db = SqliteDatabase::open_in_memory(&TestClock(AtomicI64::new(NOON)))
        .unwrap()
        .database;
    let words = [
        "\"",
        "\"\"x",
        "a\"b",
        "NEAR(",
        "*",
        "a*b",
        "OR",
        "-x",
        "^y",
        "{z}",
        "ż\u{0}x",
        "\"*\u{0}deck:\"",
    ];
    for word in words {
        let parsed = parse_query(word);
        let Ok(parsed) = parsed else { continue };
        let Some(fts) = fts_query(&parsed.words) else {
            continue;
        };
        let result: AppResult<i64> = db.with_connection(move |conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM notes_fts WHERE notes_fts MATCH ?1",
                [fts],
                |row| row.get(0),
            )
            .map_err(|error| AppError::Internal(error.to_string()))
        });
        assert!(result.is_ok(), "{word:?}: {result:?}");
    }
}

// ---------------------------------------------------------------- backup archives

struct Archive {
    bytes: Vec<u8>,
    manifest_len: usize,
}

/// A real archive: a manifest line plus a `VACUUM INTO` snapshot of a migrated collection.
fn valid_archive(dir: &Path) -> Archive {
    let clock = TestClock(AtomicI64::new(NOON));
    let db = SqliteDatabase::open(&dir.join("source.sqlite"), &clock)
        .unwrap()
        .database;
    let snapshot = dir.join("snapshot.sqlite");
    db.snapshot_to(&snapshot).unwrap();
    let database = std::fs::read(&snapshot).unwrap();
    let manifest = Manifest {
        format: FORMAT.to_owned(),
        format_version: FORMAT_VERSION,
        created_at: NOON,
        kind: BackupKind::Manual,
        app_version: "0.0.0-fuzz".to_owned(),
        schema_version: migrations::latest_version(langualinker_infrastructure::sqlite::MIGRATIONS),
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
    let manifest_len = bytes.len();
    bytes.extend_from_slice(&database);
    Archive {
        bytes,
        manifest_len,
    }
}

fn backup_service(dir: &Path) -> BackupService {
    let clock: Arc<dyn Clock> = Arc::new(TestClock(AtomicI64::new(NOON)));
    let db = Arc::new(
        SqliteDatabase::open(&dir.join(DATABASE_FILE), clock.as_ref())
            .unwrap()
            .database,
    );
    let events: Arc<dyn EventSink> = Arc::new(Silent);
    let jobs = JobService::new(db.clone(), events.clone(), clock.clone());
    BackupService::new(
        db,
        jobs,
        events,
        clock,
        Arc::new(Counter(AtomicI64::new(1))),
        None,
        BackupDirs::under(dir),
        "0.0.0-fuzz",
    )
}

#[derive(Debug, Clone)]
enum Mutation {
    Flip { at: usize, mask: u8 },
    Truncate { at: usize },
    Append(Vec<u8>),
    Insert { at: usize, byte: u8 },
}

fn mutation() -> impl Strategy<Value = Mutation> {
    prop_oneof![
        (any::<usize>(), 1u8..=255).prop_map(|(at, mask)| Mutation::Flip { at, mask }),
        any::<usize>().prop_map(|at| Mutation::Truncate { at }),
        proptest::collection::vec(any::<u8>(), 1..64).prop_map(Mutation::Append),
        (any::<usize>(), any::<u8>()).prop_map(|(at, byte)| Mutation::Insert { at, byte }),
    ]
}

/// Applies the mutation; returns whether the database part (after the manifest) changed.
fn mutate(bytes: &mut Vec<u8>, manifest_len: usize, mutation: &Mutation) -> bool {
    let len = bytes.len();
    match mutation {
        Mutation::Flip { at, mask } => {
            let at = at % len;
            bytes[at] ^= mask;
            at >= manifest_len
        }
        Mutation::Truncate { at } => {
            bytes.truncate(at % len);
            true
        }
        Mutation::Append(extra) => {
            bytes.extend_from_slice(extra);
            true
        }
        Mutation::Insert { at, byte } => {
            let at = at % len;
            bytes.insert(at, *byte);
            true
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    /// A damaged archive is refused with a typed error and never panics; a change to the
    /// database part can never pass validation (size and SHA-256 are checked).
    #[test]
    fn mutated_backup_archives_are_refused(mutations in proptest::collection::vec(mutation(), 1..4)) {
        let dir = tempfile::tempdir().unwrap();
        let archive = valid_archive(dir.path());
        let service = backup_service(dir.path());
        let mut bytes = archive.bytes.clone();
        let mut database_changed = false;
        for mutation in &mutations {
            if bytes.is_empty() {
                break;
            }
            database_changed |= mutate(&mut bytes, archive.manifest_len, mutation);
        }
        let path = dir.path().join("mutated.llbackup");
        std::fs::write(&path, &bytes).unwrap();
        match service.validate_file("mutated.llbackup", &path) {
            Ok(_) => prop_assert!(!database_changed || bytes == archive.bytes, "{mutations:?}"),
            Err(error) => prop_assert!(
                matches!(
                    error,
                    AppError::BackupCorrupt { .. }
                        | AppError::UnsupportedFormat { .. }
                        | AppError::SchemaTooNew { .. }
                ),
                "{error:?}"
            ),
        }
    }

    /// Arbitrary bytes are never taken for a backup.
    #[test]
    fn random_bytes_are_not_a_backup(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
        let dir = tempfile::tempdir().unwrap();
        let service = backup_service(dir.path());
        let path = dir.path().join("random.llbackup");
        std::fs::write(&path, &bytes).unwrap();
        prop_assert!(service.validate_file("random.llbackup", &path).is_err());
    }
}

#[test]
fn the_unmutated_fuzz_archive_is_valid() {
    let dir = tempfile::tempdir().unwrap();
    let archive = valid_archive(dir.path());
    let service = backup_service(dir.path());
    let path = dir.path().join("valid.llbackup");
    std::fs::write(&path, &archive.bytes).unwrap();
    service.validate_file("valid.llbackup", &path).unwrap();
}

// ---------------------------------------------------------------- content archive

proptest! {
    // Each attempt derives the AES key (2^19 SHA-256 rounds): few cases, still every region.
    #![proptest_config(ProptestConfig::with_cases(24))]

    /// A damaged or foreign content archive fails with an error, never a panic, and never
    /// yields a file name that could leave the target folder.
    #[test]
    fn mutated_content_archives_fail_cleanly(at in any::<usize>(), mask in 1u8..=255, cut in any::<bool>()) {
        let mut files = BTreeMap::new();
        files.insert("decks.json".to_owned(), br#"{"decks":[]}"#.to_vec());
        files.insert("de-basic.json".to_owned(), vec![b'x'; 300]);
        let mut archive = langualinker_content::archive::pack(&files, "fuzz-password").unwrap();
        let index = at % archive.len();
        if cut {
            archive.truncate(index);
        } else {
            archive[index] ^= mask;
        }
        if let Ok(unpacked) = langualinker_content::archive::unpack(&archive, "fuzz-password") {
            for name in unpacked.keys() {
                prop_assert!(!name.contains(['/', '\\']) && name != ".." , "{name}");
            }
        }
    }

    #[test]
    fn random_bytes_are_not_a_content_archive(bytes in proptest::collection::vec(any::<u8>(), 0..256)) {
        prop_assert!(langualinker_content::archive::unpack(&bytes, "fuzz-password").is_err());
    }
}

// ---------------------------------------------------------------- study state machine

#[derive(Debug, Clone)]
enum Step {
    Rate(u8),
    /// The same operation id as the previous rating (a retried request).
    RetryLastRating,
    Undo,
    Pause,
    Resume,
    Wait(u16),
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        6 => (1u8..=4).prop_map(Step::Rate),
        1 => Just(Step::RetryLastRating),
        2 => Just(Step::Undo),
        1 => Just(Step::Pause),
        1 => Just(Step::Resume),
        2 => (1u16..=600).prop_map(Step::Wait),
    ]
}

fn rating(code: u8) -> Rating {
    match code {
        1 => Rating::Again,
        2 => Rating::Hard,
        3 => Rating::Good,
        _ => Rating::Easy,
    }
}

fn seeded_collection(notes: u64) -> (Arc<SqliteDatabase>, DeckId, Vec<CardId>) {
    let db = Arc::new(
        SqliteDatabase::open_in_memory(&TestClock(AtomicI64::new(NOON)))
            .unwrap()
            .database,
    );
    let (deck, cards) = db
        .write(move |tx| {
            let deck = tx
                .insert_deck(
                    &NewDeck {
                        guid: guid(1),
                        name: "Fuzz".into(),
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
            let mut cards = Vec::new();
            for seed in 0..notes {
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
                    cards.push(
                        tx.insert_card(
                            &NewCard {
                                guid: guid(10_000 + seed * 10 + u64::from(template.ordinal)),
                                note_id: note.id,
                                deck_id: deck,
                                template_ordinal: template.ordinal,
                                position,
                            },
                            TimestampMs(0),
                        )?
                        .id,
                    );
                }
            }
            Ok((deck, cards))
        })
        .unwrap();
    (db, deck, cards)
}

fn comparable(mut card: Card) -> Card {
    card.version = 0;
    card.updated_at = TimestampMs(0);
    card.usn = 0;
    card
}

/// Valid (not undone) review logs per card, in card order.
fn valid_logs(db: &SqliteDatabase, cards: &[CardId]) -> Vec<usize> {
    let cards = cards.to_vec();
    db.read(move |tx| {
        cards
            .iter()
            .map(|id| {
                Ok(tx
                    .review_logs(*id)?
                    .iter()
                    .filter(|log| log.invalidated_at.is_none())
                    .count())
            })
            .collect()
    })
    .unwrap()
}

fn all_cards(db: &SqliteDatabase, cards: &[CardId]) -> Vec<Card> {
    let cards = cards.to_vec();
    db.read(move |tx| cards.iter().map(|id| Ok(tx.card(*id)?.unwrap())).collect())
        .unwrap()
}

/// Expected refusals of the state machine: anything else is a bug.
fn expected_refusal(error: &AppError) -> bool {
    matches!(
        error,
        AppError::Validation(_) | AppError::PreviewStale | AppError::VersionConflict { .. }
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Random rating/undo/pause/resume/retry sequences keep the session, the cards and the log
    /// consistent: the session's count equals the valid logs, every card's `reps` equals its
    /// valid logs, an undo restores the exact card state before the rating, and a retried
    /// rating changes nothing.
    #[test]
    fn study_sessions_stay_consistent_under_random_steps(steps in proptest::collection::vec(step(), 1..30)) {
        let (db, deck, cards) = seeded_collection(4);
        let mut now = NOON;
        let input = StartStudy { operation_id: "start".into(), deck_id: Some(deck), card_ids: None };
        let mut view: StudyView = db
            .write(move |tx| study::start_study(tx, &input, TimestampMs(NOON)))
            .unwrap();
        let session = view.session.id;
        let mut op = 0u32;
        let mut last_rating: Option<RateCard> = None;
        // Card states before each rating that has not been undone, newest last.
        let mut undo_stack: Vec<Vec<Card>> = Vec::new();

        for step in &steps {
            op += 1;
            let at = TimestampMs(now);
            let outcome: AppResult<Option<StudyView>> = match step {
                Step::Rate(code) => match view.current.clone() {
                    None => Ok(None),
                    Some(current) => {
                        let before = all_cards(&db, &cards);
                        let preview = db.write(move |tx| {
                            study::preview(tx, session, current.id, current.version, at)
                        });
                        match preview {
                            Err(error) => Err(error),
                            Ok(preview) => {
                                let rate = RateCard {
                                    operation_id: format!("rate-{op}"),
                                    session_id: session,
                                    card_id: preview.card_id,
                                    card_version: preview.card_version,
                                    preview_token: preview.token,
                                    rating: rating(*code),
                                    answer_ms: 3_000,
                                };
                                let sent = rate.clone();
                                let result = db.write(move |tx| study::rate_card(tx, &sent, at));
                                // Only a committed rating is recorded; retrying a refused one
                                // later is a new request.
                                if result.is_ok() {
                                    undo_stack.push(before);
                                    last_rating = Some(rate);
                                }
                                result.map(|outcome| Some(outcome.view))
                            }
                        }
                    }
                },
                Step::RetryLastRating => match last_rating.clone() {
                    None => Ok(None),
                    Some(rate) => {
                        let before = (all_cards(&db, &cards), valid_logs(&db, &cards));
                        let result = db.write(move |tx| study::rate_card(tx, &rate, at));
                        if let Ok(outcome) = &result {
                            prop_assert!(outcome.replayed, "a retry must be answered from the record");
                            prop_assert_eq!(&before.0, &all_cards(&db, &cards));
                            prop_assert_eq!(&before.1, &valid_logs(&db, &cards));
                        }
                        // A retry never moves the session view.
                        result.map(|_| None)
                    }
                },
                Step::Undo => {
                    let undo_op = format!("undo-{op}");
                    let result = db.write(move |tx| study::undo_review(tx, &undo_op, session, at));
                    if result.is_ok() {
                        let before = undo_stack.pop();
                        prop_assert!(before.is_some(), "undo without a rating to undo");
                        let restored: Vec<Card> = all_cards(&db, &cards).into_iter().map(comparable).collect();
                        let expected: Vec<Card> = before.unwrap().into_iter().map(comparable).collect();
                        prop_assert_eq!(restored, expected);
                        // Only the latest rating can be undone; older ones stay committed.
                        undo_stack.clear();
                        last_rating = None;
                    }
                    result.map(|outcome| Some(outcome.view))
                }
                Step::Pause | Step::Resume => {
                    let action = if matches!(step, Step::Pause) { SessionAction::Pause } else { SessionAction::Resume };
                    let change_op = format!("session-{op}");
                    db.write(move |tx| study::change_session(tx, &change_op, session, action, at))
                        .map(Some)
                }
                Step::Wait(minutes) => {
                    now += i64::from(*minutes) * MINUTE;
                    let at = TimestampMs(now);
                    db.write(move |tx| study::study_view(tx, session, at)).map(Some)
                }
            };
            match outcome {
                Ok(Some(next)) => view = next,
                Ok(None) => {}
                Err(error) => prop_assert!(expected_refusal(&error), "{step:?}: {error:?}"),
            }
            // Invariants after every step.
            let logs = valid_logs(&db, &cards);
            let total: usize = logs.iter().sum();
            prop_assert_eq!(view.reviewed_count as usize, total, "after {:?}", step);
            for (card, count) in all_cards(&db, &cards).iter().zip(&logs) {
                prop_assert_eq!(card.reps as usize, *count, "reps of card {:?} after {:?}", card.id, step);
            }
        }
    }
}
