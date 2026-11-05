//! Applies FSRS outcomes to cards (plan Phase 9 steps 4–6, 10, ADR-15).
//!
//! Policy on top of the reference state machine:
//! - Elapsed days are study days in the collection calendar (not 24-hour blocks).
//! - A review interval of `d` days makes the card due at the start of the study day `d` days
//!   after today; a learning step is due exactly `step` after the rating.
//! - A learning step that ends on a later study day puts the card in the day-learning queue.
//! - A lapse is a rating of Again on a card in review; it increments `lapses`.

use crate::TimestampMs;
use crate::calendar::{StudyCalendar, StudyDay};
use crate::card::{Card, CardState, QueueState};
use crate::fsrs::{MemoryState, NextInterval, Phase, ReviewInput, SchedulerConfig, fuzz_uniform};
use crate::review::{Rating, ReviewKind};

/// "Now" seen through the collection calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StudyMoment {
    pub calendar: StudyCalendar,
    pub now: TimestampMs,
    pub today: StudyDay,
    pub day_start: TimestampMs,
    pub day_end: TimestampMs,
}

impl StudyMoment {
    pub fn new(calendar: StudyCalendar, now: TimestampMs) -> Self {
        let today = calendar.day_of(now);
        Self {
            calendar,
            now,
            today,
            day_start: calendar.day_start(today),
            day_end: calendar.day_end(today),
        }
    }
}

pub fn phase_of(card: &Card) -> Phase {
    let step = card.learning_step.unwrap_or(0);
    match card.state {
        CardState::New => Phase::Learning { step: 0 },
        CardState::Learning => Phase::Learning { step },
        CardState::Review => Phase::Review,
        CardState::Relearning => Phase::Relearning { step },
    }
}

/// Input of the state machine for a card at `moment`. A card without memory (never rated, or
/// imported without FSRS state) is rated as a first review.
pub fn review_input(card: &Card, moment: &StudyMoment) -> ReviewInput {
    let memory = match (card.fsrs.stability, card.fsrs.difficulty) {
        (Some(stability), Some(difficulty)) => Some(MemoryState {
            stability,
            difficulty,
        }),
        _ => None,
    };
    ReviewInput {
        phase: phase_of(card),
        memory,
        elapsed_days: card
            .fsrs
            .last_review_at
            .map(|last| moment.calendar.days_between(last, moment.now)),
    }
}

/// The card after a rating, not yet persisted.
#[derive(Debug, Clone, PartialEq)]
pub struct ScheduledReview {
    pub card: Card,
    pub kind: ReviewKind,
    pub interval: NextInterval,
    /// The rating was a lapse (Again on a review card).
    pub lapse: bool,
}

impl ScheduledReview {
    /// Due time of the rated card (always set after a rating).
    pub fn due_at(&self) -> TimestampMs {
        self.card.due_at.unwrap_or(TimestampMs(0))
    }
}

/// Review kind recorded in the log, from the state before the rating.
pub fn review_kind(state: CardState) -> ReviewKind {
    match state {
        CardState::New | CardState::Learning => ReviewKind::Learning,
        CardState::Review => ReviewKind::Review,
        CardState::Relearning => ReviewKind::Relearning,
    }
}

/// Rates `card` at `moment`. Pure: the same card, config, rating and moment give the same result,
/// which is what lets a preview promise exactly what the commit writes.
pub fn schedule(
    card: &Card,
    config: &SchedulerConfig,
    parameters_hash: &str,
    rating: Rating,
    moment: &StudyMoment,
) -> ScheduledReview {
    let input = review_input(card, moment);
    let uniform = fuzz_uniform(card.guid.as_str(), card.version, parameters_hash);
    let output = config.review(&input, rating, uniform);
    let lapse = card.state == CardState::Review && rating == Rating::Again;

    let mut after = card.clone();
    let (state, step) = match output.phase {
        Phase::Learning { step } => (CardState::Learning, Some(step)),
        Phase::Review => (CardState::Review, None),
        Phase::Relearning { step } => (CardState::Relearning, Some(step)),
    };
    let due_at = match output.interval {
        NextInterval::Seconds(seconds) => TimestampMs(moment.now.0 + i64::from(seconds) * 1000),
        NextInterval::Days(days) => moment
            .calendar
            .day_start(moment.today.plus(i64::from(days))),
    };
    after.state = state;
    after.learning_step = step;
    after.due_at = Some(due_at);
    after.queue = match state {
        CardState::Review => QueueState::Review,
        _ if due_at >= moment.day_end => QueueState::DayLearn,
        _ => QueueState::Learning,
    };
    after.fsrs.stability = Some(output.memory.stability);
    after.fsrs.difficulty = Some(output.memory.difficulty);
    after.fsrs.last_review_at = Some(moment.now);
    after.fsrs.scheduled_days = match output.interval {
        NextInterval::Days(days) => i64::from(days),
        NextInterval::Seconds(_) => 0,
    };
    after.fsrs.elapsed_days = input.elapsed_days.unwrap_or(0).max(0);
    after.reps += 1;
    if lapse {
        after.lapses += 1;
    }
    after.buried_until = None;
    ScheduledReview {
        card: after,
        kind: review_kind(card.state),
        interval: output.interval,
        lapse,
    }
}

/// The queue a card belongs in when nothing hides it (unsuspend, unbury).
pub fn natural_queue(card: &Card, moment: &StudyMoment) -> QueueState {
    let later_day = card.due_at.is_some_and(|due| due >= moment.day_end);
    QueueState::natural(card.state, later_day)
}

/// Whether answering a card buries this sibling: new siblings, and review or day-learning siblings
/// due by the end of today. Intraday learning, suspended, buried and archived cards stay.
pub fn buries_sibling(sibling: &Card, moment: &StudyMoment) -> bool {
    if sibling.archived_at.is_some() || sibling.deleted_at.is_some() {
        return false;
    }
    match sibling.queue {
        QueueState::New => true,
        QueueState::Review | QueueState::DayLearn => {
            sibling.due_at.is_some_and(|due| due < moment.day_end)
        }
        _ => false,
    }
}

/// Hides a card until the next study day (`buried_until` = end of today).
pub fn bury(card: &mut Card, queue: QueueState, moment: &StudyMoment) {
    debug_assert!(queue.is_buried());
    card.queue = queue;
    card.buried_until = Some(moment.day_end);
}

/// Returns a buried card whose day has passed to its natural queue. `false` when nothing changed.
pub fn unbury_if_expired(card: &mut Card, moment: &StudyMoment) -> bool {
    let expired = card.buried_until.is_none_or(|until| until <= moment.now);
    if !card.queue.is_buried() || !expired {
        return false;
    }
    card.queue = natural_queue(card, moment);
    card.buried_until = None;
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::{CardFlag, CardFsrsState};
    use crate::deck::DeckConfigOptions;
    use crate::{CardId, DeckId, Guid, NoteId};

    const DAY: i64 = 86_400_000;
    // 2026-03-02 12:00 UTC
    const NOON: TimestampMs = TimestampMs(1_772_452_800_000);

    fn moment(at: TimestampMs) -> StudyMoment {
        StudyMoment::new(StudyCalendar::utc(240), at)
    }

    fn config() -> SchedulerConfig {
        let options = DeckConfigOptions {
            fuzz: false,
            ..DeckConfigOptions::default()
        };
        SchedulerConfig::from_options(&options).unwrap()
    }

    fn new_card() -> Card {
        Card {
            id: CardId(1),
            guid: Guid::from_bytes([1; 16]),
            note_id: NoteId(1),
            deck_id: DeckId(1),
            template_ordinal: 0,
            position: 3,
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

    fn rate(card: &Card, rating: Rating, at: TimestampMs) -> ScheduledReview {
        let cfg = config();
        schedule(card, &cfg, &cfg.parameters_hash(), rating, &moment(at))
    }

    #[test]
    fn new_card_walks_the_learning_steps_and_graduates() {
        let first = rate(&new_card(), Rating::Good, NOON);
        assert_eq!(first.kind, ReviewKind::Learning);
        assert_eq!(first.card.state, CardState::Learning);
        assert_eq!(first.card.learning_step, Some(1));
        assert_eq!(first.card.queue, QueueState::Learning);
        assert_eq!(first.due_at(), TimestampMs(NOON.0 + 600_000));
        assert_eq!(first.card.reps, 1);

        let mut card = first.card;
        card.version += 1;
        let second = rate(&card, Rating::Good, TimestampMs(NOON.0 + 600_000));
        assert_eq!(second.card.state, CardState::Review);
        assert_eq!(second.card.learning_step, None);
        // Two days: due at the start (04:00) of the study day after tomorrow.
        assert_eq!(second.card.fsrs.scheduled_days, 2);
        let today_start = TimestampMs(NOON.0 - 8 * 3_600_000);
        assert_eq!(second.due_at(), TimestampMs(today_start.0 + 2 * DAY));
    }

    #[test]
    fn a_lapse_counts_and_enters_relearning() {
        let mut card = new_card();
        card.state = CardState::Review;
        card.queue = QueueState::Review;
        card.due_at = Some(NOON);
        card.fsrs = CardFsrsState {
            stability: Some(20.0),
            difficulty: Some(5.0),
            last_review_at: Some(TimestampMs(NOON.0 - 20 * DAY)),
            scheduled_days: 20,
            elapsed_days: 0,
        };
        card.reps = 5;
        let lapse = rate(&card, Rating::Again, NOON);
        assert!(lapse.lapse);
        assert_eq!(lapse.kind, ReviewKind::Review);
        assert_eq!(lapse.card.lapses, 1);
        assert_eq!(lapse.card.state, CardState::Relearning);
        assert_eq!(lapse.card.fsrs.elapsed_days, 20);
        assert!(lapse.card.fsrs.stability.unwrap() < 20.0);
        let good = rate(&card, Rating::Good, NOON);
        assert!(!good.lapse);
        assert_eq!(good.card.lapses, 0);
    }

    #[test]
    fn a_step_reaching_into_tomorrow_uses_the_day_learning_queue() {
        let options = DeckConfigOptions {
            learning_steps_minutes: vec![1, 1440],
            fuzz: false,
            ..DeckConfigOptions::default()
        };
        let cfg = SchedulerConfig::from_options(&options).unwrap();
        let out = schedule(&new_card(), &cfg, "hash", Rating::Good, &moment(NOON));
        assert_eq!(out.card.queue, QueueState::DayLearn);
        assert_eq!(out.card.state, CardState::Learning);
    }

    #[test]
    fn same_inputs_give_the_same_schedule() {
        let mut cfg = config();
        cfg.fuzz = true;
        let hash = cfg.parameters_hash();
        let mut card = new_card();
        card.state = CardState::Review;
        card.queue = QueueState::Review;
        card.due_at = Some(NOON);
        card.fsrs.stability = Some(30.0);
        card.fsrs.difficulty = Some(4.0);
        card.fsrs.last_review_at = Some(TimestampMs(NOON.0 - 30 * DAY));
        let a = schedule(&card, &cfg, &hash, Rating::Good, &moment(NOON));
        let b = schedule(&card, &cfg, &hash, Rating::Good, &moment(NOON));
        assert_eq!(a, b);
    }

    #[test]
    fn imported_review_cards_without_memory_start_from_initial_values() {
        let mut card = new_card();
        card.state = CardState::Review;
        card.queue = QueueState::Review;
        card.due_at = Some(NOON);
        let out = rate(&card, Rating::Good, NOON);
        assert_eq!(
            out.card.fsrs.stability,
            Some(crate::fsrs::DEFAULT_WEIGHTS[2])
        );
        assert_eq!(out.card.state, CardState::Review);
    }

    #[test]
    fn siblings_due_today_or_new_are_buried_until_tomorrow() {
        let now = moment(NOON);
        let mut new_sibling = new_card();
        assert!(buries_sibling(&new_sibling, &now));
        let mut review = new_card();
        review.state = CardState::Review;
        review.queue = QueueState::Review;
        review.due_at = Some(TimestampMs(now.day_end.0 + 1));
        assert!(!buries_sibling(&review, &now), "due tomorrow");
        review.due_at = Some(now.day_start);
        assert!(buries_sibling(&review, &now));
        let mut learning = review.clone();
        learning.queue = QueueState::Learning;
        assert!(!buries_sibling(&learning, &now));
        learning.queue = QueueState::Suspended;
        assert!(!buries_sibling(&learning, &now));

        bury(&mut new_sibling, QueueState::SchedBuried, &now);
        assert_eq!(new_sibling.buried_until, Some(now.day_end));
        assert!(!unbury_if_expired(&mut new_sibling, &now));
        let tomorrow = moment(TimestampMs(NOON.0 + DAY));
        assert!(unbury_if_expired(&mut new_sibling, &tomorrow));
        assert_eq!(new_sibling.queue, QueueState::New);
        assert_eq!(new_sibling.buried_until, None);
    }

    #[test]
    fn natural_queue_of_learning_cards_depends_on_the_due_day() {
        let now = moment(NOON);
        let mut card = new_card();
        card.state = CardState::Relearning;
        card.due_at = Some(TimestampMs(NOON.0 + 60_000));
        assert_eq!(natural_queue(&card, &now), QueueState::Learning);
        card.due_at = Some(now.day_end);
        assert_eq!(natural_queue(&card, &now), QueueState::DayLearn);
    }
}
