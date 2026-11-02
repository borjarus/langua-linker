//! Review history. A log stores the complete card state before and after the review, so undo can
//! restore due/reps/lapses/queue exactly — `state_before` with S/D alone is not enough (step 14).

use crate::card::{Card, CardFlag, CardFsrsState, CardState, QueueState};
use crate::{CardId, ReviewLogId, SessionId, TimestampMs, coded_enum};

coded_enum! {
    /// Answer button; codes equal Anki's `revlog.ease` and FSRS grades.
    pub enum Rating { Again = 1, Hard = 2, Good = 3, Easy = 4 }
}

coded_enum! {
    /// How the review came about; codes follow Anki's `revlog.type` where one exists.
    pub enum ReviewKind { Learning = 0, Review = 1, Relearning = 2, Filtered = 3, Manual = 4 }
}

/// Every card field a review or undo may change.
#[derive(Debug, Clone, PartialEq)]
pub struct CardSnapshot {
    pub state: CardState,
    pub queue: QueueState,
    pub due_at: Option<TimestampMs>,
    pub position: i64,
    pub fsrs: CardFsrsState,
    pub legacy_ease: Option<i64>,
    pub reps: i64,
    pub lapses: i64,
    pub learning_step: Option<u32>,
    pub buried_until: Option<TimestampMs>,
    pub flag: CardFlag,
    pub version: i64,
}

impl CardSnapshot {
    pub fn of(card: &Card) -> Self {
        Self {
            state: card.state,
            queue: card.queue,
            due_at: card.due_at,
            position: card.position,
            fsrs: card.fsrs,
            legacy_ease: card.legacy_ease,
            reps: card.reps,
            lapses: card.lapses,
            learning_step: card.learning_step,
            buried_until: card.buried_until,
            flag: card.flag,
            version: card.version,
        }
    }

    /// Writes the snapshot back onto a card (undo).
    pub fn apply_to(&self, card: &mut Card) {
        card.state = self.state;
        card.queue = self.queue;
        card.due_at = self.due_at;
        card.position = self.position;
        card.fsrs = self.fsrs;
        card.legacy_ease = self.legacy_ease;
        card.reps = self.reps;
        card.lapses = self.lapses;
        card.learning_step = self.learning_step;
        card.buried_until = self.buried_until;
        card.flag = self.flag;
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReviewLog {
    pub id: ReviewLogId,
    pub card_id: CardId,
    pub session_id: Option<SessionId>,
    /// Idempotency key of the rating request; unique.
    pub operation_id: String,
    pub rating: Rating,
    pub kind: ReviewKind,
    pub reviewed_at: TimestampMs,
    /// Time spent answering, excluding pauses.
    pub duration_ms: i64,
    pub card_before: CardSnapshot,
    pub card_after: CardSnapshot,
    pub fsrs_version: String,
    pub parameters_hash: String,
    /// Set when the review was undone; the row stays for auditing but no longer counts.
    pub invalidated_at: Option<TimestampMs>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DeckId, Guid, NoteId};

    fn card() -> Card {
        Card {
            id: CardId(1),
            guid: Guid::from_bytes([2; 16]),
            note_id: NoteId(1),
            deck_id: DeckId(1),
            template_ordinal: 0,
            position: 7,
            flag: CardFlag::None,
            state: CardState::New,
            queue: QueueState::New,
            due_at: None,
            fsrs: CardFsrsState::NEW,
            legacy_ease: None,
            reps: 0,
            lapses: 0,
            learning_step: None,
            buried_until: None,
            archived_at: None,
            created_at: TimestampMs(0),
            updated_at: TimestampMs(0),
            deleted_at: None,
            usn: 0,
            version: 1,
        }
    }

    #[test]
    fn snapshot_restores_every_scheduling_field() {
        let original = card();
        let before = CardSnapshot::of(&original);
        let mut reviewed = original.clone();
        reviewed.state = CardState::Review;
        reviewed.queue = QueueState::Review;
        reviewed.due_at = Some(TimestampMs(86_400_000));
        reviewed.fsrs.stability = Some(3.2);
        reviewed.reps = 1;
        reviewed.lapses = 1;
        reviewed.learning_step = Some(1);
        reviewed.flag = CardFlag::Red;
        before.apply_to(&mut reviewed);
        assert_eq!(reviewed, original);
    }

    #[test]
    fn rating_codes_match_fsrs_grades() {
        assert_eq!(Rating::Again.db_code(), 1);
        assert_eq!(Rating::Easy.db_code(), 4);
    }
}
