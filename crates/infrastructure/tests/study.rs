//! Scheduler and study use cases on the real SQLite unit of work (plan Phase 9 DoD):
//! ratings through FSRS into card + log + session in one transaction, idempotent retries,
//! stale previews, rollback, undo of the full snapshot with siblings and leech effects, daily
//! limits over parent decks, suspended/buried cards, chosen-card sessions, limit extensions,
//! the collection time zone and a session that survives a restart.

use std::sync::Arc;

use langualinker_application::ports::Clock;
use langualinker_application::repositories::{Repositories, SessionStatus, UnitOfWork};
use langualinker_application::study::{self, RateCard, SessionAction, StartStudy, StudyView};
use langualinker_application::{AppError, AppResult};
use langualinker_domain::card::{Card, CardFlag, CardState, QueueState};
use langualinker_domain::deck::{
    DeckConfigOptions, DeckType, LeechAction, NativeLanguage, NewDeck, TargetLanguage,
};
use langualinker_domain::fsrs::FSRS_VERSION;
use langualinker_domain::note::{NewNote, NoteFields, NoteTypeKind};
use langualinker_domain::queue::QueueCounts;
use langualinker_domain::review::{CardSnapshot, Rating};
use langualinker_domain::{CardId, DeckConfigId, DeckId, Guid, NoteId, SessionId, TimestampMs};
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
    bytes[8] = 0x5a;
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

fn options(change: impl FnOnce(&mut DeckConfigOptions)) -> DeckConfigOptions {
    let mut options = DeckConfigOptions {
        fuzz: false,
        ..DeckConfigOptions::default()
    };
    change(&mut options);
    options
}

fn preset(db: &Arc<SqliteDatabase>, seed: u64, options: DeckConfigOptions) -> DeckConfigId {
    write(db, move |tx| {
        Ok(tx
            .insert_config(&guid(seed), "Preset", &options, TimestampMs(0))?
            .id)
    })
    .unwrap()
}

fn deck(
    db: &Arc<SqliteDatabase>,
    seed: u64,
    parent: Option<DeckId>,
    config: DeckConfigId,
) -> DeckId {
    write(db, move |tx| {
        Ok(tx
            .insert_deck(
                &NewDeck {
                    guid: guid(seed),
                    name: format!("Deck {seed}"),
                    parent_id: parent,
                    description: String::new(),
                    native_language: NativeLanguage::Pl,
                    target_language: TargetLanguage::De,
                    deck_type: DeckType::Linguistic,
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

/// A note with one card per template of `kind`, all in `deck`. Returns the card ids by ordinal.
fn note(db: &Arc<SqliteDatabase>, seed: u64, deck: DeckId, kind: NoteTypeKind) -> Vec<CardId> {
    write(db, move |tx| {
        let note_type = tx.note_type(kind)?;
        let note = tx.insert_note(
            &NewNote {
                guid: guid(seed),
                note_type_id: note_type.id,
                fields: NoteFields {
                    front: format!("Wort {seed}"),
                    back: format!("słowo {seed}"),
                },
                tags: Vec::new(),
            },
            TimestampMs(0),
        )?;
        let mut ids = Vec::new();
        for template in tx.templates(note_type.id)? {
            let position = tx.next_position()?;
            let card = tx.insert_card(
                &langualinker_domain::card::NewCard {
                    guid: guid(seed * 100 + u64::from(template.ordinal) + 1),
                    note_id: note.id,
                    deck_id: deck,
                    template_ordinal: template.ordinal,
                    position,
                },
                TimestampMs(0),
            )?;
            ids.push(card.id);
        }
        Ok(ids)
    })
    .unwrap()
}

fn card(db: &Arc<SqliteDatabase>, id: CardId) -> Card {
    read(db, move |tx| Ok(tx.card(id)?.unwrap()))
}

fn start(
    db: &Arc<SqliteDatabase>,
    op: &str,
    deck: Option<DeckId>,
    cards: Option<Vec<CardId>>,
    at: i64,
) -> AppResult<StudyView> {
    let input = StartStudy {
        operation_id: op.into(),
        deck_id: deck,
        card_ids: cards,
    };
    write(db, move |tx| {
        study::start_study(tx, &input, TimestampMs(at))
    })
}

/// Previews and rates the session's current card.
fn rate(
    db: &Arc<SqliteDatabase>,
    view: &StudyView,
    op: &str,
    rating: Rating,
    at: i64,
) -> AppResult<study::RateOutcome> {
    let current = view.current.clone().expect("a current card");
    let session = view.session.id;
    let preview = write(db, move |tx| {
        study::preview(tx, session, current.id, current.version, TimestampMs(at))
    })?;
    let input = RateCard {
        operation_id: op.into(),
        session_id: session,
        card_id: preview.card_id,
        card_version: preview.card_version,
        preview_token: preview.token,
        rating,
        answer_ms: 4_000,
    };
    write(db, move |tx| study::rate_card(tx, &input, TimestampMs(at)))
}

fn due(db: &Arc<SqliteDatabase>, deck: Option<DeckId>, at: i64) -> QueueCounts {
    write(db, move |tx| study::due_counts(tx, deck, TimestampMs(at))).unwrap()
}

fn without_audit(mut card: Card) -> Card {
    card.version = 0;
    card.updated_at = TimestampMs(0);
    card.usn = 0;
    card
}

fn default_deck(db: &Arc<SqliteDatabase>) -> DeckId {
    deck(db, 1, None, DeckConfigId(1))
}

// ---------------------------------------------------------------- rating flow

#[test]
fn ratings_walk_learning_steps_into_review_and_log_everything() {
    let db = db();
    let deck = default_deck(&db);
    let first = note(&db, 1, deck, NoteTypeKind::Basic)[0];
    note(&db, 2, deck, NoteTypeKind::Basic);

    let view = start(&db, "start-1", Some(deck), None, NOON).unwrap();
    assert_eq!(view.session.status, SessionStatus::Active);
    assert_eq!(view.counts.new, 2);
    assert_eq!(view.current.as_ref().unwrap().id, first);

    let outcome = rate(&db, &view, "rate-1", Rating::Good, NOON).unwrap();
    assert!(!outcome.replayed);
    assert_eq!(outcome.reviewed.state, CardState::Learning);
    assert_eq!(
        outcome.reviewed.due_at,
        Some(TimestampMs(NOON + 10 * MINUTE))
    );
    assert_eq!(outcome.view.reviewed_count, 1);
    assert!(outcome.view.undo_available);
    assert_eq!(outcome.view.counts.new, 1);
    assert_eq!(outcome.view.counts.learning, 1);

    let rated = card(&db, first);
    assert_eq!(rated.reps, 1);
    assert_eq!(rated.learning_step, Some(1));
    assert_eq!(rated.fsrs.stability, Some(2.3065));
    let logs = read(&db, move |tx| tx.review_logs(first));
    assert_eq!(logs.len(), 1);
    let log = &logs[0];
    assert_eq!(log.fsrs_version, FSRS_VERSION);
    assert_eq!(log.parameters_hash.len(), 32);
    assert_eq!(log.card_before.state, CardState::New);
    assert_eq!(log.card_after.version, rated.version);
    assert_eq!(log.duration_ms, 4_000);
    assert_eq!(log.session_id, Some(view.session.id));

    // The other new card comes next; ten minutes later the learning card graduates.
    let second = rate(&db, &outcome.view, "rate-2", Rating::Good, NOON + MINUTE).unwrap();
    let later = NOON + 11 * MINUTE;
    let view = write(&db, move |tx| study::resume_study(tx, TimestampMs(later)))
        .unwrap()
        .unwrap();
    assert_eq!(
        view.current.as_ref().map(|c| c.id),
        Some(first),
        "{second:?}"
    );
    let graduated = rate(&db, &view, "rate-3", Rating::Good, later).unwrap();
    assert_eq!(graduated.reviewed.state, CardState::Review);
    let card = card(&db, first);
    assert_eq!(card.queue, QueueState::Review);
    assert_eq!(card.fsrs.scheduled_days, 2);
    // Due at the start (04:00 UTC) of the study day two days from today.
    let today_start = NOON - 8 * 60 * MINUTE;
    assert_eq!(card.due_at, Some(TimestampMs(today_start + 2 * DAY)));
}

#[test]
fn a_retried_operation_returns_the_first_result_without_rating_twice() {
    let db = db();
    let deck = default_deck(&db);
    let id = note(&db, 1, deck, NoteTypeKind::Basic)[0];
    let view = start(&db, "start", Some(deck), None, NOON).unwrap();
    let first = rate(&db, &view, "rate-once", Rating::Good, NOON).unwrap();

    // The same request again (double tap or a retry after a lost response).
    let session = view.session.id;
    let stale = view.current.clone().unwrap();
    let retry = RateCard {
        operation_id: "rate-once".into(),
        session_id: session,
        card_id: id,
        card_version: stale.version,
        preview_token: "whatever".into(),
        rating: Rating::Good,
        answer_ms: 1,
    };
    let again = write(&db, move |tx| {
        study::rate_card(tx, &retry, TimestampMs(NOON + 5))
    })
    .unwrap();
    assert!(again.replayed);
    assert_eq!(again.reviewed, first.reviewed);
    assert_eq!(card(&db, id).reps, 1);
    assert_eq!(read(&db, move |tx| tx.review_logs(id)).len(), 1);

    // The id cannot be reused for another card or another operation.
    let other = note(&db, 2, deck, NoteTypeKind::Basic)[0];
    let reused = RateCard {
        operation_id: "rate-once".into(),
        session_id: session,
        card_id: other,
        card_version: 1,
        preview_token: String::new(),
        rating: Rating::Easy,
        answer_ms: 1,
    };
    let error = write(&db, move |tx| {
        study::rate_card(tx, &reused, TimestampMs(NOON))
    })
    .unwrap_err();
    assert!(
        matches!(error, AppError::Validation(ref e) if e.reason == "reused"),
        "{error:?}"
    );
    let error = start(&db, "rate-once", None, None, NOON).unwrap_err();
    assert!(
        matches!(error, AppError::Validation(ref e) if e.reason == "reused"),
        "{error:?}"
    );
    // Starting twice with one operation returns the same session.
    let restarted = start(&db, "start", Some(deck), None, NOON + 10).unwrap();
    assert_eq!(restarted.session.id, view.session.id);
}

#[test]
fn stale_previews_and_card_versions_are_rejected() {
    let db = db();
    let deck = default_deck(&db);
    let id = note(&db, 1, deck, NoteTypeKind::Basic)[0];
    let view = start(&db, "start", Some(deck), None, NOON).unwrap();
    let session = view.session.id;
    let preview = write(&db, move |tx| {
        study::preview(tx, session, id, 1, TimestampMs(NOON))
    })
    .unwrap();
    assert_eq!(preview.intervals.len(), 4);
    assert_eq!(preview.intervals[0].interval_ms, MINUTE);
    assert_eq!(
        preview.intervals[1].interval_ms, 330_000,
        "Hard averages 1 and 10 minutes"
    );
    assert_eq!(preview.intervals[2].interval_ms, 10 * MINUTE);

    // The preset changes between preview and rating.
    write(&db, |tx| {
        let mut config = tx.default_config()?;
        config.options.learning_steps_minutes = vec![5];
        tx.update_config(&config, TimestampMs(NOON))?;
        Ok(())
    })
    .unwrap();
    let input = RateCard {
        operation_id: "rate".into(),
        session_id: session,
        card_id: id,
        card_version: 1,
        preview_token: preview.token.clone(),
        rating: Rating::Good,
        answer_ms: 1_000,
    };
    let stale = input.clone();
    let error = write(&db, move |tx| {
        study::rate_card(tx, &stale, TimestampMs(NOON))
    })
    .unwrap_err();
    assert_eq!(error, AppError::PreviewStale);

    // A card version the UI no longer has is a conflict, not a silent rating.
    let mut old = input;
    old.card_version = 0;
    let error = write(&db, move |tx| study::rate_card(tx, &old, TimestampMs(NOON))).unwrap_err();
    assert!(matches!(
        error,
        AppError::VersionConflict {
            entity: "card",
            current: 1
        }
    ));
    assert_eq!(card(&db, id).reps, 0);
}

#[test]
fn a_failure_inside_the_rating_rolls_back_card_siblings_log_and_session() {
    let db = db();
    let deck = default_deck(&db);
    let ids = note(&db, 1, deck, NoteTypeKind::BasicAndReversed);
    let view = start(&db, "start", Some(deck), None, NOON).unwrap();
    let before: Vec<Card> = ids.iter().map(|id| card(&db, *id)).collect();
    db.with_connection(|conn| {
        conn.execute_batch(
            "CREATE TRIGGER fail_log BEFORE INSERT ON review_logs BEGIN
                 SELECT RAISE(ABORT, 'disk full');
             END;",
        )
        .map_err(|e| AppError::Internal(e.to_string()))
    })
    .unwrap();
    assert!(rate(&db, &view, "rate", Rating::Good, NOON).is_err());
    let after: Vec<Card> = ids.iter().map(|id| card(&db, *id)).collect();
    assert_eq!(after, before, "card and buried sibling are rolled back");
    let session = view.session.id;
    let (items, reviewed) = read(&db, move |tx| {
        Ok((
            tx.session_items(session)?,
            tx.session_review_count(session)?,
        ))
    });
    assert!(items.is_empty());
    assert_eq!(reviewed, 0);

    // After the cause is gone the same operation commits once.
    db.with_connection(|conn| {
        conn.execute_batch("DROP TRIGGER fail_log;")
            .map_err(|e| AppError::Internal(e.to_string()))
    })
    .unwrap();
    rate(&db, &view, "rate", Rating::Good, NOON).unwrap();
    assert_eq!(card(&db, ids[0]).reps, 1);
}

// ---------------------------------------------------------------- undo

#[test]
fn undo_restores_the_card_its_siblings_and_the_counters() {
    let db = db();
    let deck = default_deck(&db);
    let ids = note(&db, 1, deck, NoteTypeKind::BasicAndReversed);
    note(&db, 2, deck, NoteTypeKind::Basic);
    let view = start(&db, "start", Some(deck), None, NOON).unwrap();
    assert_eq!(
        view.counts.new, 2,
        "one card per note while siblings are buried"
    );
    let before: Vec<Card> = ids.iter().map(|id| card(&db, *id)).collect();

    let rated = rate(&db, &view, "rate", Rating::Easy, NOON).unwrap();
    assert_eq!(rated.reviewed.state, CardState::Review);
    let sibling = card(&db, ids[1]);
    assert_eq!(sibling.queue, QueueState::SchedBuried);
    assert_eq!(due(&db, Some(deck), NOON).new, 1);

    let session = view.session.id;
    let undone = write(&db, move |tx| {
        study::undo_review(tx, "undo", session, TimestampMs(NOON + 1))
    })
    .unwrap();
    assert!(!undone.replayed);
    assert_eq!(undone.view.current.as_ref().map(|c| c.id), Some(ids[0]));
    assert_eq!(undone.view.reviewed_count, 0);
    assert!(!undone.view.undo_available);
    for (id, original) in ids.iter().zip(&before) {
        assert_eq!(
            without_audit(card(&db, *id)),
            without_audit(original.clone())
        );
    }
    let id = ids[0];
    let logs = read(&db, move |tx| tx.review_logs(id));
    assert_eq!(logs.len(), 1, "the log stays for audit");
    assert!(logs[0].invalidated_at.is_some());
    assert_eq!(due(&db, Some(deck), NOON).new, 2);

    // Only the latest rating can be undone, once; a retry of the undo is answered from the record.
    let error = write(&db, move |tx| {
        study::undo_review(tx, "undo-2", session, TimestampMs(NOON))
    })
    .unwrap_err();
    assert!(matches!(error, AppError::Validation(ref e) if e.reason == "nothingToUndo"));
    let retry = write(&db, move |tx| {
        study::undo_review(tx, "undo", session, TimestampMs(NOON))
    })
    .unwrap();
    assert!(retry.replayed);
    // Rating again after undo works and counts once.
    let view = write(&db, move |tx| {
        study::study_view(tx, session, TimestampMs(NOON + 2))
    })
    .unwrap();
    rate(&db, &view, "rate-again", Rating::Good, NOON + 2).unwrap();
    assert_eq!(card(&db, id).reps, 1);
}

#[test]
fn undo_is_refused_after_a_later_change() {
    let db = db();
    let deck = default_deck(&db);
    let id = note(&db, 1, deck, NoteTypeKind::Basic)[0];
    let view = start(&db, "start", Some(deck), None, NOON).unwrap();
    rate(&db, &view, "rate", Rating::Good, NOON).unwrap();
    write(&db, move |tx| {
        let mut card = tx.card(id)?.unwrap();
        card.flag = CardFlag::Red;
        tx.update_card(&card, TimestampMs(NOON + 1))?;
        Ok(())
    })
    .unwrap();
    let session = view.session.id;
    let error = write(&db, move |tx| {
        study::undo_review(tx, "undo", session, TimestampMs(NOON + 2))
    })
    .unwrap_err();
    assert!(
        matches!(error, AppError::VersionConflict { entity: "card", .. }),
        "{error:?}"
    );
    assert_eq!(card(&db, id).reps, 1);
}

#[test]
fn leeches_are_tagged_or_suspended_and_undo_reverses_it() {
    let db = db();
    let tag = preset(&db, 900, options(|o| o.leech_threshold = 2));
    let suspend = preset(
        &db,
        901,
        options(|o| {
            o.leech_threshold = 2;
            o.leech_action = LeechAction::Suspend;
        }),
    );
    let tagging = deck(&db, 1, None, tag);
    let suspending = deck(&db, 2, None, suspend);
    let tagged = note(&db, 1, tagging, NoteTypeKind::Basic)[0];
    let suspended = note(&db, 2, suspending, NoteTypeKind::Basic)[0];
    // Both cards are review cards with one lapse, due now.
    for id in [tagged, suspended] {
        write(&db, move |tx| {
            let mut card = tx.card(id)?.unwrap();
            card.state = CardState::Review;
            card.queue = QueueState::Review;
            card.due_at = Some(TimestampMs(NOON - DAY));
            card.fsrs.stability = Some(5.0);
            card.fsrs.difficulty = Some(6.0);
            card.fsrs.last_review_at = Some(TimestampMs(NOON - 6 * DAY));
            card.lapses = 1;
            card.reps = 4;
            tx.update_card(&card, TimestampMs(0))?;
            Ok(())
        })
        .unwrap();
    }

    let view = start(&db, "start-tag", Some(tagging), None, NOON).unwrap();
    rate(&db, &view, "lapse-tag", Rating::Again, NOON).unwrap();
    let note_id = card(&db, tagged).note_id;
    let note_tags =
        |db: &Arc<SqliteDatabase>, id: NoteId| read(db, move |tx| Ok(tx.note(id)?.unwrap().tags));
    assert_eq!(note_tags(&db, note_id), ["leech"]);
    let session = view.session.id;
    write(&db, move |tx| {
        study::undo_review(tx, "undo-tag", session, TimestampMs(NOON + 1))
    })
    .unwrap();
    assert!(note_tags(&db, note_id).is_empty());

    let view = start(&db, "start-suspend", Some(suspending), None, NOON).unwrap();
    let outcome = rate(&db, &view, "lapse-suspend", Rating::Again, NOON).unwrap();
    assert_eq!(card(&db, suspended).queue, QueueState::Suspended);
    assert!(
        outcome.view.current.is_none(),
        "a suspended leech leaves the queue"
    );
}

// ---------------------------------------------------------------- queue

#[test]
fn parent_and_child_limits_apply_to_the_tree() {
    let db = db();
    let parent_preset = preset(&db, 900, options(|o| o.new_per_day = 3));
    let child_preset = preset(&db, 901, options(|o| o.new_per_day = 2));
    let parent = deck(&db, 1, None, parent_preset);
    let child = deck(&db, 2, Some(parent), child_preset);
    for seed in 0..4 {
        note(&db, 10 + seed, child, NoteTypeKind::Basic);
        note(&db, 20 + seed, parent, NoteTypeKind::Basic);
    }
    assert_eq!(due(&db, Some(child), NOON).new, 2);
    assert_eq!(due(&db, Some(parent), NOON).new, 3);
    assert_eq!(due(&db, None, NOON).new, 3);

    // Two new cards introduced in the child use the child's limit and the parent's.
    let view = start(&db, "start", Some(child), None, NOON).unwrap();
    let view = rate(&db, &view, "a", Rating::Good, NOON).unwrap().view;
    let view = rate(&db, &view, "b", Rating::Good, NOON + 1).unwrap().view;
    assert_eq!(view.counts.new, 0);
    assert!(view.limit_reached.new);
    assert_eq!(due(&db, Some(parent), NOON + 2).new, 1);
    // Tomorrow the limits start again.
    assert_eq!(due(&db, Some(child), NOON + DAY).new, 2);
}

#[test]
fn suspended_and_buried_cards_never_reach_the_queue() {
    let db = db();
    let deck = default_deck(&db);
    let suspended = note(&db, 1, deck, NoteTypeKind::Basic)[0];
    let buried = note(&db, 2, deck, NoteTypeKind::Basic)[0];
    let visible = note(&db, 3, deck, NoteTypeKind::Basic)[0];
    write(&db, move |tx| {
        let mut card = tx.card(suspended)?.unwrap();
        card.queue = QueueState::Suspended;
        tx.update_card(&card, TimestampMs(0))?;
        let mut card = tx.card(buried)?.unwrap();
        card.queue = QueueState::UserBuried;
        card.buried_until = Some(TimestampMs(NOON + DAY / 2));
        tx.update_card(&card, TimestampMs(0))?;
        Ok(())
    })
    .unwrap();
    assert_eq!(due(&db, Some(deck), NOON).new, 1);
    let view = start(&db, "start", Some(deck), None, NOON).unwrap();
    assert_eq!(view.current.as_ref().map(|c| c.id), Some(visible));
    let after = rate(&db, &view, "rate", Rating::Easy, NOON).unwrap();
    assert!(after.view.current.is_none());
    assert_eq!(after.view.session.status, SessionStatus::Finished);

    // Chosen-card sessions refuse suspended cards.
    let error = start(&db, "chosen", None, Some(vec![suspended]), NOON).unwrap_err();
    assert!(matches!(error, AppError::Validation(ref e) if e.reason == "notStudyable"));

    // The next study day lifts the burial (not the suspension).
    assert_eq!(due(&db, Some(deck), NOON + DAY).new, 1);
    assert_eq!(card(&db, buried).queue, QueueState::New);
    assert_eq!(card(&db, suspended).queue, QueueState::Suspended);
}

#[test]
fn reviews_and_new_cards_interleave_and_overdue_cards_are_counted() {
    let db = db();
    let deck = default_deck(&db);
    let mut reviews = Vec::new();
    for seed in 0..4 {
        let id = note(&db, 10 + seed, deck, NoteTypeKind::Basic)[0];
        let due_at = NOON - (seed as i64) * DAY;
        write(&db, move |tx| {
            let mut card = tx.card(id)?.unwrap();
            card.state = CardState::Review;
            card.queue = QueueState::Review;
            card.due_at = Some(TimestampMs(due_at));
            card.fsrs.stability = Some(10.0);
            card.fsrs.difficulty = Some(5.0);
            card.fsrs.last_review_at = Some(TimestampMs(due_at - 10 * DAY));
            tx.update_card(&card, TimestampMs(0))?;
            Ok(())
        })
        .unwrap();
        reviews.push(id);
    }
    note(&db, 20, deck, NoteTypeKind::Basic);
    note(&db, 21, deck, NoteTypeKind::Basic);
    let counts = due(&db, Some(deck), NOON);
    assert_eq!(
        counts,
        QueueCounts {
            new: 2,
            learning: 0,
            review: 4,
            overdue: 3
        }
    );

    let mut view = start(&db, "start", Some(deck), None, NOON).unwrap();
    let mut kinds = Vec::new();
    for step in 0..6 {
        let current = view.current.clone().unwrap();
        kinds.push(current.state);
        view = rate(&db, &view, &format!("r{step}"), Rating::Easy, NOON + step)
            .unwrap()
            .view;
    }
    // Reviews come first and new cards are mixed in, not left to the end.
    assert_eq!(kinds[0], CardState::Review);
    assert_eq!(kinds.iter().filter(|k| **k == CardState::New).count(), 2);
    let first_new = kinds.iter().position(|k| *k == CardState::New).unwrap();
    assert!(first_new < 4, "{kinds:?}");
    assert_eq!(view.session.status, SessionStatus::Finished);
}

#[test]
fn chosen_card_sessions_ignore_limits_and_bring_learning_cards_back() {
    let db = db();
    let zero = preset(&db, 900, options(|o| o.new_per_day = 0));
    let deck = deck(&db, 1, None, zero);
    let a = note(&db, 1, deck, NoteTypeKind::Basic)[0];
    let b = note(&db, 2, deck, NoteTypeKind::Basic)[0];
    note(&db, 3, deck, NoteTypeKind::Basic);
    assert_eq!(due(&db, Some(deck), NOON).new, 0);

    let view = start(&db, "chosen", None, Some(vec![b, a, b]), NOON).unwrap();
    assert!(view.session.restricted);
    assert_eq!(view.counts.review, 2);
    assert_eq!(view.current.as_ref().map(|c| c.id), Some(b));
    let view = rate(&db, &view, "r1", Rating::Again, NOON).unwrap().view;
    assert_eq!(view.current.as_ref().map(|c| c.id), Some(a));
    let view = rate(&db, &view, "r2", Rating::Easy, NOON + 1).unwrap().view;
    // Only the failed card is left; it comes back within the learn-ahead window.
    assert_eq!(view.current.as_ref().map(|c| c.id), Some(b));
    assert_eq!(
        view.counts,
        QueueCounts {
            new: 0,
            learning: 1,
            review: 0,
            overdue: 0
        }
    );
}

#[test]
fn extending_todays_limit_shows_more_new_cards_for_today_only() {
    let db = db();
    let one = preset(&db, 900, options(|o| o.new_per_day = 1));
    let deck = deck(&db, 1, None, one);
    for seed in 0..5 {
        note(&db, seed + 1, deck, NoteTypeKind::Basic);
    }
    assert_eq!(due(&db, Some(deck), NOON).new, 1);
    let counts = write(&db, move |tx| {
        study::extend_today_limits(tx, "extend", deck, 2, 0, TimestampMs(NOON))
    })
    .unwrap();
    assert_eq!(counts.new, 3);
    let again = write(&db, move |tx| {
        study::extend_today_limits(tx, "extend", deck, 2, 0, TimestampMs(NOON))
    })
    .unwrap();
    assert_eq!(again.new, 3, "a retried extension is applied once");
    assert_eq!(due(&db, Some(deck), NOON + DAY).new, 1);
    let error = write(&db, move |tx| {
        study::extend_today_limits(tx, "too-much", deck, 10_000, 0, TimestampMs(NOON))
    })
    .unwrap_err();
    assert!(matches!(error, AppError::Validation(ref e) if e.field == "extraNew"));
}

#[test]
fn the_collection_time_zone_sets_the_study_day() {
    let db = db();
    db.with_connection(|conn| {
        conn.execute("UPDATE collection SET time_zone = 'Asia/Tokyo'", [])
            .map(drop)
            .map_err(|e| AppError::Internal(e.to_string()))
    })
    .unwrap();
    let deck = default_deck(&db);
    let id = note(&db, 1, deck, NoteTypeKind::Basic)[0];
    // NOON UTC is 21:00 in Tokyo; Easy schedules for the start of a later Tokyo study day,
    // which begins at 04:00 JST = 19:00 UTC of the previous UTC date.
    let jst_start = NOON - 17 * 60 * MINUTE; // 2026-03-01 19:00 UTC = 2026-03-02 04:00 JST
    let view = start(&db, "start", Some(deck), None, NOON).unwrap();
    rate(&db, &view, "rate", Rating::Easy, NOON).unwrap();
    let card = card(&db, id);
    let due = card.due_at.unwrap().0;
    assert_eq!((due - jst_start) % DAY, 0, "due at a Tokyo cutoff");
    assert_eq!(card.fsrs.scheduled_days, (due - jst_start) / DAY);
}

#[test]
fn a_session_survives_a_restart_and_can_be_paused_and_ended() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("collection.sqlite");
    let session: SessionId;
    let first: CardId;
    let second: CardId;
    {
        let db = Arc::new(SqliteDatabase::open(&path, &Fixed).unwrap().database);
        let deck = default_deck(&db);
        first = note(&db, 1, deck, NoteTypeKind::Basic)[0];
        second = note(&db, 2, deck, NoteTypeKind::Basic)[0];
        let view = start(&db, "start", Some(deck), None, NOON).unwrap();
        session = view.session.id;
        rate(&db, &view, "rate", Rating::Again, NOON).unwrap();
        let paused = write(&db, move |tx| {
            study::change_session(
                tx,
                "pause",
                session,
                SessionAction::Pause,
                TimestampMs(NOON + 1),
            )
        })
        .unwrap();
        assert_eq!(paused.session.status, SessionStatus::Paused);
    }
    let db = Arc::new(SqliteDatabase::open(&path, &Fixed).unwrap().database);
    let resumed = write(&db, |tx| {
        study::resume_study(tx, TimestampMs(NOON + 2 * MINUTE))
    })
    .unwrap()
    .unwrap();
    assert_eq!(resumed.session.id, session);
    assert_eq!(resumed.session.status, SessionStatus::Active);
    assert_eq!(resumed.reviewed_count, 1);
    assert!(resumed.undo_available);
    // The card that was on screen when the session paused is shown again.
    assert_eq!(resumed.current.as_ref().map(|c| c.id), Some(second));

    let stats = read(&db, move |tx| {
        study::session_stats(tx, session, TimestampMs(NOON + 3 * MINUTE))
    });
    assert_eq!(stats.ratings, [1, 0, 0, 0]);
    assert_eq!(stats.again_cards, [first]);
    assert_eq!(stats.session.active_ms, 4_000);

    let ended = write(&db, move |tx| {
        study::change_session(
            tx,
            "end",
            session,
            SessionAction::End,
            TimestampMs(NOON + 4 * MINUTE),
        )
    })
    .unwrap();
    assert_eq!(ended.session.status, SessionStatus::Finished);
    assert!(
        write(&db, |tx| study::resume_study(tx, TimestampMs(NOON)))
            .unwrap()
            .is_none()
    );
    let error = write(&db, move |tx| {
        study::change_session(
            tx,
            "pause-2",
            session,
            SessionAction::Pause,
            TimestampMs(NOON),
        )
    })
    .unwrap_err();
    assert!(matches!(error, AppError::Validation(ref e) if e.reason == "finished"));
}

#[test]
fn ratings_on_a_paused_or_foreign_session_are_refused() {
    let db = db();
    let a = default_deck(&db);
    let b = deck(&db, 2, None, DeckConfigId(1));
    let in_a = note(&db, 1, a, NoteTypeKind::Basic)[0];
    let in_b = note(&db, 2, b, NoteTypeKind::Basic)[0];
    let view = start(&db, "start", Some(a), None, NOON).unwrap();
    assert_eq!(view.current.as_ref().map(|c| c.id), Some(in_a));
    let session = view.session.id;
    let foreign = RateCard {
        operation_id: "foreign".into(),
        session_id: session,
        card_id: in_b,
        card_version: 1,
        preview_token: String::new(),
        rating: Rating::Good,
        answer_ms: 0,
    };
    let error = write(&db, move |tx| {
        study::rate_card(tx, &foreign, TimestampMs(NOON))
    })
    .unwrap_err();
    assert!(matches!(error, AppError::Validation(ref e) if e.reason == "outsideSession"));
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
    let error = rate(&db, &view, "paused", Rating::Good, NOON).unwrap_err();
    assert!(matches!(error, AppError::Validation(ref e) if e.reason == "paused"));
    // Starting another session finishes this one.
    let next = start(&db, "start-b", Some(b), None, NOON).unwrap();
    let old = read(&db, move |tx| Ok(tx.session(session)?.unwrap()));
    assert_eq!(old.status, SessionStatus::Finished);
    assert_eq!(next.current.as_ref().map(|c| c.id), Some(in_b));
}

#[test]
fn snapshots_in_logs_capture_every_scheduling_field() {
    let db = db();
    let deck = default_deck(&db);
    let id = note(&db, 1, deck, NoteTypeKind::Basic)[0];
    let view = start(&db, "start", Some(deck), None, NOON).unwrap();
    let before = card(&db, id);
    rate(&db, &view, "rate", Rating::Hard, NOON).unwrap();
    let after = card(&db, id);
    let log = read(&db, move |tx| {
        Ok(tx.review_log_by_operation("rate")?.unwrap())
    });
    assert_eq!(log.card_before, CardSnapshot::of(&before));
    assert_eq!(log.card_after, CardSnapshot::of(&after));
    assert_eq!(after.due_at, Some(TimestampMs(NOON + 330_000)));
}

// ---------------------------------------------------------------- open_study and scope_counts

fn open(db: &Arc<SqliteDatabase>, session: SessionId, at: i64) -> AppResult<StudyView> {
    write(db, move |tx| {
        study::open_study(tx, session, TimestampMs(at))
    })
}

/// Today's counts of a scope, computed read-only (no burials are lifted).
fn scope(db: &Arc<SqliteDatabase>, deck: Option<DeckId>, at: i64) -> QueueCounts {
    write(db, move |tx| {
        let moment = study::study_moment(tx, TimestampMs(at))?;
        study::scope_counts(tx, deck, &moment)
    })
    .unwrap()
}

/// Buries a card by hand with a burial that ended before `NOON`, as a previous day would leave it.
fn bury_expired(db: &Arc<SqliteDatabase>, id: CardId) {
    write(db, move |tx| {
        let mut card = tx.card(id)?.unwrap();
        card.queue = QueueState::SchedBuried;
        card.buried_until = Some(TimestampMs(NOON - 1));
        tx.update_card(&card, TimestampMs(NOON))?;
        Ok(())
    })
    .unwrap();
}

#[test]
fn open_study_keeps_a_paused_session_paused_on_the_same_card() {
    let db = db();
    let deck = default_deck(&db);
    for seed in 1..=3 {
        note(&db, seed, deck, NoteTypeKind::Basic);
    }
    let view = start(&db, "start", Some(deck), None, NOON).unwrap();
    let session = view.session.id;
    let after = rate(&db, &view, "rate", Rating::Good, NOON).unwrap().view;
    let shown = after.current.clone().map(|card| card.id);
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

    let reopened = open(&db, session, NOON + MINUTE).unwrap();
    assert_eq!(reopened.session.status, SessionStatus::Paused);
    assert_eq!(reopened.current.map(|card| card.id), shown);
    assert_eq!(reopened.reviewed_count, 1);
}

#[test]
fn open_study_returns_a_finished_session_without_changing_it() {
    let db = db();
    let deck = default_deck(&db);
    note(&db, 1, deck, NoteTypeKind::Basic);
    note(&db, 2, deck, NoteTypeKind::Basic);
    let view = start(&db, "start", Some(deck), None, NOON).unwrap();
    let session = view.session.id;
    rate(&db, &view, "rate", Rating::Good, NOON).unwrap();
    write(&db, move |tx| {
        study::change_session(tx, "end", session, SessionAction::End, TimestampMs(NOON))
    })
    .unwrap();

    let finished = open(&db, session, NOON + MINUTE).unwrap();
    assert_eq!(finished.session.status, SessionStatus::Finished);
    assert_eq!(finished.session.finished_at, Some(TimestampMs(NOON)));
    assert!(finished.current.is_none());
    assert_eq!(finished.reviewed_count, 1);
}

#[test]
fn open_study_returns_expired_burials_to_their_queues() {
    let db = db();
    let deck = default_deck(&db);
    let first = note(&db, 1, deck, NoteTypeKind::Basic)[0];
    note(&db, 2, deck, NoteTypeKind::Basic);
    let view = start(&db, "start", Some(deck), None, NOON).unwrap();
    bury_expired(&db, first);

    let opened = open(&db, view.session.id, NOON).unwrap();
    assert_eq!(card(&db, first).queue, QueueState::New);
    assert_eq!(opened.counts.new, 2, "the lifted card is due again");
}

#[test]
fn scope_counts_does_not_lift_burials_but_due_counts_does() {
    let db = db();
    let deck = default_deck(&db);
    let first = note(&db, 1, deck, NoteTypeKind::Basic)[0];
    note(&db, 2, deck, NoteTypeKind::Basic);
    bury_expired(&db, first);

    let read_only = scope(&db, Some(deck), NOON);
    assert_eq!(
        read_only.new, 1,
        "the buried card is not in today's queue yet"
    );
    assert_eq!(
        card(&db, first).queue,
        QueueState::SchedBuried,
        "nothing was written"
    );

    let counted = due(&db, Some(deck), NOON);
    assert_eq!(counted.new, 2);
    assert_eq!(card(&db, first).queue, QueueState::New);
}

#[test]
fn scope_counts_only_covers_the_chosen_deck_tree() {
    let db = db();
    let first = default_deck(&db);
    let second = deck(&db, 2, None, DeckConfigId(1));
    note(&db, 1, first, NoteTypeKind::Basic);
    note(&db, 2, first, NoteTypeKind::Basic);
    for seed in 3..=5 {
        note(&db, seed, second, NoteTypeKind::Basic);
    }
    assert_eq!(scope(&db, Some(first), NOON).new, 2);
    assert_eq!(scope(&db, Some(second), NOON).new, 3);
    assert_eq!(scope(&db, None, NOON).new, 5, "all decks together");
}
