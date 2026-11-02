//! Repository, transaction and storage-failure tests (plan Phase 7 DoD). The KMP repository test
//! names (`SqlDelightRepositoryTest`, docs/migration/kmp-manifest.md §4) are the checklist.

use std::time::Instant;

use langualinker_application::jobs::{JobDetail, JobFailure, JobId, JobKind, JobStatus, NewJob};
use langualinker_application::ports::Clock;
use langualinker_application::repositories::{RecordedOperation, Repositories, UnitOfWork};
use langualinker_application::{AppError, AppResult};
use langualinker_domain::card::{CardFlag, CardState, NewCard, QueueState};
use langualinker_domain::content::{
    AssociationProvenance, ModerationStatus, NewAssociation, SentenceWordLink, TipProvenance,
    WordLinkResolution,
};
use langualinker_domain::deck::{Deck, DeckType, NativeLanguage, NewDeck, TargetLanguage};
use langualinker_domain::note::{NewNote, NoteFields, NoteTypeKind};
use langualinker_domain::review::{CardSnapshot, Rating, ReviewKind, ReviewLog};
use langualinker_domain::{
    CardId, DeckConfigId, DeckId, Guid, NoteId, ReviewLogId, TimestampMs, ValidationError,
    WordLinkId,
};

use super::seed::{SEED_EPOCH_MS, SeedSize, seed_collection};
use super::*;

pub struct FixedClock(pub i64);

impl Clock for FixedClock {
    fn now(&self) -> TimestampMs {
        TimestampMs(self.0)
    }
}

const T0: TimestampMs = TimestampMs(1_772_438_400_000);

fn db() -> SqliteDatabase {
    SqliteDatabase::open_in_memory(&FixedClock(T0.0))
        .unwrap()
        .database
}

fn guid(n: u8) -> Guid {
    Guid::from_bytes([n; 16])
}

fn new_deck(n: u8, parent: Option<DeckId>) -> NewDeck {
    NewDeck {
        guid: guid(n),
        name: format!("Deutsch {n}"),
        parent_id: parent,
        description: String::new(),
        native_language: NativeLanguage::Pl,
        target_language: TargetLanguage::De,
        deck_type: DeckType::Linguistic,
        config_id: DeckConfigId(1),
        content_version: None,
        bundled: false,
    }
}

/// Inserts a deck and a note of `kind` with one card per template.
fn note_with_cards(
    tx: &mut dyn Repositories,
    seed: u8,
    kind: NoteTypeKind,
    front: &str,
) -> AppResult<(DeckId, NoteId, Vec<CardId>)> {
    let deck = tx.insert_deck(&new_deck(seed, None), T0)?;
    let note_type = tx.note_type(kind)?;
    let note = tx.insert_note(
        &NewNote {
            guid: guid(seed.wrapping_add(100)),
            note_type_id: note_type.id,
            fields: NoteFields {
                front: front.into(),
                back: "pies".into(),
            },
            tags: vec!["A1".into(), "Tiere".into()],
        },
        T0,
    )?;
    let mut cards = Vec::new();
    for template in tx.templates(note_type.id)? {
        let position = tx.next_position()?;
        let card = tx.insert_card(
            &NewCard {
                guid: guid(seed.wrapping_add(150 + template.ordinal as u8)),
                note_id: note.id,
                deck_id: deck.id,
                template_ordinal: template.ordinal,
                position,
            },
            T0,
        )?;
        cards.push(card.id);
    }
    Ok((deck.id, note.id, cards))
}

#[test]
fn file_database_uses_wal_foreign_keys_and_records_the_collection() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data").join(DATABASE_FILE);
    let opened = SqliteDatabase::open(&path, &FixedClock(T0.0)).unwrap();
    assert_eq!(opened.migration.to_version, 1);
    assert_eq!(opened.database.schema_version(), 1);
    let (mode, fk, created): (String, bool, i64) = opened
        .database
        .with_connection(|conn| {
            let mode = conn
                .pragma_query_value(None, "journal_mode", |r| r.get(0))
                .map_err(map_sqlite)?;
            let fk = conn
                .pragma_query_value(None, "foreign_keys", |r| r.get(0))
                .map_err(map_sqlite)?;
            let created = conn
                .query_row("SELECT created_at FROM collection", [], |r| r.get(0))
                .map_err(map_sqlite)?;
            Ok((mode, fk, created))
        })
        .unwrap();
    assert_eq!((mode.as_str(), fk, created), ("wal", true, T0.0));
    drop(opened);
    // Reopening later keeps the original creation time and migrates nothing.
    let again = SqliteDatabase::open(&path, &FixedClock(T0.0 + 1)).unwrap();
    assert_eq!(again.migration.from_version, 1);
    let created: i64 = again
        .database
        .with_connection(|conn| {
            conn.query_row("SELECT created_at FROM collection", [], |r| r.get(0))
                .map_err(map_sqlite)
        })
        .unwrap();
    assert_eq!(created, T0.0);
}

#[test]
fn built_in_note_types_and_preset_are_seeded() {
    let db = db();
    db.read(|tx| {
        for kind in NoteTypeKind::ALL {
            let note_type = tx.note_type(*kind)?;
            assert_eq!(note_type.guid.as_str(), kind.builtin_guid());
            let directions: Vec<_> = tx
                .templates(note_type.id)?
                .iter()
                .map(|t| t.direction)
                .collect();
            assert_eq!(directions, kind.templates());
        }
        let preset = tx.default_config()?;
        assert!(preset.built_in);
        assert_eq!(
            preset.options,
            langualinker_domain::deck::DeckConfigOptions::default()
        );
        Ok(())
    })
    .unwrap();
}

#[test]
fn deck_round_trip_preserves_guid_text_and_dates() {
    let db = db();
    let deck = db
        .write(|tx| {
            let mut new = new_deck(1, None);
            new.name = "  Niemiecki — „słówka” 🇩🇪  ".into();
            new.description = "Zażółć gęślą jaźń\nzweite Zeile".into();
            tx.insert_deck(&new, T0)
        })
        .unwrap();
    assert_eq!(deck.name, "Niemiecki — „słówka” 🇩🇪");
    assert_eq!(deck.guid, guid(1));
    assert_eq!(
        (deck.created_at, deck.updated_at, deck.version),
        (T0, T0, 1)
    );
    assert!(deck.deck_type.capabilities().word_links);
    let read = db.read(move |tx| tx.deck(deck.id)).unwrap().unwrap();
    assert_eq!(read, deck);
}

#[test]
fn stale_deck_update_conflicts_and_deleted_deck_reports_deleted() {
    let db = db();
    let deck = db
        .write(|tx| tx.insert_deck(&new_deck(1, None), T0))
        .unwrap();
    let mut renamed = deck.clone();
    renamed.name = "Neu".into();
    let updated = {
        let renamed = renamed.clone();
        db.write(move |tx| tx.update_deck(&renamed, T0.plus_ms(5)))
            .unwrap()
    };
    assert_eq!((updated.version, updated.updated_at), (2, T0.plus_ms(5)));
    // `renamed` still carries version 1.
    let stale = db
        .write(move |tx| tx.update_deck(&renamed, T0))
        .unwrap_err();
    assert_eq!(
        stale,
        AppError::VersionConflict {
            entity: "deck",
            current: 2
        }
    );

    let id = deck.id;
    db.write(move |tx| tx.soft_delete_deck(id, 2, T0)).unwrap();
    let after = db
        .write(move |tx| tx.update_deck(&updated, T0))
        .unwrap_err();
    assert_eq!(after, AppError::Deleted { entity: "deck" });
    assert!(db.read(|tx| tx.active_decks()).unwrap().is_empty());
    // The tombstone stays for synchronization.
    assert!(
        db.read(move |tx| tx.deck(id))
            .unwrap()
            .unwrap()
            .deleted_at
            .is_some()
    );
    let missing = db
        .write(|tx| tx.soft_delete_deck(DeckId(999), 1, T0))
        .unwrap_err();
    assert_eq!(missing, AppError::NotFound { entity: "deck" });
}

#[test]
fn deck_hierarchy_rejects_cycles_and_unknown_parents() {
    let db = db();
    let (root, child) = db
        .write(|tx| {
            let root = tx.insert_deck(&new_deck(1, None), T0)?;
            let child = tx.insert_deck(&new_deck(2, Some(root.id)), T0)?;
            Ok((root, child))
        })
        .unwrap();
    let mut moved = root.clone();
    moved.parent_id = Some(child.id);
    let error = db.write(move |tx| tx.update_deck(&moved, T0)).unwrap_err();
    assert_eq!(
        error,
        AppError::Validation(ValidationError::new("parentId", "cycle"))
    );
    let orphan = db
        .write(|tx| tx.insert_deck(&new_deck(3, Some(DeckId(404))), T0))
        .unwrap_err();
    assert_eq!(orphan.code(), "VALIDATION_FAILED");
}

#[test]
fn foreign_keys_reject_dangling_references() {
    let db = db();
    let error = db
        .write(|tx| {
            let mut deck = new_deck(1, None);
            deck.config_id = DeckConfigId(77);
            tx.insert_deck(&deck, T0)
        })
        .unwrap_err();
    assert!(
        matches!(error, AppError::Internal(ref m) if m.contains("FOREIGN KEY")),
        "{error:?}"
    );
    let card = db
        .write(|tx| {
            tx.insert_card(
                &NewCard {
                    guid: guid(9),
                    note_id: NoteId(1),
                    deck_id: DeckId(1),
                    template_ordinal: 0,
                    position: 1,
                },
                T0,
            )
        })
        .unwrap_err();
    assert_eq!(card.code(), "INTERNAL");
}

#[test]
fn note_with_cards_round_trips_and_keeps_one_live_card_per_template() {
    let db = db();
    let (deck, note, cards) = db
        .write(|tx| note_with_cards(tx, 1, NoteTypeKind::BasicAndReversed, " der Hund 🐕 "))
        .unwrap();
    assert_eq!(cards.len(), 2);
    let (stored, stored_cards) = db
        .read(move |tx| Ok((tx.note(note)?.unwrap(), tx.cards_of_note(note)?)))
        .unwrap();
    assert_eq!(stored.fields.front, "der Hund 🐕");
    assert_eq!(stored.sort_field, "der hund 🐕");
    assert_eq!(stored.tags, ["A1", "Tiere"]);
    assert!(stored_cards.iter().all(|card| card.state == CardState::New
        && card.queue == QueueState::New
        && card.due_at.is_none()
        && card.deck_id == deck));
    assert_eq!(stored_cards[1].template_ordinal, 1);

    let duplicate = db
        .write(move |tx| {
            tx.insert_card(
                &NewCard {
                    guid: guid(200),
                    note_id: note,
                    deck_id: deck,
                    template_ordinal: 1,
                    position: 9,
                },
                T0,
            )
        })
        .unwrap_err();
    assert_eq!(
        duplicate,
        AppError::Validation(ValidationError::new("templateOrdinal", "duplicate"))
    );
}

#[test]
fn cards_of_one_note_may_live_in_different_decks() {
    let db = db();
    let (_, note, cards) = db
        .write(|tx| note_with_cards(tx, 1, NoteTypeKind::BasicAndReversed, "Haus"))
        .unwrap();
    let other = db
        .write(|tx| tx.insert_deck(&new_deck(2, None), T0))
        .unwrap();
    let card = cards[1];
    db.write(move |tx| {
        let mut card = tx.card(card)?.unwrap();
        card.deck_id = other.id;
        tx.update_card(&card, T0)
    })
    .unwrap();
    let decks: Vec<_> = db
        .read(move |tx| tx.cards_of_note(note))
        .unwrap()
        .iter()
        .map(|card| card.deck_id)
        .collect();
    assert_eq!(decks.len(), 2);
    assert_ne!(decks[0], decks[1]);
}

#[test]
fn soft_deleting_a_note_tombstones_its_cards_and_frees_the_template_slot() {
    let db = db();
    let (deck, note, _) = db
        .write(|tx| note_with_cards(tx, 1, NoteTypeKind::Basic, "Katze"))
        .unwrap();
    db.write(move |tx| tx.soft_delete_note(note, 1, T0))
        .unwrap();
    assert!(
        db.read(move |tx| tx.cards_of_note(note))
            .unwrap()
            .is_empty()
    );
    let stale = db
        .write(move |tx| tx.soft_delete_note(note, 1, T0))
        .unwrap_err();
    assert_eq!(stale, AppError::Deleted { entity: "note" });
    // A deleted card no longer blocks its (note, ordinal) slot.
    db.write(move |tx| {
        tx.insert_card(
            &NewCard {
                guid: guid(201),
                note_id: note,
                deck_id: deck,
                template_ordinal: 0,
                position: 1,
            },
            T0,
        )
    })
    .unwrap();
}

#[test]
fn new_cards_have_no_due_date_and_scheduled_cards_must_have_one() {
    let db = db();
    let (_, _, cards) = db
        .write(|tx| note_with_cards(tx, 1, NoteTypeKind::Basic, "Baum"))
        .unwrap();
    let card = cards[0];
    let error = db
        .write(move |tx| {
            let mut card = tx.card(card)?.unwrap();
            card.state = CardState::Review;
            card.queue = QueueState::Review;
            tx.update_card(&card, T0)
        })
        .unwrap_err();
    assert!(
        matches!(error, AppError::Internal(ref m) if m.contains("CHECK")),
        "{error:?}"
    );
    let stored = db
        .write(move |tx| {
            let mut card = tx.card(card)?.unwrap();
            card.state = CardState::Review;
            card.queue = QueueState::Review;
            card.due_at = Some(T0.plus_ms(TimestampMs::MS_PER_DAY));
            card.fsrs.stability = Some(2.5);
            card.fsrs.difficulty = Some(5.0);
            card.flag = CardFlag::Green;
            tx.update_card(&card, T0)
        })
        .unwrap();
    assert_eq!(stored.version, 2);
    assert_eq!(stored.flag, CardFlag::Green);
    assert_eq!(stored.fsrs.stability, Some(2.5));
}

fn review_log(
    card: CardId,
    operation: &str,
    before: CardSnapshot,
    after: CardSnapshot,
) -> ReviewLog {
    ReviewLog {
        id: ReviewLogId(0),
        card_id: card,
        session_id: None,
        operation_id: operation.into(),
        rating: Rating::Good,
        kind: ReviewKind::Learning,
        reviewed_at: T0,
        duration_ms: 3_250,
        card_before: before,
        card_after: after,
        fsrs_version: "FSRS-6".into(),
        parameters_hash: "abc".into(),
        invalidated_at: None,
    }
}

#[test]
fn review_logs_keep_full_snapshots_and_unique_operation_ids() {
    let db = db();
    let (_, _, cards) = db
        .write(|tx| note_with_cards(tx, 1, NoteTypeKind::Basic, "Brot"))
        .unwrap();
    let card_id = cards[0];
    let (before, after) = db
        .write(move |tx| {
            let card = tx.card(card_id)?.unwrap();
            let before = CardSnapshot::of(&card);
            let mut reviewed = card.clone();
            reviewed.state = CardState::Learning;
            reviewed.queue = QueueState::Learning;
            reviewed.due_at = Some(T0.plus_ms(600_000));
            reviewed.learning_step = Some(1);
            reviewed.reps = 1;
            let reviewed = tx.update_card(&reviewed, T0)?;
            let after = CardSnapshot::of(&reviewed);
            tx.insert_review_log(&review_log(card_id, "op-1", before.clone(), after.clone()))?;
            Ok((before, after))
        })
        .unwrap();
    let logs = db.read(move |tx| tx.review_logs(card_id)).unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(
        (logs[0].card_before.clone(), logs[0].card_after.clone()),
        (before.clone(), after.clone())
    );

    let duplicate = db
        .write(move |tx| tx.insert_review_log(&review_log(card_id, "op-1", before, after)))
        .unwrap_err();
    assert_eq!(
        duplicate,
        AppError::Validation(ValidationError::new("operationId", "duplicate"))
    );

    db.write(|tx| tx.invalidate_review_log("op-1", T0.plus_ms(1)))
        .unwrap();
    let log = db
        .read(|tx| tx.review_log_by_operation("op-1"))
        .unwrap()
        .unwrap();
    assert_eq!(log.invalidated_at, Some(T0.plus_ms(1)));
    let twice = db
        .write(|tx| tx.invalidate_review_log("op-1", T0))
        .unwrap_err();
    assert_eq!(twice, AppError::NotFound { entity: "review" });
}

fn association(note: NoteId, n: u8, text: &str) -> NewAssociation {
    NewAssociation {
        guid: guid(n),
        note_id: note,
        text: text.into(),
        provenance: AssociationProvenance::Manual,
        native_language: "pl".into(),
        target_language: "de".into(),
        prompt_version: None,
        model: None,
        moderation: ModerationStatus::NotRequired,
        cache_key: None,
    }
}

#[test]
fn one_favorite_per_note_also_after_the_previous_favorite_was_deleted() {
    let db = db();
    let (_, note, _) = db
        .write(|tx| note_with_cards(tx, 1, NoteTypeKind::Basic, "Hund"))
        .unwrap();
    let (a, b) = db
        .write(move |tx| {
            let a = tx.insert_association(&association(note, 50, "Hund — hunt"), T0)?;
            let b = tx.insert_association(&association(note, 51, "Hund — hood"), T0)?;
            Ok((a.id, b.id))
        })
        .unwrap();
    db.write(move |tx| tx.set_favorite_association(note, Some(a), T0))
        .unwrap();
    db.write(move |tx| tx.set_favorite_association(note, Some(b), T0))
        .unwrap();
    let favorites = |db: &SqliteDatabase| {
        db.read(move |tx| tx.associations(note))
            .unwrap()
            .iter()
            .filter(|a| a.is_favorite)
            .map(|a| a.id)
            .collect::<Vec<_>>()
    };
    assert_eq!(favorites(&db), [b]);

    // Deleting the favorite keeps its row (and flag) as a tombstone; a new favorite is allowed.
    db.write(move |tx| tx.soft_delete_association(b, T0))
        .unwrap();
    db.write(move |tx| tx.set_favorite_association(note, Some(a), T0))
        .unwrap();
    assert_eq!(favorites(&db), [a]);

    // The partial unique index is the last line of defence.
    db.write(move |tx| tx.insert_association(&association(note, 52, "Hund — hound"), T0))
        .unwrap();
    let raw = db
        .with_connection(move |conn| {
            conn.execute(
                "UPDATE associations SET is_favorite = 1 WHERE note_id = ?1 AND deleted_at IS NULL",
                [note.0],
            )
            .map_err(map_sqlite)
        })
        .unwrap_err();
    assert!(
        matches!(raw, AppError::Internal(ref m) if m.contains("UNIQUE")),
        "{raw:?}"
    );
    let foreign = db
        .write(move |tx| tx.set_favorite_association(NoteId(999), Some(a), T0))
        .unwrap_err();
    assert_eq!(
        foreign,
        AppError::NotFound {
            entity: "association"
        }
    );
}

#[test]
fn associations_and_tips_keep_insertion_order() {
    let db = db();
    let (_, note, _) = db
        .write(|tx| note_with_cards(tx, 1, NoteTypeKind::Sentence, "Ich gehe nach Hause."))
        .unwrap();
    let (associations, tips) = db
        .write(move |tx| {
            for (n, text) in [(60, "pierwsza"), (61, "druga")] {
                tx.insert_association(&association(note, n, text), T0)?;
            }
            tx.insert_grammar_tip(
                note,
                &guid(70),
                "nach Hause = do domu",
                TipProvenance::Bundled,
                T0,
            )?;
            tx.insert_grammar_tip(
                note,
                &guid(71),
                "gehen → ging → gegangen",
                TipProvenance::Manual,
                T0,
            )?;
            Ok((tx.associations(note)?, tx.grammar_tips(note)?))
        })
        .unwrap();
    let texts: Vec<_> = associations.iter().map(|a| a.text.as_str()).collect();
    assert_eq!(texts, ["pierwsza", "druga"]);
    assert_eq!(
        tips.iter().map(|t| t.sort_order).collect::<Vec<_>>(),
        [0, 1]
    );
    assert_eq!(tips[1].text, "gehen → ging → gegangen");
}

#[test]
fn word_links_become_unresolved_when_their_word_goes_away() {
    let db = db();
    let (sentence, word) = db
        .write(|tx| {
            let (_, sentence, _) =
                note_with_cards(tx, 1, NoteTypeKind::Sentence, "Der Hund schläft.")?;
            let (_, word, _) = note_with_cards(tx, 2, NoteTypeKind::Basic, "der Hund")?;
            Ok((sentence, word))
        })
        .unwrap();
    let link = SentenceWordLink {
        id: WordLinkId(0),
        sentence_note_id: sentence,
        word_note_id: Some(word),
        preferred_word_card_id: None,
        surface_form: "Hund".into(),
        lemma: Some("Hund".into()),
        start_index: 4,
        end_index: 8,
        resolution: WordLinkResolution::Resolved,
        created_at: T0,
        updated_at: T0,
    };
    let stored = db
        .write(move |tx| tx.replace_word_links(sentence, &[link], T0))
        .unwrap();
    assert_eq!(stored[0].word_note_id, Some(word));

    let changed = db
        .write(move |tx| {
            tx.soft_delete_note(word, 1, T0)?;
            tx.unresolve_links_to(word, T0)
        })
        .unwrap();
    assert_eq!(changed, 1);
    let links = db.read(move |tx| tx.word_links(sentence)).unwrap();
    assert_eq!(links.len(), 1, "the link itself is kept");
    assert_eq!(links[0].resolution, WordLinkResolution::Unresolved);
    assert_eq!(links[0].word_note_id, None);

    let mut bad = links[0].clone();
    bad.resolution = WordLinkResolution::Resolved;
    let error = db
        .write(move |tx| tx.replace_word_links(sentence, &[bad], T0))
        .unwrap_err();
    assert_eq!(error.code(), "VALIDATION_FAILED");
}

#[test]
fn full_text_index_follows_note_edits() {
    let db = db();
    let (_, note, _) = db
        .write(|tx| note_with_cards(tx, 1, NoteTypeKind::Basic, "Kühlschrank"))
        .unwrap();
    let hits = |db: &SqliteDatabase, term: &'static str| {
        db.with_connection(move |conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM notes_fts WHERE notes_fts MATCH ?1",
                [term],
                |row| row.get::<_, i64>(0),
            )
            .map_err(map_sqlite)
        })
        .unwrap()
    };
    // remove_diacritics folds ü, so the search works without the umlaut too.
    assert_eq!(hits(&db, "kuhlschrank"), 1);
    db.write(move |tx| {
        let mut stored = tx.note(note)?.unwrap();
        stored.fields.front = "Gefrierschrank".into();
        tx.update_note(&stored, T0)
    })
    .unwrap();
    assert_eq!(hits(&db, "kuhlschrank"), 0);
    assert_eq!(hits(&db, "gefrierschrank"), 1);
}

#[test]
fn settings_store_json_only() {
    let db = db();
    db.write(|tx| tx.put_setting("ui.language", "\"pl\"", T0))
        .unwrap();
    db.write(|tx| tx.put_setting("ui.language", "\"en\"", T0))
        .unwrap();
    assert_eq!(
        db.read(|tx| tx.setting("ui.language")).unwrap().as_deref(),
        Some("\"en\"")
    );
    let error = db
        .write(|tx| tx.put_setting("x", "not json", T0))
        .unwrap_err();
    assert_eq!(error.code(), "VALIDATION_FAILED");
}

#[test]
fn job_sequence_only_moves_forward() {
    let db = db();
    let job = db
        .write(|tx| {
            tx.insert_job(
                &NewJob {
                    kind: JobKind::Import,
                    operation_id: Some("op-job".into()),
                    total: Some(4),
                },
                T0,
            )
        })
        .unwrap();
    assert_eq!((job.status, job.sequence), (JobStatus::Queued, 0));
    assert_eq!(job.progress.map(|p| (p.done, p.total)), Some((0, 4)));
    let mut failed = job.clone();
    failed.status = JobStatus::Failed;
    failed.sequence = 1;
    failed.finished_at = Some(T0);
    failed.error = Some(JobFailure {
        code: "STORAGE_FULL".into(),
        details: vec![("bytes".into(), JobDetail::Number(42))],
    });
    {
        let failed = failed.clone();
        db.write(move |tx| tx.update_job(&failed)).unwrap();
    }
    let id = job.id.clone();
    assert_eq!(db.read(move |tx| tx.job(&id)).unwrap().unwrap(), failed);
    let stale = db.write(move |tx| tx.update_job(&failed)).unwrap_err();
    assert!(matches!(stale, AppError::Internal(ref m) if m.contains("stale")));
    assert!(db.read(|tx| tx.unfinished_jobs()).unwrap().is_empty());
    assert!(
        db.read(|tx| tx.job(&JobId("nope".into())))
            .unwrap()
            .is_none()
    );
}

#[test]
fn operations_are_recorded_once() {
    let db = db();
    let recorded = RecordedOperation {
        operation: "createDeck".into(),
        result_json: "{\"id\":\"1\"}".into(),
    };
    {
        let recorded = recorded.clone();
        db.write(move |tx| tx.record_operation("op-1", &recorded, T0))
            .unwrap();
    }
    assert_eq!(
        db.read(|tx| tx.recorded_operation("op-1")).unwrap(),
        Some(recorded.clone())
    );
    let again = db
        .write(move |tx| tx.record_operation("op-1", &recorded, T0))
        .unwrap_err();
    assert_eq!(again.code(), "VALIDATION_FAILED");
}

#[test]
fn failed_work_rolls_back_the_whole_transaction() {
    let db = db();
    let error = db
        .write(|tx| {
            tx.insert_deck(&new_deck(1, None), T0)?;
            Err::<(), _>(AppError::Cancelled)
        })
        .unwrap_err();
    assert_eq!(error, AppError::Cancelled);
    assert!(db.read(|tx| tx.active_decks()).unwrap().is_empty());
}

#[test]
fn a_full_disk_fails_with_storage_full_and_keeps_committed_data() {
    let db = db();
    db.write(|tx| tx.insert_deck(&new_deck(1, None), T0))
        .unwrap();
    // Cap the database at its current size plus a few pages, then overflow it.
    db.with_connection(|conn| {
        let pages: i64 = conn
            .query_row("PRAGMA page_count", [], |r| r.get(0))
            .map_err(map_sqlite)?;
        conn.pragma_update(None, "max_page_count", pages + 2)
            .map_err(map_sqlite)
    })
    .unwrap();
    let error = db
        .write(|tx| {
            let note_type = tx.note_type(NoteTypeKind::Basic)?;
            for n in 0..2_000u32 {
                let mut bytes = [0u8; 16];
                bytes[..4].copy_from_slice(&n.to_be_bytes());
                tx.insert_note(
                    &NewNote {
                        guid: Guid::from_bytes(bytes),
                        note_type_id: note_type.id,
                        fields: NoteFields {
                            front: format!("Wort {n} {}", "x".repeat(200)),
                            back: "y".repeat(200),
                        },
                        tags: Vec::new(),
                    },
                    T0,
                )?;
            }
            Ok(())
        })
        .unwrap_err();
    assert_eq!(error, AppError::StorageFull);
    let decks = db.read(|tx| tx.active_decks()).unwrap();
    assert_eq!(decks.len(), 1);
    assert_eq!(
        db.read(|tx| tx.note_by_guid(&Guid::from_bytes([0; 16])))
            .unwrap(),
        None
    );
}

#[test]
fn uncommitted_work_is_gone_after_reopening_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(DATABASE_FILE);
    {
        let opened = SqliteDatabase::open(&path, &FixedClock(T0.0)).unwrap();
        opened
            .database
            .write(|tx| tx.insert_deck(&new_deck(1, None), T0))
            .unwrap();
        // Simulates a process stopping mid-transaction: the transaction is never committed.
        opened
            .database
            .with_connection(|conn| {
                conn.execute_batch("BEGIN IMMEDIATE; DELETE FROM deck_settings; DELETE FROM decks;")
                    .map_err(map_sqlite)
            })
            .unwrap();
    }
    let reopened = SqliteDatabase::open(&path, &FixedClock(T0.0))
        .unwrap()
        .database;
    assert_eq!(reopened.read(|tx| tx.active_decks()).unwrap().len(), 1);
}

#[test]
fn integer_ids_beyond_javascript_precision_survive_storage() {
    let db = db();
    let big: i64 = 9_007_199_254_740_993;
    let deck = db
        .write(move |tx| {
            let deck = tx.insert_deck(&new_deck(1, None), T0)?;
            Ok(deck)
        })
        .unwrap();
    db.with_connection(move |conn| {
        conn.execute(
            "UPDATE deck_settings SET deck_id = ?1 WHERE deck_id = ?2",
            [big, deck.id.0],
        )
        .and_then(|_| conn.execute("UPDATE decks SET id = ?1 WHERE id = ?2", [big, deck.id.0]))
        .map_err(map_sqlite)
    })
    .unwrap_err(); // FK from deck_settings blocks the first update — use deferred order instead.
    db.with_connection(move |conn| {
        conn.execute_batch(&format!(
            "PRAGMA foreign_keys = OFF;
             UPDATE decks SET id = {big} WHERE id = {old};
             UPDATE deck_settings SET deck_id = {big} WHERE deck_id = {old};
             PRAGMA foreign_keys = ON;",
            old = deck.id.0
        ))
        .map_err(map_sqlite)
    })
    .unwrap();
    let stored: Deck = db.read(move |tx| tx.deck(DeckId(big))).unwrap().unwrap();
    assert_eq!(stored.id.to_string(), "9007199254740993");
}

#[test]
fn due_query_uses_the_deck_queue_due_index() {
    let db = db();
    db.with_connection(|conn| {
        seed_collection(
            conn,
            SeedSize {
                decks: 3,
                cards: 600,
                logs_per_card: 1,
            },
        )
    })
    .unwrap();
    let plan: Vec<String> = db
        .with_connection(|conn| {
            let mut statement = conn
                .prepare(
                    "EXPLAIN QUERY PLAN SELECT id FROM cards
                     WHERE deck_id IN (?, ?) AND queue IN (2, 3) AND due_at <= ?
                       AND deleted_at IS NULL AND archived_at IS NULL
                     ORDER BY due_at, id LIMIT ?",
                )
                .map_err(map_sqlite)?;
            statement
                .query_map(rusqlite::params![1, 2, SEED_EPOCH_MS, 200], |row| {
                    row.get::<_, String>(3)
                })
                .and_then(Iterator::collect)
                .map_err(map_sqlite)
        })
        .unwrap();
    assert!(
        plan.iter()
            .any(|step| step.contains("idx_cards_deck_queue_due")),
        "{plan:?}"
    );
    let due = db
        .read(|tx| {
            tx.due_card_ids(
                &[DeckId(1), DeckId(2), DeckId(3)],
                TimestampMs(SEED_EPOCH_MS),
                1_000,
            )
        })
        .unwrap();
    assert!(!due.is_empty());
    let new = db.read(|tx| tx.new_card_ids(&[DeckId(1)], 10)).unwrap();
    assert_eq!(new.len(), 10);
}

/// Plan step 25 on the full 50 000-card / 500 000-log collection. Ignored in the default run
/// (seeding takes a while in debug builds); run with
/// `cargo test -p langualinker-infrastructure --release -- --ignored due_query_budget`.
#[test]
#[ignore]
fn due_query_budget_on_the_plan_collection() {
    let db = db();
    let seeded = Instant::now();
    db.with_connection(|conn| seed_collection(conn, SeedSize::PLAN))
        .unwrap();
    eprintln!("seeded in {:?}", seeded.elapsed());
    let decks: Vec<DeckId> = (1..=SeedSize::PLAN.decks).map(DeckId).collect();
    let mut worst = std::time::Duration::ZERO;
    for _ in 0..20 {
        let decks = decks.clone();
        let started = Instant::now();
        let due = db
            .read(move |tx| tx.due_card_ids(&decks, TimestampMs(SEED_EPOCH_MS), 200))
            .unwrap();
        worst = worst.max(started.elapsed());
        assert_eq!(due.len(), 200);
    }
    eprintln!("worst due query: {worst:?}");
    assert!(worst.as_millis() < 50, "due query took {worst:?}");
}
