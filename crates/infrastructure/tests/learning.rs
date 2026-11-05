//! Learning on the existing screens (plan Phase 10) on the real SQLite unit of work: study card
//! content by deck type and direction, card actions, favorites and word links, the session
//! summary, the dashboard (limit-aware counts, resumable session, zoned streak) and recovery of
//! a session and of a committed rating after the process died.

use std::sync::Arc;

use langualinker_application::learning::{self, CardAction};
use langualinker_application::ports::Clock;
use langualinker_application::repositories::{Repositories, SessionStatus, UnitOfWork};
use langualinker_application::study::{self, RateCard, SessionAction, StartStudy, StudyView};
use langualinker_application::{AppError, AppResult, dashboard};
use langualinker_domain::card::{CardFlag, QueueState};
use langualinker_domain::content::{
    AssociationProvenance, ModerationStatus, NewAssociation, SentenceWordLink, TipProvenance,
    WordLinkResolution,
};
use langualinker_domain::deck::{DeckType, NativeLanguage, NewDeck, TargetLanguage};
use langualinker_domain::note::{CardDirection, NewNote, NoteFields, NoteTypeKind};
use langualinker_domain::review::Rating;
use langualinker_domain::{CardId, DeckId, Guid, NoteId, SessionId, TimestampMs, WordLinkId};
use langualinker_infrastructure::sqlite::SqliteDatabase;

/// 2026-03-02 12:00 UTC; with the default 04:00 UTC cutoff the study day began at 04:00.
const NOON: i64 = 1_772_452_800_000;
const MINUTE: i64 = 60_000;
const DAY: i64 = 86_400_000;

struct Fixed;

impl Clock for Fixed {
    fn now(&self) -> TimestampMs {
        TimestampMs(NOON)
    }
}

fn db() -> Arc<SqliteDatabase> {
    Arc::new(SqliteDatabase::open_in_memory(&Fixed).unwrap().database)
}

fn guid(seed: u64) -> Guid {
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&seed.to_le_bytes());
    bytes[8] = 0x6b;
    Guid::from_bytes(bytes)
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

fn deck(db: &Arc<SqliteDatabase>, seed: u64, deck_type: DeckType) -> DeckId {
    write(db, move |tx| {
        let config = tx.default_config()?.id;
        Ok(tx
            .insert_deck(
                &NewDeck {
                    guid: guid(seed),
                    name: format!("Deck {seed}"),
                    parent_id: None,
                    description: String::new(),
                    native_language: NativeLanguage::Pl,
                    target_language: TargetLanguage::De,
                    deck_type,
                    config_id: config,
                    content_version: None,
                    bundled: false,
                },
                TimestampMs(0),
            )?
            .id)
    })
    .unwrap()
}

/// A note with one card per template of `kind`. Returns the note and its cards by ordinal.
fn note(
    db: &Arc<SqliteDatabase>,
    seed: u64,
    deck: DeckId,
    kind: NoteTypeKind,
    front: &str,
    back: &str,
) -> (NoteId, Vec<CardId>) {
    let (front, back) = (front.to_owned(), back.to_owned());
    write(db, move |tx| {
        let note_type = tx.note_type(kind)?;
        let note = tx.insert_note(
            &NewNote {
                guid: guid(seed),
                note_type_id: note_type.id,
                fields: NoteFields { front, back },
                tags: Vec::new(),
            },
            TimestampMs(0),
        )?;
        let mut ids = Vec::new();
        for template in tx.templates(note_type.id)? {
            let position = tx.next_position()?;
            ids.push(
                tx.insert_card(
                    &langualinker_domain::card::NewCard {
                        guid: guid(seed * 100 + u64::from(template.ordinal) + 1),
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
        Ok((note.id, ids))
    })
    .unwrap()
}

fn association(db: &Arc<SqliteDatabase>, seed: u64, note: NoteId, text: &str) -> i64 {
    let text = text.to_owned();
    write(db, move |tx| {
        Ok(tx
            .insert_association(
                &NewAssociation {
                    guid: guid(seed),
                    note_id: note,
                    text,
                    provenance: AssociationProvenance::Bundled,
                    native_language: "pl".into(),
                    target_language: "de".into(),
                    prompt_version: None,
                    model: None,
                    moderation: ModerationStatus::NotRequired,
                    cache_key: None,
                },
                TimestampMs(0),
            )?
            .id
            .0)
    })
    .unwrap()
}

fn start(db: &Arc<SqliteDatabase>, op: &str, deck: Option<DeckId>, at: i64) -> StudyView {
    let input = StartStudy {
        operation_id: op.into(),
        deck_id: deck,
        card_ids: None,
    };
    write(db, move |tx| {
        study::start_study(tx, &input, TimestampMs(at))
    })
    .unwrap()
}

fn rate(
    db: &Arc<SqliteDatabase>,
    view: &StudyView,
    op: &str,
    rating: Rating,
    at: i64,
) -> AppResult<study::RateOutcome> {
    let current = view.current.clone().expect("a current card");
    let session = view.session.id;
    let preview = read(db, move |tx| {
        study::preview(tx, session, current.id, current.version, TimestampMs(at))
    });
    let input = RateCard {
        operation_id: op.into(),
        session_id: session,
        card_id: preview.card_id,
        card_version: preview.card_version,
        preview_token: preview.token,
        rating,
        answer_ms: 3_000,
    };
    write(db, move |tx| study::rate_card(tx, &input, TimestampMs(at)))
}

fn action(
    db: &Arc<SqliteDatabase>,
    op: &str,
    card: CardId,
    action: CardAction,
) -> AppResult<learning::CardActionOutcome> {
    let op = op.to_owned();
    let version = read(db, move |tx| Ok(tx.card(card)?.unwrap().version));
    write(db, move |tx| {
        learning::apply_card_action(tx, &op, card, version, action, TimestampMs(NOON))
    })
}

fn screen(db: &Arc<SqliteDatabase>, session: SessionId, at: i64) -> learning::StudyScreen {
    write(db, move |tx| {
        let view = study::open_study(tx, session, TimestampMs(at))?;
        learning::screen(tx, view)
    })
    .unwrap()
}

// ---------------------------------------------------------------- study card content

#[test]
fn the_study_card_carries_content_by_deck_type_and_direction() {
    let db = db();
    let words = deck(&db, 1, DeckType::Linguistic);
    let (word, _) = note(&db, 1, words, NoteTypeKind::Basic, "Hund", "pies");
    let (sentence, cards) = note(
        &db,
        2,
        words,
        NoteTypeKind::SentenceAndReversed,
        "Der Hund schläft.",
        "Pies śpi.",
    );
    association(&db, 10, sentence, "Hund jak hałas");
    write(&db, move |tx| {
        tx.insert_grammar_tip(
            sentence,
            &guid(20),
            "der → rodzaj męski",
            TipProvenance::Bundled,
            TimestampMs(0),
        )?;
        tx.replace_word_links(
            sentence,
            &[SentenceWordLink {
                id: WordLinkId(0),
                sentence_note_id: sentence,
                word_note_id: Some(word),
                preferred_word_card_id: None,
                surface_form: "Hund".into(),
                lemma: Some("Hund".into()),
                start_index: 4,
                end_index: 8,
                resolution: WordLinkResolution::Resolved,
                created_at: TimestampMs(0),
                updated_at: TimestampMs(0),
            }],
            TimestampMs(0),
        )?;
        Ok(())
    })
    .unwrap();

    let [forward, reverse] = [cards[0], cards[1]];
    let forward = write(&db, move |tx| {
        let card = tx.card(forward)?.unwrap();
        learning::study_card(tx, card)
    })
    .unwrap();
    assert_eq!(forward.direction, CardDirection::Forward);
    assert_eq!(
        (forward.front.as_str(), forward.back.as_str()),
        ("Der Hund schläft.", "Pies śpi.")
    );
    assert_eq!(forward.associations.len(), 1);
    assert_eq!(forward.grammar_tips.len(), 1);
    assert_eq!(forward.word_links.len(), 1);
    assert_eq!(forward.word_links[0].translation.as_deref(), Some("pies"));

    let reverse = write(&db, move |tx| {
        let card = tx.card(reverse)?.unwrap();
        learning::study_card(tx, card)
    })
    .unwrap();
    assert_eq!(reverse.direction, CardDirection::Reverse);
    assert_eq!(reverse.front, "Pies śpi.", "a reverse card asks in Polish");

    // A simple deck hides associations, tips and links.
    let simple = deck(&db, 3, DeckType::Simple);
    let moved = cards[0];
    write(&db, move |tx| {
        let mut card = tx.card(moved)?.unwrap();
        card.deck_id = simple;
        tx.update_card(&card, TimestampMs(1))?;
        let card = tx.card(moved)?.unwrap();
        let shown = learning::study_card(tx, card)?;
        assert!(shown.associations.is_empty());
        assert!(shown.grammar_tips.is_empty());
        assert!(shown.word_links.is_empty());
        Ok(())
    })
    .unwrap();
}

#[test]
fn the_word_sheet_reads_a_notes_content() {
    let db = db();
    let words = deck(&db, 1, DeckType::Linguistic);
    let (word, _) = note(&db, 1, words, NoteTypeKind::Basic, "Hund", "pies");
    association(&db, 2, word, "Hund — hałas");
    let content = read(&db, move |tx| learning::note_content(tx, word));
    assert_eq!(content.note.fields.back, "pies");
    assert_eq!(content.deck.id, words);
    assert_eq!(content.associations.len(), 1);

    let error = db
        .read(|tx| learning::note_content(tx, NoteId(999)))
        .unwrap_err();
    assert!(matches!(error, AppError::NotFound { entity: "note" }));
}

// ---------------------------------------------------------------- card actions

#[test]
fn suspend_bury_and_flag_change_the_queue_and_retries_change_nothing() {
    let db = db();
    let words = deck(&db, 1, DeckType::Simple);
    let first = note(&db, 1, words, NoteTypeKind::Basic, "eins", "jeden").1[0];
    let second = note(&db, 2, words, NoteTypeKind::Basic, "zwei", "dwa").1[0];
    let view = start(&db, "start", Some(words), NOON);
    assert_eq!(view.counts.new, 2);

    let suspended = action(&db, "suspend", first, CardAction::Suspend).unwrap();
    assert_eq!(suspended.card.queue, QueueState::Suspended);
    // The retry returns the same card without another change.
    let version = suspended.card.version;
    let retry = write(&db, move |tx| {
        learning::apply_card_action(
            tx,
            "suspend",
            first,
            version - 1,
            CardAction::Suspend,
            TimestampMs(NOON),
        )
    })
    .unwrap();
    assert_eq!(retry.card.version, version);

    let after = screen(&db, view.session.id, NOON + 1);
    assert_eq!(after.view.counts.new, 1);
    assert_eq!(
        after.current.as_ref().map(|card| card.card.id),
        Some(second)
    );

    let unsuspended = action(&db, "unsuspend", first, CardAction::Unsuspend).unwrap();
    assert_eq!(unsuspended.card.queue, QueueState::New);

    let buried = action(&db, "bury", second, CardAction::Bury).unwrap();
    assert_eq!(buried.card.queue, QueueState::UserBuried);
    assert!(buried.card.buried_until.is_some());
    let unburied = action(&db, "unbury", second, CardAction::Unbury).unwrap();
    assert_eq!(unburied.card.queue, QueueState::New);
    assert_eq!(unburied.card.buried_until, None);

    let flagged = action(&db, "flag", second, CardAction::Flag(CardFlag::Red)).unwrap();
    assert_eq!(flagged.card.flag, CardFlag::Red);
    assert_eq!(flagged.card.version, unburied.card.version + 1);

    // A stale version is a conflict, not a silent overwrite.
    let error = write(&db, move |tx| {
        learning::apply_card_action(
            tx,
            "late",
            second,
            1,
            CardAction::Suspend,
            TimestampMs(NOON),
        )
    })
    .unwrap_err();
    assert!(matches!(error, AppError::VersionConflict { .. }));
}

#[test]
fn deleting_the_last_card_deletes_the_note_and_unresolves_links_to_it() {
    let db = db();
    let words = deck(&db, 1, DeckType::Linguistic);
    let (word, word_cards) = note(&db, 1, words, NoteTypeKind::Basic, "Hund", "pies");
    let (sentence, pair) = note(
        &db,
        2,
        words,
        NoteTypeKind::BasicAndReversed,
        "Der Hund",
        "Pies",
    );
    write(&db, move |tx| {
        tx.replace_word_links(
            sentence,
            &[SentenceWordLink {
                id: WordLinkId(0),
                sentence_note_id: sentence,
                word_note_id: Some(word),
                preferred_word_card_id: None,
                surface_form: "Hund".into(),
                lemma: None,
                start_index: 4,
                end_index: 8,
                resolution: WordLinkResolution::Resolved,
                created_at: TimestampMs(0),
                updated_at: TimestampMs(0),
            }],
            TimestampMs(0),
        )?;
        Ok(())
    })
    .unwrap();

    // One of two cards: the note stays.
    let deleted = action(&db, "delete-one", pair[0], CardAction::Delete).unwrap();
    assert!(deleted.deleted);
    assert!(
        read(&db, move |tx| tx.note(sentence))
            .unwrap()
            .deleted_at
            .is_none()
    );

    // The only card of the word: the note goes and the sentence link becomes unresolved.
    action(&db, "delete-word", word_cards[0], CardAction::Delete).unwrap();
    assert!(
        read(&db, move |tx| tx.note(word))
            .unwrap()
            .deleted_at
            .is_some()
    );
    let links = read(&db, move |tx| tx.word_links(sentence));
    assert_eq!(links[0].resolution, WordLinkResolution::Unresolved);
    assert_eq!(links[0].word_note_id, None);
}

// ---------------------------------------------------------------- favorites and links

#[test]
fn a_bundled_association_becomes_the_single_favorite() {
    let db = db();
    let words = deck(&db, 1, DeckType::TextWithAssociations);
    let (word, _) = note(&db, 1, words, NoteTypeKind::Basic, "Hund", "pies");
    let first = association(&db, 2, word, "pierwsza");
    let second = association(&db, 3, word, "druga");
    let set = |op: &'static str, id: i64| {
        write(&db, move |tx| {
            learning::set_favorite_association(
                tx,
                op,
                word,
                langualinker_domain::AssociationId(id),
                TimestampMs(NOON),
            )
        })
        .unwrap()
    };
    let list = set("fav-1", first);
    assert_eq!(list.iter().filter(|item| item.is_favorite).count(), 1);
    let list = set("fav-2", second);
    let favorite: Vec<i64> = list
        .iter()
        .filter(|item| item.is_favorite)
        .map(|item| item.id.0)
        .collect();
    assert_eq!(favorite, [second]);
    // Retrying the first request does not move the favorite back.
    let list = set("fav-1", first);
    assert!(
        list.iter()
            .any(|item| item.id.0 == second && item.is_favorite)
    );
}

#[test]
fn choosing_a_word_for_a_link_keeps_the_link_id() {
    let db = db();
    let words = deck(&db, 1, DeckType::Linguistic);
    let (word, _) = note(&db, 1, words, NoteTypeKind::Basic, "Bank", "ławka");
    let (sentence, _) = note(
        &db,
        2,
        words,
        NoteTypeKind::Sentence,
        "Die Bank ist alt.",
        "Ławka jest stara.",
    );
    let link = write(&db, move |tx| {
        Ok(tx.replace_word_links(
            sentence,
            &[SentenceWordLink {
                id: WordLinkId(0),
                sentence_note_id: sentence,
                word_note_id: None,
                preferred_word_card_id: None,
                surface_form: "Bank".into(),
                lemma: Some("Bank".into()),
                start_index: 4,
                end_index: 8,
                resolution: WordLinkResolution::Ambiguous,
                created_at: TimestampMs(0),
                updated_at: TimestampMs(0),
            }],
            TimestampMs(0),
        )?[0]
            .id)
    })
    .unwrap();
    let view = write(&db, move |tx| {
        learning::resolve_word_link(tx, "pick", link, word, TimestampMs(NOON))
    })
    .unwrap();
    assert_eq!(view.link.id, link);
    assert_eq!(view.link.word_note_id, Some(word));
    assert_eq!(view.link.resolution, WordLinkResolution::UserOverridden);
    assert_eq!(view.translation.as_deref(), Some("ławka"));

    let error = write(&db, move |tx| {
        learning::resolve_word_link(tx, "self", link, sentence, TimestampMs(NOON))
    })
    .unwrap_err();
    assert!(matches!(error, AppError::Validation(ref e) if e.reason == "sameNote"));
}

// ---------------------------------------------------------------- summary and dashboard

#[test]
fn the_summary_counts_valid_ratings_and_points_to_the_next_deck() {
    let db = db();
    let first_deck = deck(&db, 1, DeckType::Simple);
    let other_deck = deck(&db, 2, DeckType::Simple);
    note(&db, 1, first_deck, NoteTypeKind::Basic, "eins", "jeden");
    note(&db, 2, first_deck, NoteTypeKind::Basic, "zwei", "dwa");
    note(
        &db,
        3,
        other_deck,
        NoteTypeKind::BasicAndReversed,
        "drei",
        "trzy",
    );

    let view = start(&db, "start", Some(first_deck), NOON);
    let session = view.session.id;
    let after_again = rate(&db, &view, "r1", Rating::Again, NOON).unwrap().view;
    let after_good = rate(&db, &after_again, "r2", Rating::Good, NOON + MINUTE)
        .unwrap()
        .view;
    // Undo the Good: it no longer counts.
    write(&db, move |tx| {
        study::undo_review(tx, "undo", session, TimestampMs(NOON + 2 * MINUTE))
    })
    .unwrap();
    assert!(after_good.reviewed_count == 2);
    write(&db, move |tx| {
        study::change_session(
            tx,
            "end",
            session,
            SessionAction::End,
            TimestampMs(NOON + 3 * MINUTE),
        )
    })
    .unwrap();

    let summary = read(&db, move |tx| {
        learning::session_summary(tx, session, TimestampMs(NOON + 4 * MINUTE))
    });
    assert_eq!(summary.ratings, [1, 0, 0, 0]);
    assert_eq!(summary.reviewed_count, 1);
    assert_eq!(summary.again_cards.len(), 1);
    assert_eq!(summary.again_cards[0].front, "eins");
    assert_eq!(summary.session.active_ms, 3_000);
    assert_eq!(summary.deck_name.as_deref(), Some("Deck 1"));
    assert!(summary.first_completed);
    assert!(
        summary.remaining_due >= 1,
        "the undone and the learning card remain"
    );
    let next = summary.next_deck.expect("the other deck has new cards");
    assert_eq!(next.deck.id, other_deck);
    assert_eq!(
        next.due, 1,
        "siblings of one note are not shown on the same day"
    );

    // A second session is not the first completed one any more.
    let view = start(&db, "start-2", Some(other_deck), NOON + 5 * MINUTE);
    let second = view.session.id;
    rate(&db, &view, "r3", Rating::Good, NOON + 5 * MINUTE).unwrap();
    write(&db, move |tx| {
        study::change_session(
            tx,
            "end-2",
            second,
            SessionAction::End,
            TimestampMs(NOON + 6 * MINUTE),
        )
    })
    .unwrap();
    let summary = read(&db, move |tx| {
        learning::session_summary(tx, second, TimestampMs(NOON + 6 * MINUTE))
    });
    assert!(!summary.first_completed);
}

#[test]
fn the_dashboard_uses_todays_limits_the_open_session_and_a_zoned_streak() {
    let db = db();
    let words = deck(&db, 1, DeckType::Simple);
    for seed in 1..=30 {
        note(
            &db,
            seed,
            words,
            NoteTypeKind::Basic,
            &format!("Wort {seed}"),
            &format!("słowo {seed}"),
        );
    }
    let board = write(&db, |tx| dashboard::dashboard(tx, TimestampMs(NOON), &[])).unwrap();
    let summary = &board.decks[0];
    assert_eq!(
        summary.counts.new, 20,
        "the default preset admits 20 new cards a day"
    );
    assert_eq!(summary.counts.total, 30);
    assert_eq!(board.streak_days, 0);
    assert!(board.resumable.is_none());

    // Tokyo: the study day starts at 04:00 JST = 19:00 UTC. Ratings at 18:30 and 19:30 UTC fall on
    // one UTC study day but on two Tokyo study days, so the streak is 2 only with the zone.
    db.with_connection(|conn| {
        conn.execute("UPDATE collection SET time_zone = 'Asia/Tokyo'", [])
            .map(drop)
            .map_err(|e| AppError::Internal(e.to_string()))
    })
    .unwrap();
    let (before, after) = (NOON + 390 * MINUTE, NOON + 450 * MINUTE);
    let view = start(&db, "start", Some(words), before);
    let session = view.session.id;
    rate(&db, &view, "yesterday", Rating::Good, before).unwrap();
    let view = write(&db, move |tx| {
        study::open_study(tx, session, TimestampMs(after))
    })
    .unwrap();
    rate(&db, &view, "today", Rating::Good, after).unwrap();
    write(&db, move |tx| {
        study::change_session(
            tx,
            "pause",
            session,
            SessionAction::Pause,
            TimestampMs(after),
        )
    })
    .unwrap();

    let board = write(&db, move |tx| {
        dashboard::dashboard(tx, TimestampMs(after + MINUTE), &[])
    })
    .unwrap();
    assert_eq!(board.streak_days, 2);
    assert_eq!(board.studied_today, 1);
    let open = board.resumable.expect("the paused session is offered");
    assert_eq!(open.session_id, session);
    assert!(open.paused);
    assert_eq!(open.reviewed_count, 2);
    assert_eq!(
        board.decks[0].counts.new, 19,
        "one new card was introduced today"
    );

    // Two days later the streak is broken.
    let board = write(&db, move |tx| {
        dashboard::dashboard(tx, TimestampMs(after + 2 * DAY), &[])
    })
    .unwrap();
    assert_eq!(board.streak_days, 0);
}

// ---------------------------------------------------------------- recovery

#[test]
fn a_reopened_session_keeps_its_card_and_pause_and_a_committed_rating_is_not_repeated() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("collection.sqlite");
    let session: SessionId;
    let shown: CardId;
    let rated: CardId;
    let request: RateCard;
    {
        let db = Arc::new(SqliteDatabase::open(&path, &Fixed).unwrap().database);
        let words = deck(&db, 1, DeckType::Simple);
        for seed in 1..=3 {
            note(
                &db,
                seed,
                words,
                NoteTypeKind::Basic,
                &format!("w{seed}"),
                "x",
            );
        }
        let view = start(&db, "start", Some(words), NOON);
        session = view.session.id;
        let current = view.current.clone().unwrap();
        rated = current.id;
        let preview = read(&db, move |tx| {
            study::preview(tx, session, current.id, current.version, TimestampMs(NOON))
        });
        request = RateCard {
            operation_id: "rate-before-kill".into(),
            session_id: session,
            card_id: preview.card_id,
            card_version: preview.card_version,
            preview_token: preview.token,
            rating: Rating::Good,
            answer_ms: 2_000,
        };
        let input = request.clone();
        // Committed; the process dies before the UI sees the response.
        let outcome = write(&db, move |tx| {
            study::rate_card(tx, &input, TimestampMs(NOON))
        })
        .unwrap();
        shown = outcome.view.current.unwrap().id;
        write(&db, move |tx| {
            study::change_session(
                tx,
                "pause",
                session,
                SessionAction::Pause,
                TimestampMs(NOON),
            )
        })
        .unwrap();
    }
    let db = Arc::new(SqliteDatabase::open(&path, &Fixed).unwrap().database);
    let reopened = screen(&db, session, NOON + MINUTE);
    assert_eq!(
        reopened.view.session.status,
        SessionStatus::Paused,
        "pause survives"
    );
    assert_eq!(reopened.current.map(|card| card.card.id), Some(shown));
    assert_eq!(reopened.view.reviewed_count, 1);

    // The UI retries the request it never got an answer for: same result, no second log.
    write(&db, move |tx| {
        study::change_session(
            tx,
            "resume",
            session,
            SessionAction::Resume,
            TimestampMs(NOON),
        )
    })
    .unwrap();
    let retry = request.clone();
    let outcome = write(&db, move |tx| {
        study::rate_card(tx, &retry, TimestampMs(NOON + MINUTE))
    })
    .unwrap();
    assert!(outcome.replayed);
    assert_eq!(read(&db, move |tx| tx.review_logs(rated)).len(), 1);
}
