//! Converter against a synthetic schema-11 collection (every mapping rule, deterministic) and
//! against the real schema-18 export in `tests/fixtures/kmp/apkg` (zstd, FSRS, 5 329 reviews).

use std::io::Write;
use std::path::{Path, PathBuf};

use langualinker_anki::{FieldRef, Options, convert, read_apkg};
use langualinker_content::Package;
use langualinker_content::check::Severity;
use langualinker_content::format::{
    CardStateName, ContentKind, NoteRecord, QueueName, ReviewKindName,
};
use langualinker_domain::deck::{DeckType, TargetLanguage};
use rusqlite::{Connection, params};

const CRT: i64 = 1_600_000_000;
const DAY_MS: i64 = 86_400_000;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/kmp/apkg/sample_german_a1_sentences.apkg")
}

/// Writes a schema-11 collection (`collection.anki21`, as Anki 2.1 exported it) into an .apkg.
fn synthetic_apkg(dir: &Path) -> PathBuf {
    let db = dir.join("collection.anki21");
    let conn = Connection::open(&db).unwrap();
    conn.execute_batch(
        "CREATE TABLE col (id INTEGER PRIMARY KEY, crt INTEGER, mod INTEGER, scm INTEGER, ver INTEGER,
             dty INTEGER, usn INTEGER, ls INTEGER, conf TEXT, models TEXT, decks TEXT, dconf TEXT, tags TEXT);
         CREATE TABLE notes (id INTEGER PRIMARY KEY, guid TEXT, mid INTEGER, mod INTEGER, usn INTEGER,
             tags TEXT, flds TEXT, sfld TEXT, csum INTEGER, flags INTEGER, data TEXT);
         CREATE TABLE cards (id INTEGER PRIMARY KEY, nid INTEGER, did INTEGER, ord INTEGER, mod INTEGER,
             usn INTEGER, type INTEGER, queue INTEGER, due INTEGER, ivl INTEGER, factor INTEGER,
             reps INTEGER, lapses INTEGER, left INTEGER, odue INTEGER, odid INTEGER, flags INTEGER, data TEXT);
         CREATE TABLE revlog (id INTEGER PRIMARY KEY, cid INTEGER, usn INTEGER, ease INTEGER, ivl INTEGER,
             lastIvl INTEGER, factor INTEGER, time INTEGER, type INTEGER);",
    )
    .unwrap();
    let models = serde_json::json!({
        "1": {"name": "Basic (and reversed card)", "type": 0,
              "flds": [{"name": "Front", "ord": 0}, {"name": "Back", "ord": 1}],
              "tmpls": [{"name": "Card 1", "ord": 0}, {"name": "Card 2", "ord": 1}]},
        "2": {"name": "Cloze", "type": 1,
              "flds": [{"name": "Text", "ord": 0}, {"name": "Extra", "ord": 1}],
              "tmpls": [{"name": "Cloze", "ord": 0}]}
    });
    let decks = serde_json::json!({
        "1": {"name": "Default"}, "10": {"name": "Deutsch"}, "11": {"name": "Deutsch::Verben"}, "20": {"name": "Filtered"}
    });
    conn.execute(
        "INSERT INTO col VALUES (1, ?1, 0, 0, 11, 0, 0, 0, '{}', ?2, ?3, '{}', '{}')",
        params![CRT, models.to_string(), decks.to_string()],
    )
    .unwrap();
    let note = |id: i64, guid: &str, mid: i64, tags: &str, front: &str, back: &str| {
        conn.execute(
            "INSERT INTO notes VALUES (?1, ?2, ?3, 0, 0, ?4, ?5, '', 0, 0, '')",
            params![id, guid, mid, tags, format!("{front}\u{1f}{back}")],
        )
        .unwrap();
    };
    note(
        1,
        "g/1!",
        1,
        " noun A1 ",
        "der <b>Hund</b>",
        "pies [sound:hund.mp3]<br>(zwierzę)",
    );
    note(2, "g2", 1, "", "gehen", "iść");
    note(3, "g3", 2, "", "Das {{c1::Haus}}", "");
    note(4, "g4", 1, "", "die Katze", "kot");
    note(5, "g5", 1, "", "<br>", "pusty");
    note(6, "g6", 1, "", "der Baum", "drzewo");
    #[allow(clippy::too_many_arguments)]
    let card = |id: i64,
                nid: i64,
                did: i64,
                ord: i64,
                kind: i64,
                queue: i64,
                due: i64,
                ivl: i64,
                factor: i64,
                reps: i64,
                lapses: i64,
                odue: i64,
                odid: i64,
                flags: i64,
                data: &str| {
        conn.execute(
            "INSERT INTO cards VALUES (?1, ?2, ?3, ?4, 0, 0, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 0, ?12, ?13, ?14, ?15)",
            params![id, nid, did, ord, kind, queue, due, ivl, factor, reps, lapses, odue, odid, flags, data],
        )
        .unwrap();
    };
    card(
        101,
        1,
        10,
        0,
        2,
        2,
        100,
        30,
        2500,
        5,
        1,
        0,
        0,
        1,
        r#"{"s":30.5,"d":5.2}"#,
    );
    card(102, 1, 10, 1, 0, 0, 7, 0, 0, 0, 0, 0, 0, 0, "");
    card(
        201,
        2,
        11,
        0,
        1,
        1,
        1_700_000_000,
        -600,
        2500,
        1,
        0,
        0,
        0,
        0,
        "",
    );
    card(202, 2, 11, 1, 2, -1, 120, 12, 2300, 3, 0, 0, 0, 0, "");
    card(301, 3, 10, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, "");
    // In the filtered deck 20, home deck 10, original due 50.
    card(401, 4, 20, 0, 2, 2, -100_000, 10, 2500, 2, 0, 50, 10, 0, "");
    card(501, 5, 10, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, "");
    card(601, 6, 1, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, "");
    let review = |id: i64, cid: i64, ease: i64, ivl: i64, factor: i64, time: i64, kind: i64| {
        conn.execute(
            "INSERT INTO revlog VALUES (?1, ?2, 0, ?3, ?4, 0, ?5, ?6, ?7)",
            params![id, cid, ease, ivl, factor, time, kind],
        )
        .unwrap();
    };
    review(1_650_000_000_000, 101, 3, -600, 2500, 4_000, 0);
    review(1_650_100_000_000, 101, 0, 0, 2500, 0, 4); // manual reschedule: no rating
    review(1_651_000_000_000, 101, 1, 30, 2300, 8_000, 1);
    drop(conn);

    let apkg = dir.join("deutsch.apkg");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&apkg).unwrap());
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("collection.anki21", stored).unwrap();
    zip.write_all(&std::fs::read(&db).unwrap()).unwrap();
    zip.start_file("media", stored).unwrap();
    zip.write_all(br#"{"0": "hund.mp3"}"#).unwrap();
    zip.finish().unwrap();
    apkg
}

fn options(kind: ContentKind, reverse: bool) -> Options {
    Options {
        name: Some("Niemiecki — moje".into()),
        description: None,
        language: TargetLanguage::De,
        kind,
        reverse,
        deck_type: DeckType::Linguistic,
        level: "A1".into(),
        anki_deck: Some("Deutsch".into()),
        front_field: FieldRef::Index(0),
        back_field: FieldRef::Index(1),
        scheduling: true,
        reviews: true,
        deck_guid: None,
        file_name: "deutsch.apkg".into(),
        today: chrono::NaiveDate::from_ymd_opt(2026, 11, 4).unwrap(),
        strict: false,
    }
}

fn records(package: &[u8]) -> Vec<NoteRecord> {
    let parsed = Package::parse(package).unwrap().unwrap();
    let check = parsed.check();
    let errors: Vec<_> = check
        .issues
        .0
        .iter()
        .filter(|issue| issue.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");
    let text = std::str::from_utf8(package).unwrap();
    text.lines()
        .skip(1)
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn find<'a>(records: &'a [NoteRecord], front: &str) -> &'a NoteRecord {
    records
        .iter()
        .find(|record| record.front == front)
        .unwrap_or_else(|| panic!("no note {front}"))
}

#[test]
fn scheduling_and_history_carry_over_from_a_schema_11_collection() {
    let dir = tempfile::tempdir().unwrap();
    let collection = read_apkg(&synthetic_apkg(dir.path())).unwrap();
    assert_eq!((collection.schema, collection.media_files), (11, 1));
    let conversion = convert(&collection, &options(ContentKind::Words, true)).unwrap();
    let report = &conversion.report;
    assert_eq!(
        report.notes, 3,
        "Hund, gehen, Katze — Baum is in another deck"
    );
    assert_eq!((report.skipped_cloze, report.skipped_empty_front), (1, 1));
    assert_eq!((report.reviews, report.manual_reviews_skipped), (2, 1));
    assert_eq!(report.media_refs_removed, 1);
    assert_eq!(report.media_files_not_converted, 1);
    assert_eq!(conversion.manifest.note_type, "BasicAndReversed");
    assert_eq!(conversion.manifest.expected_cards, 6);
    assert_eq!(conversion.manifest.name, "Niemiecki — moje");

    let records = records(&conversion.package);
    let hund = find(&records, "der Hund");
    assert_eq!(hund.back, "pies\n(zwierzę)");
    assert_eq!(hund.tags, ["noun", "A1"]);
    let review = &hund.cards[0];
    assert_eq!(
        (review.state, review.queue),
        (CardStateName::Review, QueueName::Review)
    );
    assert_eq!(review.due_at_ms, Some(CRT * 1_000 + 100 * DAY_MS));
    assert_eq!(
        (review.stability, review.difficulty),
        (Some(30.5), Some(5.2))
    );
    assert_eq!(
        (review.ease, review.reps, review.lapses, review.flag),
        (Some(2500), 5, 1, 1)
    );
    assert_eq!(review.last_review_at_ms, Some(1_651_000_000_000));
    let reverse = &hund.cards[1];
    assert_eq!(
        (reverse.state, reverse.due_at_ms, reverse.position),
        (CardStateName::New, None, Some(7))
    );
    assert_eq!(hund.reviews.len(), 2);
    assert_eq!(
        (hund.reviews[0].rating, hund.reviews[0].kind),
        (3, ReviewKindName::Learning)
    );
    assert_eq!(
        (hund.reviews[1].rating, hund.reviews[1].scheduled_days),
        (1, 30)
    );

    let gehen = find(&records, "gehen");
    assert_eq!(
        (gehen.cards[0].state, gehen.cards[0].queue),
        (CardStateName::Learning, QueueName::Learning)
    );
    assert_eq!(
        gehen.cards[0].due_at_ms,
        Some(1_700_000_000_000),
        "intraday due is epoch seconds"
    );
    assert_eq!(gehen.cards[0].scheduled_days, 0);
    assert_eq!(
        (gehen.cards[1].state, gehen.cards[1].queue),
        (CardStateName::Review, QueueName::Suspended)
    );

    let katze = find(&records, "die Katze");
    assert_eq!(
        katze.cards[0].due_at_ms,
        Some(CRT * 1_000 + 50 * DAY_MS),
        "filtered deck: original due"
    );
    assert_eq!(katze.cards[0].queue, QueueName::Review);
}

#[test]
fn without_reverse_the_second_cards_are_dropped_and_reported() {
    let dir = tempfile::tempdir().unwrap();
    let collection = read_apkg(&synthetic_apkg(dir.path())).unwrap();
    let conversion = convert(&collection, &options(ContentKind::Words, false)).unwrap();
    assert_eq!(conversion.manifest.note_type, "Basic");
    assert_eq!(conversion.report.dropped_cards, 2);
    assert_eq!(
        conversion.report.dropped_cards_with_progress, 1,
        "the suspended review card"
    );
    assert!(
        records(&conversion.package)
            .iter()
            .all(|record| record.cards.iter().all(|card| card.ordinal == 0))
    );
}

#[test]
fn options_choose_fields_kind_and_what_to_carry() {
    let dir = tempfile::tempdir().unwrap();
    let collection = read_apkg(&synthetic_apkg(dir.path())).unwrap();
    let mut swapped = options(ContentKind::Sentences, false);
    swapped.front_field = FieldRef::Name("back".into());
    swapped.back_field = FieldRef::Name("Front".into());
    swapped.scheduling = false;
    let conversion = convert(&collection, &swapped).unwrap();
    assert_eq!(
        (
            conversion.manifest.note_type.as_str(),
            conversion.manifest.content_kind
        ),
        ("Sentence", ContentKind::Sentences)
    );
    let records = records(&conversion.package);
    assert!(
        records
            .iter()
            .any(|record| record.front == "pies (zwierzę)" && record.back == "der Hund")
    );
    assert!(
        records
            .iter()
            .all(|record| record.cards.is_empty() && record.reviews.is_empty())
    );

    let mut all = options(ContentKind::Words, true);
    all.anki_deck = None;
    assert_eq!(
        convert(&collection, &all).unwrap().report.notes,
        4,
        "every deck"
    );
    all.anki_deck = Some("Nope".into());
    assert!(
        convert(&collection, &all)
            .unwrap_err()
            .contains("no Anki deck")
    );
}

#[test]
fn a_real_schema_18_export_converts_with_fsrs_state_and_history() {
    let collection = read_apkg(&fixture()).unwrap();
    assert_eq!(collection.schema, 18);
    assert_eq!(collection.notes.len(), 432);
    let mut options = options(ContentKind::Sentences, true);
    options.anki_deck = None;
    // The deck has Polish on the front; the German sentence becomes the front.
    options.front_field = FieldRef::Name("Back".into());
    options.back_field = FieldRef::Name("Front".into());
    let conversion = convert(&collection, &options).unwrap();
    let report = &conversion.report;
    assert_eq!(report.notes, 432);
    assert_eq!(report.cards, 864);
    assert_eq!(report.reviews, 5_329);
    assert!(report.fsrs_cards > 800, "{report:?}");
    let records = records(&conversion.package);
    let studied = records
        .iter()
        .flat_map(|record| &record.cards)
        .filter(|card| card.state != CardStateName::New)
        .count();
    assert_eq!(studied as u32, report.cards_with_progress);
    // Under FSRS the revlog factor is a difficulty, not an SM-2 ease.
    assert!(
        records
            .iter()
            .flat_map(|record| &record.reviews)
            .all(|review| review.ease.is_none_or(|ease| ease >= 1_300))
    );
    let before = conversion.package.clone();
    let again = convert(&collection, &options).unwrap();
    assert_eq!(again.package, before, "deterministic for the same date");
}
