//! CSV import and deck export (plan Phase 14 steps 1–3) on real collection files: the preview
//! writes nothing and predicts the commit; duplicates follow the policy; an export read back by
//! the importer (CSV) or by the Anki reader (`.apkg`, schema 11) keeps GUIDs, text, tags,
//! directions, scheduling and history; a cancel rolls the import back; 20k rows are measured.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use langualinker_application::jobs::{JobEvent, JobId, JobRecord, JobService, JobStatus};
use langualinker_application::ports::{Clock, CollectionScope, EventSink, Rng};
use langualinker_application::repositories::UnitOfWork;
use langualinker_application::study::{self, RateCard, StartStudy};
use langualinker_application::transfer::{
    CsvOptions, CsvOutcome, CsvTarget, DuplicatePolicy, ExportFormat,
};
use langualinker_application::{AppError, AppResult};
use langualinker_domain::card::{CardState, NewCard, QueueState};
use langualinker_domain::deck::{DeckType, NativeLanguage, NewDeck, TargetLanguage};
use langualinker_domain::note::{NewNote, NoteFields, NoteTypeKind};
use langualinker_domain::review::Rating;
use langualinker_domain::{DeckId, Guid, TimestampMs};
use langualinker_infrastructure::sqlite::{DATABASE_FILE, SqliteDatabase};
use langualinker_infrastructure::transfer::{
    ExportOutcome, ExportRequest, TransferEncoders, TransferService,
};
use langualinker_interop::csv::Delimiter;

/// 2026-03-02 12:00 UTC.
const NOON: i64 = 1_772_452_800_000;
const DAY: i64 = 86_400_000;

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

#[derive(Default)]
struct Events(Mutex<Vec<Vec<CollectionScope>>>);

impl EventSink for Events {
    fn job_progress(&self, _: &JobEvent) {}
    fn collection_changed(&self, scopes: &[CollectionScope], _: TimestampMs) {
        self.0.lock().unwrap().push(scopes.to_vec());
    }
}

fn encode_import(outcome: &CsvOutcome) -> String {
    serde_json::json!({
        "deckId": outcome.deck_id.0.to_string(),
        "create": outcome.counts.create,
        "update": outcome.counts.update,
        "skip": outcome.counts.skip,
        "duplicate": outcome.counts.duplicate,
        "error": outcome.counts.error,
    })
    .to_string()
}

fn encode_export(outcome: &ExportOutcome) -> String {
    serde_json::json!({
        "fileName": outcome.file_name,
        "cardCount": outcome.card_count,
        "losses": outcome.losses.iter().map(|(loss, count)| (loss.code(), count)).collect::<Vec<_>>(),
    })
    .to_string()
}

struct Install {
    _dir: tempfile::TempDir,
    dir: PathBuf,
    db: Arc<SqliteDatabase>,
    jobs: JobService<SqliteDatabase>,
    transfer: TransferService,
}

fn install(seed: i64) -> Install {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().to_owned();
    let clock = Arc::new(TestClock(AtomicI64::new(NOON)));
    let db = Arc::new(
        SqliteDatabase::open(&dir.join(DATABASE_FILE), clock.as_ref())
            .unwrap()
            .database,
    );
    let events: Arc<dyn EventSink> = Arc::new(Events::default());
    let jobs = JobService::new(db.clone(), events.clone(), clock.clone());
    let transfer = TransferService::new(
        db.clone(),
        jobs.clone(),
        events,
        clock,
        Arc::new(Counter(AtomicI64::new(seed))),
        dir.join("exports"),
        TransferEncoders {
            import: encode_import,
            export: encode_export,
        },
    );
    Install {
        _dir: temp,
        dir,
        db,
        jobs,
        transfer,
    }
}

fn guid(seed: u64) -> Guid {
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&seed.to_le_bytes());
    bytes[8] = 0x7e;
    Guid::from_bytes(bytes)
}

/// A deck "Niemiecki" with `notes` reversed notes and a subdeck "Czasowniki" with one basic note.
fn content(db: &Arc<SqliteDatabase>, notes: u64) -> DeckId {
    db.write(move |tx| {
        let config = tx.default_config()?.id;
        let new_deck = |name: &str, seed: u64, parent: Option<DeckId>| NewDeck {
            guid: guid(seed),
            name: name.into(),
            parent_id: parent,
            description: String::new(),
            native_language: NativeLanguage::Pl,
            target_language: TargetLanguage::De,
            deck_type: DeckType::Linguistic,
            config_id: config,
            content_version: None,
            bundled: false,
        };
        let deck = tx.insert_deck(&new_deck("Niemiecki", 1, None), TimestampMs(NOON - DAY))?;
        let sub = tx.insert_deck(
            &new_deck("Czasowniki", 2, Some(deck.id)),
            TimestampMs(NOON - DAY),
        )?;
        let mut add = |seed: u64,
                       front: String,
                       back: String,
                       kind: NoteTypeKind,
                       deck_id: DeckId|
         -> AppResult<()> {
            let note_type = tx.note_type(kind)?;
            let note = tx.insert_note(
                &NewNote {
                    guid: guid(100 + seed),
                    note_type_id: note_type.id,
                    fields: NoteFields { front, back },
                    tags: vec!["test".into(), "A1".into()],
                },
                TimestampMs(NOON - DAY),
            )?;
            for template in tx.templates(note_type.id)? {
                let position = tx.next_position()?;
                tx.insert_card(
                    &NewCard {
                        guid: guid(10_000 + seed * 10 + u64::from(template.ordinal)),
                        note_id: note.id,
                        deck_id,
                        template_ordinal: template.ordinal,
                        position,
                    },
                    TimestampMs(NOON - DAY),
                )?;
            }
            Ok(())
        };
        for seed in 0..notes {
            add(
                seed,
                format!("das Wort {seed}"),
                format!("słowo {seed}"),
                NoteTypeKind::BasicAndReversed,
                deck.id,
            )?;
        }
        add(
            9_999,
            "gehen".into(),
            "iść <szybko> & \"daleko\"\nnowa linia".into(),
            NoteTypeKind::Basic,
            sub.id,
        )?;
        Ok(deck.id)
    })
    .unwrap()
}

/// Rates the first `count` cards Good/Again alternately at `at`.
fn study(db: &Arc<SqliteDatabase>, count: usize, at: i64) {
    let input = StartStudy {
        operation_id: format!("start-{at}"),
        deck_id: None,
        card_ids: None,
    };
    let mut view = db
        .write(move |tx| study::start_study(tx, &input, TimestampMs(at)))
        .unwrap();
    for index in 0..count {
        let Some(card) = view.current.clone() else {
            break;
        };
        let session = view.session.id;
        let preview = db
            .read(move |tx| study::preview(tx, session, card.id, card.version, TimestampMs(at)))
            .unwrap();
        let input = RateCard {
            operation_id: format!("rate-{at}-{index}"),
            session_id: session,
            card_id: preview.card_id,
            card_version: preview.card_version,
            preview_token: preview.token,
            rating: if index % 2 == 0 {
                Rating::Good
            } else {
                Rating::Again
            },
            answer_ms: 3_000,
        };
        view = db
            .write(move |tx| study::rate_card(tx, &input, TimestampMs(at)))
            .unwrap()
            .view;
    }
}

fn wait(jobs: &JobService<SqliteDatabase>, job: &JobRecord) -> JobRecord {
    let started = Instant::now();
    loop {
        let current = jobs.get(&job.id).unwrap();
        if current.status.is_finished() {
            return current;
        }
        assert!(
            started.elapsed() < Duration::from_secs(120),
            "job did not finish"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn options(target: Option<DeckId>, policy: DuplicatePolicy) -> CsvOptions {
    CsvOptions {
        delimiter: None,
        encoding: None,
        has_header: true,
        mapping: None,
        duplicate_policy: policy,
        target_deck: target,
        formula_guard: false,
    }
}

fn count(install: &Install, sql: &str) -> i64 {
    let conn = rusqlite::Connection::open(install.dir.join(DATABASE_FILE)).unwrap();
    conn.query_row(sql, [], |row| row.get(0)).unwrap()
}

fn commit(install: &Install, op: &str, token: &str) -> JobRecord {
    let job = install.transfer.commit_csv(op.into(), token).unwrap();
    wait(&install.jobs, &job)
}

fn result(job: &JobRecord) -> serde_json::Value {
    serde_json::from_str(job.result_json.as_deref().unwrap()).unwrap()
}

#[test]
fn preview_writes_nothing_and_predicts_the_commit() {
    let install = install(1);
    let deck = content(&install.db, 3);
    let notes_before = count(&install, "SELECT COUNT(*) FROM notes");
    let fingerprint = install.db.read(|tx| tx.collection_fingerprint()).unwrap();
    let csv = "\u{feff}Front;Back;Tags\r\n\
               das Wort 1;stays;x\r\n\
               \"Straße; mit \"\"Quote\"\"\";\"ulica\nz nową linią\";de::A1 neu\r\n\
               Käse;ser;\r\n\
               ;missing front;\r\n\
               Käse;ser again;\r\n\
               DAS WORT 2 ;case folded;\r\n";
    let preview = install
        .transfer
        .preview_csv(
            "lista.csv",
            csv.as_bytes(),
            options(Some(deck), DuplicatePolicy::Skip),
        )
        .unwrap();
    assert_eq!(
        install.db.read(|tx| tx.collection_fingerprint()).unwrap(),
        fingerprint
    );
    assert_eq!(count(&install, "SELECT COUNT(*) FROM notes"), notes_before);
    assert_eq!(preview.file.delimiter, Delimiter::Semicolon);
    assert!(preview.file.has_bom);
    let counts = preview.plan.counts;
    assert_eq!(
        (
            counts.create,
            counts.update,
            counts.skip,
            counts.duplicate,
            counts.error
        ),
        (2, 0, 3, 0, 1)
    );
    assert_eq!(preview.plan.row_errors[0].line, 6);
    assert_eq!(preview.plan.row_errors[0].error.field, "front");
    assert!(preview.plan.new_decks.is_empty());

    let job = commit(&install, "op-import", &preview.token);
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let result = result(&job);
    assert_eq!(
        (
            result["create"].as_u64(),
            result["skip"].as_u64(),
            result["error"].as_u64()
        ),
        (Some(2), Some(3), Some(1))
    );
    assert_eq!(
        count(&install, "SELECT COUNT(*) FROM notes"),
        notes_before + 2
    );
    let fields: String = rusqlite::Connection::open(install.dir.join(DATABASE_FILE))
        .unwrap()
        .query_row(
            "SELECT fields FROM notes WHERE sort_field LIKE 'straße%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(fields.contains("Straße; mit \\\"Quote\\\""), "{fields}");
    assert!(fields.contains("ulica\\nz nową linią"), "{fields}");
    // The same operation id returns the same job; a second commit of the preview is stale.
    assert_eq!(
        install
            .transfer
            .commit_csv("op-import".into(), &preview.token)
            .unwrap()
            .id,
        job.id
    );
    assert_eq!(
        install
            .transfer
            .commit_csv("op-other".into(), &preview.token)
            .unwrap_err(),
        AppError::PreviewStale
    );
}

#[test]
fn update_policy_changes_text_and_tags_but_not_schedules() {
    let install = install(2);
    let deck = content(&install.db, 2);
    study(&install.db, 4, NOON);
    let before = count(&install, "SELECT SUM(reps) FROM cards");
    let preview = install
        .transfer
        .preview_csv(
            "u.csv",
            "front,back,tags\ndas Wort 0,nowe słowo,nowy\nneu,new,\n".as_bytes(),
            options(Some(deck), DuplicatePolicy::Update),
        )
        .unwrap();
    assert_eq!(
        (preview.plan.counts.update, preview.plan.counts.create),
        (1, 1)
    );
    let job = commit(&install, "op-update", &preview.token);
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    assert_eq!(count(&install, "SELECT SUM(reps) FROM cards"), before);
    assert_eq!(
        count(
            &install,
            "SELECT COUNT(*) FROM notes WHERE fields LIKE '%nowe słowo%'"
        ),
        1
    );
    assert_eq!(
        count(
            &install,
            "SELECT COUNT(*) FROM note_tags nt JOIN tags t ON t.id = nt.tag_id WHERE t.name = 'nowy'"
        ),
        1
    );
}

#[test]
fn a_changed_collection_makes_the_preview_stale() {
    let install = install(3);
    let deck = content(&install.db, 1);
    let preview = install
        .transfer
        .preview_csv(
            "s.csv",
            b"front,back\na,b\n",
            options(Some(deck), DuplicatePolicy::Skip),
        )
        .unwrap();
    content_note(&install.db, deck, "zmiana");
    assert_eq!(
        install
            .transfer
            .commit_csv("op-stale".into(), &preview.token)
            .unwrap_err(),
        AppError::PreviewStale
    );
    assert_eq!(
        install
            .transfer
            .commit_csv("op-unknown".into(), "nope")
            .unwrap_err(),
        AppError::PreviewStale
    );
}

fn content_note(db: &Arc<SqliteDatabase>, deck: DeckId, front: &'static str) {
    db.write(move |tx| {
        let note_type = tx.note_type(NoteTypeKind::Basic)?;
        let note = tx.insert_note(
            &NewNote {
                guid: guid(77_777),
                note_type_id: note_type.id,
                fields: NoteFields {
                    front: front.into(),
                    back: "x".into(),
                },
                tags: Vec::new(),
            },
            TimestampMs(NOON),
        )?;
        let position = tx.next_position()?;
        tx.insert_card(
            &NewCard {
                guid: guid(77_778),
                note_id: note.id,
                deck_id: deck,
                template_ordinal: 0,
                position,
            },
            TimestampMs(NOON),
        )?;
        Ok(())
    })
    .unwrap();
}

fn export(
    install: &Install,
    op: &str,
    deck: DeckId,
    format: ExportFormat,
    scheduling: bool,
    guard: bool,
) -> (JobRecord, PathBuf) {
    let job = install
        .transfer
        .export(
            op.into(),
            ExportRequest {
                deck_id: deck,
                format,
                include_scheduling: scheduling,
                formula_guard: guard,
            },
        )
        .unwrap();
    let job = wait(&install.jobs, &job);
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let (path, _) = install.transfer.export_file(&job.id).unwrap();
    (job, path)
}

#[test]
fn csv_export_imports_back_with_guids_decks_and_scheduling() {
    let source = install(4);
    let deck = content(&source.db, 6);
    study(&source.db, 6, NOON);
    let (job, path) = export(&source, "op-export", deck, ExportFormat::Csv, true, false);
    let result = result(&job);
    assert_eq!(result["fileName"], "Niemiecki.csv");
    assert_eq!(result["cardCount"], 7);
    let losses: Vec<(String, u64)> = serde_json::from_value(result["losses"].clone()).unwrap();
    assert!(
        losses.iter().any(|(code, _)| code == "reviewHistory"),
        "{losses:?}"
    );
    let bytes = std::fs::read(&path).unwrap();
    let text = String::from_utf8(bytes.clone()).unwrap();
    assert!(text.starts_with(
        "\u{feff}guid,front,back,tags,deck,noteType,due,interval,reps,lapses,position\r\n"
    ));
    assert!(text.contains("Niemiecki::Czasowniki"));

    // Into a fresh collection: a new deck named after the file, subdecks from the deck column.
    let target = install(5);
    let preview = target
        .transfer
        .preview_csv(
            "Niemiecki.csv",
            &bytes,
            options(None, DuplicatePolicy::Skip),
        )
        .unwrap();
    assert_eq!(preview.plan.counts.create, 7);
    assert_eq!(
        preview.plan.mapping,
        [
            CsvTarget::Guid,
            CsvTarget::Front,
            CsvTarget::Back,
            CsvTarget::Tags,
            CsvTarget::Deck,
            CsvTarget::NoteType,
            CsvTarget::Due,
            CsvTarget::Interval,
            CsvTarget::Reps,
            CsvTarget::Lapses,
            CsvTarget::Position
        ]
    );
    assert_eq!(
        preview.plan.new_decks,
        ["Niemiecki", "Niemiecki::Czasowniki"]
    );
    let job = commit(&target, "op-reimport", &preview.token);
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);

    // GUIDs, text and tags survive; forward-card due dates and states too.
    let query = "SELECT n.guid, n.fields, c.state, c.due_at, c.scheduled_days, c.reps
                   FROM notes n JOIN cards c ON c.note_id = n.id AND c.template_ordinal = 0
                  ORDER BY n.guid";
    let rows = |dir: &Path| -> Vec<(String, String, i64, Option<i64>, i64, i64)> {
        let conn = rusqlite::Connection::open(dir.join(DATABASE_FILE)).unwrap();
        let mut statement = conn.prepare(query).unwrap();
        statement
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    let before = rows(&source.dir);
    let after = rows(&target.dir);
    assert_eq!(before.len(), after.len());
    for (old, new) in before.iter().zip(&after) {
        assert_eq!((&old.0, &old.1), (&new.0, &new.1));
        if old.2 == CardState::Review.db_code() {
            assert_eq!((new.2, new.3, new.4), (old.2, old.3, old.4), "{}", old.0);
        } else {
            // Learning cards become review cards; new cards stay new.
            assert_eq!(
                new.2 == CardState::New.db_code(),
                old.2 == CardState::New.db_code()
            );
        }
    }
    assert_eq!(
        count(
            &target,
            "SELECT COUNT(*) FROM decks WHERE deleted_at IS NULL"
        ),
        2
    );
    assert_eq!(count(&target, "SELECT COUNT(*) FROM cards"), 13);

    // A second import of the same file matches every row by GUID.
    let again = target
        .transfer
        .preview_csv(
            "Niemiecki.csv",
            &bytes,
            options(None, DuplicatePolicy::Skip),
        )
        .unwrap();
    assert_eq!((again.plan.counts.create, again.plan.counts.skip), (0, 7));
}

#[test]
fn formula_guard_is_explicit_and_reversible() {
    let install = install(6);
    let deck = content(&install.db, 0);
    content_note(&install.db, deck, "=SUM(A1:A3)");
    let (_, raw) = export(&install, "op-raw", deck, ExportFormat::Csv, false, false);
    let raw = String::from_utf8(std::fs::read(raw).unwrap()).unwrap();
    assert!(raw.contains(",=SUM(A1:A3),"), "{raw}");
    let (_, guarded) = export(&install, "op-guarded", deck, ExportFormat::Csv, false, true);
    let guarded = std::fs::read(guarded).unwrap();
    assert!(
        String::from_utf8(guarded.clone())
            .unwrap()
            .contains(",'=SUM(A1:A3),")
    );

    let target = install_with_guard_import(&guarded);
    assert_eq!(target, "=SUM(A1:A3)");
}

fn install_with_guard_import(bytes: &[u8]) -> String {
    let install = install(7);
    let mut options = options(None, DuplicatePolicy::Skip);
    options.formula_guard = true;
    let preview = install
        .transfer
        .preview_csv("g.csv", bytes, options)
        .unwrap();
    let job = commit(&install, "op-guard", &preview.token);
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let conn = rusqlite::Connection::open(install.dir.join(DATABASE_FILE)).unwrap();
    let fields: String = conn
        .query_row(
            "SELECT fields FROM notes WHERE sort_field LIKE '%sum%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    serde_json::from_str::<serde_json::Value>(&fields).unwrap()["front"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn apkg_export_reads_back_as_an_anki_schema_11_collection() {
    let install = install(8);
    let deck = content(&install.db, 5);
    study(&install.db, 8, NOON);
    // One suspended card.
    install
        .db
        .write(|tx| {
            let cards = tx.cards_of_decks(&[DeckId(1)])?;
            let mut card = cards
                .into_iter()
                .find(|card| card.state == CardState::New)
                .unwrap();
            card.queue = QueueState::Suspended;
            tx.update_card(&card, TimestampMs(NOON))?;
            Ok(())
        })
        .unwrap();
    let valid_reviews = count(
        &install,
        "SELECT COUNT(*) FROM review_logs WHERE invalidated_at IS NULL",
    );
    let (job, path) = export(&install, "op-apkg", deck, ExportFormat::Apkg, true, false);
    assert!(path.extension().is_some_and(|ext| ext == "apkg"));
    let result = result(&job);
    assert_eq!(result["fileName"], "Niemiecki.apkg");
    assert_eq!(result["cardCount"], 11);

    let collection = langualinker_anki::read_apkg(&path).unwrap();
    assert_eq!(collection.schema, 11);
    assert_eq!(collection.notes.len(), 6);
    assert_eq!(collection.cards.len(), 11);
    assert_eq!(collection.reviews.len() as i64, valid_reviews);
    let mut names: Vec<&str> = collection
        .decks
        .values()
        .map(|deck| deck.name.as_str())
        .collect();
    names.sort_unstable();
    assert_eq!(names, ["Default", "Niemiecki", "Niemiecki::Czasowniki"]);
    let gehen = collection
        .notes
        .iter()
        .find(|note| note.fields[0] == "gehen")
        .unwrap();
    assert_eq!(
        gehen.fields[1],
        "iść &lt;szybko&gt; &amp; \"daleko\"<br>nowa linia"
    );
    assert_eq!(gehen.tags, ["test", "A1"]);
    assert_eq!(gehen.guid, guid(100 + 9_999).to_string());
    let reversed = collection
        .note_types
        .values()
        .find(|note_type| note_type.templates.len() == 2)
        .unwrap();
    assert_eq!(reversed.fields, ["Front", "Back"]);

    // Card states match the collection, day-based due values are ≥ 0.
    let conn = rusqlite::Connection::open(install.dir.join(DATABASE_FILE)).unwrap();
    let states: Vec<(i64, i64)> = conn
        .prepare("SELECT state, queue FROM cards WHERE deleted_at IS NULL ORDER BY note_id, template_ordinal")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let mut exported: Vec<(i64, i64)> = collection
        .cards
        .iter()
        .map(|card| (card.kind, card.queue))
        .collect();
    let mut expected = states.clone();
    exported.sort_unstable();
    expected.sort_unstable();
    assert_eq!(exported, expected);
    assert!(collection.cards.iter().any(|card| card.queue == -1));
    for card in &collection.cards {
        match card.queue {
            2 | 3 => assert!(card.due >= 0),
            1 => assert!(card.due > 1_000_000_000),
            _ => {}
        }
    }
    let losses: Vec<(String, u64)> = serde_json::from_value(result["losses"].clone()).unwrap();
    assert!(
        losses
            .iter()
            .any(|(code, count)| code == "reviewSnapshots" && *count as i64 == valid_reviews),
        "{losses:?}"
    );
}

#[test]
fn an_unknown_deck_fails_before_the_job_and_an_unknown_export_is_not_found() {
    let install = install(9);
    let error = install
        .transfer
        .export(
            "op-missing".into(),
            ExportRequest {
                deck_id: DeckId(404),
                format: ExportFormat::Csv,
                include_scheduling: false,
                formula_guard: false,
            },
        )
        .unwrap_err();
    assert_eq!(error, AppError::NotFound { entity: "deck" });
    assert_eq!(
        install
            .transfer
            .export_file(&JobId("nope".into()))
            .unwrap_err(),
        AppError::NotFound { entity: "job" }
    );
}

#[test]
fn twenty_thousand_rows_import_export_and_cancel() {
    // The measurement is the release run (`cargo test --release -p langualinker-infrastructure
    // --test transfer twenty`); a debug build (SQLite unoptimised) checks the flow on 2k rows.
    let rows: i64 = if cfg!(debug_assertions) {
        2_000
    } else {
        20_000
    };
    let install = install(10);
    let mut csv = String::from("front,back,tags\n");
    for index in 0..rows {
        csv.push_str(&format!(
            "Wort {index},słowo {index},batch t{}\n",
            index % 50
        ));
    }
    let started = Instant::now();
    let preview = install
        .transfer
        .preview_csv(
            "big.csv",
            csv.as_bytes(),
            options(None, DuplicatePolicy::Skip),
        )
        .unwrap();
    let preview_time = started.elapsed();
    assert_eq!(i64::from(preview.plan.counts.create), rows);

    // Cancel right away: either nothing was written or (if the commit already began) all of it.
    let job = install
        .transfer
        .commit_csv("op-cancel".into(), &preview.token)
        .unwrap();
    install.jobs.cancel(&job.id).unwrap();
    let cancelled = wait(&install.jobs, &job);
    let notes = count(&install, "SELECT COUNT(*) FROM notes");
    match cancelled.status {
        JobStatus::Cancelled => assert_eq!(notes, 0),
        JobStatus::Succeeded => assert_eq!(notes, rows),
        other => panic!("unexpected {other:?} {:?}", cancelled.error),
    }

    let preview = install
        .transfer
        .preview_csv(
            "big.csv",
            csv.as_bytes(),
            options(None, DuplicatePolicy::Skip),
        )
        .unwrap();
    let started = Instant::now();
    let job = commit(&install, "op-big", &preview.token);
    let import_time = started.elapsed();
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    let deck = DeckId(result(&job)["deckId"].as_str().unwrap().parse().unwrap());
    let started = Instant::now();
    let (_, csv_path) = export(&install, "op-big-csv", deck, ExportFormat::Csv, true, false);
    let csv_time = started.elapsed();
    let started = Instant::now();
    let (_, apkg_path) = export(
        &install,
        "op-big-apkg",
        deck,
        ExportFormat::Apkg,
        true,
        false,
    );
    let apkg_time = started.elapsed();
    assert_eq!(
        langualinker_anki::read_apkg(&apkg_path)
            .unwrap()
            .cards
            .len() as i64,
        rows
    );
    eprintln!(
        "{rows} rows: preview {preview_time:?}, import {import_time:?}, CSV export {csv_time:?} ({} KB), APKG export {apkg_time:?} ({} KB)",
        std::fs::metadata(&csv_path).unwrap().len() / 1024,
        std::fs::metadata(&apkg_path).unwrap().len() / 1024,
    );
}
