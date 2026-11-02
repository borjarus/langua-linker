//! Cards: scheduling state kept separate from queue placement, with explicit DB/Anki mappings.

use crate::{CardId, DeckId, Guid, NoteId, TimestampMs, coded_enum};

coded_enum! {
    /// Memory state of a card (FSRS view). Codes equal Anki's `cards.type`.
    pub enum CardState { New = 0, Learning = 1, Review = 2, Relearning = 3 }
}

coded_enum! {
    /// Where the card currently sits. Codes equal Anki's `cards.queue`, so interop needs no table;
    /// the mapping is still written out here rather than taken from variant order.
    pub enum QueueState {
        New = 0,
        Learning = 1,
        Review = 2,
        /// Learning card whose next step is at least one day away.
        DayLearn = 3,
        Suspended = -1,
        SchedBuried = -2,
        UserBuried = -3,
    }
}

impl QueueState {
    /// The queue a card returns to when unsuspended or unburied.
    pub fn natural(state: CardState, next_step_on_another_day: bool) -> Self {
        match state {
            CardState::New => Self::New,
            CardState::Review => Self::Review,
            CardState::Learning | CardState::Relearning if next_step_on_another_day => {
                Self::DayLearn
            }
            CardState::Learning | CardState::Relearning => Self::Learning,
        }
    }

    pub fn is_buried(self) -> bool {
        matches!(self, Self::SchedBuried | Self::UserBuried)
    }
}

coded_enum! {
    /// User flag; codes equal Anki's flag numbers.
    pub enum CardFlag {
        None = 0, Red = 1, Orange = 2, Green = 3, Blue = 4, Pink = 5, Turquoise = 6, Purple = 7,
    }
}

/// FSRS memory state. `None` stability/difficulty means the card was never reviewed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CardFsrsState {
    pub stability: Option<f64>,
    pub difficulty: Option<f64>,
    pub last_review_at: Option<TimestampMs>,
    pub scheduled_days: i64,
    pub elapsed_days: i64,
}

impl CardFsrsState {
    pub const NEW: Self = Self {
        stability: None,
        difficulty: None,
        last_review_at: None,
        scheduled_days: 0,
        elapsed_days: 0,
    };
}

#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    pub id: CardId,
    pub guid: Guid,
    pub note_id: NoteId,
    /// Scheduling deck; cards of one note may sit in different decks (plan step 10).
    pub deck_id: DeckId,
    pub template_ordinal: u32,
    /// Introduction order among new cards; meaningful only while `state` is `New`.
    pub position: i64,
    pub flag: CardFlag,
    pub state: CardState,
    pub queue: QueueState,
    /// `None` while New (position applies); UTC epoch ms once scheduled (plan step 16).
    pub due_at: Option<TimestampMs>,
    pub fsrs: CardFsrsState,
    /// Anki ease in permille, kept only for interop.
    pub legacy_ease: Option<i64>,
    pub reps: i64,
    pub lapses: i64,
    pub learning_step: Option<u32>,
    pub buried_until: Option<TimestampMs>,
    /// Set when a note-type change archives the card; restoring keeps its history.
    pub archived_at: Option<TimestampMs>,
    pub created_at: TimestampMs,
    pub updated_at: TimestampMs,
    pub deleted_at: Option<TimestampMs>,
    pub usn: i64,
    pub version: i64,
}

/// A new card as created by a note save or import.
#[derive(Debug, Clone, PartialEq)]
pub struct NewCard {
    pub guid: Guid,
    pub note_id: NoteId,
    pub deck_id: DeckId,
    pub template_ordinal: u32,
    pub position: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_match_anki() {
        assert_eq!(CardState::Relearning.db_code(), 3);
        assert_eq!(QueueState::DayLearn.db_code(), 3);
        assert_eq!(QueueState::Suspended.db_code(), -1);
        assert_eq!(QueueState::UserBuried.db_code(), -3);
        assert_eq!(CardFlag::Purple.db_code(), 7);
    }

    #[test]
    fn every_code_round_trips_and_unknown_codes_fail() {
        for queue in QueueState::ALL {
            assert_eq!(QueueState::from_db_code(queue.db_code()).unwrap(), *queue);
        }
        for state in CardState::ALL {
            assert_eq!(CardState::from_db_code(state.db_code()).unwrap(), *state);
        }
        let error = QueueState::from_db_code(9).unwrap_err();
        assert_eq!((error.kind, error.code), ("QueueState", 9));
    }

    #[test]
    fn natural_queue_depends_on_state() {
        assert_eq!(QueueState::natural(CardState::New, false), QueueState::New);
        assert_eq!(
            QueueState::natural(CardState::Review, true),
            QueueState::Review
        );
        assert_eq!(
            QueueState::natural(CardState::Learning, false),
            QueueState::Learning
        );
        assert_eq!(
            QueueState::natural(CardState::Relearning, true),
            QueueState::DayLearn
        );
        assert!(QueueState::UserBuried.is_buried());
        assert!(!QueueState::Suspended.is_buried());
    }
}
