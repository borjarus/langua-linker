//! Study days (plan Phase 9 step 11, ADR-15).
//!
//! A study day starts at the collection's cutoff (minutes after local midnight, 04:00 by
//! default) in the collection's IANA time zone. Days are calendar days, not 24-hour blocks: across
//! a DST change a study day lasts 23 or 25 hours, and "due in 3 days" means the start of the third
//! study day from today, whatever the wall clock did in between. All instants stay UTC epoch
//! milliseconds; only day arithmetic uses the zone.

use chrono::{DateTime, Duration, LocalResult, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;

use crate::TimestampMs;
use crate::validation::{ValidationError, ValidationResult};

/// Day number of a study day: days since 1970-01-01 of its local start date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StudyDay(pub i64);

impl StudyDay {
    pub fn plus(self, days: i64) -> Self {
        Self(self.0 + days)
    }

    /// `YYYY-MM-DD` of the local date the study day starts on (contract `StudyDay`).
    pub fn iso_date(self) -> String {
        epoch_date()
            .checked_add_signed(Duration::days(self.0))
            .map_or_else(String::new, |date| date.format("%Y-%m-%d").to_string())
    }
}

fn epoch_date() -> NaiveDate {
    NaiveDate::from_ymd_opt(1970, 1, 1).expect("valid epoch date")
}

/// Time zone and cutoff of the collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StudyCalendar {
    zone: Tz,
    cutoff_minutes: u32,
}

impl StudyCalendar {
    /// `zone` is an IANA name such as `Europe/Warsaw`; `cutoff_minutes` is 0–1439.
    pub fn new(zone: &str, cutoff_minutes: u32) -> ValidationResult<Self> {
        let zone: Tz = zone
            .parse()
            .map_err(|_| ValidationError::new("timeZone", "unknown"))?;
        if cutoff_minutes >= 24 * 60 {
            return Err(ValidationError::new("dayCutoffMinutes", "outOfRange"));
        }
        Ok(Self {
            zone,
            cutoff_minutes,
        })
    }

    pub fn utc(cutoff_minutes: u32) -> Self {
        Self {
            zone: Tz::UTC,
            cutoff_minutes: cutoff_minutes.min(24 * 60 - 1),
        }
    }

    pub fn zone_name(&self) -> &'static str {
        self.zone.name()
    }

    pub fn cutoff_minutes(&self) -> u32 {
        self.cutoff_minutes
    }

    fn local(&self, at: TimestampMs) -> NaiveDateTime {
        let utc = DateTime::<Utc>::from_timestamp_millis(at.0).unwrap_or_default();
        utc.with_timezone(&self.zone).naive_local()
    }

    /// The study day an instant belongs to.
    pub fn day_of(&self, at: TimestampMs) -> StudyDay {
        let shifted = self.local(at) - Duration::minutes(i64::from(self.cutoff_minutes));
        StudyDay((shifted.date() - epoch_date()).num_days())
    }

    /// First instant of a study day: local `cutoff` on its date. When DST skips that local time,
    /// the day starts at the first existing minute after it; when the time occurs twice, at the
    /// earlier one.
    pub fn day_start(&self, day: StudyDay) -> TimestampMs {
        let date = epoch_date() + Duration::days(day.0);
        let mut local = date.and_hms_opt(0, 0, 0).expect("midnight exists")
            + Duration::minutes(i64::from(self.cutoff_minutes));
        // Gaps are at most a few hours; step forward minute by minute until the time exists.
        for _ in 0..(24 * 60) {
            match self.zone.from_local_datetime(&local) {
                LocalResult::Single(at) => return TimestampMs(at.timestamp_millis()),
                LocalResult::Ambiguous(earlier, _) => {
                    return TimestampMs(earlier.timestamp_millis());
                }
                LocalResult::None => local += Duration::minutes(1),
            }
        }
        TimestampMs(local.and_utc().timestamp_millis())
    }

    /// First instant of the following study day.
    pub fn day_end(&self, day: StudyDay) -> TimestampMs {
        self.day_start(day.plus(1))
    }

    /// Whole study days from `from` to `to`; negative when the clock went backwards.
    pub fn days_between(&self, from: TimestampMs, to: TimestampMs) -> i64 {
        self.day_of(to).0 - self.day_of(from).0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: i64 = 3_600_000;

    fn utc_ms(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> TimestampMs {
        TimestampMs(
            Utc.with_ymd_and_hms(year, month, day, hour, minute, 0)
                .unwrap()
                .timestamp_millis(),
        )
    }

    #[test]
    fn day_starts_at_the_local_cutoff() {
        let warsaw = StudyCalendar::new("Europe/Warsaw", 240).unwrap();
        // 2026-03-02 02:30 UTC is 03:30 in Warsaw: still the day that began on March 1st.
        let night = utc_ms(2026, 3, 2, 2, 30);
        let day = warsaw.day_of(night);
        assert_eq!(day.iso_date(), "2026-03-01");
        assert_eq!(warsaw.day_start(day), utc_ms(2026, 3, 1, 3, 0));
        assert_eq!(warsaw.day_end(day), utc_ms(2026, 3, 2, 3, 0));
        assert_eq!(warsaw.day_of(utc_ms(2026, 3, 2, 3, 0)), day.plus(1));
    }

    #[test]
    fn study_days_across_dst_are_23_and_25_hours_long() {
        let warsaw = StudyCalendar::new("Europe/Warsaw", 240).unwrap();
        // Spring forward on 2026-03-29, fall back on 2026-10-25 (at 02:00/03:00 local).
        let spring = warsaw.day_of(utc_ms(2026, 3, 28, 12, 0));
        let length = warsaw.day_end(spring).0 - warsaw.day_start(spring).0;
        assert_eq!(length, 23 * HOUR);
        let autumn = warsaw.day_of(utc_ms(2026, 10, 24, 12, 0));
        let length = warsaw.day_end(autumn).0 - warsaw.day_start(autumn).0;
        assert_eq!(length, 25 * HOUR);
    }

    #[test]
    fn a_cutoff_inside_a_dst_gap_starts_after_the_gap() {
        // 02:30 does not exist in Warsaw on 2026-03-29; the day starts at 03:00 CEST (01:00 UTC).
        let calendar = StudyCalendar::new("Europe/Warsaw", 150).unwrap();
        let day = calendar.day_of(utc_ms(2026, 3, 29, 12, 0));
        assert_eq!(day.iso_date(), "2026-03-29");
        assert_eq!(calendar.day_start(day), utc_ms(2026, 3, 29, 1, 0));
        // An ambiguous cutoff (02:30 twice on 2026-10-25) starts at the first occurrence (CEST).
        let day = calendar.day_of(utc_ms(2026, 10, 25, 12, 0));
        assert_eq!(calendar.day_start(day), utc_ms(2026, 10, 25, 0, 30));
    }

    #[test]
    fn day_of_and_day_start_agree_for_every_hour_of_a_year() {
        for zone in [
            "Europe/Warsaw",
            "America/New_York",
            "Australia/Lord_Howe",
            "UTC",
        ] {
            let calendar = StudyCalendar::new(zone, 240).unwrap();
            let mut at = utc_ms(2026, 1, 1, 0, 0).0;
            while at < utc_ms(2027, 1, 1, 0, 0).0 {
                let day = calendar.day_of(TimestampMs(at));
                assert!(calendar.day_start(day).0 <= at, "{zone} {at}");
                assert!(at < calendar.day_end(day).0, "{zone} {at}");
                at += HOUR;
            }
        }
    }

    #[test]
    fn travelling_east_does_not_move_instants_only_day_labels() {
        let warsaw = StudyCalendar::new("Europe/Warsaw", 240).unwrap();
        let tokyo = StudyCalendar::new("Asia/Tokyo", 240).unwrap();
        // 22:00 UTC: 23:00 in Warsaw (same day), 07:00 next morning in Tokyo (next day).
        let at = utc_ms(2026, 6, 10, 22, 0);
        assert_eq!(tokyo.day_of(at), warsaw.day_of(at).plus(1));
    }

    #[test]
    fn days_between_counts_calendar_days_and_may_be_negative() {
        let utc = StudyCalendar::utc(240);
        let evening = utc_ms(2026, 5, 1, 23, 0);
        let next_morning = utc_ms(2026, 5, 2, 5, 0);
        assert_eq!(utc.days_between(evening, next_morning), 1);
        assert_eq!(utc.days_between(next_morning, evening), -1);
        // 03:59 belongs to the previous study day with a 04:00 cutoff.
        assert_eq!(utc.days_between(evening, utc_ms(2026, 5, 2, 3, 59)), 0);
    }

    #[test]
    fn invalid_zones_and_cutoffs_are_rejected() {
        assert_eq!(
            StudyCalendar::new("Mars/Olympus", 0).unwrap_err().field,
            "timeZone"
        );
        assert_eq!(
            StudyCalendar::new("UTC", 1440).unwrap_err().field,
            "dayCutoffMinutes"
        );
        assert_eq!(StudyCalendar::utc(240).zone_name(), "UTC");
    }
}
