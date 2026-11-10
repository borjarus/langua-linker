//! Learning commands (plan Phase 10): sessions, rating preview, rating, undo, card actions, the
//! word sheet and the session summary, mapped to contract v1 DTOs.
//!
//! Every mutation runs in one write transaction and is idempotent by `operationId`; a committed
//! change emits `collection_changed` as a read signal (a retry that changed nothing does not).

use langualinker_application::AppError;
use langualinker_application::learning::{
    self, CardAction, LinkView, NoteContent, SessionSummary, StudyCard, StudyScreen,
};
use langualinker_application::llm::{self, Generation, GenerationState};
use langualinker_application::ports::CollectionScope;
use langualinker_application::repositories::{SessionStatus, UnitOfWork};
use langualinker_application::study::{
    self, IntervalPreview, RateCard, RatingPreview, ReviewedCard, SessionAction, StartStudy,
};
use langualinker_contracts::ErrorDto;
use langualinker_contracts::dto::{
    AssociationDto, AssociationProvenance, AssociationStatus, CardActionInput, CardActionKind,
    CardActionResultDto, CardDirection, CardFlag, CardState, DeckCountsDto, GrammarTipDto,
    IntervalPreviewDto, NextDeckDto, NoteContentDto, PreviewInput, QueueState, RateCardInput,
    Rating, RatingCountsDto, RatingIntervalsDto, RatingPreviewDto, ResolveWordLinkInput,
    ReviewResultDto, ReviewedCardDto, SessionCardDto, SessionSummaryDto,
    SetFavoriteAssociationInput, StartStudyInput, StudyCardDto, StudySessionAction,
    StudySessionDto, StudySessionStatus, UndoInput, UpdateStudySessionInput, WordCandidateDto,
    WordLinkDto, WordLinkResolution,
};
use langualinker_domain::content::{Association, GrammarTip};
use langualinker_domain::queue::QueueCounts;
use langualinker_domain::{
    AssociationId, CardId, DeckId, NoteId, SessionId, TimestampMs, ValidationError, WordLinkId,
    card, content, note, review,
};
use serde_json::Value;
use tauri::State;

use crate::backend::{Backend, Collection, error_dto, iso};
use crate::commands::{blocking, operation_id, parse_input};
use crate::content::{capabilities, clamp, deck_type, target_language};

// ---------------------------------------------------------------- input helpers

/// Parses a transported id (`Id` is a decimal string of a positive SQLite rowid).
pub(crate) fn id(field: &'static str, value: &str) -> Result<i64, ErrorDto> {
    value
        .parse::<i64>()
        .ok()
        .filter(|id| *id > 0 && value.len() <= 19)
        .ok_or_else(|| error_dto(&ValidationError::new(field, "format").into()))
}

pub(crate) fn app_error(error: AppError) -> ErrorDto {
    error_dto(&error)
}

pub(crate) fn emit(collection: &Collection, scopes: &[CollectionScope], at: TimestampMs) {
    collection.events.collection_changed(scopes, at);
}

// ---------------------------------------------------------------- mapping

pub(crate) fn text_id(value: i64) -> String {
    value.to_string()
}

pub(crate) fn counts_dto(counts: QueueCounts) -> DeckCountsDto {
    DeckCountsDto {
        new: counts.new,
        learning: counts.learning,
        review: counts.review,
        overdue: counts.overdue,
        total: counts.new + counts.learning + counts.review,
    }
}

fn status_dto(status: SessionStatus) -> StudySessionStatus {
    match status {
        SessionStatus::Active => StudySessionStatus::Active,
        SessionStatus::Paused => StudySessionStatus::Paused,
        SessionStatus::Finished => StudySessionStatus::Finished,
    }
}

pub(crate) fn card_state(state: card::CardState) -> CardState {
    match state {
        card::CardState::New => CardState::New,
        card::CardState::Learning => CardState::Learning,
        card::CardState::Review => CardState::Review,
        card::CardState::Relearning => CardState::Relearning,
    }
}

pub(crate) fn queue_state(queue: card::QueueState) -> QueueState {
    match queue {
        card::QueueState::New => QueueState::New,
        card::QueueState::Learning => QueueState::Learning,
        card::QueueState::Review => QueueState::Review,
        card::QueueState::DayLearn => QueueState::DayLearn,
        card::QueueState::Suspended => QueueState::Suspended,
        card::QueueState::SchedBuried => QueueState::SchedBuried,
        card::QueueState::UserBuried => QueueState::UserBuried,
    }
}

pub(crate) fn flag_dto(flag: card::CardFlag) -> CardFlag {
    match flag {
        card::CardFlag::None => CardFlag::None,
        card::CardFlag::Red => CardFlag::Red,
        card::CardFlag::Orange => CardFlag::Orange,
        card::CardFlag::Green => CardFlag::Green,
        card::CardFlag::Blue => CardFlag::Blue,
        card::CardFlag::Pink => CardFlag::Pink,
        card::CardFlag::Turquoise => CardFlag::Turquoise,
        card::CardFlag::Purple => CardFlag::Purple,
    }
}

pub(crate) fn domain_flag(flag: CardFlag) -> card::CardFlag {
    match flag {
        CardFlag::None => card::CardFlag::None,
        CardFlag::Red => card::CardFlag::Red,
        CardFlag::Orange => card::CardFlag::Orange,
        CardFlag::Green => card::CardFlag::Green,
        CardFlag::Blue => card::CardFlag::Blue,
        CardFlag::Pink => card::CardFlag::Pink,
        CardFlag::Turquoise => card::CardFlag::Turquoise,
        CardFlag::Purple => card::CardFlag::Purple,
    }
}

pub(crate) fn direction_dto(direction: note::CardDirection) -> CardDirection {
    match direction {
        note::CardDirection::Forward => CardDirection::Forward,
        note::CardDirection::Reverse => CardDirection::Reverse,
    }
}

fn domain_rating(rating: Rating) -> review::Rating {
    match rating {
        Rating::Again => review::Rating::Again,
        Rating::Hard => review::Rating::Hard,
        Rating::Good => review::Rating::Good,
        Rating::Easy => review::Rating::Easy,
    }
}

pub(crate) fn association_dto(association: Association) -> AssociationDto {
    AssociationDto {
        id: text_id(association.id.0),
        note_id: text_id(association.note_id.0),
        text: association.text,
        provenance: match association.provenance {
            content::AssociationProvenance::Bundled => AssociationProvenance::Bundled,
            content::AssociationProvenance::Generated => AssociationProvenance::Generated,
            content::AssociationProvenance::Manual => AssociationProvenance::Manual,
        },
        favorite: association.is_favorite,
        // Stored associations are ready; requests not attached yet are placeholders (below).
        status: AssociationStatus::Ready,
        model: association.model,
        error: None,
    }
}

/// A requested association that is not attached yet (Phase 13): pending, offline queued,
/// failed or blocked. Its id is `llm-<request id>`; deleting it dismisses the request.
pub(crate) fn generation_dto(generation: Generation) -> AssociationDto {
    AssociationDto {
        id: generation.placeholder_id(),
        note_id: text_id(generation.note_id.0),
        text: String::new(),
        provenance: AssociationProvenance::Generated,
        favorite: false,
        status: match generation.state {
            GenerationState::Pending => AssociationStatus::Pending,
            GenerationState::OfflineQueued => AssociationStatus::OfflineQueued,
            GenerationState::Error => AssociationStatus::Error,
            GenerationState::Blocked => AssociationStatus::Blocked,
        },
        model: Some(generation.model),
        error: generation.error.as_ref().map(error_dto),
    }
}

/// Stored associations first, then placeholders in request order.
pub(crate) fn associations_dto(
    stored: Vec<Association>,
    generations: Vec<Generation>,
) -> Vec<AssociationDto> {
    stored
        .into_iter()
        .map(association_dto)
        .chain(generations.into_iter().map(generation_dto))
        .collect()
}

fn tip_dto(tip: GrammarTip, order: usize) -> GrammarTipDto {
    GrammarTipDto {
        id: text_id(tip.id.0),
        note_id: text_id(tip.note_id.0),
        text: tip.text,
        order: u32::try_from(order).unwrap_or(u32::MAX),
    }
}

pub(crate) fn tips_dto(tips: Vec<GrammarTip>) -> Vec<GrammarTipDto> {
    tips.into_iter()
        .enumerate()
        .map(|(order, tip)| tip_dto(tip, order))
        .collect()
}

pub(crate) fn link_dto(view: LinkView) -> WordLinkDto {
    let link = view.link;
    WordLinkDto {
        id: text_id(link.id.0),
        sentence_note_id: text_id(link.sentence_note_id.0),
        surface: link.surface_form,
        start_index: link.start_index,
        end_index: link.end_index,
        lemma: link.lemma,
        resolution: match link.resolution {
            content::WordLinkResolution::Resolved => WordLinkResolution::Resolved,
            content::WordLinkResolution::Unresolved => WordLinkResolution::Unresolved,
            content::WordLinkResolution::Ambiguous => WordLinkResolution::Ambiguous,
            content::WordLinkResolution::UserOverridden => WordLinkResolution::UserOverridden,
        },
        word_note_id: link.word_note_id.map(|id| text_id(id.0)),
        translation: view.translation,
        candidates: view
            .candidates
            .into_iter()
            .map(|candidate| WordCandidateDto {
                note_id: text_id(candidate.note_id.0),
                lemma: candidate.lemma,
                translation: candidate.translation,
            })
            .collect(),
    }
}

fn study_card_dto(card: StudyCard) -> StudyCardDto {
    StudyCardDto {
        card_id: text_id(card.card.id.0),
        note_id: text_id(card.note.id.0),
        deck_id: text_id(card.deck.id.0),
        version: clamp(card.card.version),
        deck_type: deck_type(card.deck.deck_type),
        capabilities: capabilities(card.deck.deck_type),
        direction: direction_dto(card.direction),
        state: card_state(card.card.state),
        flag: flag_dto(card.card.flag),
        front: card.front,
        back: card.back,
        target_language: target_language(card.deck.target_language),
        associations: associations_dto(card.associations, card.generations),
        grammar_tips: tips_dto(card.grammar_tips),
        word_links: card.word_links.into_iter().map(link_dto).collect(),
    }
}

fn session_dto(screen: StudyScreen) -> StudySessionDto {
    let session = &screen.view.session;
    StudySessionDto {
        session_id: text_id(session.id.0),
        deck_id: session.deck_id.map(|id| text_id(id.0)),
        deck_name: screen.deck_name.clone(),
        status: status_dto(session.status),
        started_at: iso(session.started_at),
        counts: counts_dto(screen.view.counts),
        reviewed_count: screen.view.reviewed_count,
        undo_available: screen.view.undo_available,
        current: screen.current.map(study_card_dto),
    }
}

fn reviewed_dto(card: ReviewedCard) -> ReviewedCardDto {
    ReviewedCardDto {
        card_id: text_id(card.card_id.0),
        version: clamp(card.version),
        state: card_state(card.state),
        due_at: card.due_at.map(iso),
    }
}

fn review_result_dto(
    operation_id: String,
    reviewed: ReviewedCard,
    screen: StudyScreen,
) -> ReviewResultDto {
    let session = session_dto(screen);
    ReviewResultDto {
        operation_id,
        session_id: session.session_id,
        reviewed: reviewed_dto(reviewed),
        next: session.current,
        counts: session.counts,
        reviewed_count: session.reviewed_count,
        undo_available: session.undo_available,
        session_status: session.status,
    }
}

fn interval_dto(interval: &IntervalPreview) -> IntervalPreviewDto {
    IntervalPreviewDto {
        due_at: iso(interval.due_at),
        interval_seconds: clamp(interval.interval_ms.max(0) / 1_000),
    }
}

fn preview_dto(preview: RatingPreview) -> Result<RatingPreviewDto, ErrorDto> {
    let find = |rating: review::Rating| {
        preview
            .intervals
            .iter()
            .find(|interval| interval.rating == rating)
            .map(interval_dto)
            .ok_or_else(|| app_error(AppError::Internal("preview without a rating".into())))
    };
    Ok(RatingPreviewDto {
        card_id: text_id(preview.card_id.0),
        card_version: clamp(preview.card_version),
        intervals: RatingIntervalsDto {
            again: find(review::Rating::Again)?,
            hard: find(review::Rating::Hard)?,
            good: find(review::Rating::Good)?,
            easy: find(review::Rating::Easy)?,
        },
        preview_token: preview.token,
    })
}

fn note_content_dto(content: NoteContent) -> NoteContentDto {
    NoteContentDto {
        note_id: text_id(content.note.id.0),
        deck_id: text_id(content.deck.id.0),
        front: content.note.fields.front,
        back: content.note.fields.back,
        target_language: target_language(content.deck.target_language),
        capabilities: capabilities(content.deck.deck_type),
        associations: associations_dto(content.associations, content.generations),
        grammar_tips: tips_dto(content.grammar_tips),
    }
}

fn summary_dto(summary: SessionSummary) -> SessionSummaryDto {
    let [again, hard, good, easy] = summary.ratings;
    SessionSummaryDto {
        session_id: text_id(summary.session.id.0),
        deck_id: summary.session.deck_id.map(|id| text_id(id.0)),
        deck_name: summary.deck_name,
        status: status_dto(summary.session.status),
        ratings: RatingCountsDto {
            again,
            hard,
            good,
            easy,
        },
        reviewed_count: summary.reviewed_count,
        active_ms: clamp(summary.session.active_ms),
        again_cards: summary
            .again_cards
            .into_iter()
            .map(|card| SessionCardDto {
                card_id: text_id(card.card_id.0),
                front: card.front,
                back: card.back,
                target_language: target_language(card.deck.target_language),
                direction: direction_dto(card.direction),
            })
            .collect(),
        next_due_at: summary.next_due_at.map(iso),
        remaining_due: summary.remaining_due,
        next_deck: summary.next_deck.map(|next| NextDeckDto {
            id: text_id(next.deck.id.0),
            name: next.deck.name,
            due: next.due,
        }),
        first_completed: summary.first_completed,
        started_at: iso(summary.session.started_at),
        finished_at: summary.session.finished_at.map(iso),
    }
}

// ---------------------------------------------------------------- commands

#[tauri::command]
pub async fn start_study(
    backend: State<'_, Backend>,
    input: Value,
) -> Result<StudySessionDto, ErrorDto> {
    let input: StartStudyInput = parse_input(input)?;
    operation_id(&input.operation_id)?;
    let deck_id = input
        .deck_id
        .as_deref()
        .map(|value| id("deckId", value).map(DeckId))
        .transpose()?;
    let card_ids = match input.card_ids {
        None => None,
        Some(ids) => {
            if ids.len() > study::MAX_CHOSEN_CARDS {
                return Err(error_dto(&ValidationError::new("cardIds", "count").into()));
            }
            Some(
                ids.iter()
                    .map(|value| id("cardIds", value).map(CardId))
                    .collect::<Result<Vec<_>, _>>()?,
            )
        }
    };
    let collection = backend.collection()?;
    let (db, now) = (collection.db.clone(), collection.clock.now());
    blocking(move || {
        db.write(move |tx| {
            let start = StartStudy {
                operation_id: input.operation_id,
                deck_id,
                card_ids,
            };
            let view = study::start_study(tx, &start, now)?;
            learning::screen(tx, view)
        })
        .map(session_dto)
        .map_err(app_error)
    })
    .await
}

#[tauri::command]
pub async fn resume_study(
    backend: State<'_, Backend>,
    input: Value,
) -> Result<StudySessionDto, ErrorDto> {
    let session: String = parse_input(input)?;
    let session = SessionId(id("sessionId", &session)?);
    let collection = backend.collection()?;
    let (db, now) = (collection.db.clone(), collection.clock.now());
    blocking(move || {
        db.write(move |tx| {
            let view = study::open_study(tx, session, now)?;
            learning::screen(tx, view)
        })
        .map(session_dto)
        .map_err(app_error)
    })
    .await
}

#[tauri::command]
pub async fn update_study_session(
    backend: State<'_, Backend>,
    input: Value,
) -> Result<StudySessionDto, ErrorDto> {
    let input: UpdateStudySessionInput = parse_input(input)?;
    operation_id(&input.operation_id)?;
    let session = SessionId(id("sessionId", &input.session_id)?);
    let action = match input.action {
        StudySessionAction::Pause => SessionAction::Pause,
        StudySessionAction::Resume => SessionAction::Resume,
        StudySessionAction::End => SessionAction::End,
    };
    let collection = backend.collection()?;
    let (db, now) = (collection.db.clone(), collection.clock.now());
    let screen = blocking(move || {
        db.write(move |tx| {
            let view = study::change_session(tx, &input.operation_id, session, action, now)?;
            learning::screen(tx, view)
        })
        .map_err(app_error)
    })
    .await?;
    if action == SessionAction::End {
        emit(collection, &[CollectionScope::Stats], now);
    }
    Ok(session_dto(screen))
}

#[tauri::command]
pub async fn get_rating_preview(
    backend: State<'_, Backend>,
    input: Value,
) -> Result<RatingPreviewDto, ErrorDto> {
    let input: PreviewInput = parse_input(input)?;
    let session = SessionId(id("sessionId", &input.session_id)?);
    let card = CardId(id("cardId", &input.card_id)?);
    let collection = backend.collection()?;
    let (db, now) = (collection.db.clone(), collection.clock.now());
    blocking(move || {
        db.read(move |tx| study::preview(tx, session, card, i64::from(input.card_version), now))
            .map_err(app_error)
            .and_then(preview_dto)
    })
    .await
}

#[tauri::command]
pub async fn rate_card(
    backend: State<'_, Backend>,
    input: Value,
) -> Result<ReviewResultDto, ErrorDto> {
    let input: RateCardInput = parse_input(input)?;
    operation_id(&input.operation_id)?;
    let rate = RateCard {
        operation_id: input.operation_id,
        session_id: SessionId(id("sessionId", &input.session_id)?),
        card_id: CardId(id("cardId", &input.card_id)?),
        card_version: i64::from(input.card_version),
        preview_token: input.preview_token,
        rating: domain_rating(input.rating),
        answer_ms: i64::from(input.answer_ms),
    };
    let collection = backend.collection()?;
    let (db, now) = (collection.db.clone(), collection.clock.now());
    let (outcome, screen) = blocking(move || {
        db.write(move |tx| {
            let outcome = study::rate_card(tx, &rate, now)?;
            let screen = learning::screen(tx, outcome.view.clone())?;
            Ok((outcome, screen))
        })
        .map_err(app_error)
    })
    .await?;
    if !outcome.replayed {
        emit(
            collection,
            &[CollectionScope::Cards, CollectionScope::Stats],
            now,
        );
    }
    Ok(review_result_dto(
        outcome.operation_id,
        outcome.reviewed,
        screen,
    ))
}

#[tauri::command]
pub async fn undo_review(
    backend: State<'_, Backend>,
    input: Value,
) -> Result<ReviewResultDto, ErrorDto> {
    let input: UndoInput = parse_input(input)?;
    operation_id(&input.operation_id)?;
    let session = SessionId(id("sessionId", &input.session_id)?);
    let collection = backend.collection()?;
    let (db, now) = (collection.db.clone(), collection.clock.now());
    let (outcome, screen) = blocking(move || {
        db.write(move |tx| {
            let outcome = study::undo_review(tx, &input.operation_id, session, now)?;
            let screen = learning::screen(tx, outcome.view.clone())?;
            Ok((outcome, screen))
        })
        .map_err(app_error)
    })
    .await?;
    if !outcome.replayed {
        emit(
            collection,
            &[
                CollectionScope::Cards,
                CollectionScope::Notes,
                CollectionScope::Stats,
            ],
            now,
        );
    }
    Ok(review_result_dto(
        outcome.operation_id,
        outcome.restored,
        screen,
    ))
}

#[tauri::command]
pub async fn apply_card_action(
    backend: State<'_, Backend>,
    input: Value,
) -> Result<CardActionResultDto, ErrorDto> {
    let input: CardActionInput = parse_input(input)?;
    operation_id(&input.operation_id)?;
    let card = CardId(id("cardId", &input.card_id)?);
    let action = match (input.action, input.flag) {
        (CardActionKind::Suspend, _) => CardAction::Suspend,
        (CardActionKind::Unsuspend, _) => CardAction::Unsuspend,
        (CardActionKind::Bury, _) => CardAction::Bury,
        (CardActionKind::Unbury, _) => CardAction::Unbury,
        (CardActionKind::Delete, _) => CardAction::Delete,
        (CardActionKind::Flag, flag) => {
            CardAction::Flag(domain_flag(flag.unwrap_or(CardFlag::None)))
        }
    };
    let collection = backend.collection()?;
    let (db, now) = (collection.db.clone(), collection.clock.now());
    let outcome = blocking(move || {
        db.write(move |tx| {
            learning::apply_card_action(
                tx,
                &input.operation_id,
                card,
                i64::from(input.expected_version),
                action,
                now,
            )
        })
        .map_err(app_error)
    })
    .await?;
    let scopes: &[CollectionScope] = if outcome.deleted {
        &[
            CollectionScope::Cards,
            CollectionScope::Notes,
            CollectionScope::Decks,
            CollectionScope::Stats,
        ]
    } else {
        &[CollectionScope::Cards, CollectionScope::Decks]
    };
    emit(collection, scopes, now);
    Ok(CardActionResultDto {
        card_id: text_id(outcome.card.id.0),
        version: clamp(outcome.card.version),
        queue: queue_state(outcome.card.queue),
        flag: flag_dto(outcome.card.flag),
        deleted: outcome.deleted,
    })
}

#[tauri::command]
pub async fn get_session_summary(
    backend: State<'_, Backend>,
    input: Value,
) -> Result<SessionSummaryDto, ErrorDto> {
    let session: String = parse_input(input)?;
    let session = SessionId(id("sessionId", &session)?);
    let collection = backend.collection()?;
    let (db, now) = (collection.db.clone(), collection.clock.now());
    blocking(move || {
        db.read(move |tx| learning::session_summary(tx, session, now))
            .map(summary_dto)
            .map_err(app_error)
    })
    .await
}

#[tauri::command]
pub async fn get_note_content(
    backend: State<'_, Backend>,
    input: Value,
) -> Result<NoteContentDto, ErrorDto> {
    let note: String = parse_input(input)?;
    let note = NoteId(id("noteId", &note)?);
    let db = backend.collection()?.db.clone();
    blocking(move || {
        db.read(move |tx| learning::note_content(tx, note))
            .map(note_content_dto)
            .map_err(app_error)
    })
    .await
}

#[tauri::command]
pub async fn set_favorite_association(
    backend: State<'_, Backend>,
    input: Value,
) -> Result<Vec<AssociationDto>, ErrorDto> {
    let input: SetFavoriteAssociationInput = parse_input(input)?;
    operation_id(&input.operation_id)?;
    let note = NoteId(id("noteId", &input.note_id)?);
    let association = AssociationId(id("associationId", &input.association_id)?);
    let collection = backend.collection()?;
    let (db, now) = (collection.db.clone(), collection.clock.now());
    let (associations, generations) = blocking(move || {
        db.write(move |tx| {
            let associations = learning::set_favorite_association(
                tx,
                &input.operation_id,
                note,
                association,
                now,
            )?;
            Ok((associations, llm::generations(tx, note)?))
        })
        .map_err(app_error)
    })
    .await?;
    emit(collection, &[CollectionScope::Notes], now);
    Ok(associations_dto(associations, generations))
}

#[tauri::command]
pub async fn resolve_word_link(
    backend: State<'_, Backend>,
    input: Value,
) -> Result<WordLinkDto, ErrorDto> {
    let input: ResolveWordLinkInput = parse_input(input)?;
    operation_id(&input.operation_id)?;
    let link = WordLinkId(id("linkId", &input.link_id)?);
    let word = NoteId(id("wordNoteId", &input.word_note_id)?);
    let collection = backend.collection()?;
    let (db, now) = (collection.db.clone(), collection.clock.now());
    let view = blocking(move || {
        db.write(move |tx| learning::resolve_word_link(tx, &input.operation_id, link, word, now))
            .map_err(app_error)
    })
    .await?;
    emit(collection, &[CollectionScope::Notes], now);
    Ok(link_dto(view))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_positive_decimal_rowids() {
        assert_eq!(id("cardId", "42").unwrap(), 42);
        for bad in ["", "0", "-1", "x", "1.5", "99999999999999999999"] {
            let error = id("cardId", bad).unwrap_err();
            assert_eq!(error.details.unwrap()["field"], "cardId", "{bad}");
        }
    }

    #[test]
    fn session_counts_total_the_three_queues() {
        let counts = counts_dto(QueueCounts {
            new: 2,
            learning: 1,
            review: 4,
            overdue: 3,
        });
        assert_eq!(counts.total, 7);
        assert_eq!(counts.overdue, 3);
    }

    #[test]
    fn negative_intervals_are_reported_as_zero_seconds() {
        let interval = IntervalPreview {
            rating: review::Rating::Again,
            due_at: TimestampMs(0),
            interval_ms: -5,
        };
        assert_eq!(interval_dto(&interval).interval_seconds, 0);
        let minute = IntervalPreview {
            interval_ms: 60_000,
            ..interval
        };
        assert_eq!(interval_dto(&minute).interval_seconds, 60);
    }

    #[test]
    fn flags_map_both_ways() {
        for flag in [
            CardFlag::None,
            CardFlag::Red,
            CardFlag::Orange,
            CardFlag::Green,
            CardFlag::Blue,
            CardFlag::Pink,
            CardFlag::Turquoise,
            CardFlag::Purple,
        ] {
            assert_eq!(flag_dto(domain_flag(flag)), flag);
        }
    }
}
