//! Opt-in usage metrics (plan Phase 16 step 1, ADR-13).
//!
//! Events are a closed set of typed variants whose fields are small numbers: there is no way to
//! put card text, a prompt, a key or a file name into one. They are recorded only while the
//! learner has opted in (the infrastructure store drops them otherwise) and stay on the device;
//! [`summarize`] turns them into the beta metrics and checks them against the plan's targets.
//! The targets are goals, not results: a value is judged only once enough samples exist.

use langualinker_domain::TimestampMs;

const DAY_MS: i64 = 86_400_000;

/// One recorded fact. Fields are counts, durations and enum codes only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricEvent {
    /// The first screen was shown; `cold_start_ms` is measured by the WebView from navigation
    /// start, so the native process start before it is not included.
    AppStarted {
        cold_start_ms: u32,
    },
    /// A study session ended (by the learner or because the queue ran out).
    SessionEnded {
        reviews: u32,
        duration_ms: u32,
    },
    /// A committed rating, 1 (Again) to 4 (Easy).
    Rated {
        rating: u8,
    },
    Undone,
    /// One association provider call finished; `ok` = an association was produced.
    Generated {
        latency_ms: u32,
        ok: bool,
    },
    /// A provider call is retried after a timeout or an overload.
    GenerationRetried,
    /// The learner marked an association as the favorite.
    Favorited,
    /// A restore from a backup finished.
    Restored {
        ok: bool,
    },
    /// The previous run ended in a Rust panic (read from the crash report on the next start).
    NativeCrash,
    /// An uncaught error or rejected promise in the WebView (no message is kept).
    ClientError,
}

/// Storage form of an event: a stable name and up to two integer fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedEvent {
    pub name: &'static str,
    pub a: Option<i64>,
    pub b: Option<i64>,
}

impl MetricEvent {
    pub fn encode(self) -> EncodedEvent {
        let (name, a, b) = match self {
            Self::AppStarted { cold_start_ms } => ("appStarted", Some(cold_start_ms.into()), None),
            Self::SessionEnded {
                reviews,
                duration_ms,
            } => (
                "sessionEnded",
                Some(reviews.into()),
                Some(duration_ms.into()),
            ),
            Self::Rated { rating } => ("rated", Some(rating.into()), None),
            Self::Undone => ("undone", None, None),
            Self::Generated { latency_ms, ok } => {
                ("generated", Some(latency_ms.into()), Some(i64::from(ok)))
            }
            Self::GenerationRetried => ("generationRetried", None, None),
            Self::Favorited => ("favorited", None, None),
            Self::Restored { ok } => ("restored", Some(i64::from(ok)), None),
            Self::NativeCrash => ("nativeCrash", None, None),
            Self::ClientError => ("clientError", None, None),
        };
        EncodedEvent { name, a, b }
    }

    /// Reads a stored event back; unknown names or out-of-range fields yield `None` (a row
    /// written by a newer build is skipped, never misread).
    pub fn decode(name: &str, a: Option<i64>, b: Option<i64>) -> Option<Self> {
        let small = |value: Option<i64>| value.and_then(|value| u32::try_from(value).ok());
        let flag = |value: Option<i64>| match value {
            Some(0) => Some(false),
            Some(1) => Some(true),
            _ => None,
        };
        Some(match name {
            "appStarted" => Self::AppStarted {
                cold_start_ms: small(a)?,
            },
            "sessionEnded" => Self::SessionEnded {
                reviews: small(a)?,
                duration_ms: small(b)?,
            },
            "rated" => {
                let rating = u8::try_from(a?).ok().filter(|r| (1..=4).contains(r))?;
                Self::Rated { rating }
            }
            "undone" => Self::Undone,
            "generated" => Self::Generated {
                latency_ms: small(a)?,
                ok: flag(b)?,
            },
            "generationRetried" => Self::GenerationRetried,
            "favorited" => Self::Favorited,
            "restored" => Self::Restored { ok: flag(a)? },
            "nativeCrash" => Self::NativeCrash,
            "clientError" => Self::ClientError,
            _ => return None,
        })
    }

    /// Activity that counts a day as "active" for the D7 cohort.
    fn is_activity(self) -> bool {
        matches!(
            self,
            Self::AppStarted { .. } | Self::Rated { .. } | Self::SessionEnded { .. }
        )
    }
}

/// An event with the time it was recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordedEvent {
    pub at: TimestampMs,
    pub event: MetricEvent,
}

/// How a target compares with the measured value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetStatus {
    Met,
    Missed,
    /// Fewer samples than the target needs to be judged.
    NotEnoughData,
    /// A cohort value: one device cannot judge it, the beta aggregate does.
    CohortOnly,
}

impl TargetStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Met => "met",
            Self::Missed => "missed",
            Self::NotEnoughData => "notEnoughData",
            Self::CohortOnly => "cohortOnly",
        }
    }
}

/// The plan's beta targets (Phase 16 step 3) and the samples each needs before it is judged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Metric {
    /// Share of the cohort active on day 7 after opting in; target ≥ 25 %.
    RetainedD7,
    /// Median cold start; target < 2 s, judged from 5 starts.
    ColdStartMedian,
    /// 90th percentile of provider latency; target < 4 s, judged from 10 calls.
    GenerationP90,
    /// Favorites per successful generation; target ≥ 20 %, judged from 10 generations.
    FavoriteRate,
    /// Starts without a native crash; target ≥ 99.5 %, judged from 200 starts.
    CrashFree,
}

impl Metric {
    pub const ALL: [Metric; 5] = [
        Self::RetainedD7,
        Self::ColdStartMedian,
        Self::GenerationP90,
        Self::FavoriteRate,
        Self::CrashFree,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::RetainedD7 => "retainedD7",
            Self::ColdStartMedian => "coldStartMedian",
            Self::GenerationP90 => "generationP90",
            Self::FavoriteRate => "favoriteRate",
            Self::CrashFree => "crashFree",
        }
    }

    /// Target as shown to the learner and in the export.
    pub fn target(self) -> &'static str {
        match self {
            Self::RetainedD7 => ">= 25%",
            Self::ColdStartMedian => "< 2000 ms",
            Self::GenerationP90 => "< 4000 ms",
            Self::FavoriteRate => ">= 20%",
            Self::CrashFree => ">= 99.5%",
        }
    }

    fn min_samples(self) -> usize {
        match self {
            Self::RetainedD7 => 1,
            Self::ColdStartMedian => 5,
            Self::GenerationP90 | Self::FavoriteRate => 10,
            // One crash in fewer than 200 starts already misses 99.5 %; fewer starts cannot
            // show the target was met.
            Self::CrashFree => 200,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TargetCheck {
    pub metric: Metric,
    /// The measured value (ms for latencies, a 0–1 ratio otherwise); `None` without samples.
    pub value: Option<f64>,
    pub samples: u32,
    pub status: TargetStatus,
}

/// Local metrics since the learner opted in.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MetricsSummary {
    /// Opt-in time; day 0 of the D7 cohort.
    pub since: Option<TimestampMs>,
    /// Whole days since opting in.
    pub days_observed: u32,
    pub active_days: u32,
    pub app_starts: u32,
    /// Active on day 7 after opting in; `None` until day 7 has passed.
    pub retained_d7: Option<bool>,
    pub cold_start_median_ms: Option<u32>,
    pub generations: u32,
    pub generation_failures: u32,
    pub generation_retries: u32,
    pub generation_p90_ms: Option<u32>,
    pub favorites: u32,
    pub favorite_rate: Option<f64>,
    pub native_crashes: u32,
    pub crash_free_rate: Option<f64>,
    pub client_errors: u32,
    pub sessions: u32,
    /// Ratings by grade: Again, Hard, Good, Easy.
    pub ratings: [u32; 4],
    pub undos: u32,
    pub restores: u32,
    pub restore_failures: u32,
    pub targets: Vec<TargetCheck>,
}

/// Nearest-rank percentile (`p` in 0–100) of a non-empty, sorted slice.
fn percentile(sorted: &[u32], p: usize) -> u32 {
    let rank = (p * sorted.len()).div_ceil(100).max(1);
    sorted[rank - 1]
}

fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// Computes the summary from the events recorded since `since` (the opt-in time).
pub fn summarize(events: &[RecordedEvent], since: TimestampMs, now: TimestampMs) -> MetricsSummary {
    let day_of = |at: TimestampMs| (at.0 - since.0).div_euclid(DAY_MS);
    let days_observed = day_of(now).max(0);
    let mut summary = MetricsSummary {
        since: Some(since),
        days_observed: u32::try_from(days_observed).unwrap_or(u32::MAX),
        ..MetricsSummary::default()
    };
    let mut active = std::collections::BTreeSet::new();
    let mut cold_starts = Vec::new();
    let mut latencies = Vec::new();
    for recorded in events.iter().filter(|e| e.at.0 >= since.0) {
        if recorded.event.is_activity() {
            active.insert(day_of(recorded.at));
        }
        match recorded.event {
            MetricEvent::AppStarted { cold_start_ms } => {
                summary.app_starts += 1;
                cold_starts.push(cold_start_ms);
            }
            MetricEvent::SessionEnded { .. } => summary.sessions += 1,
            MetricEvent::Rated { rating } => {
                summary.ratings[usize::from(rating.clamp(1, 4)) - 1] += 1;
            }
            MetricEvent::Undone => summary.undos += 1,
            MetricEvent::Generated { latency_ms, ok } => {
                summary.generations += 1;
                latencies.push(latency_ms);
                if !ok {
                    summary.generation_failures += 1;
                }
            }
            MetricEvent::GenerationRetried => summary.generation_retries += 1,
            MetricEvent::Favorited => summary.favorites += 1,
            MetricEvent::Restored { ok } => {
                summary.restores += 1;
                if !ok {
                    summary.restore_failures += 1;
                }
            }
            MetricEvent::NativeCrash => summary.native_crashes += 1,
            MetricEvent::ClientError => summary.client_errors += 1,
        }
    }
    summary.active_days = count(active.len());
    if days_observed > 7 {
        summary.retained_d7 = Some(active.contains(&7));
    }
    cold_starts.sort_unstable();
    latencies.sort_unstable();
    if !cold_starts.is_empty() {
        summary.cold_start_median_ms = Some(percentile(&cold_starts, 50));
    }
    if !latencies.is_empty() {
        summary.generation_p90_ms = Some(percentile(&latencies, 90));
    }
    let successful = summary.generations - summary.generation_failures;
    if successful > 0 {
        summary.favorite_rate =
            Some((f64::from(summary.favorites) / f64::from(successful)).min(1.0));
    }
    if summary.app_starts > 0 {
        let crashes = summary.native_crashes.min(summary.app_starts);
        summary.crash_free_rate = Some(1.0 - f64::from(crashes) / f64::from(summary.app_starts));
    }
    summary.targets = Metric::ALL
        .iter()
        .map(|metric| check(*metric, &summary, successful))
        .collect();
    summary
}

fn check(metric: Metric, summary: &MetricsSummary, successful: u32) -> TargetCheck {
    let (value, samples, met) = match metric {
        Metric::RetainedD7 => (
            summary.retained_d7.map(|kept| if kept { 1.0 } else { 0.0 }),
            u32::from(summary.retained_d7.is_some()),
            None,
        ),
        Metric::ColdStartMedian => (
            summary.cold_start_median_ms.map(f64::from),
            summary.app_starts,
            summary.cold_start_median_ms.map(|ms| ms < 2_000),
        ),
        Metric::GenerationP90 => (
            summary.generation_p90_ms.map(f64::from),
            summary.generations,
            summary.generation_p90_ms.map(|ms| ms < 4_000),
        ),
        Metric::FavoriteRate => (
            summary.favorite_rate,
            successful,
            summary.favorite_rate.map(|rate| rate >= 0.20),
        ),
        Metric::CrashFree => (
            summary.crash_free_rate,
            summary.app_starts,
            summary.crash_free_rate.map(|rate| rate >= 0.995),
        ),
    };
    let status = match (metric, met) {
        (Metric::RetainedD7, _) => TargetStatus::CohortOnly,
        // A crash rate already below the target cannot recover by waiting for more samples.
        (Metric::CrashFree, Some(false)) => TargetStatus::Missed,
        (_, _) if (samples as usize) < metric.min_samples() => TargetStatus::NotEnoughData,
        (_, Some(true)) => TargetStatus::Met,
        (_, Some(false)) => TargetStatus::Missed,
        (_, None) => TargetStatus::NotEnoughData,
    };
    TargetCheck {
        metric,
        value,
        samples,
        status,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SINCE: i64 = 1_772_438_400_000;

    fn at(day: i64, ms: i64) -> TimestampMs {
        TimestampMs(SINCE + day * DAY_MS + ms)
    }

    fn event(day: i64, event: MetricEvent) -> RecordedEvent {
        RecordedEvent {
            at: at(day, 1_000),
            event,
        }
    }

    fn target(summary: &MetricsSummary, metric: Metric) -> TargetCheck {
        *summary
            .targets
            .iter()
            .find(|check| check.metric == metric)
            .unwrap()
    }

    #[test]
    fn every_event_round_trips_through_its_storage_form() {
        let events = [
            MetricEvent::AppStarted { cold_start_ms: 812 },
            MetricEvent::SessionEnded {
                reviews: 20,
                duration_ms: 300_000,
            },
            MetricEvent::Rated { rating: 1 },
            MetricEvent::Rated { rating: 4 },
            MetricEvent::Undone,
            MetricEvent::Generated {
                latency_ms: 2_400,
                ok: true,
            },
            MetricEvent::Generated {
                latency_ms: 30_000,
                ok: false,
            },
            MetricEvent::GenerationRetried,
            MetricEvent::Favorited,
            MetricEvent::Restored { ok: true },
            MetricEvent::Restored { ok: false },
            MetricEvent::NativeCrash,
            MetricEvent::ClientError,
        ];
        for event in events {
            let encoded = event.encode();
            assert_eq!(
                MetricEvent::decode(encoded.name, encoded.a, encoded.b),
                Some(event),
                "{encoded:?}"
            );
        }
    }

    #[test]
    fn unknown_or_out_of_range_rows_are_skipped() {
        assert_eq!(MetricEvent::decode("cardText", Some(1), None), None);
        assert_eq!(MetricEvent::decode("rated", Some(5), None), None);
        assert_eq!(MetricEvent::decode("rated", None, None), None);
        assert_eq!(MetricEvent::decode("generated", Some(10), Some(2)), None);
        assert_eq!(MetricEvent::decode("appStarted", Some(-1), None), None);
    }

    #[test]
    fn empty_log_judges_nothing() {
        let summary = summarize(&[], TimestampMs(SINCE), at(0, 5));
        assert_eq!(summary.days_observed, 0);
        assert_eq!(summary.retained_d7, None);
        assert_eq!(summary.cold_start_median_ms, None);
        for check in &summary.targets {
            let expected = if check.metric == Metric::RetainedD7 {
                TargetStatus::CohortOnly
            } else {
                TargetStatus::NotEnoughData
            };
            assert_eq!(check.status, expected, "{check:?}");
        }
    }

    #[test]
    fn d7_needs_activity_on_day_seven_and_is_known_only_after_it() {
        let events = [
            event(0, MetricEvent::AppStarted { cold_start_ms: 900 }),
            event(7, MetricEvent::Rated { rating: 3 }),
        ];
        // Day 7 is still running: not decided yet.
        let during = summarize(&events, TimestampMs(SINCE), at(7, 5_000));
        assert_eq!(during.retained_d7, None);
        let after = summarize(&events, TimestampMs(SINCE), at(8, 0));
        assert_eq!(after.retained_d7, Some(true));
        assert_eq!(after.active_days, 2);
        // Active on days 6 and 8 but not 7: not retained on day 7.
        let gap = [
            event(6, MetricEvent::AppStarted { cold_start_ms: 900 }),
            event(8, MetricEvent::AppStarted { cold_start_ms: 900 }),
            // Favorites and errors are not activity.
            event(7, MetricEvent::ClientError),
        ];
        assert_eq!(
            summarize(&gap, TimestampMs(SINCE), at(9, 0)).retained_d7,
            Some(false)
        );
    }

    #[test]
    fn events_before_the_opt_in_are_ignored() {
        let events = [RecordedEvent {
            at: TimestampMs(SINCE - 1),
            event: MetricEvent::NativeCrash,
        }];
        assert_eq!(
            summarize(&events, TimestampMs(SINCE), at(1, 0)).native_crashes,
            0
        );
    }

    #[test]
    fn cold_start_median_and_generation_p90_use_nearest_rank() {
        let mut events: Vec<RecordedEvent> = [1_500, 900, 2_500, 1_100, 1_300]
            .into_iter()
            .map(|ms| event(0, MetricEvent::AppStarted { cold_start_ms: ms }))
            .collect();
        for latency in (1..=10).map(|n| n * 500) {
            events.push(event(
                0,
                MetricEvent::Generated {
                    latency_ms: latency,
                    ok: true,
                },
            ));
        }
        let summary = summarize(&events, TimestampMs(SINCE), at(1, 0));
        assert_eq!(summary.cold_start_median_ms, Some(1_300));
        assert_eq!(
            target(&summary, Metric::ColdStartMedian).status,
            TargetStatus::Met
        );
        // 10 samples 500..5000: the 9th (4500 ms) is p90 and misses < 4 s.
        assert_eq!(summary.generation_p90_ms, Some(4_500));
        assert_eq!(
            target(&summary, Metric::GenerationP90).status,
            TargetStatus::Missed
        );
    }

    #[test]
    fn favorite_rate_counts_only_successful_generations() {
        let mut events = vec![event(0, MetricEvent::Favorited); 3];
        for ok in [true; 10].into_iter().chain([false; 5]) {
            events.push(event(
                0,
                MetricEvent::Generated {
                    latency_ms: 1_000,
                    ok,
                },
            ));
        }
        let summary = summarize(&events, TimestampMs(SINCE), at(0, 9_000));
        assert_eq!(summary.generations, 15);
        assert_eq!(summary.generation_failures, 5);
        assert_eq!(summary.favorite_rate, Some(0.3));
        let check = target(&summary, Metric::FavoriteRate);
        assert_eq!((check.samples, check.status), (10, TargetStatus::Met));
    }

    #[test]
    fn one_crash_in_few_starts_already_misses_crash_free() {
        let mut events = vec![
            event(
                0,
                MetricEvent::AppStarted {
                    cold_start_ms: 1_000
                }
            );
            10
        ];
        assert_eq!(
            target(
                &summarize(&events, TimestampMs(SINCE), at(1, 0)),
                Metric::CrashFree
            )
            .status,
            TargetStatus::NotEnoughData
        );
        events.push(event(0, MetricEvent::NativeCrash));
        let summary = summarize(&events, TimestampMs(SINCE), at(1, 0));
        assert_eq!(summary.crash_free_rate, Some(0.9));
        assert_eq!(
            target(&summary, Metric::CrashFree).status,
            TargetStatus::Missed
        );
    }

    #[test]
    fn undo_retry_and_restore_are_counted() {
        let events = [
            event(0, MetricEvent::Rated { rating: 1 }),
            event(0, MetricEvent::Rated { rating: 3 }),
            event(0, MetricEvent::Rated { rating: 3 }),
            event(0, MetricEvent::Undone),
            event(0, MetricEvent::GenerationRetried),
            event(0, MetricEvent::Restored { ok: true }),
            event(0, MetricEvent::Restored { ok: false }),
            event(
                0,
                MetricEvent::SessionEnded {
                    reviews: 3,
                    duration_ms: 60_000,
                },
            ),
        ];
        let summary = summarize(&events, TimestampMs(SINCE), at(0, 9_000));
        assert_eq!(summary.ratings, [1, 0, 2, 0]);
        assert_eq!(summary.undos, 1);
        assert_eq!(summary.generation_retries, 1);
        assert_eq!((summary.restores, summary.restore_failures), (2, 1));
        assert_eq!(summary.sessions, 1);
    }
}
