//! Dashboard read model (plan Phase 8 step 13, Phase 10 step 4).
//!
//! Due counts are the scheduler's queue of each deck (with subdecks) within today's limits, so
//! they match what a session would show. `total` and progress come from the cards' stored state.
//! Today's boundaries, the streak and "studied today" follow the collection calendar (IANA zone
//! and cutoff). The streak counts consecutive study days with at least one valid rating, ending
//! today or yesterday; v1 has no protection days (ADR-16).

use std::collections::{HashMap, HashSet};

use langualinker_domain::deck::Deck;
use langualinker_domain::{DeckId, TimestampMs};

use crate::AppResult;
use crate::content::{DAILY_GOAL_KEY, DEFAULT_DAILY_GOAL};
use crate::repositories::{DeckCardCounts, Repositories, SessionStatus};
use crate::study;

pub const DAY_MS: i64 = 86_400_000;

#[derive(Debug, Clone, PartialEq)]
pub struct DashboardDeck {
    pub deck: Deck,
    /// The deck and its subdecks.
    pub counts: DeckCardCounts,
    /// Share of mature cards, 0..1, rounded to two decimals.
    pub progress: f64,
    /// A bundled update of this deck is pending.
    pub preparing: bool,
}

/// An unfinished session the dashboard offers to resume.
#[derive(Debug, Clone, PartialEq)]
pub struct Resumable {
    pub session_id: langualinker_domain::SessionId,
    pub deck_id: Option<DeckId>,
    pub deck_name: Option<String>,
    pub paused: bool,
    pub reviewed_count: u32,
    pub remaining: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Dashboard {
    pub decks: Vec<DashboardDeck>,
    pub resumable: Option<Resumable>,
    pub streak_days: u32,
    pub studied_today: u32,
    pub due_today: u32,
    pub daily_goal: u32,
    pub next_due_at: Option<TimestampMs>,
    pub generated_at: TimestampMs,
}

/// Start of the study day containing `now`: the last `cutoff` (minutes after midnight, UTC).
pub fn study_day_start(now: TimestampMs, cutoff_minutes: u32) -> TimestampMs {
    let offset = i64::from(cutoff_minutes) * 60_000;
    TimestampMs((now.0 - offset).div_euclid(DAY_MS) * DAY_MS + offset)
}

/// Consecutive study days ending today or yesterday. `days` are distinct day numbers, newest first.
pub fn streak(days: &[i64], today: i64) -> u32 {
    let Some(&first) = days.first() else { return 0 };
    if first < today - 1 {
        return 0;
    }
    let mut count = 1;
    for pair in days.windows(2) {
        if pair[0] - pair[1] != 1 {
            break;
        }
        count += 1;
    }
    count
}

/// Longest streak looked back on (ten years of study days).
const MAX_STREAK_DAYS: u32 = 3_660;

/// Consecutive study days with a valid rating, ending today or yesterday, in the collection
/// calendar (days of 23 or 25 hours across DST count as one day).
pub fn zoned_streak(
    tx: &mut dyn Repositories,
    moment: &langualinker_domain::scheduling::StudyMoment,
) -> AppResult<u32> {
    let calendar = &moment.calendar;
    let mut day = moment.today;
    if !tx.has_review_between(calendar.day_start(day), calendar.day_end(day))? {
        day = day.plus(-1);
    }
    let mut count = 0;
    while count < MAX_STREAK_DAYS
        && tx.has_review_between(calendar.day_start(day), calendar.day_end(day))?
    {
        count += 1;
        day = day.plus(-1);
    }
    Ok(count)
}

/// Builds the dashboard inside one write transaction (expired burials return to their queues
/// first, as a session would see them). `pending` are deck GUIDs with an update queued.
pub fn dashboard(
    tx: &mut dyn Repositories,
    now: TimestampMs,
    pending: &[String],
) -> AppResult<Dashboard> {
    let moment = study::study_moment(tx, now)?;
    study::unbury_expired(tx, &moment)?;
    let decks = tx.active_decks()?;
    let ids: Vec<DeckId> = decks.iter().map(|deck| deck.id).collect();
    let result = summaries(tx, &moment, &decks, &ids, pending)?;
    let all = study::scope_counts(tx, None, &moment)?;
    let daily_goal = tx
        .setting(DAILY_GOAL_KEY)?
        .and_then(|json| serde_json::from_str::<u32>(&json).ok())
        .unwrap_or(DEFAULT_DAILY_GOAL);
    let resumable = match tx.open_session()? {
        Some(session) => {
            let view = study::study_view(tx, session.id, now)?;
            let deck_name = match session.deck_id {
                Some(id) => tx.deck(id)?.map(|deck| deck.name),
                None => None,
            };
            Some(Resumable {
                session_id: session.id,
                deck_id: session.deck_id,
                deck_name,
                paused: session.status == SessionStatus::Paused,
                reviewed_count: view.reviewed_count,
                remaining: view.counts.new + view.counts.learning + view.counts.review,
            })
        }
        None => None,
    };
    Ok(Dashboard {
        decks: result,
        resumable,
        streak_days: zoned_streak(tx, &moment)?,
        studied_today: tx.reviews_since(moment.day_start)?,
        due_today: all.review + all.learning,
        daily_goal,
        next_due_at: tx.next_due_after(moment.day_end)?,
        generated_at: now,
    })
}

/// Dashboard rows of the decks `ids` (in that order): queue counts of each deck with its
/// subdecks within today's limits, stored totals and progress rolled up without counting a deck
/// twice. `decks` are the active decks; `pending` are deck GUIDs with an update queued.
pub fn summaries(
    tx: &mut dyn Repositories,
    moment: &langualinker_domain::scheduling::StudyMoment,
    decks: &[Deck],
    ids: &[DeckId],
    pending: &[String],
) -> AppResult<Vec<DashboardDeck>> {
    let own: HashMap<DeckId, DeckCardCounts> = tx
        .deck_card_counts(moment.day_start, moment.day_end)?
        .into_iter()
        .collect();
    let pending: HashSet<&str> = pending.iter().map(String::as_str).collect();
    let children = |parent: DeckId| {
        decks
            .iter()
            .filter(move |deck| deck.parent_id == Some(parent))
            .map(|deck| deck.id)
    };
    let mut result = Vec::with_capacity(ids.len());
    for id in ids {
        let Some(deck) = decks.iter().find(|deck| deck.id == *id) else {
            continue;
        };
        let mut stored = DeckCardCounts::default();
        let mut stack = vec![deck.id];
        let mut visited = HashSet::new();
        while let Some(id) = stack.pop() {
            if !visited.insert(id) {
                continue;
            }
            if let Some(own) = own.get(&id) {
                stored.total += own.total;
                stored.mature += own.mature;
            }
            stack.extend(children(id));
        }
        let queue = study::scope_counts(tx, Some(deck.id), moment)?;
        let counts = DeckCardCounts {
            new: queue.new,
            learning: queue.learning,
            review: queue.review,
            overdue: queue.overdue,
            total: stored.total,
            mature: stored.mature,
        };
        let progress = if counts.total == 0 {
            0.0
        } else {
            (f64::from(counts.mature) / f64::from(counts.total) * 100.0).round() / 100.0
        };
        result.push(DashboardDeck {
            deck: deck.clone(),
            counts,
            progress,
            preparing: deck.bundled && pending.contains(deck.guid.as_str()),
        });
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn study_day_starts_at_the_cutoff() {
        // 2026-03-02 03:00 UTC is still the study day that began 2026-03-01 04:00.
        let at = TimestampMs(1_772_420_400_000);
        assert_eq!(study_day_start(at, 240), TimestampMs(1_772_337_600_000));
        // 05:00 belongs to the day starting the same morning at 04:00.
        let later = TimestampMs(at.0 + 2 * 3_600_000);
        assert_eq!(study_day_start(later, 240), TimestampMs(1_772_424_000_000));
        assert_eq!(study_day_start(later, 0), TimestampMs(1_772_409_600_000));
    }

    #[test]
    fn streak_counts_consecutive_days_up_to_yesterday() {
        assert_eq!(streak(&[], 10), 0);
        assert_eq!(streak(&[10, 9, 8, 6], 10), 3);
        assert_eq!(streak(&[9, 8], 10), 2, "today not studied yet");
        assert_eq!(streak(&[8, 7], 10), 0);
    }
}
