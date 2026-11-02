//! Production implementations of the clock and randomness ports.

use std::time::{SystemTime, UNIX_EPOCH};

use langualinker_application::ports::{Clock, Rng};
use langualinker_domain::TimestampMs;

/// Wall clock in UTC epoch milliseconds.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> TimestampMs {
        let ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis() as i64)
            .unwrap_or_default();
        TimestampMs(ms)
    }
}

/// Operating-system randomness (GUIDs, fuzz).
#[derive(Debug, Default, Clone, Copy)]
pub struct OsRng;

impl Rng for OsRng {
    fn fill(&self, bytes: &mut [u8]) {
        getrandom::fill(bytes).expect("operating system randomness is unavailable");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guids_are_distinct() {
        let rng = OsRng;
        assert_ne!(rng.guid(), rng.guid());
    }

    #[test]
    fn clock_is_after_2026() {
        assert!(SystemClock.now().0 > 1_767_225_600_000);
    }
}
