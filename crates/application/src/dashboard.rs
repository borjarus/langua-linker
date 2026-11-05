//! Dashboard read model (plan Phase 8 step 13; scheduler-dependent parts arrive in Phase 10).
//!
//! Counts come from the cards' stored state and queue. Today's boundaries follow the collection
//! calendar (IANA zone and cutoff, Phase 9); the limit-aware counts of the scheduler
//! (`study::due_counts`) and the zone-aware streak replace these in Phase 10.

use std::collections::{HashMap, HashSet};

use langualinker_domain::deck::Deck;
use langualinker_domain::{DeckId, TimestampMs};

use crate::AppResult;
use crate::content::{DAILY_GOAL_KEY, DEFAULT_DAILY_GOAL};
use crate::repositories::{DeckCardCounts, Repositories};

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

#[derive(Debug, Clone, PartialEq)]
pub struct Dashboard {
    pub decks: Vec<DashboardDeck>,
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

/// Builds the dashboard inside one read transaction. `pending` are deck GUIDs with an update queued.
pub fn dashboard(
    tx: &mut dyn Repositories,
    now: TimestampMs,
    pending: &[String],
) -> AppResult<Dashboard> {
    let cutoff = tx.day_cutoff_minutes()?;
    let moment = crate::study::study_moment(tx, now)?;
    let (day_start, day_end) = (moment.day_start, moment.day_end);
    let decks = tx.active_decks()?;
    let own: HashMap<DeckId, DeckCardCounts> = tx
        .deck_card_counts(day_start, day_end)?
        .into_iter()
        .collect();
    let pending: HashSet<&str> = pending.iter().map(String::as_str).collect();

    let children = |parent: DeckId| {
        decks
            .iter()
            .filter(move |deck| deck.parent_id == Some(parent))
            .map(|deck| deck.id)
    };
    let mut result = Vec::with_capacity(decks.len());
    for deck in &decks {
        let mut counts = DeckCardCounts::default();
        let mut stack = vec![deck.id];
        let mut visited = HashSet::new();
        while let Some(id) = stack.pop() {
            if !visited.insert(id) {
                continue;
            }
            if let Some(own) = own.get(&id) {
                counts.new += own.new;
                counts.learning += own.learning;
                counts.review += own.review;
                counts.overdue += own.overdue;
                counts.total += own.total;
                counts.mature += own.mature;
            }
            stack.extend(children(id));
        }
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
    let due_today = result
        .iter()
        .filter(|item| item.deck.parent_id.is_none())
        .map(|item| item.counts.review + item.counts.learning)
        .sum();
    let today = (now.0 - i64::from(cutoff) * 60_000).div_euclid(DAY_MS);
    let days = tx.review_days(cutoff, 3_660)?;
    let daily_goal = tx
        .setting(DAILY_GOAL_KEY)?
        .and_then(|json| serde_json::from_str::<u32>(&json).ok())
        .unwrap_or(DEFAULT_DAILY_GOAL);
    Ok(Dashboard {
        decks: result,
        streak_days: streak(&days, today),
        studied_today: tx.reviews_since(day_start)?,
        due_today,
        daily_goal,
        next_due_at: tx.next_due_after(day_end)?,
        generated_at: now,
    })
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
