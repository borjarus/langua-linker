//! Daily study reminders (plan Phase 15 steps 3–7, ADR-20).
//!
//! The platform cannot run our code at reminder time (Rust is not assumed alive after the app is
//! killed), so reminders are planned ahead: a rolling window of one-shot notifications for the
//! next [`REMINDER_DAYS`] days, recomputed whenever the app starts, returns to the foreground,
//! goes to the background, or the settings change. Rules:
//! - one reminder per local day at the chosen `HH:MM` in the collection time zone (DST gaps move
//!   it to the first existing minute, repeated times use the first occurrence);
//! - **no material, no reminder**: a day gets a reminder only if review cards are due by the end
//!   of its study day (stored due dates, i.e. a forecast) or new cards are waiting;
//! - **done today, no reminder**: today's reminder is dropped once the daily goal is reached and
//!   no review is due any more;
//! - **free day, no reminder**: a study-free day (a weekday or a date in the settings) gets none;
//! - the review count in the text is a forecast from the stored due dates at planning time.

use langualinker_domain::TimestampMs;
use langualinker_domain::calendar::StudyCalendar;
use langualinker_domain::card::{CardState, QueueState};
use langualinker_domain::free_days::FreeDays;

use crate::repositories::{CardFact, Repositories};
use crate::{AppResult, settings, study};

/// Days ahead the rolling window covers (iOS keeps at most 64 pending requests per app).
pub const REMINDER_DAYS: i64 = 7;

/// One planned notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReminderSlot {
    pub at: TimestampMs,
    /// Review cards due by the end of that study day (forecast at planning time).
    pub reviews: u32,
    /// New cards are waiting to be learned.
    pub new_available: bool,
}

/// What the platform should have scheduled.
#[derive(Debug, Clone, PartialEq)]
pub struct ReminderPlan {
    pub slots: Vec<ReminderSlot>,
    pub language: settings::UiLanguage,
}

/// `HH:MM` → minutes after midnight.
pub fn parse_time(value: &str) -> Option<u32> {
    let (hours, minutes) = value.split_once(':')?;
    if hours.len() != 2 || minutes.len() != 2 {
        return None;
    }
    let (hours, minutes): (u32, u32) = (hours.parse().ok()?, minutes.parse().ok()?);
    (hours < 24 && minutes < 60).then_some(hours * 60 + minutes)
}

/// The reminders to schedule now; empty when the reminder is off or nothing is left to study.
pub fn plan(tx: &mut dyn Repositories, now: TimestampMs) -> AppResult<ReminderPlan> {
    let settings = settings::read(tx)?;
    let language = settings.doc.ui_language;
    let Some(minutes) = settings.doc.reminder_time.as_deref().and_then(parse_time) else {
        return Ok(ReminderPlan {
            slots: Vec::new(),
            language,
        });
    };
    let moment = study::study_moment(tx, now)?;
    let calendar = &moment.calendar;
    let cards = tx.card_facts(None)?;
    let today_start = calendar.day_start(moment.today);
    let reviewed_today = u32::try_from(
        tx.review_facts(None, today_start)?
            .iter()
            .filter(|fact| fact.reviewed_at < calendar.day_end(moment.today))
            .count(),
    )
    .unwrap_or(u32::MAX);
    let slots = slots(
        calendar,
        now,
        minutes,
        &cards,
        Progress {
            reviewed_today,
            daily_goal: settings.daily_goal,
        },
        &settings.doc.free_days(),
    );
    Ok(ReminderPlan { slots, language })
}

#[derive(Debug, Clone, Copy)]
pub struct Progress {
    pub reviewed_today: u32,
    pub daily_goal: u32,
}

/// Pure planning step (see the module rules).
pub fn slots(
    calendar: &StudyCalendar,
    now: TimestampMs,
    minutes: u32,
    cards: &[CardFact],
    progress: Progress,
    free: &FreeDays,
) -> Vec<ReminderSlot> {
    let today = calendar.day_of(now);
    let first_date = calendar.local_date(now);
    let new_available = cards
        .iter()
        .any(|card| card.state == CardState::New && card.queue == QueueState::New);
    let mut slots = Vec::new();
    for offset in 0..=REMINDER_DAYS {
        let at = calendar.local_instant(first_date + offset, minutes);
        if at.0 <= now.0 || slots.len() as i64 >= REMINDER_DAYS {
            continue;
        }
        let day = calendar.day_of(at);
        if free.is_free(day) {
            continue;
        }
        let reviews = cards
            .iter()
            .filter(|card| card.state != CardState::New)
            .filter(|card| match card.queue {
                QueueState::Suspended => false,
                // Buried cards return on the next study day.
                QueueState::SchedBuried | QueueState::UserBuried => day.0 > today.0,
                _ => true,
            })
            .filter(|card| {
                card.due_at
                    .is_some_and(|due| calendar.day_of(due).0 <= day.0)
            })
            .count();
        let reviews = u32::try_from(reviews).unwrap_or(u32::MAX);
        if reviews == 0 && !new_available {
            continue;
        }
        let done_today = day.0 == today.0
            && progress.daily_goal > 0
            && progress.reviewed_today >= progress.daily_goal
            && reviews == 0;
        if done_today {
            continue;
        }
        slots.push(ReminderSlot {
            at,
            reviews,
            new_available,
        });
    }
    slots
}

/// Notification text in the UI language: generic, with the forecast count when there is one.
pub fn text(language: settings::UiLanguage, slot: &ReminderSlot) -> (String, String) {
    use settings::UiLanguage;
    match language {
        UiLanguage::Pl => (
            "Czas na naukę".to_owned(),
            if slot.reviews > 0 {
                format!(
                    "Do powtórki czeka około {} {}.",
                    slot.reviews,
                    polish_cards(slot.reviews)
                )
            } else {
                "Czekają nowe słówka. Kilka minut wystarczy.".to_owned()
            },
        ),
        UiLanguage::En => (
            "Time to study".to_owned(),
            if slot.reviews > 0 {
                format!(
                    "About {} {} waiting for review.",
                    slot.reviews,
                    if slot.reviews == 1 {
                        "card is"
                    } else {
                        "cards are"
                    }
                )
            } else {
                "New words are waiting. A few minutes is enough.".to_owned()
            },
        ),
    }
}

fn polish_cards(count: u32) -> &'static str {
    let (tens, ones) = (count % 100, count % 10);
    if count == 1 {
        "karta"
    } else if (2..=4).contains(&ones) && !(12..=14).contains(&tens) {
        "karty"
    } else {
        "kart"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use langualinker_domain::{CardId, DeckId};

    // 2026-03-02 10:00 UTC = 11:00 in Warsaw.
    const NOW: i64 = 1_772_445_600_000;
    const DAY: i64 = 86_400_000;

    fn warsaw() -> StudyCalendar {
        StudyCalendar::new("Europe/Warsaw", 240).unwrap()
    }

    fn card(id: i64, state: CardState, queue: QueueState, due: Option<i64>) -> CardFact {
        CardFact {
            card_id: CardId(id),
            deck_id: DeckId(1),
            state,
            queue,
            due_at: due.map(TimestampMs),
            stability: None,
            difficulty: None,
            interval_days: 1,
        }
    }

    fn idle() -> Progress {
        Progress {
            reviewed_today: 0,
            daily_goal: 20,
        }
    }

    #[test]
    fn times_parse_on_a_24_hour_clock() {
        assert_eq!(parse_time("19:30"), Some(19 * 60 + 30));
        assert_eq!(parse_time("00:00"), Some(0));
        assert_eq!(parse_time("24:00"), None);
        assert_eq!(parse_time("7:30"), None);
        assert_eq!(parse_time("19:60"), None);
    }

    #[test]
    fn a_study_free_day_gets_no_reminder() {
        let cards = [card(1, CardState::New, QueueState::New, None)];
        let all = slots(
            &warsaw(),
            TimestampMs(NOW),
            19 * 60,
            &cards,
            idle(),
            &FreeDays::default(),
        );
        let first = warsaw().day_of(all[0].at);
        let free = FreeDays::new([FreeDays::weekday(first)], []);
        let planned = slots(&warsaw(), TimestampMs(NOW), 19 * 60, &cards, idle(), &free);
        assert!(planned.iter().all(|slot| warsaw().day_of(slot.at) != first));
        assert!(planned.len() < all.len());
    }

    #[test]
    fn nothing_to_study_means_no_reminder() {
        let cards = [card(1, CardState::Review, QueueState::Suspended, Some(NOW))];
        assert!(
            slots(
                &warsaw(),
                TimestampMs(NOW),
                19 * 60,
                &cards,
                idle(),
                &FreeDays::default()
            )
            .is_empty()
        );
        assert!(
            slots(
                &warsaw(),
                TimestampMs(NOW),
                19 * 60,
                &[],
                idle(),
                &FreeDays::default()
            )
            .is_empty()
        );
    }

    #[test]
    fn reminders_fall_on_days_with_due_reviews_at_the_local_time() {
        // One review card due in three days; no new cards.
        let due = NOW + 3 * DAY;
        let cards = [card(1, CardState::Review, QueueState::Review, Some(due))];
        let planned = slots(
            &warsaw(),
            TimestampMs(NOW),
            19 * 60,
            &cards,
            idle(),
            &FreeDays::default(),
        );
        // Days 3..=6 after today plus the 7th: every day from the due day on keeps the card due.
        assert_eq!(planned.len(), 5);
        // 19:00 in Warsaw (UTC+1 in March) = 18:00 UTC.
        assert_eq!(planned[0].at.0, NOW + 3 * DAY + 8 * 3_600_000);
        assert!(
            planned
                .iter()
                .all(|slot| slot.reviews == 1 && !slot.new_available)
        );
    }

    #[test]
    fn new_cards_keep_the_daily_reminder_and_a_past_time_starts_tomorrow() {
        let cards = [card(1, CardState::New, QueueState::New, None)];
        let planned = slots(
            &warsaw(),
            TimestampMs(NOW),
            8 * 60,
            &cards,
            idle(),
            &FreeDays::default(),
        );
        assert_eq!(planned.len() as i64, REMINDER_DAYS);
        // 08:00 already passed today (11:00): the first one is tomorrow 07:00 UTC.
        assert_eq!(planned[0].at.0, NOW + DAY - 3 * 3_600_000);
        assert!(
            planned
                .iter()
                .all(|slot| slot.new_available && slot.reviews == 0)
        );
    }

    #[test]
    fn todays_reminder_is_dropped_once_the_goal_is_met_and_nothing_is_due() {
        let cards = [card(1, CardState::New, QueueState::New, None)];
        let done = Progress {
            reviewed_today: 20,
            daily_goal: 20,
        };
        let planned = slots(
            &warsaw(),
            TimestampMs(NOW),
            19 * 60,
            &cards,
            done,
            &FreeDays::default(),
        );
        assert_eq!(planned[0].at.0, NOW + DAY + 8 * 3_600_000);
        // A review still due keeps today's reminder.
        let due = [
            cards[0],
            card(2, CardState::Review, QueueState::Review, Some(NOW - DAY)),
        ];
        let planned = slots(
            &warsaw(),
            TimestampMs(NOW),
            19 * 60,
            &due,
            done,
            &FreeDays::default(),
        );
        assert_eq!(planned[0].at.0, NOW + 8 * 3_600_000);
    }

    #[test]
    fn buried_cards_count_from_the_next_study_day() {
        let cards = [card(
            1,
            CardState::Review,
            QueueState::UserBuried,
            Some(NOW - DAY),
        )];
        let planned = slots(
            &warsaw(),
            TimestampMs(NOW),
            19 * 60,
            &cards,
            idle(),
            &FreeDays::default(),
        );
        assert_eq!(planned[0].at.0, NOW + DAY + 8 * 3_600_000);
    }

    #[test]
    fn a_reminder_inside_a_dst_gap_moves_after_it() {
        // 2026-03-28 12:00 UTC; 02:30 on 2026-03-29 does not exist in Warsaw → 03:00 CEST.
        let now = 1_774_699_200_000;
        let cards = [card(1, CardState::New, QueueState::New, None)];
        let planned = slots(
            &warsaw(),
            TimestampMs(now),
            150,
            &cards,
            idle(),
            &FreeDays::default(),
        );
        // 2026-03-29 01:00 UTC.
        assert_eq!(planned[0].at.0, 1_774_746_000_000);
    }

    #[test]
    fn texts_use_the_ui_language_and_polish_plurals() {
        let slot = |reviews| ReminderSlot {
            at: TimestampMs(NOW),
            reviews,
            new_available: true,
        };
        let pl = |n| text(settings::UiLanguage::Pl, &slot(n)).1;
        assert_eq!(pl(1), "Do powtórki czeka około 1 karta.");
        assert_eq!(pl(3), "Do powtórki czeka około 3 karty.");
        assert_eq!(pl(12), "Do powtórki czeka około 12 kart.");
        assert_eq!(pl(22), "Do powtórki czeka około 22 karty.");
        assert!(pl(0).starts_with("Czekają nowe"));
        assert_eq!(
            text(settings::UiLanguage::En, &slot(1)).1,
            "About 1 card is waiting for review."
        );
    }
}
