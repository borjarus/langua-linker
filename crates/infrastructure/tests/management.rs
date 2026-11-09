//! Content management (plan Phase 11) on the real SQLite unit of work: notes and card generation,
//! note-type changes that archive and restore cards, word links and the word search, note
//! content edits, the deck hierarchy, presets, and the card browser with keyset paging, the
//! search language and atomic bulk actions.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use langualinker_application::browser::{self, BulkAction, BulkSelection, CardSearch, SortName};
use langualinker_application::management::{
    self, CreateDeck, DeckPatch, LinkDraft, NoteContentChange, PresetConfig, SaveNote, SavePreset,
};
use langualinker_application::ports::{Clock, Rng};
use langualinker_application::repositories::{BundledNote, Repositories, UnitOfWork};
use langualinker_application::{AppError, AppResult};
use langualinker_domain::card::{CardState, QueueState};
use langualinker_domain::content::{TipProvenance, WordLinkResolution};
use langualinker_domain::deck::{DeckType, TargetLanguage};
use langualinker_domain::note::NoteTypeKind;
use langualinker_domain::{CardId, DeckConfigId, DeckId, Guid, NoteId, TimestampMs};
use langualinker_infrastructure::sqlite::SqliteDatabase;

/// 2026-03-02 12:00 UTC.
const NOON: i64 = 1_772_452_800_000;
const DAY: i64 = 86_400_000;

struct Fixed;

impl Clock for Fixed {
    fn now(&self) -> TimestampMs {
        TimestampMs(NOON)
    }
}

/// Deterministic GUIDs.
struct Sequence(AtomicU64);

impl Rng for Sequence {
    fn fill(&self, bytes: &mut [u8]) {
        let value = self.0.fetch_add(1, Ordering::SeqCst) + 1;
        bytes.fill(0);
        bytes[..8].copy_from_slice(&value.to_le_bytes());
    }
}

fn rng() -> Arc<Sequence> {
    Arc::new(Sequence(AtomicU64::new(0)))
}

fn db() -> Arc<SqliteDatabase> {
    Arc::new(SqliteDatabase::open_in_memory(&Fixed).unwrap().database)
}

fn write<T: Send + 'static>(
    db: &Arc<SqliteDatabase>,
    work: impl FnOnce(&mut dyn Repositories) -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    db.write(work)
}

fn read<T: Send + 'static>(
    db: &Arc<SqliteDatabase>,
    work: impl FnOnce(&mut dyn Repositories) -> AppResult<T> + Send + 'static,
) -> T {
    db.read(work).unwrap()
}

fn now() -> TimestampMs {
    TimestampMs(NOON)
}

fn create_deck(
    db: &Arc<SqliteDatabase>,
    rng: &Arc<Sequence>,
    op: &str,
    name: &str,
    parent: Option<DeckId>,
) -> DeckId {
    let rng = rng.clone();
    let input = CreateDeck {
        operation_id: op.into(),
        name: name.into(),
        parent_id: parent,
        target_language: TargetLanguage::De,
        deck_type: DeckType::Linguistic,
    };
    write(db, move |tx| {
        management::create_deck(tx, rng.as_ref(), &input, now())
    })
    .unwrap()
}

fn note_input(op: &str, deck: DeckId, kind: NoteTypeKind, front: &str, back: &str) -> SaveNote {
    SaveNote {
        operation_id: op.into(),
        note_id: None,
        expected_version: None,
        deck_id: deck,
        kind,
        front: front.into(),
        back: back.into(),
        tags: Vec::new(),
        manual_association: None,
        grammar_tips: Vec::new(),
        word_links: None,
    }
}

fn save(db: &Arc<SqliteDatabase>, rng: &Arc<Sequence>, input: SaveNote) -> AppResult<NoteId> {
    let rng = rng.clone();
    write(db, move |tx| {
        management::save_note(tx, rng.as_ref(), &input, now())
    })
}

fn details(db: &Arc<SqliteDatabase>, note: NoteId) -> management::NoteDetails {
    read(db, move |tx| management::note_details(tx, note, now()))
}

fn validation_reason(error: AppError) -> &'static str {
    match error {
        AppError::Validation(error) => error.reason,
        other => panic!("expected a validation error, got {other:?}"),
    }
}

// ---------------------------------------------------------------- notes and cards

/// DoD: a reverse note gets two cards with independent schedules; text edits keep them; a type
/// change archives the reverse card with its history and switching back restores the same card.
#[test]
fn a_reverse_note_has_two_independent_cards_that_survive_edits_and_type_changes() {
    let (db, rng) = (db(), rng());
    let deck = create_deck(&db, &rng, "deck", "Niemiecki", None);
    let note = save(
        &db,
        &rng,
        note_input(
            "n1",
            deck,
            NoteTypeKind::BasicAndReversed,
            "der Hund",
            "pies",
        ),
    )
    .unwrap();
    let shown = details(&db, note);
    assert_eq!(shown.cards.len(), 2);
    let (forward, reverse) = (shown.cards[0].card.id, shown.cards[1].card.id);
    // Only the forward card is studied.
    write(&db, move |tx| {
        let mut card = tx.card(forward)?.unwrap();
        card.state = CardState::Review;
        card.queue = QueueState::Review;
        card.due_at = Some(TimestampMs(NOON + 5 * DAY));
        card.reps = 3;
        tx.update_card(&card, now())?;
        Ok(())
    })
    .unwrap();
    let version = shown.note.version;
    let mut edit = note_input(
        "n1-edit",
        deck,
        NoteTypeKind::BasicAndReversed,
        "der Hund ",
        "pies (zwierzę)",
    );
    edit.note_id = Some(note);
    edit.expected_version = Some(version);
    save(&db, &rng, edit.clone()).unwrap();
    let edited = details(&db, note);
    assert_eq!(edited.note.fields.back, "pies (zwierzę)");
    assert_eq!(edited.cards[0].card.state, CardState::Review);
    assert_eq!(edited.cards[0].card.reps, 3);
    assert_eq!(edited.cards[1].card.state, CardState::New);

    // A stale version is a conflict, not a silent overwrite.
    let mut stale = edit.clone();
    stale.operation_id = "n1-stale".into();
    let error = save(&db, &rng, stale).unwrap_err();
    assert!(
        matches!(error, AppError::VersionConflict { entity: "note", .. }),
        "{error:?}"
    );

    // Basic drops the reverse template: its card is archived, not deleted.
    let mut basic = edit.clone();
    basic.operation_id = "n1-basic".into();
    basic.kind = NoteTypeKind::Basic;
    basic.expected_version = Some(edited.note.version);
    save(&db, &rng, basic).unwrap();
    let archived = details(&db, note);
    assert_eq!(archived.cards.len(), 1);
    assert_eq!(archived.archived_ordinals, [1]);
    assert_eq!(archived.kind, NoteTypeKind::Basic);

    let mut back = edit;
    back.operation_id = "n1-back".into();
    back.expected_version = Some(archived.note.version);
    save(&db, &rng, back).unwrap();
    let restored = details(&db, note);
    assert_eq!(restored.cards.len(), 2);
    assert!(restored.archived_ordinals.is_empty());
    assert_eq!(
        restored.cards[1].card.id, reverse,
        "the archived card comes back"
    );
    assert_eq!(restored.cards[0].card.id, forward);
    assert_eq!(restored.cards[0].card.reps, 3);
}

#[test]
fn saving_is_idempotent_and_validates_its_input() {
    let (db, rng) = (db(), rng());
    let deck = create_deck(&db, &rng, "deck", "Niemiecki", None);
    let input = note_input("same", deck, NoteTypeKind::Basic, "die Katze", "kot");
    let first = save(&db, &rng, input.clone()).unwrap();
    let again = save(&db, &rng, input).unwrap();
    assert_eq!(first, again);
    let cards = read(&db, move |tx| tx.cards_of_note(first));
    assert_eq!(cards.len(), 1, "a retry creates nothing");

    let empty_back = note_input("bad-1", deck, NoteTypeKind::Basic, "x", "  ");
    assert_eq!(
        validation_reason(save(&db, &rng, empty_back).unwrap_err()),
        "required"
    );
    let mut tags = note_input("bad-2", deck, NoteTypeKind::Basic, "x", "y");
    tags.tags = vec!["two words".into()];
    assert_eq!(
        validation_reason(save(&db, &rng, tags).unwrap_err()),
        "whitespace"
    );
    let mut tip = note_input("bad-3", deck, NoteTypeKind::Basic, "x", "y");
    tip.grammar_tips = vec!["t".repeat(501)];
    assert_eq!(
        validation_reason(save(&db, &rng, tip).unwrap_err()),
        "tooLong"
    );
    let missing_deck = note_input("bad-4", DeckId(999), NoteTypeKind::Basic, "x", "y");
    assert_eq!(
        validation_reason(save(&db, &rng, missing_deck).unwrap_err()),
        "notFound"
    );
}

/// Duplicates warn (homographs stay valid); the word matcher offers exact, then normalized
/// matches of the deck language and never guesses.
#[test]
fn duplicates_warn_and_the_word_search_never_guesses() {
    let (db, rng) = (db(), rng());
    let deck = create_deck(&db, &rng, "deck", "Niemiecki", None);
    let child = create_deck(&db, &rng, "child", "Jedzenie", Some(deck));
    save(
        &db,
        &rng,
        note_input("tee", child, NoteTypeKind::Basic, "der Tee", "herbata"),
    )
    .unwrap();
    save(
        &db,
        &rng,
        note_input(
            "bank-1",
            deck,
            NoteTypeKind::Basic,
            "die Bank (Sitz)",
            "ławka",
        ),
    )
    .unwrap();
    save(
        &db,
        &rng,
        note_input(
            "bank-2",
            deck,
            NoteTypeKind::Basic,
            "die Bank (Geld)",
            "bank",
        ),
    )
    .unwrap();
    save(
        &db,
        &rng,
        note_input("bruecke", deck, NoteTypeKind::Basic, "die Brücke", "most"),
    )
    .unwrap();

    let found = read(&db, move |tx| {
        management::duplicates(tx, deck, " DER tee", None)
    });
    assert_eq!(found.len(), 1, "subdecks are searched");
    assert_eq!(found[0].1, "Jedzenie");

    let search = |text: &'static str| {
        read(&db, move |tx| {
            management::search_words(tx, TargetLanguage::De, text, 10)
        })
    };
    let tee = search("Tee");
    assert_eq!(
        (tee.len(), tee[0].exact, tee[0].lemma.as_str()),
        (1, true, "der Tee")
    );
    let bank = search("bank");
    assert_eq!(bank.len(), 2, "both senses are offered, none is picked");
    assert!(bank.iter().all(|found| !found.exact));
    assert_eq!(search("Brucke")[0].lemma, "die Brücke");
    assert!(search("Haus").is_empty());
    assert!(
        read(&db, |tx| management::search_words(
            tx,
            TargetLanguage::En,
            "Tee",
            10
        ))
        .is_empty()
    );
    let error = db
        .read(|tx| management::search_words(tx, TargetLanguage::De, "Tee", 0))
        .unwrap_err();
    assert_eq!(validation_reason(error), "outOfRange");
}

/// Word links: drafts are checked against the front, a user choice is `UserOverridden`, an
/// edited sentence remaps or drops its links, and a deleted word leaves the link unresolved.
#[test]
fn word_links_follow_the_sentence_and_their_word() {
    let (db, rng) = (db(), rng());
    let deck = create_deck(&db, &rng, "deck", "Niemiecki", None);
    let tee = save(
        &db,
        &rng,
        note_input("tee", deck, NoteTypeKind::Basic, "der Tee", "herbata"),
    )
    .unwrap();
    let mut sentence = note_input(
        "s",
        deck,
        NoteTypeKind::Sentence,
        "Ich trinke Tee.",
        "Piję herbatę.",
    );
    sentence.word_links = Some(vec![LinkDraft {
        start: 11,
        end: 14,
        surface: "Tee".into(),
        word: Some(tee),
    }]);
    let note = save(&db, &rng, sentence.clone()).unwrap();
    let shown = details(&db, note);
    assert_eq!(shown.word_links.len(), 1);
    assert_eq!(
        shown.word_links[0].link.resolution,
        WordLinkResolution::UserOverridden
    );
    assert_eq!(shown.word_links[0].translation.as_deref(), Some("herbata"));

    // A range that does not cover the surface is refused.
    let mut wrong = sentence.clone();
    wrong.operation_id = "s-wrong".into();
    wrong.word_links = Some(vec![LinkDraft {
        start: 10,
        end: 13,
        surface: "Tee".into(),
        word: Some(tee),
    }]);
    assert_eq!(
        validation_reason(save(&db, &rng, wrong).unwrap_err()),
        "range"
    );

    // The phrase moves: the link follows it without new drafts.
    let mut moved = sentence;
    moved.operation_id = "s-moved".into();
    moved.note_id = Some(note);
    moved.expected_version = Some(shown.note.version);
    moved.front = "Morgens trinke ich Tee.".into();
    moved.word_links = None;
    save(&db, &rng, moved).unwrap();
    let remapped = details(&db, note);
    assert_eq!(remapped.word_links.len(), 1);
    let link = &remapped.word_links[0].link;
    assert_eq!((link.start_index, link.end_index), (19, 22));

    // Deleting the word leaves the phrase marked but unresolved.
    let version = details(&db, tee).note.version;
    let deleted = write(&db, move |tx| {
        management::delete_note(tx, "del-tee", tee, version, now())
    })
    .unwrap();
    assert_eq!(deleted, 1);
    let after = details(&db, note);
    assert_eq!(
        after.word_links[0].link.resolution,
        WordLinkResolution::Unresolved
    );
    assert_eq!(after.word_links[0].link.word_note_id, None);
}

/// Tips and associations change one at a time; the order stays consistent; editing a bundled
/// tip is kept as an override so a content update does not undo it.
#[test]
fn note_content_edits_are_transactional_and_bundled_edits_are_kept() {
    let (db, rng) = (db(), rng());
    let deck = create_deck(&db, &rng, "deck", "Niemiecki", None);
    let mut input = note_input("n", deck, NoteTypeKind::Basic, "der Vertrag", "umowa");
    input.grammar_tips = vec!["pierwsza".into(), "druga".into()];
    input.manual_association = Some("Wer trägt den Vertrag?".into());
    let note = save(&db, &rng, input).unwrap();
    let change = |op: &'static str, change: NoteContentChange| {
        let rng = rng.clone();
        write(&db, move |tx| {
            management::update_note_content(tx, rng.as_ref(), op, note, &change, now())
        })
    };
    change("add", NoteContentChange::AddTip("trzecia".into())).unwrap();
    let tips = details(&db, note).grammar_tips;
    assert_eq!(
        tips.iter().map(|tip| tip.text.as_str()).collect::<Vec<_>>(),
        ["pierwsza", "druga", "trzecia"]
    );
    let ids: Vec<_> = tips.iter().map(|tip| tip.id).collect();
    change(
        "reorder",
        NoteContentChange::ReorderTips(vec![ids[2], ids[0], ids[1]]),
    )
    .unwrap();
    assert_eq!(details(&db, note).grammar_tips[0].text, "trzecia");
    let error = change("reorder-bad", NoteContentChange::ReorderTips(vec![ids[0]])).unwrap_err();
    assert_eq!(validation_reason(error), "mismatch");
    change("delete", NoteContentChange::DeleteTip(ids[2])).unwrap();
    let left = details(&db, note).grammar_tips;
    assert_eq!(
        left.iter().map(|tip| tip.sort_order).collect::<Vec<_>>(),
        [0, 1]
    );

    let association = details(&db, note).associations[0].id;
    change(
        "assoc",
        NoteContentChange::EditAssociation(association, "Nowy obraz".into()),
    )
    .unwrap();
    assert_eq!(details(&db, note).associations[0].text, "Nowy obraz");
    // The note version does not move, so an open editor draft stays valid.
    assert_eq!(details(&db, note).note.version, 1);

    // A bundled tip: editing it records an override for the content updater.
    let bundled_tip = write(&db, move |tx| {
        tx.put_bundled_note(&BundledNote {
            note_id: note,
            deck_guid: "deck".into(),
            content_version: "2026.11.09".into(),
            removed_in_version: None,
            archived_by_import: false,
        })?;
        let guid = Guid::parse("bundled-tip-1").unwrap();
        Ok(tx
            .insert_grammar_tip(note, &guid, "wbudowana", TipProvenance::Bundled, now())?
            .id)
    })
    .unwrap();
    change(
        "bundled",
        NoteContentChange::EditTip(bundled_tip, "poprawiona".into()),
    )
    .unwrap();
    let overrides = read(&db, move |tx| tx.content_overrides(note));
    assert_eq!(overrides, ["tip:bundled-tip-1"]);
    // A replay changes nothing.
    change(
        "bundled",
        NoteContentChange::EditTip(bundled_tip, "inna".into()),
    )
    .unwrap();
    assert!(
        details(&db, note)
            .grammar_tips
            .iter()
            .any(|tip| tip.text == "poprawiona")
    );
}

// ---------------------------------------------------------------- decks and presets

#[test]
fn the_deck_hierarchy_refuses_cycles_and_deletes_whole_subtrees() {
    let (db, rng) = (db(), rng());
    let root = create_deck(&db, &rng, "root", "Niemiecki", None);
    let child = create_deck(&db, &rng, "child", "Zdania", Some(root));
    let grandchild = create_deck(&db, &rng, "grandchild", "Trudne", Some(child));
    save(
        &db,
        &rng,
        note_input("a", root, NoteTypeKind::Basic, "eins", "jeden"),
    )
    .unwrap();
    save(
        &db,
        &rng,
        note_input(
            "b",
            grandchild,
            NoteTypeKind::BasicAndReversed,
            "zwei",
            "dwa",
        ),
    )
    .unwrap();

    let overview = write(&db, move |tx| {
        management::deck_overview(tx, root, now(), &[])
    })
    .unwrap();
    assert_eq!(
        overview
            .subdecks
            .iter()
            .map(|row| row.deck.id)
            .collect::<Vec<_>>(),
        [child, grandchild]
    );
    assert_eq!(
        overview.deck.counts.total, 3,
        "rolled up without counting a deck twice"
    );
    assert_eq!(overview.subdecks[0].counts.total, 2);

    let version = overview.deck.deck.version;
    let cycle = DeckPatch {
        parent_id: Some(Some(grandchild)),
        ..DeckPatch::default()
    };
    let error = write(&db, move |tx| {
        management::update_deck(tx, "cycle", root, version, &cycle, now())
    })
    .unwrap_err();
    assert_eq!(validation_reason(error), "cycle");

    let rename = DeckPatch {
        name: Some(" Deutsch ".into()),
        deck_type: Some(DeckType::Simple),
        ..DeckPatch::default()
    };
    write(&db, move |tx| {
        management::update_deck(tx, "rename", root, version, &rename, now())
    })
    .unwrap();
    let renamed = write(&db, move |tx| {
        management::deck_overview(tx, root, now(), &[])
    })
    .unwrap();
    assert_eq!(renamed.deck.deck.name, "Deutsch");
    assert_eq!(renamed.deck.deck.deck_type, DeckType::Simple);
    let stale = write(&db, move |tx| {
        management::update_deck(tx, "stale", root, version, &DeckPatch::default(), now())
    })
    .unwrap_err();
    assert!(
        matches!(stale, AppError::VersionConflict { .. }),
        "{stale:?}"
    );

    let child_version = renamed.subdecks[0].deck.version;
    let outcome = write(&db, move |tx| {
        management::delete_deck(tx, "delete", child, child_version, now())
    })
    .unwrap();
    assert_eq!((outcome.deleted_decks, outcome.deleted_cards), (2, 2));
    let replay = write(&db, move |tx| {
        management::delete_deck(tx, "delete", child, child_version, now())
    })
    .unwrap();
    assert_eq!(replay, outcome);
    let left = write(&db, move |tx| {
        management::deck_overview(tx, root, now(), &[])
    })
    .unwrap();
    assert!(left.subdecks.is_empty());
    assert_eq!(left.deck.counts.total, 1);
    let gone = write(&db, move |tx| {
        management::deck_overview(tx, child, now(), &[])
    })
    .unwrap_err();
    assert_eq!(gone, AppError::Deleted { entity: "deck" });
}

#[test]
fn presets_are_created_cloned_and_updated_at_their_version() {
    let (db, rng) = (db(), rng());
    let deck = create_deck(&db, &rng, "deck", "Niemiecki", None);
    let presets = read(&db, management::presets);
    assert_eq!(presets.len(), 1);
    assert!(presets[0].0.built_in);
    assert_eq!(presets[0].1, 1, "the new deck uses the default preset");
    let config = PresetConfig {
        new_per_day: 10,
        reviews_per_day: 100,
        desired_retention: 0.85,
        max_interval_days: 365,
        learning_steps_minutes: vec![1, 10],
        relearning_steps_minutes: vec![10],
        weights: presets[0].0.options.weights.clone(),
    };
    let create = SavePreset {
        operation_id: "p1".into(),
        preset_id: None,
        expected_version: None,
        name: "Lekki".into(),
        config: config.clone(),
    };
    let saved = {
        let rng = rng.clone();
        write(&db, move |tx| {
            management::save_preset(tx, rng.as_ref(), &create, now())
        })
        .unwrap()
    };
    assert_eq!((saved.0.options.new_per_day, saved.1), (10, 0));
    let preset = saved.0.id;
    let patch = DeckPatch {
        preset_id: Some(preset),
        ..DeckPatch::default()
    };
    write(&db, move |tx| {
        management::update_deck(tx, "use", deck, 1, &patch, now())
    })
    .unwrap();
    let listed = read(&db, management::presets);
    assert_eq!(listed[1].0.name, "Lekki");
    assert_eq!(listed[1].1, 1);

    let mut update = SavePreset {
        operation_id: "p2".into(),
        preset_id: Some(preset),
        expected_version: Some(1),
        name: "Lekki".into(),
        config: PresetConfig {
            desired_retention: 0.995,
            ..config.clone()
        },
    };
    let rng2 = rng.clone();
    let invalid = update.clone();
    let error = write(&db, move |tx| {
        management::save_preset(tx, rng2.as_ref(), &invalid, now())
    })
    .unwrap_err();
    assert_eq!(validation_reason(error), "outOfRange");
    update.config.desired_retention = 0.92;
    let rng3 = rng.clone();
    let first = update.clone();
    write(&db, move |tx| {
        management::save_preset(tx, rng3.as_ref(), &first, now())
    })
    .unwrap();
    update.operation_id = "p3".into();
    let error = write(&db, move |tx| {
        management::save_preset(tx, rng.as_ref(), &update, now())
    })
    .unwrap_err();
    assert!(
        matches!(
            error,
            AppError::VersionConflict {
                entity: "preset",
                current: 2
            }
        ),
        "{error:?}"
    );
    let missing = DeckPatch {
        preset_id: Some(DeckConfigId(99)),
        ..DeckPatch::default()
    };
    let error = write(&db, move |tx| {
        management::update_deck(tx, "missing", deck, 2, &missing, now())
    })
    .unwrap_err();
    assert_eq!(validation_reason(error), "notFound");
}

// ---------------------------------------------------------------- browser

fn search(query: &str) -> CardSearch {
    CardSearch {
        query: query.into(),
        deck_id: None,
        tag: None,
        states: Vec::new(),
        flags: Vec::new(),
        sort: SortName::Due,
        descending: false,
    }
}

/// `count` Basic notes "Wort N"/"słowo N" in `deck`, every tenth tagged `zehn`.
fn many_notes(db: &Arc<SqliteDatabase>, deck: DeckId, count: u32) -> Vec<CardId> {
    write(db, move |tx| {
        let rng = Sequence(AtomicU64::new(1_000_000));
        let mut cards = Vec::new();
        for index in 0..count {
            let mut input = note_input(
                &format!("bulk-{index}"),
                deck,
                NoteTypeKind::Basic,
                &format!("Wort {index:05}"),
                &format!("słowo {index}"),
            );
            if index % 10 == 0 {
                input.tags = vec!["zehn".into()];
            }
            let note = management::save_note(tx, &rng, &input, now())?;
            cards.push(tx.cards_of_note(note)?[0].id);
        }
        Ok(cards)
    })
    .unwrap()
}

#[test]
fn keyset_pages_visit_every_match_once_with_a_separate_total() {
    let (db, rng) = (db(), rng());
    let deck = create_deck(&db, &rng, "deck", "Niemiecki", None);
    many_notes(&db, deck, 120);
    for (sort, descending) in [
        (SortName::Due, false),
        (SortName::Front, true),
        (SortName::Created, false),
        (SortName::Deck, false),
        (SortName::Reps, true),
    ] {
        let mut seen = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let mut input = search("");
            input.sort = sort;
            input.descending = descending;
            let after = cursor.clone();
            let page = read(&db, move |tx| {
                browser::search_cards(tx, &input, after.as_deref(), 50, now())
            });
            assert_eq!(page.total, 120);
            seen.extend(page.rows.iter().map(|row| row.card.id));
            match page.next_cursor {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }
        let unique: std::collections::HashSet<_> = seen.iter().collect();
        assert_eq!((seen.len(), unique.len()), (120, 120), "{sort:?}");
    }
    let mut front_desc = search("");
    front_desc.sort = SortName::Front;
    front_desc.descending = true;
    let page = read(&db, move |tx| {
        browser::search_cards(tx, &front_desc, None, 2, now())
    });
    assert_eq!(page.rows[0].note.fields.front, "Wort 00119");
    let error = db
        .read(|tx| browser::search_cards(tx, &search(""), Some("zz"), 10, now()))
        .unwrap_err();
    assert_eq!(validation_reason(error), "format");
}

#[test]
fn the_search_language_filters_by_text_deck_tag_state_and_age() {
    let (db, rng) = (db(), rng());
    let root = create_deck(&db, &rng, "root", "Niemiecki", None);
    let child = create_deck(&db, &rng, "child", "Zwierzęta", Some(root));
    let other = create_deck(&db, &rng, "other", "Angielski", None);
    many_notes(&db, root, 30);
    let mut dog = note_input("dog", child, NoteTypeKind::Basic, "der Hund", "pies");
    dog.tags = vec!["Tiere".into()];
    let dog = save(&db, &rng, dog).unwrap();
    save(
        &db,
        &rng,
        note_input("en", other, NoteTypeKind::Basic, "the dog", "pies"),
    )
    .unwrap();
    let dog_card = read(&db, move |tx| tx.cards_of_note(dog))[0].id;
    write(&db, move |tx| {
        let mut card = tx.card(dog_card)?.unwrap();
        card.state = CardState::Review;
        card.queue = QueueState::Review;
        card.due_at = Some(TimestampMs(NOON - DAY));
        tx.update_card(&card, now())?;
        Ok(())
    })
    .unwrap();

    let total = |query: &'static str| {
        read(&db, move |tx| {
            browser::search_cards(tx, &search(query), None, 100, now())
        })
        .total
    };
    assert_eq!(total("hund"), 1, "full-text, case-insensitive");
    assert_eq!(total("pies"), 2);
    assert_eq!(total("wort"), 30, "prefix match per word");
    assert_eq!(total("deck:niem"), 31, "deck prefix includes subdecks");
    assert_eq!(total("deck:zwierz pies"), 1);
    assert_eq!(total("tag:tiere"), 1, "tags are case-insensitive");
    assert_eq!(total("tag:zehn"), 3);
    assert_eq!(total("is:due"), 1);
    assert_eq!(total("is:new"), 31);
    assert_eq!(total("added:1"), 32);
    assert_eq!(total("x' OR 1=1 --"), 0, "user text is never SQL");
    assert_eq!(total("\"der Hund\""), 1);
    let mut scoped = search("pies");
    scoped.deck_id = Some(other.0);
    assert_eq!(
        read(&db, move |tx| browser::search_cards(
            tx,
            &scoped,
            None,
            10,
            now()
        ))
        .total,
        1
    );
    let error = db
        .read(|tx| browser::search_cards(tx, &search("flag:red"), None, 10, now()))
        .unwrap_err();
    assert_eq!(validation_reason(error), "unknownField");
}

/// DoD: 1,000 suspends are atomic: a failure after them leaves no partial change; a stale "all
/// matches" selection is refused; reposition renumbers only new cards.
#[test]
fn bulk_actions_are_atomic_idempotent_and_refuse_stale_selections() {
    let (db, rng) = (db(), rng());
    let deck = create_deck(&db, &rng, "deck", "Niemiecki", None);
    let cards = many_notes(&db, deck, 1_000);
    let selection = BulkSelection::Ids(cards.clone());

    let preview = {
        let selection = selection.clone();
        read(&db, move |tx| {
            browser::preview_bulk_action(tx, &selection, &BulkAction::Suspend, now())
        })
    };
    assert_eq!(
        (
            preview.selected,
            preview.affected,
            preview.notes,
            preview.decks
        ),
        (1_000, 1_000, 1_000, 1)
    );

    // A failure in the same transaction rolls every suspension back.
    let failing = selection.clone();
    let error = write(&db, move |tx| {
        browser::apply_bulk_action(tx, "suspend-fails", &failing, &BulkAction::Suspend, now())?;
        Err::<(), _>(AppError::Internal("simulated crash before commit".into()))
    })
    .unwrap_err();
    assert!(matches!(error, AppError::Internal(_)));
    assert_eq!(
        read(&db, |tx| browser::search_cards(
            tx,
            &search("is:suspended"),
            None,
            1,
            now()
        ))
        .total,
        0
    );

    let applied = selection.clone();
    let affected = write(&db, move |tx| {
        browser::apply_bulk_action(tx, "suspend", &applied, &BulkAction::Suspend, now())
    })
    .unwrap();
    assert_eq!(affected, 1_000);
    let replay = selection.clone();
    assert_eq!(
        write(&db, move |tx| browser::apply_bulk_action(
            tx,
            "suspend",
            &replay,
            &BulkAction::Suspend,
            now()
        ))
        .unwrap(),
        1_000
    );
    let again = read(&db, move |tx| {
        browser::preview_bulk_action(tx, &selection, &BulkAction::Suspend, now())
    });
    assert_eq!((again.affected, again.unchanged), (0, 1_000));

    // "All matches" of a page is refused once the collection changed after the page.
    let page = read(&db, |tx| {
        browser::search_cards(tx, &search("tag:zehn"), None, 10, now())
    });
    let all = BulkSelection::Query {
        filter_version: page.filter_version.clone(),
        excluded: vec![page.rows[0].card.id],
    };
    let tag_preview = {
        let all = all.clone();
        read(&db, move |tx| {
            browser::preview_bulk_action(tx, &all, &BulkAction::AddTag("wybrane".into()), now())
        })
    };
    assert_eq!((tag_preview.selected, tag_preview.affected), (99, 99));
    let first = cards[0];
    write(&db, move |tx| {
        browser::apply_bulk_action(
            tx,
            "unsuspend-one",
            &BulkSelection::Ids(vec![first]),
            &BulkAction::Unsuspend,
            now(),
        )
    })
    .unwrap();
    let stale = db
        .read(move |tx| browser::preview_bulk_action(tx, &all, &BulkAction::Delete, now()))
        .unwrap_err();
    assert_eq!(stale, AppError::PreviewStale);

    // Reposition: new cards only, in their current order.
    let three = vec![cards[2], cards[1], cards[0]];
    write(&db, move |tx| {
        let mut studied = tx.card(three[0])?.unwrap();
        studied.state = CardState::Review;
        studied.queue = QueueState::Review;
        studied.due_at = Some(TimestampMs(NOON + DAY));
        tx.update_card(&studied, now())?;
        browser::apply_bulk_action(
            tx,
            "reposition",
            &BulkSelection::Ids(three),
            &BulkAction::Reposition { start: 5, step: 10 },
            now(),
        )
    })
    .unwrap();
    let positions = read(&db, move |tx| {
        Ok((
            tx.card(cards[0])?.unwrap().position,
            tx.card(cards[1])?.unwrap().position,
        ))
    });
    assert_eq!(positions, (5, 15));
    let error = db
        .read(|tx| {
            browser::preview_bulk_action(
                tx,
                &BulkSelection::Ids(Vec::new()),
                &BulkAction::AddTag("two words".into()),
                now(),
            )
        })
        .unwrap_err();
    assert_eq!(validation_reason(error), "invalid");
}

/// Plan DoD: search p95 under 200 ms on 50k cards (run with `--ignored --nocapture`).
#[test]
#[ignore = "benchmark: 50k cards"]
fn browser_search_p95_on_fifty_thousand_cards() {
    let (db, rng) = (db(), rng());
    let deck = create_deck(&db, &rng, "deck", "Niemiecki", None);
    let started = Instant::now();
    many_notes(&db, deck, 50_000);
    println!("fixture: {:?}", started.elapsed());
    let queries = [
        "",
        "wort",
        "wort 0499",
        "tag:zehn",
        "is:new",
        "słowo 1",
        "deck:niem",
    ];
    let mut timings = Vec::new();
    for round in 0..5 {
        for query in queries {
            for sort in [SortName::Due, SortName::Front] {
                let mut input = search(query);
                input.sort = sort;
                input.descending = round % 2 == 1;
                let started = Instant::now();
                let page = read(&db, move |tx| {
                    browser::search_cards(tx, &input, None, 100, now())
                });
                timings.push(started.elapsed());
                assert!(page.total > 0, "{query}");
            }
        }
    }
    timings.sort();
    let p95 = timings[timings.len() * 95 / 100];
    println!(
        "search over 50k cards: p50 {:?}, p95 {:?}, max {:?}",
        timings[timings.len() / 2],
        p95,
        timings.last().unwrap()
    );
    assert!(p95.as_millis() < 200, "p95 {p95:?}");
}
