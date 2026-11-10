//! Statistics read model (plan Phase 12 steps 1–5, ADR-17).
//!
//! Definitions:
//! - **Reviews per day**: valid (not undone) ratings, excluding manual entries, grouped into study
//!   days of the collection calendar (IANA zone + cutoff); time is the stored answer time, which
//!   the session already capped and measured without pauses.
//! - **True retention**: ratings of cards whose state before the rating was Review, made on or
//!   after the card's due day (no early reviews), first such rating per card per study day.
//!   Again fails; Hard, Good and Easy pass. Young/mature by the interval before the rating
//!   (mature from 21 days). Fewer than ten ratings → `None` ("not enough data"), never 0%.
//!   Learning, relearning, manual and undone ratings never count.
//! - **Forecast**: stored due dates per study day for 30 days (day 0 includes the overdue cards),
//!   and a projection that carries the backlog forward under the preset's review limit. New
//!   cards are not forecast: future answers are unknown, so the projection never guesses them.
//! - **Card states** come from the cards as stored (suspended counted separately); maturity uses
//!   the current interval of review cards.

use std::collections::HashSet;

use langualinker_domain::calendar::StudyDay;
use langualinker_domain::card::{CardState, QueueState};
use langualinker_domain::review::{Rating, ReviewKind};
use langualinker_domain::{CardId, DeckId, TimestampMs};

use crate::repositories::{CardFact, Repositories, ReviewFact};
use crate::{AppError, AppResult, management, settings, study};

/// Interval from which a review card is mature (days).
pub const MATURE_DAYS: i64 = 21;
/// Ratings needed before a retention figure is shown.
pub const MIN_RETENTION_SAMPLE: u32 = 10;
pub const FORECAST_DAYS: usize = 30;
/// Points of the difficulty/stability scatter; larger collections are sampled evenly.
pub const MAX_SCATTER_POINTS: usize = 2_000;
/// Days the "all" range looks back at most.
const MAX_RANGE_DAYS: i64 = 3_650;
const MAX_STREAK_DAYS: u32 = 3_660;
/// Stability buckets in days, `[min, max)`.
pub const STABILITY_BUCKETS: &[(u32, Option<u32>)] = &[
    (0, Some(1)),
    (1, Some(3)),
    (3, Some(7)),
    (7, Some(14)),
    (14, Some(30)),
    (30, Some(90)),
    (90, None),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatsRange {
    Month,
    Quarter,
    Year,
    All,
}

impl StatsRange {
    fn days(self) -> Option<i64> {
        match self {
            Self::Month => Some(30),
            Self::Quarter => Some(90),
            Self::Year => Some(365),
            Self::All => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ratio {
    pub passed: u32,
    pub total: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DayReviews {
    pub day: StudyDay,
    pub count: u32,
    pub active_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForecastDay {
    pub day: StudyDay,
    pub stored: u32,
    pub projected: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StateCounts {
    pub new: u32,
    pub learning: u32,
    pub review: u32,
    pub relearning: u32,
    pub suspended: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StabilityBucket {
    pub min_days: u32,
    pub max_days: Option<u32>,
    pub count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MemoryPoint {
    pub card_id: CardId,
    pub stability: f64,
    pub difficulty: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stats {
    pub reviews_by_day: Vec<DayReviews>,
    pub forecast: Vec<ForecastDay>,
    pub reviews_per_day: u32,
    pub new_per_day: u32,
    pub overdue: u32,
    pub states: StateCounts,
    pub young: u32,
    pub mature: u32,
    pub retention_young: Option<Ratio>,
    pub retention_mature: Option<Ratio>,
    pub stability: Vec<StabilityBucket>,
    pub memory: Vec<MemoryPoint>,
    pub streak_days: u32,
    pub longest_streak_days: u32,
    pub generated_at: TimestampMs,
}

/// Statistics of one deck with its subdecks (`None` = every deck) over `range`.
pub fn stats(
    tx: &mut dyn Repositories,
    now: TimestampMs,
    deck: Option<DeckId>,
    range: StatsRange,
) -> AppResult<Stats> {
    let moment = study::study_moment(tx, now)?;
    let calendar = &moment.calendar;
    let today = moment.today;

    let (scope, config_id) = match deck {
        Some(id) => {
            let stored = tx.deck(id)?.ok_or(AppError::NotFound { entity: "deck" })?;
            if stored.deleted_at.is_some() {
                return Err(AppError::Deleted { entity: "deck" });
            }
            let decks = tx.active_decks()?;
            (Some(management::subtree(&decks, id)), stored.config_id)
        }
        None => (None, settings::default_preset(tx)?),
    };
    let scope = scope.as_deref();

    let first_day = match range.days() {
        Some(days) => today.plus(-(days - 1)),
        None => {
            let oldest = tx
                .first_review_at(scope)?
                .map_or(today, |at| calendar.day_of(at));
            StudyDay(oldest.0.clamp(today.0 - (MAX_RANGE_DAYS - 1), today.0 - 29))
        }
    };
    let facts = tx.review_facts(scope, calendar.day_start(first_day))?;
    let reviews_by_day = reviews_by_day(&facts, |at| calendar.day_of(at), first_day, today);
    let (retention_young, retention_mature) = true_retention(&facts, |at| calendar.day_of(at));

    let cards = tx.card_facts(scope)?;
    let preset = tx
        .config(config_id)?
        .ok_or(AppError::NotFound { entity: "preset" })?;
    let reviews_per_day = preset.options.reviews_per_day;
    let (forecast, overdue) = forecast(&cards, |at| calendar.day_of(at), today, reviews_per_day);
    let (states, young, mature) = states(&cards);

    let mut streak_days = 0;
    let mut day = today;
    if !tx.has_review_in(scope, calendar.day_start(day), calendar.day_end(day))? {
        day = day.plus(-1);
    }
    while streak_days < MAX_STREAK_DAYS
        && tx.has_review_in(scope, calendar.day_start(day), calendar.day_end(day))?
    {
        streak_days += 1;
        day = day.plus(-1);
    }
    let longest_streak_days = longest_run(&reviews_by_day).max(streak_days);

    Ok(Stats {
        reviews_by_day,
        forecast,
        reviews_per_day,
        new_per_day: preset.options.new_per_day,
        overdue,
        states,
        young,
        mature,
        retention_young,
        retention_mature,
        stability: stability_histogram(&cards),
        memory: memory_points(&cards),
        streak_days,
        longest_streak_days,
        generated_at: now,
    })
}

/// Every study day from `first` to `today` with its count and answer time.
pub fn reviews_by_day(
    facts: &[ReviewFact],
    day_of: impl Fn(TimestampMs) -> StudyDay,
    first: StudyDay,
    today: StudyDay,
) -> Vec<DayReviews> {
    let mut days: Vec<DayReviews> = (first.0..=today.0)
        .map(|day| DayReviews {
            day: StudyDay(day),
            count: 0,
            active_ms: 0,
        })
        .collect();
    for fact in facts.iter().filter(|fact| fact.kind != ReviewKind::Manual) {
        let index = day_of(fact.reviewed_at).0 - first.0;
        if let Some(day) = usize::try_from(index)
            .ok()
            .and_then(|index| days.get_mut(index))
        {
            day.count += 1;
            day.active_ms += fact.duration_ms.max(0);
        }
    }
    days
}

/// True retention of young and mature cards (see the module docs).
pub fn true_retention(
    facts: &[ReviewFact],
    day_of: impl Fn(TimestampMs) -> StudyDay,
) -> (Option<Ratio>, Option<Ratio>) {
    let mut seen: HashSet<(CardId, i64)> = HashSet::new();
    let mut young = Ratio {
        passed: 0,
        total: 0,
    };
    let mut mature = Ratio {
        passed: 0,
        total: 0,
    };
    for fact in facts {
        if fact.kind != ReviewKind::Review || fact.state_before != CardState::Review {
            continue;
        }
        let day = day_of(fact.reviewed_at);
        // A review before the due day is early, not a test of retention at the interval.
        if fact.due_before.is_none_or(|due| day_of(due).0 > day.0) {
            continue;
        }
        if !seen.insert((fact.card_id, day.0)) {
            continue;
        }
        let bucket = if fact.interval_before_days >= MATURE_DAYS {
            &mut mature
        } else {
            &mut young
        };
        bucket.total += 1;
        if fact.rating != Rating::Again {
            bucket.passed += 1;
        }
    }
    let enough = |ratio: Ratio| (ratio.total >= MIN_RETENTION_SAMPLE).then_some(ratio);
    (enough(young), enough(mature))
}

/// Stored due dates per study day and the projected workload; returns the overdue count too.
pub fn forecast(
    cards: &[CardFact],
    day_of: impl Fn(TimestampMs) -> StudyDay,
    today: StudyDay,
    reviews_per_day: u32,
) -> (Vec<ForecastDay>, u32) {
    let mut stored = [0u32; FORECAST_DAYS];
    let mut overdue = 0;
    for card in cards {
        if card.state == CardState::New || card.queue == QueueState::Suspended {
            continue;
        }
        let Some(due) = card.due_at else { continue };
        let offset = day_of(due).0 - today.0;
        if offset < 0 {
            overdue += 1;
        }
        if let Some(slot) = usize::try_from(offset.max(0))
            .ok()
            .and_then(|index| stored.get_mut(index))
        {
            *slot += 1;
        }
    }
    let mut backlog: u64 = 0;
    let days = stored
        .iter()
        .enumerate()
        .map(|(index, count)| {
            backlog += u64::from(*count);
            let projected = backlog.min(u64::from(reviews_per_day));
            backlog -= projected;
            ForecastDay {
                day: today.plus(index as i64),
                stored: *count,
                projected: projected as u32,
            }
        })
        .collect();
    (days, overdue)
}

/// Cards per state and review cards split into young and mature.
pub fn states(cards: &[CardFact]) -> (StateCounts, u32, u32) {
    let mut states = StateCounts::default();
    let (mut young, mut mature) = (0, 0);
    for card in cards {
        if card.queue == QueueState::Suspended {
            states.suspended += 1;
            continue;
        }
        match card.state {
            CardState::New => states.new += 1,
            CardState::Learning => states.learning += 1,
            CardState::Relearning => states.relearning += 1,
            CardState::Review => {
                states.review += 1;
                if card.interval_days >= MATURE_DAYS {
                    mature += 1;
                } else {
                    young += 1;
                }
            }
        }
    }
    (states, young, mature)
}

fn stability_histogram(cards: &[CardFact]) -> Vec<StabilityBucket> {
    STABILITY_BUCKETS
        .iter()
        .map(|&(min_days, max_days)| StabilityBucket {
            min_days,
            max_days,
            count: cards
                .iter()
                .filter_map(|card| card.stability)
                .filter(|stability| {
                    *stability >= f64::from(min_days)
                        && max_days.is_none_or(|max| *stability < f64::from(max))
                })
                .count() as u32,
        })
        .collect()
}

fn memory_points(cards: &[CardFact]) -> Vec<MemoryPoint> {
    let studied: Vec<MemoryPoint> = cards
        .iter()
        .filter_map(|card| {
            Some(MemoryPoint {
                card_id: card.card_id,
                stability: card.stability?,
                difficulty: card.difficulty?,
            })
        })
        .collect();
    if studied.len() <= MAX_SCATTER_POINTS {
        return studied;
    }
    let step = studied.len().div_ceil(MAX_SCATTER_POINTS);
    studied.into_iter().step_by(step).collect()
}

fn longest_run(days: &[DayReviews]) -> u32 {
    let (mut longest, mut run) = (0, 0);
    for day in days {
        run = if day.count > 0 { run + 1 } else { 0 };
        longest = longest.max(run);
    }
    longest
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400_000;

    fn day_of(at: TimestampMs) -> StudyDay {
        StudyDay(at.0.div_euclid(DAY))
    }

    fn fact(card: i64, day: i64, rating: Rating, interval: i64) -> ReviewFact {
        ReviewFact {
            card_id: CardId(card),
            reviewed_at: TimestampMs(day * DAY + 3_600_000),
            duration_ms: 4_000,
            rating,
            kind: ReviewKind::Review,
            state_before: CardState::Review,
            interval_before_days: interval,
            due_before: Some(TimestampMs(day * DAY)),
        }
    }

    #[test]
    fn retention_counts_hard_as_a_pass_and_only_the_first_rating_of_a_day() {
        let mut facts: Vec<ReviewFact> = (0..10)
            .map(|card| fact(card, 100, Rating::Hard, 5))
            .collect();
        facts[0].rating = Rating::Again;
        // A second rating of card 1 on the same day does not count again.
        facts.push(fact(1, 100, Rating::Again, 5));
        let (young, mature) = true_retention(&facts, day_of);
        assert_eq!(
            young,
            Some(Ratio {
                passed: 9,
                total: 10
            })
        );
        assert_eq!(mature, None, "no mature ratings is not enough data, not 0%");
    }

    #[test]
    fn retention_ignores_learning_manual_and_early_reviews_and_splits_by_interval_before() {
        let mut facts = Vec::new();
        for card in 0..10 {
            facts.push(fact(card, 50, Rating::Good, 21));
        }
        let mut learning = fact(20, 50, Rating::Again, 0);
        learning.kind = ReviewKind::Learning;
        learning.state_before = CardState::Learning;
        let mut manual = fact(21, 50, Rating::Again, 30);
        manual.kind = ReviewKind::Manual;
        let mut early = fact(22, 50, Rating::Again, 30);
        early.due_before = Some(TimestampMs(52 * DAY));
        facts.extend([learning, manual, early]);
        let (young, mature) = true_retention(&facts, day_of);
        assert_eq!(young, None);
        assert_eq!(
            mature,
            Some(Ratio {
                passed: 10,
                total: 10
            })
        );
    }

    fn card(
        id: i64,
        state: CardState,
        queue: QueueState,
        due_day: Option<i64>,
        interval: i64,
    ) -> CardFact {
        CardFact {
            card_id: CardId(id),
            deck_id: DeckId(1),
            state,
            queue,
            due_at: due_day.map(|day| TimestampMs(day * DAY + 1)),
            stability: (state != CardState::New).then_some(interval as f64),
            difficulty: (state != CardState::New).then_some(5.0),
            interval_days: interval,
        }
    }

    #[test]
    fn forecast_puts_overdue_cards_on_day_zero_and_carries_the_backlog() {
        let today = StudyDay(100);
        let cards = vec![
            card(1, CardState::Review, QueueState::Review, Some(98), 3),
            card(2, CardState::Review, QueueState::Review, Some(100), 3),
            card(3, CardState::Review, QueueState::Review, Some(100), 3),
            card(4, CardState::Review, QueueState::Review, Some(101), 3),
            card(5, CardState::Review, QueueState::Suspended, Some(100), 3),
            card(6, CardState::New, QueueState::New, None, 0),
            card(7, CardState::Review, QueueState::Review, Some(200), 90),
        ];
        let (days, overdue) = forecast(&cards, day_of, today, 2);
        assert_eq!(overdue, 1);
        assert_eq!(days.len(), FORECAST_DAYS);
        assert_eq!((days[0].stored, days[0].projected), (3, 2));
        assert_eq!(
            (days[1].stored, days[1].projected),
            (1, 2),
            "the backlog moves on"
        );
        assert_eq!(days[2].projected, 0);
        assert_eq!(days[0].day, today);
    }

    #[test]
    fn states_count_suspended_cards_apart_and_split_reviews_by_interval() {
        let cards = vec![
            card(1, CardState::New, QueueState::New, None, 0),
            card(2, CardState::Review, QueueState::Review, Some(1), 21),
            card(3, CardState::Review, QueueState::Review, Some(1), 20),
            card(4, CardState::Review, QueueState::Suspended, Some(1), 40),
            card(5, CardState::Relearning, QueueState::Learning, Some(1), 1),
        ];
        let (states, young, mature) = states(&cards);
        assert_eq!(
            states,
            StateCounts {
                new: 1,
                learning: 0,
                review: 2,
                relearning: 1,
                suspended: 1
            }
        );
        assert_eq!((young, mature), (1, 1));
        let histogram = stability_histogram(&cards);
        assert_eq!(histogram.iter().map(|bucket| bucket.count).sum::<u32>(), 4);
        assert_eq!(histogram.last().unwrap().count, 0);
    }

    #[test]
    fn days_without_reviews_are_listed_and_manual_entries_are_not_counted() {
        let mut manual = fact(1, 11, Rating::Good, 3);
        manual.kind = ReviewKind::Manual;
        let facts = vec![
            fact(1, 10, Rating::Good, 3),
            fact(2, 10, Rating::Good, 3),
            manual,
        ];
        let days = reviews_by_day(&facts, day_of, StudyDay(9), StudyDay(12));
        let counts: Vec<u32> = days.iter().map(|day| day.count).collect();
        assert_eq!(counts, vec![0, 2, 0, 0]);
        assert_eq!(days[1].active_ms, 8_000);
        assert_eq!(longest_run(&days), 1);
    }

    #[test]
    fn large_collections_are_sampled_for_the_scatter() {
        let cards: Vec<CardFact> = (0..5_000)
            .map(|id| card(id, CardState::Review, QueueState::Review, Some(1), 5))
            .collect();
        let points = memory_points(&cards);
        assert!(points.len() <= MAX_SCATTER_POINTS && points.len() >= MAX_SCATTER_POINTS / 2);
    }
}
