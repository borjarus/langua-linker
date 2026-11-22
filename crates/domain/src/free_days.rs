//! Study-free days: weekdays that repeat every week (every Sunday) and single dates.
//!
//! A free day is a promise to the learner, not a lock: studying on it is still allowed. The app
//! keeps reviews off it ([`FreeDays::shift_interval`]), sends no reminder on it and does not let it
//! break the streak.

use std::collections::BTreeSet;

use crate::calendar::StudyDay;

/// Largest number of days a review is moved away from a free day, in each direction.
pub const MAX_SHIFT_DAYS: u32 = 3;

/// Weekdays are ISO numbers, 1 = Monday … 7 = Sunday; dates are study days.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FreeDays {
    /// Bit `n - 1` set = ISO weekday `n` is free.
    weekdays: u8,
    dates: BTreeSet<StudyDay>,
}

impl FreeDays {
    /// `weekdays` outside 1–7 are ignored.
    pub fn new(
        weekdays: impl IntoIterator<Item = u8>,
        dates: impl IntoIterator<Item = StudyDay>,
    ) -> Self {
        let weekdays = weekdays
            .into_iter()
            .filter(|day| (1..=7).contains(day))
            .fold(0u8, |mask, day| mask | (1 << (day - 1)));
        Self {
            weekdays,
            dates: dates.into_iter().collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.weekdays == 0 && self.dates.is_empty()
    }

    /// Stable text of the free days, for tokens that must change when they change.
    pub fn fingerprint(&self) -> String {
        let dates: Vec<String> = self.dates.iter().map(|day| day.0.to_string()).collect();
        format!("{}:{}", self.weekdays, dates.join(","))
    }

    /// ISO weekday (1 = Monday) of a study day; 1970-01-01 was a Thursday.
    pub fn weekday(day: StudyDay) -> u8 {
        (day.0 + 3).rem_euclid(7) as u8 + 1
    }

    pub fn is_free(&self, day: StudyDay) -> bool {
        self.weekdays & (1 << (Self::weekday(day) - 1)) != 0 || self.dates.contains(&day)
    }

    /// Every weekday is free: nothing can be moved, so free days are not applied to scheduling.
    fn whole_week(&self) -> bool {
        self.weekdays == 0b0111_1111
    }

    /// A review interval of `days` from `today` moved off a free day: the nearest day that is not
    /// free, at most [`MAX_SHIFT_DAYS`] away, earlier first (an earlier review costs a little
    /// workload, a later one costs recall). Never today and never above `maximum_days`. When no
    /// nearby day is free of study, the interval stays.
    pub fn shift_interval(&self, today: StudyDay, days: u32, maximum_days: u32) -> u32 {
        if days == 0 || self.is_empty() || self.whole_week() {
            return days;
        }
        if !self.is_free(today.plus(i64::from(days))) {
            return days;
        }
        for offset in 1..=MAX_SHIFT_DAYS {
            if let Some(earlier) = days.checked_sub(offset)
                && earlier >= 1
                && !self.is_free(today.plus(i64::from(earlier)))
            {
                return earlier;
            }
            let later = days + offset;
            if later <= maximum_days.max(days) && !self.is_free(today.plus(i64::from(later))) {
                return later;
            }
        }
        days
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 2026-11-22 is a Sunday.
    fn date(text: &str) -> StudyDay {
        StudyDay::from_iso_date(text).unwrap()
    }

    #[test]
    fn iso_dates_round_trip() {
        assert_eq!(date("2026-11-22").iso_date(), "2026-11-22");
        assert_eq!(StudyDay::from_iso_date("2026-2-1"), None);
        assert_eq!(StudyDay::from_iso_date("2026-02-30"), None);
    }

    #[test]
    fn weekdays_follow_iso_numbering() {
        assert_eq!(FreeDays::weekday(StudyDay(0)), 4); // Thursday
        assert_eq!(FreeDays::weekday(date("2026-11-22")), 7);
        assert_eq!(FreeDays::weekday(date("2026-11-23")), 1);
    }

    #[test]
    fn a_weekday_repeats_every_week_and_a_date_once() {
        let free = FreeDays::new([7], [date("2026-12-24")]);
        assert!(free.is_free(date("2026-11-22")));
        assert!(free.is_free(date("2026-11-29")));
        assert!(!free.is_free(date("2026-11-23")));
        assert!(free.is_free(date("2026-12-24")));
        assert!(!free.is_free(date("2027-12-24")));
    }

    #[test]
    fn out_of_range_weekdays_are_ignored() {
        assert!(FreeDays::new([0, 8], []).is_empty());
    }

    #[test]
    fn a_review_on_a_free_day_moves_to_the_nearest_day_earlier_first() {
        let friday = date("2026-11-20");
        let sunday_only = FreeDays::new([7], []);
        // Friday + 2 = Sunday → Saturday (1 day earlier).
        assert_eq!(sunday_only.shift_interval(friday, 2, 36_500), 1);
        // Friday + 9 = Sunday → Saturday.
        assert_eq!(sunday_only.shift_interval(friday, 9, 36_500), 8);
        // Not a free day: unchanged.
        assert_eq!(sunday_only.shift_interval(friday, 3, 36_500), 3);
    }

    #[test]
    fn a_review_never_moves_to_today_and_goes_later_instead() {
        let saturday = date("2026-11-21");
        let sunday_only = FreeDays::new([7], []);
        // Saturday + 1 = Sunday; 0 days would be today → Monday.
        assert_eq!(sunday_only.shift_interval(saturday, 1, 36_500), 2);
    }

    #[test]
    fn a_weekend_moves_a_saturday_review_to_friday() {
        let thursday = date("2026-11-19");
        let weekend = FreeDays::new([6, 7], []);
        assert_eq!(weekend.shift_interval(thursday, 2, 36_500), 1);
        // Sunday: Saturday is free too, Monday is one day later.
        assert_eq!(weekend.shift_interval(thursday, 3, 36_500), 4);
    }

    #[test]
    fn the_maximum_interval_is_kept() {
        let saturday = date("2026-11-21");
        let sunday_only = FreeDays::new([7], []);
        assert_eq!(sunday_only.shift_interval(saturday, 1, 1), 1);
    }

    #[test]
    fn a_whole_free_week_changes_nothing() {
        let free = FreeDays::new(1..=7, []);
        assert_eq!(free.shift_interval(date("2026-11-20"), 2, 36_500), 2);
    }
}
