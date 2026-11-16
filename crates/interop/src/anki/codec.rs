//! Card codec between the collection's card state and Anki's scheduling columns (plan Phase 14
//! step 2).
//!
//! **What `due` means depends on the queue**, not on the card state:
//!
//! | queue | meaning of `due` |
//! |---|---|
//! | 0 new | position in the new-card order |
//! | 1 learning (intraday) | epoch **seconds** |
//! | 2 review, 3 day-learning | **study days** since the collection's creation day (`col.crt`) |
//! | −1 suspended, −2/−3 buried | as for the queue the card would be in (`type` decides) |
//!
//! A card in a filtered deck keeps its home deck in `odid` and its home due in `odue`.
//!
//! **Days are calendar study days**, not `(due − crt) / 86400`: the day of an instant comes from
//! the collection's zone and cutoff ([`StudyCalendar`]), so a review due "in 3 days" stays 3
//! study days away across a DST change. `crt` is the start of a study day; Anki's rollover hour
//! is the cutoff in whole hours (a cutoff such as 04:30 is reported as rounded).
//!
//! **Scheduler versions.** v1 kept relearning cards as `type = 2` in the learning queue and had
//! only one buried queue (−2); v2/v3 use `type = 3` and split −2 (scheduler) / −3 (user). The
//! writer emits v2 numbers (`schedVer = 2`), which v3 shares. The raw columns of a decoded card
//! are kept ([`RawOrigin`]) so nothing the codec does not understand is silently dropped.

use langualinker_domain::TimestampMs;
use langualinker_domain::calendar::{StudyCalendar, StudyDay};
use langualinker_domain::card::{Card, CardFlag, CardState, QueueState};
use serde_json::{Map, Value, json};

use super::{Loss, Losses};

/// A learning `due` above this is epoch seconds, below it a day number (Anki's own heuristic).
pub const EPOCH_SECONDS_THRESHOLD: i64 = 1_000_000_000;
/// Anki's starting ease (permille) for a card that has left the new queue.
pub const DEFAULT_FACTOR: i64 = 2_500;
/// `left` of a card with one learning step to go today (`today * 1000 + total`).
const ONE_STEP_LEFT: i64 = 1_001;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedulerVersion {
    V1,
    /// v2 and v3 (same card numbers).
    V2,
}

impl SchedulerVersion {
    /// From `col.conf.schedVer` (absent means v1).
    pub fn from_conf(sched_ver: Option<i64>) -> Self {
        match sched_ver {
            Some(2..) => Self::V2,
            _ => Self::V1,
        }
    }
}

/// The collection's day origin and calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnkiClock {
    calendar: StudyCalendar,
    crt_day: StudyDay,
    crt_secs: i64,
}

impl AnkiClock {
    /// A clock whose creation day is the study day of `origin` (the export uses the oldest
    /// instant it writes, so every day offset is ≥ 0).
    pub fn starting_at(calendar: StudyCalendar, origin: TimestampMs) -> Self {
        let crt_day = calendar.day_of(origin);
        Self {
            calendar,
            crt_day,
            crt_secs: calendar.day_start(crt_day).0.div_euclid(1000),
        }
    }

    /// The clock of an existing collection (`col.crt` in epoch seconds).
    pub fn from_crt(calendar: StudyCalendar, crt_secs: i64) -> Self {
        Self {
            calendar,
            crt_day: calendar.day_of(TimestampMs(crt_secs * 1000)),
            crt_secs,
        }
    }

    pub fn crt_secs(&self) -> i64 {
        self.crt_secs
    }

    pub fn calendar(&self) -> &StudyCalendar {
        &self.calendar
    }

    /// Anki's `rollover` (hour of the day cutoff) and whether the cutoff had to be rounded.
    pub fn rollover_hour(&self) -> (u32, bool) {
        let minutes = self.calendar.cutoff_minutes();
        (minutes / 60, !minutes.is_multiple_of(60))
    }

    /// Anki's `creationOffset`: minutes **west** of UTC at the creation day.
    pub fn creation_offset(&self) -> i32 {
        -self
            .calendar
            .utc_offset_minutes(TimestampMs(self.crt_secs * 1000))
    }

    /// Study days from the creation day to the day of `at`.
    pub fn day_offset(&self, at: TimestampMs) -> i64 {
        self.calendar.day_of(at).0 - self.crt_day.0
    }

    /// Start of the study day `offset` days after the creation day.
    pub fn day_start(&self, offset: i64) -> TimestampMs {
        self.calendar.day_start(self.crt_day.plus(offset))
    }
}

/// Anki's scheduling columns of one card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnkiCard {
    pub ctype: i64,
    pub queue: i64,
    pub due: i64,
    pub ivl: i64,
    pub factor: i64,
    pub reps: i64,
    pub lapses: i64,
    pub left: i64,
    pub odue: i64,
    pub odid: i64,
    pub flags: i64,
    pub data: String,
}

/// The raw columns a decoded card came with, kept for the loss report and a later re-export.
pub type RawOrigin = AnkiCard;

/// Scheduling of one card on the collection's side.
#[derive(Debug, Clone, PartialEq)]
pub struct Schedule {
    pub state: CardState,
    pub queue: QueueState,
    /// `None` exactly for new cards.
    pub due_at: Option<TimestampMs>,
    /// New-card order (meaningful for new cards).
    pub position: i64,
    pub scheduled_days: i64,
    pub stability: Option<f64>,
    pub difficulty: Option<f64>,
    pub legacy_ease: Option<i64>,
    pub reps: i64,
    pub lapses: i64,
    pub flag: CardFlag,
}

impl Schedule {
    pub fn of(card: &Card) -> Self {
        Self {
            state: card.state,
            queue: card.queue,
            due_at: card.due_at,
            position: card.position,
            scheduled_days: card.fsrs.scheduled_days,
            stability: card.fsrs.stability,
            difficulty: card.fsrs.difficulty,
            legacy_ease: card.legacy_ease,
            reps: card.reps,
            lapses: card.lapses,
            flag: card.flag,
        }
    }
}

fn state_code(state: CardState) -> i64 {
    state.db_code()
}

/// Encodes a card for a v2 collection. Losses: learning step position, bury expiry, FSRS memory
/// carried only as card data.
pub fn encode(card: &Card, clock: &AnkiClock, losses: &mut Losses) -> AnkiCard {
    let schedule = Schedule::of(card);
    let mut out = encode_schedule(&schedule, clock, losses);
    if card.learning_step.is_some() && out.left != 0 {
        losses.add(Loss::LearningSteps, 1);
    }
    if card.buried_until.is_some() && card.queue.is_buried() {
        losses.add(Loss::BuryExpiry, 1);
    }
    out.flags = card.flag.db_code();
    out
}

/// [`encode`] without the card-only fields.
pub fn encode_schedule(schedule: &Schedule, clock: &AnkiClock, losses: &mut Losses) -> AnkiCard {
    let state = schedule.state;
    // The queue the card would be in when not suspended or buried. A suspended learning card
    // keeps its instant as epoch seconds, which the decoder recognises by size: lossless.
    let natural = match schedule.queue {
        QueueState::Suspended | QueueState::SchedBuried | QueueState::UserBuried => {
            QueueState::natural(state, false)
        }
        queue => queue,
    };
    let due_at = schedule
        .due_at
        .unwrap_or(TimestampMs(clock.crt_secs * 1000));
    let due = match natural {
        QueueState::New => schedule.position.clamp(0, i64::from(i32::MAX)),
        QueueState::Learning => due_at.0.div_euclid(1000),
        _ => clock.day_offset(due_at).max(0),
    };
    let learning = matches!(state, CardState::Learning | CardState::Relearning);
    let ivl = match state {
        CardState::New | CardState::Learning => 0,
        CardState::Review | CardState::Relearning => schedule.scheduled_days.max(1),
    };
    let factor = match state {
        CardState::New => 0,
        _ => schedule.legacy_ease.unwrap_or(DEFAULT_FACTOR),
    };
    let data = match (schedule.stability, schedule.difficulty) {
        (Some(stability), Some(difficulty)) if state != CardState::New => {
            losses.add(Loss::FsrsMemoryUnverified, 1);
            json!({ "s": round4(stability), "d": round4(difficulty) }).to_string()
        }
        _ => String::new(),
    };
    AnkiCard {
        ctype: state_code(state),
        queue: schedule.queue.db_code(),
        due,
        ivl,
        factor,
        reps: schedule.reps,
        lapses: schedule.lapses,
        left: if learning { ONE_STEP_LEFT } else { 0 },
        odue: 0,
        odid: 0,
        flags: schedule.flag.db_code(),
        data,
    }
}

fn round4(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

/// A decoded Anki card.
#[derive(Debug, Clone, PartialEq)]
pub struct Decoded {
    pub schedule: Schedule,
    /// The card sat in a filtered deck: `home_deck` is `odid`, the due value was `odue`.
    pub filtered: bool,
    pub home_deck: Option<i64>,
    pub raw: RawOrigin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnknownColumn {
    pub column: &'static str,
    pub value: i64,
}

/// Decodes Anki's columns. Unknown `type`/`queue` values are an error, never a guess.
pub fn decode(
    card: &AnkiCard,
    clock: &AnkiClock,
    version: SchedulerVersion,
) -> Result<Decoded, UnknownColumn> {
    let filtered = card.odid != 0;
    let due = if filtered && card.odue != 0 {
        card.odue
    } else {
        card.due
    };
    let mut state = match card.ctype {
        0 => CardState::New,
        1 => CardState::Learning,
        2 => CardState::Review,
        3 => CardState::Relearning,
        value => {
            return Err(UnknownColumn {
                column: "type",
                value,
            });
        }
    };
    let queue = match card.queue {
        0 => QueueState::New,
        1 => QueueState::Learning,
        2 => QueueState::Review,
        3 => QueueState::DayLearn,
        -1 => QueueState::Suspended,
        // v1 had a single buried queue; it was the user's or the scheduler's, unknown which.
        -2 => QueueState::SchedBuried,
        -3 if version == SchedulerVersion::V2 => QueueState::UserBuried,
        value => {
            return Err(UnknownColumn {
                column: "queue",
                value,
            });
        }
    };
    // v1: a lapsed review card relearns in the learning queues with type 2.
    if version == SchedulerVersion::V1
        && state == CardState::Review
        && matches!(queue, QueueState::Learning | QueueState::DayLearn)
    {
        state = CardState::Relearning;
    }
    // A filtered deck may hold a card in a queue that is not its home queue; the home queue
    // follows from its state.
    let queue = if filtered && !matches!(queue, QueueState::Suspended) && !queue.is_buried() {
        QueueState::natural(state, queue == QueueState::DayLearn)
    } else {
        queue
    };
    let natural = match queue {
        QueueState::Suspended | QueueState::SchedBuried | QueueState::UserBuried => match state {
            CardState::New => QueueState::New,
            CardState::Review => QueueState::Review,
            _ if due > EPOCH_SECONDS_THRESHOLD => QueueState::Learning,
            _ => QueueState::DayLearn,
        },
        queue => queue,
    };
    let (due_at, position) = match natural {
        QueueState::New => (None, due),
        QueueState::Learning => (Some(TimestampMs(due.saturating_mul(1000))), 0),
        _ => (Some(clock.day_start(due)), 0),
    };
    let (stability, difficulty) = memory(&card.data);
    let scheduled_days = match state {
        CardState::Review | CardState::Relearning => card.ivl.max(0),
        _ => 0,
    };
    Ok(Decoded {
        schedule: Schedule {
            state,
            queue,
            due_at: if state == CardState::New {
                None
            } else {
                due_at
            },
            position,
            scheduled_days,
            stability,
            difficulty,
            legacy_ease: (card.factor > 0).then_some(card.factor),
            reps: card.reps,
            lapses: card.lapses,
            flag: CardFlag::from_db_code(card.flags & 0b111).unwrap_or(CardFlag::None),
        },
        filtered,
        home_deck: filtered.then_some(card.odid),
        raw: card.clone(),
    })
}

/// FSRS memory from `cards.data` (`{"s": …, "d": …}` on Anki 23.10+).
fn memory(data: &str) -> (Option<f64>, Option<f64>) {
    let Ok(Value::Object(map)) = serde_json::from_str::<Value>(data) else {
        return (None, None);
    };
    let number = |map: &Map<String, Value>, key: &str| map.get(key).and_then(Value::as_f64);
    match (number(&map, "s"), number(&map, "d")) {
        (Some(stability), Some(difficulty)) => (Some(stability), Some(difficulty)),
        _ => (None, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use langualinker_domain::card::CardFsrsState;
    use langualinker_domain::{CardId, DeckId, Guid, NoteId};

    const HOUR: i64 = 3_600_000;
    const DAY: i64 = 24 * HOUR;

    fn warsaw() -> StudyCalendar {
        StudyCalendar::new("Europe/Warsaw", 4 * 60).unwrap()
    }

    /// 2026-03-01 12:00 UTC.
    const MARCH_1: TimestampMs = TimestampMs(1_772_366_400_000);

    fn card(state: CardState, queue: QueueState, due_at: Option<TimestampMs>) -> Card {
        Card {
            id: CardId(1),
            guid: Guid::from_bytes([1; 16]),
            note_id: NoteId(1),
            deck_id: DeckId(1),
            template_ordinal: 0,
            position: 42,
            flag: CardFlag::Blue,
            state,
            queue,
            due_at,
            fsrs: CardFsrsState {
                stability: Some(12.345_67),
                difficulty: Some(5.5),
                last_review_at: None,
                scheduled_days: 9,
                elapsed_days: 0,
            },
            legacy_ease: Some(2_300),
            reps: 7,
            lapses: 1,
            learning_step: None,
            buried_until: None,
            archived_at: None,
            created_at: MARCH_1,
            updated_at: MARCH_1,
            deleted_at: None,
            usn: -1,
            version: 1,
        }
    }

    fn round_trip(card: &Card, clock: &AnkiClock) -> (AnkiCard, Schedule) {
        let mut losses = Losses::default();
        let encoded = encode(card, clock, &mut losses);
        let decoded = decode(&encoded, clock, SchedulerVersion::V2).unwrap();
        (encoded, decoded.schedule)
    }

    #[test]
    fn crt_is_the_start_of_a_study_day_with_rollover_and_offset() {
        let clock = AnkiClock::starting_at(warsaw(), MARCH_1);
        // 2026-03-01 04:00 in Warsaw (CET, UTC+1) = 03:00 UTC.
        assert_eq!(clock.crt_secs() * 1000, MARCH_1.0 - 9 * HOUR);
        assert_eq!(clock.rollover_hour(), (4, false));
        assert_eq!(clock.creation_offset(), -60);
        let half = AnkiClock::starting_at(StudyCalendar::new("UTC", 270).unwrap(), MARCH_1);
        assert_eq!(half.rollover_hour(), (4, true));
    }

    #[test]
    fn review_due_counts_calendar_days_across_dst() {
        let clock = AnkiClock::starting_at(warsaw(), MARCH_1);
        // 2026-03-29 is the spring-forward day; 30 study days later is still day 30.
        let due = warsaw().day_start(warsaw().day_of(MARCH_1).plus(30));
        let (encoded, decoded) = round_trip(
            &card(CardState::Review, QueueState::Review, Some(due)),
            &clock,
        );
        assert_eq!((encoded.ctype, encoded.queue, encoded.due), (2, 2, 30));
        // A naive (due − crt) / 86400 would give 29 days and 23 hours.
        assert_eq!((due.0 / 1000 - clock.crt_secs()) / 86_400, 29);
        assert_eq!(decoded.due_at, Some(due));
        assert_eq!(
            (encoded.ivl, encoded.factor, encoded.reps, encoded.lapses),
            (9, 2_300, 7, 1)
        );
        assert_eq!(encoded.flags, 4);
        assert_eq!(encoded.data, r#"{"d":5.5,"s":12.3457}"#);
    }

    #[test]
    fn new_cards_carry_their_position_and_no_due() {
        let clock = AnkiClock::starting_at(warsaw(), MARCH_1);
        let mut new = card(CardState::New, QueueState::New, None);
        new.fsrs = CardFsrsState::NEW;
        new.legacy_ease = None;
        let (encoded, decoded) = round_trip(&new, &clock);
        assert_eq!(
            (
                encoded.ctype,
                encoded.queue,
                encoded.due,
                encoded.ivl,
                encoded.factor
            ),
            (0, 0, 42, 0, 0)
        );
        assert_eq!((decoded.due_at, decoded.position), (None, 42));
        assert!(encoded.data.is_empty());
    }

    #[test]
    fn intraday_learning_is_epoch_seconds_and_day_learning_a_day_number() {
        let clock = AnkiClock::starting_at(warsaw(), MARCH_1);
        let soon = TimestampMs(MARCH_1.0 + 10 * 60_000 + 500);
        let (encoded, decoded) = round_trip(
            &card(CardState::Learning, QueueState::Learning, Some(soon)),
            &clock,
        );
        assert_eq!(
            (encoded.ctype, encoded.queue, encoded.due),
            (1, 1, soon.0 / 1000)
        );
        assert_eq!((encoded.ivl, encoded.left), (0, 1_001));
        // Anki keeps seconds: the milliseconds are the precision lost.
        assert_eq!(decoded.due_at, Some(TimestampMs(soon.0 - 500)));

        let tomorrow = warsaw().day_start(warsaw().day_of(MARCH_1).plus(1));
        let (encoded, decoded) = round_trip(
            &card(CardState::Relearning, QueueState::DayLearn, Some(tomorrow)),
            &clock,
        );
        assert_eq!(
            (encoded.ctype, encoded.queue, encoded.due, encoded.ivl),
            (3, 3, 1, 9)
        );
        assert_eq!(
            (decoded.state, decoded.queue, decoded.due_at),
            (CardState::Relearning, QueueState::DayLearn, Some(tomorrow))
        );
    }

    #[test]
    fn suspended_and_buried_keep_their_underlying_type() {
        let clock = AnkiClock::starting_at(warsaw(), MARCH_1);
        let due = TimestampMs(MARCH_1.0 + 5 * DAY);
        for queue in [
            QueueState::Suspended,
            QueueState::SchedBuried,
            QueueState::UserBuried,
        ] {
            let (encoded, decoded) = round_trip(&card(CardState::Review, queue, Some(due)), &clock);
            assert_eq!(
                (encoded.ctype, encoded.queue, encoded.due),
                (2, queue.db_code(), 5)
            );
            assert_eq!((decoded.state, decoded.queue), (CardState::Review, queue));
        }
        let soon = TimestampMs(MARCH_1.0 + HOUR);
        let (encoded, decoded) = round_trip(
            &card(CardState::Learning, QueueState::Suspended, Some(soon)),
            &clock,
        );
        assert_eq!(
            (encoded.ctype, encoded.queue, encoded.due),
            (1, -1, soon.0 / 1000)
        );
        assert_eq!(decoded.due_at, Some(soon));
        let mut losses = Losses::default();
        let mut buried = card(CardState::Review, QueueState::UserBuried, Some(due));
        buried.buried_until = Some(due);
        encode(&buried, &clock, &mut losses);
        assert_eq!(losses.count(Loss::BuryExpiry), 1);
    }

    #[test]
    fn filtered_cards_decode_to_their_home_deck_and_due() {
        let clock = AnkiClock::starting_at(warsaw(), MARCH_1);
        let raw = AnkiCard {
            ctype: 2,
            queue: 2,
            due: -100_000,
            ivl: 12,
            factor: 2_500,
            reps: 4,
            lapses: 0,
            left: 0,
            odue: 17,
            odid: 1_700_000_000_000,
            flags: 0,
            data: String::new(),
        };
        let decoded = decode(&raw, &clock, SchedulerVersion::V2).unwrap();
        assert!(decoded.filtered);
        assert_eq!(decoded.home_deck, Some(1_700_000_000_000));
        assert_eq!(decoded.schedule.due_at, Some(clock.day_start(17)));
        assert_eq!(decoded.raw, raw);
    }

    #[test]
    fn scheduler_v1_relearning_and_buried_are_version_aware() {
        let clock = AnkiClock::starting_at(warsaw(), MARCH_1);
        let v1 = AnkiCard {
            ctype: 2,
            queue: 1,
            due: 1_772_400_000,
            ivl: 3,
            factor: 2_100,
            reps: 9,
            lapses: 2,
            left: 1_001,
            odue: 0,
            odid: 0,
            flags: 0,
            data: String::new(),
        };
        let decoded = decode(&v1, &clock, SchedulerVersion::V1).unwrap();
        assert_eq!(
            (decoded.schedule.state, decoded.schedule.queue),
            (CardState::Relearning, QueueState::Learning)
        );
        assert_eq!(
            decoded.schedule.due_at,
            Some(TimestampMs(1_772_400_000_000))
        );
        let v2 = decode(&v1, &clock, SchedulerVersion::V2).unwrap();
        assert_eq!(v2.schedule.state, CardState::Review);
        let user_buried = AnkiCard {
            queue: -3,
            ..v1.clone()
        };
        assert_eq!(
            decode(&user_buried, &clock, SchedulerVersion::V1).unwrap_err(),
            UnknownColumn {
                column: "queue",
                value: -3
            }
        );
        assert_eq!(
            decode(&AnkiCard { ctype: 7, ..v1 }, &clock, SchedulerVersion::V2)
                .unwrap_err()
                .column,
            "type"
        );
        assert_eq!(SchedulerVersion::from_conf(None), SchedulerVersion::V1);
        assert_eq!(SchedulerVersion::from_conf(Some(2)), SchedulerVersion::V2);
    }

    #[test]
    fn fsrs_memory_comes_from_card_data() {
        assert_eq!(
            memory(r#"{"s":3.2,"d":6.1,"dr":0.9}"#),
            (Some(3.2), Some(6.1))
        );
        assert_eq!(memory(r#"{"pos":3}"#), (None, None));
        assert_eq!(memory(""), (None, None));
        assert_eq!(memory("not json"), (None, None));
    }
}
