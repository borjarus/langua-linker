//! Learning on the existing screens (plan Phase 10): what the study card, the word sheet and the
//! session summary show, plus the card actions and content choices available during study.
//!
//! Every function runs inside the caller's transaction; mutations need a write transaction and
//! are idempotent by `operationId` (a retry returns the current result instead of acting twice).

use langualinker_domain::card::{Card, CardFlag, QueueState};
use langualinker_domain::content::{Association, GrammarTip, SentenceWordLink, WordLinkResolution};
use langualinker_domain::deck::Deck;
use langualinker_domain::note::{CardDirection, Note};
use langualinker_domain::review::Rating;
use langualinker_domain::scheduling;
use langualinker_domain::{
    AssociationId, CardId, DeckId, NoteId, SessionId, TimestampMs, ValidationError, WordLinkId,
};

use crate::repositories::{RecordedOperation, Repositories, SessionStatus, StudySession};
use crate::study::{self, StudyView};
use crate::{AppError, AppResult};

const OP_CARD_ACTION: &str = "applyCardAction";
const OP_FAVORITE: &str = "setFavoriteAssociation";
const OP_RESOLVE_LINK: &str = "resolveWordLink";

// ---------------------------------------------------------------- read models

/// A word note offered for an ambiguous link.
#[derive(Debug, Clone, PartialEq)]
pub struct WordCandidate {
    pub note_id: NoteId,
    pub lemma: String,
    pub translation: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LinkView {
    pub link: SentenceWordLink,
    /// Back of the linked word note.
    pub translation: Option<String>,
    pub candidates: Vec<WordCandidate>,
}

/// A card as the study screen shows it: the question side first.
#[derive(Debug, Clone, PartialEq)]
pub struct StudyCard {
    pub card: Card,
    pub note: Note,
    pub deck: Deck,
    pub direction: CardDirection,
    /// Question side (the note's back for a reverse card).
    pub front: String,
    pub back: String,
    /// Empty when the deck type hides the section.
    pub associations: Vec<Association>,
    pub grammar_tips: Vec<GrammarTip>,
    pub word_links: Vec<LinkView>,
}

/// The study screen: the session view, its current card's content and the deck name.
#[derive(Debug, Clone, PartialEq)]
pub struct StudyScreen {
    pub view: StudyView,
    pub deck_name: Option<String>,
    pub current: Option<StudyCard>,
}

/// Note content for the word sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct NoteContent {
    pub note: Note,
    pub deck: Deck,
    pub associations: Vec<Association>,
    pub grammar_tips: Vec<GrammarTip>,
}

/// A card listed in the summary.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionCard {
    pub card_id: CardId,
    pub deck: Deck,
    pub direction: CardDirection,
    pub front: String,
    pub back: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NextDeck {
    pub deck: Deck,
    pub due: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionSummary {
    pub session: StudySession,
    pub deck_name: Option<String>,
    /// Valid ratings by button: Again, Hard, Good, Easy.
    pub ratings: [u32; 4],
    pub reviewed_count: u32,
    pub again_cards: Vec<SessionCard>,
    pub next_due_at: Option<TimestampMs>,
    pub remaining_due: u32,
    pub next_deck: Option<NextDeck>,
    pub first_completed: bool,
}

pub(crate) fn live_deck(tx: &mut dyn Repositories, id: DeckId) -> AppResult<Deck> {
    tx.deck(id)?.ok_or(AppError::NotFound { entity: "deck" })
}

pub(crate) fn live_note(tx: &mut dyn Repositories, id: NoteId) -> AppResult<Note> {
    let note = tx.note(id)?.ok_or(AppError::NotFound { entity: "note" })?;
    if note.deleted_at.is_some() {
        return Err(AppError::Deleted { entity: "note" });
    }
    Ok(note)
}

pub(crate) fn direction_of(
    tx: &mut dyn Repositories,
    note: &Note,
    card: &Card,
) -> AppResult<CardDirection> {
    Ok(tx
        .templates(note.note_type_id)?
        .into_iter()
        .find(|template| template.ordinal == card.template_ordinal)
        .map_or(CardDirection::Forward, |template| template.direction))
}

pub(crate) fn sides(note: &Note, direction: CardDirection) -> (String, String) {
    match direction {
        CardDirection::Forward => (note.fields.front.clone(), note.fields.back.clone()),
        CardDirection::Reverse => (note.fields.back.clone(), note.fields.front.clone()),
    }
}

pub(crate) fn link_view(tx: &mut dyn Repositories, link: SentenceWordLink) -> AppResult<LinkView> {
    let translation = match link.word_note_id {
        Some(word) => tx
            .note(word)?
            .filter(|note| note.deleted_at.is_none())
            .map(|note| note.fields.back),
        None => None,
    };
    // Bundled links are resolved by GUID, so nothing is ambiguous yet; the word search that
    // offers candidates arrives with the editor (Phase 11).
    Ok(LinkView {
        link,
        translation,
        candidates: Vec::new(),
    })
}

/// The content of a card for the study screen, limited to the sections its deck type shows.
pub fn study_card(tx: &mut dyn Repositories, card: Card) -> AppResult<StudyCard> {
    let note = live_note(tx, card.note_id)?;
    let deck = live_deck(tx, card.deck_id)?;
    let direction = direction_of(tx, &note, &card)?;
    let (front, back) = sides(&note, direction);
    let capabilities = deck.deck_type.capabilities();
    let associations = if capabilities.associations {
        tx.associations(note.id)?
    } else {
        Vec::new()
    };
    let grammar_tips = if capabilities.grammar_tips {
        tx.grammar_tips(note.id)?
    } else {
        Vec::new()
    };
    let mut word_links = Vec::new();
    if capabilities.word_links {
        for link in tx.word_links(note.id)? {
            word_links.push(link_view(tx, link)?);
        }
    }
    Ok(StudyCard {
        card,
        note,
        deck,
        direction,
        front,
        back,
        associations,
        grammar_tips,
        word_links,
    })
}

fn deck_name(tx: &mut dyn Repositories, deck: Option<DeckId>) -> AppResult<Option<String>> {
    match deck {
        Some(id) => Ok(tx.deck(id)?.map(|deck| deck.name)),
        None => Ok(None),
    }
}

/// Adds the current card's content and the deck name to a session view.
pub fn screen(tx: &mut dyn Repositories, view: StudyView) -> AppResult<StudyScreen> {
    let deck_name = deck_name(tx, view.session.deck_id)?;
    let current = match view.current.clone() {
        Some(card) => Some(study_card(tx, card)?),
        None => None,
    };
    Ok(StudyScreen {
        view,
        deck_name,
        current,
    })
}

/// GetNoteContent: the word sheet of a note. Read-only.
pub fn note_content(tx: &mut dyn Repositories, note_id: NoteId) -> AppResult<NoteContent> {
    let note = live_note(tx, note_id)?;
    let card = tx
        .cards_of_note(note.id)?
        .into_iter()
        .next()
        .ok_or(AppError::NotFound { entity: "card" })?;
    let deck = live_deck(tx, card.deck_id)?;
    let capabilities = deck.deck_type.capabilities();
    let associations = if capabilities.associations {
        tx.associations(note.id)?
    } else {
        Vec::new()
    };
    let grammar_tips = if capabilities.grammar_tips {
        tx.grammar_tips(note.id)?
    } else {
        Vec::new()
    };
    Ok(NoteContent {
        note,
        deck,
        associations,
        grammar_tips,
    })
}

// ---------------------------------------------------------------- card actions

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardAction {
    Suspend,
    Unsuspend,
    Bury,
    Unbury,
    Flag(CardFlag),
    Delete,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CardActionOutcome {
    pub card: Card,
    pub deleted: bool,
}

pub(crate) fn check_operation_id(operation_id: &str) -> AppResult<()> {
    if operation_id.is_empty() || operation_id.len() > 128 {
        return Err(ValidationError::new("operationId", "format").into());
    }
    Ok(())
}

/// Returns `true` when the operation already ran as `operation`; fails when it ran as anything
/// else (including a rating).
pub(crate) fn replayed(
    tx: &mut dyn Repositories,
    operation_id: &str,
    operation: &str,
) -> AppResult<bool> {
    check_operation_id(operation_id)?;
    if let Some(recorded) = tx.recorded_operation(operation_id)? {
        if recorded.operation != operation {
            return Err(ValidationError::new("operationId", "reused").into());
        }
        return Ok(true);
    }
    if tx.review_log_by_operation(operation_id)?.is_some() {
        return Err(ValidationError::new("operationId", "reused").into());
    }
    Ok(false)
}

pub(crate) fn record(
    tx: &mut dyn Repositories,
    operation_id: &str,
    operation: &str,
    result: String,
    now: TimestampMs,
) -> AppResult<()> {
    tx.record_operation(
        operation_id,
        &RecordedOperation {
            operation: operation.into(),
            result_json: result,
        },
        now,
    )
}

/// Suspend, unsuspend, bury, unbury, flag or delete one card at the version the UI showed.
/// Deleting the last card of a note deletes the note; links to it become unresolved.
pub fn apply_card_action(
    tx: &mut dyn Repositories,
    operation_id: &str,
    card_id: CardId,
    expected_version: i64,
    action: CardAction,
    now: TimestampMs,
) -> AppResult<CardActionOutcome> {
    let current = |tx: &mut dyn Repositories| -> AppResult<CardActionOutcome> {
        let card = tx
            .card(card_id)?
            .ok_or(AppError::NotFound { entity: "card" })?;
        Ok(CardActionOutcome {
            deleted: card.deleted_at.is_some(),
            card,
        })
    };
    if replayed(tx, operation_id, OP_CARD_ACTION)? {
        return current(tx);
    }
    let mut card = tx
        .card(card_id)?
        .ok_or(AppError::NotFound { entity: "card" })?;
    if card.deleted_at.is_some() {
        return Err(AppError::Deleted { entity: "card" });
    }
    if card.version != expected_version {
        return Err(AppError::VersionConflict {
            entity: "card",
            current: card.version,
        });
    }
    let moment = study::study_moment(tx, now)?;
    let changed = match action {
        CardAction::Suspend if card.queue != QueueState::Suspended => {
            card.queue = QueueState::Suspended;
            card.buried_until = None;
            true
        }
        CardAction::Bury if !card.queue.is_buried() && card.queue != QueueState::Suspended => {
            scheduling::bury(&mut card, QueueState::UserBuried, &moment);
            true
        }
        CardAction::Unsuspend if card.queue == QueueState::Suspended => {
            card.queue = scheduling::natural_queue(&card, &moment);
            true
        }
        CardAction::Unbury if card.queue.is_buried() => {
            card.queue = scheduling::natural_queue(&card, &moment);
            card.buried_until = None;
            true
        }
        CardAction::Flag(flag) if card.flag != flag => {
            card.flag = flag;
            true
        }
        CardAction::Delete => {
            let siblings = tx.cards_of_note(card.note_id)?;
            if siblings.iter().all(|sibling| sibling.id == card.id) {
                let note = live_note(tx, card.note_id)?;
                tx.soft_delete_note(note.id, note.version, now)?;
                tx.unresolve_links_to(note.id, now)?;
            } else {
                tx.soft_delete_card(card.id, card.version, now)?;
            }
            record(tx, operation_id, OP_CARD_ACTION, card.id.0.to_string(), now)?;
            return current(tx);
        }
        _ => false,
    };
    if changed {
        tx.update_card(&card, now)?;
    }
    record(tx, operation_id, OP_CARD_ACTION, card.id.0.to_string(), now)?;
    current(tx)
}

// ---------------------------------------------------------------- content choices

/// Makes an association the note's favorite. Returns the note's associations.
pub fn set_favorite_association(
    tx: &mut dyn Repositories,
    operation_id: &str,
    note_id: NoteId,
    association_id: AssociationId,
    now: TimestampMs,
) -> AppResult<Vec<Association>> {
    if replayed(tx, operation_id, OP_FAVORITE)? {
        return tx.associations(note_id);
    }
    live_note(tx, note_id)?;
    if !tx
        .associations(note_id)?
        .iter()
        .any(|association| association.id == association_id)
    {
        return Err(AppError::NotFound {
            entity: "association",
        });
    }
    tx.set_favorite_association(note_id, Some(association_id), now)?;
    record(
        tx,
        operation_id,
        OP_FAVORITE,
        association_id.0.to_string(),
        now,
    )?;
    tx.associations(note_id)
}

/// Links a phrase of a sentence to the word note the learner chose (also for ambiguous links).
pub fn resolve_word_link(
    tx: &mut dyn Repositories,
    operation_id: &str,
    link_id: WordLinkId,
    word_note_id: NoteId,
    now: TimestampMs,
) -> AppResult<LinkView> {
    let load = |tx: &mut dyn Repositories| -> AppResult<SentenceWordLink> {
        tx.word_link(link_id)?
            .ok_or(AppError::NotFound { entity: "wordLink" })
    };
    if replayed(tx, operation_id, OP_RESOLVE_LINK)? {
        let link = load(tx)?;
        return link_view(tx, link);
    }
    let mut link = load(tx)?;
    if word_note_id == link.sentence_note_id {
        return Err(ValidationError::new("wordNoteId", "sameNote").into());
    }
    live_note(tx, link.sentence_note_id)?;
    live_note(tx, word_note_id)?;
    link.word_note_id = Some(word_note_id);
    link.preferred_word_card_id = None;
    link.resolution = WordLinkResolution::UserOverridden;
    let saved = tx.update_word_link(&link, now)?;
    record(
        tx,
        operation_id,
        OP_RESOLVE_LINK,
        link_id.0.to_string(),
        now,
    )?;
    link_view(tx, saved)
}

// ---------------------------------------------------------------- summary

/// GetSessionSummary: the session's valid ratings, time, Again cards, what is still due and where
/// to go next. Read-only.
pub fn session_summary(
    tx: &mut dyn Repositories,
    session_id: SessionId,
    now: TimestampMs,
) -> AppResult<SessionSummary> {
    let stats = study::session_stats(tx, session_id, now)?;
    let mut again_cards = Vec::new();
    for id in &stats.again_cards {
        let Some(card) = tx.card(*id)? else { continue };
        if card.deleted_at.is_some() {
            continue;
        }
        let Some(note) = tx
            .note(card.note_id)?
            .filter(|note| note.deleted_at.is_none())
        else {
            continue;
        };
        let deck = live_deck(tx, card.deck_id)?;
        let direction = direction_of(tx, &note, &card)?;
        let (front, back) = sides(&note, direction);
        again_cards.push(SessionCard {
            card_id: card.id,
            deck,
            direction,
            front,
            back,
        });
    }

    // Another root deck with cards due today, outside this session's scope.
    let next_deck = match stats.session.deck_id {
        None => None,
        Some(scope) => {
            let decks = tx.active_decks()?;
            let root_of = |mut id: DeckId| {
                let mut guard = 0;
                while let Some(parent) = decks
                    .iter()
                    .find(|deck| deck.id == id)
                    .and_then(|deck| deck.parent_id)
                    .filter(|parent| decks.iter().any(|deck| deck.id == *parent))
                {
                    id = parent;
                    guard += 1;
                    if guard > decks.len() {
                        break;
                    }
                }
                id
            };
            let own_root = root_of(scope);
            let moment = study::study_moment(tx, now)?;
            let mut best: Option<NextDeck> = None;
            for deck in decks.iter().filter(|deck| deck.parent_id.is_none()) {
                if deck.id == own_root {
                    continue;
                }
                let counts = study::scope_counts(tx, Some(deck.id), &moment)?;
                let due = counts.new + counts.learning + counts.review;
                if due > 0 && best.as_ref().is_none_or(|current| due > current.due) {
                    best = Some(NextDeck {
                        deck: deck.clone(),
                        due,
                    });
                }
            }
            best
        }
    };
    let first_completed = stats.session.status == SessionStatus::Finished
        && stats.reviewed_count > 0
        && tx.first_reviewed_session()? == Some(stats.session.id);
    let remaining = stats.remaining;
    Ok(SessionSummary {
        deck_name: deck_name(tx, stats.session.deck_id)?,
        ratings: stats.ratings,
        reviewed_count: stats.reviewed_count,
        again_cards,
        next_due_at: stats.next_due_at,
        remaining_due: remaining.new + remaining.learning + remaining.review,
        next_deck,
        first_completed,
        session: stats.session,
    })
}

/// Index of a rating in `[Again, Hard, Good, Easy]` arrays.
pub fn rating_index(rating: Rating) -> usize {
    match rating {
        Rating::Again => 0,
        Rating::Hard => 1,
        Rating::Good => 2,
        Rating::Easy => 3,
    }
}

#[cfg(test)]
mod tests {
    //! The use cases need a repository and are covered on SQLite by
    //! `crates/infrastructure/tests/learning.rs`.

    use langualinker_domain::Guid;
    use langualinker_domain::note::NoteFields;

    use super::*;

    fn note() -> Note {
        Note {
            id: NoteId(1),
            guid: Guid::from_bytes([1; 16]),
            note_type_id: langualinker_domain::NoteTypeId(1),
            fields: NoteFields {
                front: "der Hund".into(),
                back: "pies".into(),
            },
            sort_field: "der Hund".into(),
            tags: Vec::new(),
            created_at: TimestampMs(0),
            updated_at: TimestampMs(0),
            deleted_at: None,
            usn: 0,
            version: 1,
        }
    }

    #[test]
    fn a_reverse_card_asks_with_the_back() {
        let note = note();
        assert_eq!(
            sides(&note, CardDirection::Forward),
            ("der Hund".to_owned(), "pies".to_owned())
        );
        assert_eq!(
            sides(&note, CardDirection::Reverse),
            ("pies".to_owned(), "der Hund".to_owned())
        );
    }

    #[test]
    fn operation_ids_are_checked_before_anything_else() {
        assert!(check_operation_id("").is_err());
        assert!(check_operation_id(&"x".repeat(129)).is_err());
        assert!(check_operation_id("op-1").is_ok());
    }

    #[test]
    fn rating_indexes_follow_the_button_order() {
        let indexes: Vec<usize> = Rating::ALL
            .iter()
            .map(|rating| rating_index(*rating))
            .collect();
        assert_eq!(indexes, [0, 1, 2, 3]);
    }
}
