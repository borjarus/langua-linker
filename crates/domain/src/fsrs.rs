//! FSRS-6 memory model and per-card state machine (plan Phase 9 steps 1–8, ADR-15).
//!
//! Own port of the pinned reference implementation py-fsrs 6.3.0 (MIT): same formulas, same
//! operation order, f64 throughout, Python's round-half-to-even for intervals and the same
//! learning-step rules. Conformance is checked against vectors generated from the reference
//! (`tests/fixtures/fsrs/vectors-py-fsrs-6.3.0.json`).
//!
//! This module knows nothing about wall-clock time or study days: callers pass the number of
//! elapsed days, so the study-day policy (time zone, cutoff, DST) lives in [`crate::calendar`]
//! and the conformance harness can feed the reference's 24-hour arithmetic.

use std::f64::consts::E;

use sha2::{Digest, Sha256};

use crate::review::Rating;
use crate::validation::{ValidationError, ValidationResult};

/// Algorithm and reference recorded in every review log (`review_logs.fsrs_version`).
pub const FSRS_VERSION: &str = "fsrs-6/py-fsrs-6.3.0";

/// Number of FSRS-6 weights w0–w20.
pub const WEIGHT_COUNT: usize = 21;

/// FSRS-6 default weights; identical to py-fsrs 6.3.0 `DEFAULT_PARAMETERS` and to the KMP
/// specification (`tests/fixtures/fsrs/kmp-fsrs-spec.json`).
pub const DEFAULT_WEIGHTS: [f64; WEIGHT_COUNT] = [
    0.2120, 1.2931, 2.3065, 8.2956, 6.4133, 0.8334, 3.0194, 0.0010, 1.8722, 0.1666, 0.7960, 1.4835,
    0.0614, 0.2629, 1.6483, 0.6014, 1.8729, 0.5425, 0.0912, 0.0658, 0.1542,
];

/// Lowest stability the model produces (reference `STABILITY_MIN`; the KMP port used 0.1).
pub const STABILITY_MIN: f64 = 0.001;
pub const DIFFICULTY_MIN: f64 = 1.0;
pub const DIFFICULTY_MAX: f64 = 10.0;

/// Inclusive weight bounds of the reference (`LOWER_BOUNDS_PARAMETERS`).
pub const WEIGHT_LOWER_BOUNDS: [f64; WEIGHT_COUNT] = [
    STABILITY_MIN,
    STABILITY_MIN,
    STABILITY_MIN,
    STABILITY_MIN,
    1.0,
    0.001,
    0.001,
    0.001,
    0.0,
    0.0,
    0.001,
    0.001,
    0.001,
    0.001,
    0.0,
    0.0,
    1.0,
    0.0,
    0.0,
    0.0,
    0.1,
];

/// Inclusive weight bounds of the reference (`UPPER_BOUNDS_PARAMETERS`).
pub const WEIGHT_UPPER_BOUNDS: [f64; WEIGHT_COUNT] = [
    100.0, 100.0, 100.0, 100.0, 10.0, 4.0, 4.0, 0.75, 4.5, 0.8, 3.5, 5.0, 0.25, 0.9, 4.0, 1.0, 6.0,
    2.0, 2.0, 0.8, 0.8,
];

/// Desired retention accepted by presets (contract v1 `savePreset`, KMP specification).
pub const RETENTION_RANGE: std::ops::RangeInclusive<f64> = 0.70..=0.99;
pub const MAX_INTERVAL_DAYS: u32 = 36_500;

/// Checks a weight vector against the count, finiteness and the reference bounds.
pub fn validate_weights(weights: &[f64]) -> ValidationResult {
    if weights.len() != WEIGHT_COUNT {
        return Err(ValidationError::new("weights", "count"));
    }
    if weights.iter().any(|weight| !weight.is_finite()) {
        return Err(ValidationError::new("weights", "notFinite"));
    }
    let in_bounds = weights
        .iter()
        .zip(WEIGHT_LOWER_BOUNDS.iter().zip(WEIGHT_UPPER_BOUNDS.iter()))
        .all(|(weight, (lower, upper))| (*lower..=*upper).contains(weight));
    if !in_bounds {
        return Err(ValidationError::new("weights", "outOfRange"));
    }
    Ok(())
}

/// Validated FSRS-6 parameters with the derived decay and factor.
#[derive(Debug, Clone, PartialEq)]
pub struct FsrsParameters {
    weights: [f64; WEIGHT_COUNT],
    desired_retention: f64,
    maximum_interval_days: u32,
    decay: f64,
    factor: f64,
}

impl FsrsParameters {
    pub fn new(
        weights: &[f64],
        desired_retention: f64,
        maximum_interval_days: u32,
    ) -> ValidationResult<Self> {
        validate_weights(weights)?;
        if !RETENTION_RANGE.contains(&desired_retention) {
            return Err(ValidationError::new("desiredRetention", "outOfRange"));
        }
        if !(1..=MAX_INTERVAL_DAYS).contains(&maximum_interval_days) {
            return Err(ValidationError::new("maxIntervalDays", "outOfRange"));
        }
        let mut array = [0.0; WEIGHT_COUNT];
        array.copy_from_slice(weights);
        let decay = -array[20];
        Ok(Self {
            weights: array,
            desired_retention,
            maximum_interval_days,
            decay,
            factor: 0.9_f64.powf(1.0 / decay) - 1.0,
        })
    }

    pub fn weights(&self) -> &[f64; WEIGHT_COUNT] {
        &self.weights
    }

    pub fn desired_retention(&self) -> f64 {
        self.desired_retention
    }

    pub fn maximum_interval_days(&self) -> u32 {
        self.maximum_interval_days
    }

    /// Probability of recall after `elapsed_days` at `stability`; 0 for a non-positive stability.
    pub fn retrievability(&self, elapsed_days: f64, stability: f64) -> f64 {
        if stability <= 0.0 {
            return 0.0;
        }
        (1.0 + self.factor * elapsed_days.max(0.0) / stability).powf(self.decay)
    }

    pub fn initial_stability(&self, rating: Rating) -> f64 {
        self.weights[grade(rating) - 1].max(STABILITY_MIN)
    }

    fn raw_initial_difficulty(&self, rating: Rating) -> f64 {
        let w = &self.weights;
        w[4] - E.powf(w[5] * (grade(rating) as f64 - 1.0)) + 1.0
    }

    pub fn initial_difficulty(&self, rating: Rating) -> f64 {
        clamp_difficulty(self.raw_initial_difficulty(rating))
    }

    /// Difficulty after a rating: linear damping towards 10, then mean reversion to D0(Easy).
    pub fn next_difficulty(&self, difficulty: f64, rating: Rating) -> f64 {
        let w = &self.weights;
        let target = self.raw_initial_difficulty(Rating::Easy);
        let delta = -(w[6] * (grade(rating) as f64 - 3.0));
        let damped = difficulty + (10.0 - difficulty) * delta / 9.0;
        clamp_difficulty(w[7] * target + (1.0 - w[7]) * damped)
    }

    /// Same-day review (w17–w19): Good and Easy never lower the stability.
    pub fn short_term_stability(&self, stability: f64, rating: Rating) -> f64 {
        let w = &self.weights;
        let mut increase =
            E.powf(w[17] * (grade(rating) as f64 - 3.0 + w[18])) * stability.powf(-w[19]);
        if matches!(rating, Rating::Good | Rating::Easy) {
            increase = increase.max(1.0);
        }
        (stability * increase).max(STABILITY_MIN)
    }

    pub fn recall_stability(
        &self,
        difficulty: f64,
        stability: f64,
        retrievability: f64,
        rating: Rating,
    ) -> f64 {
        let w = &self.weights;
        let hard_penalty = if rating == Rating::Hard { w[15] } else { 1.0 };
        let easy_bonus = if rating == Rating::Easy { w[16] } else { 1.0 };
        stability
            * (1.0
                + E.powf(w[8])
                    * (11.0 - difficulty)
                    * stability.powf(-w[9])
                    * (E.powf((1.0 - retrievability) * w[10]) - 1.0)
                    * hard_penalty
                    * easy_bonus)
    }

    pub fn forget_stability(&self, difficulty: f64, stability: f64, retrievability: f64) -> f64 {
        let w = &self.weights;
        let long_term = w[11]
            * difficulty.powf(-w[12])
            * ((stability + 1.0).powf(w[13]) - 1.0)
            * E.powf((1.0 - retrievability) * w[14]);
        let short_term = stability / E.powf(w[17] * w[18]);
        long_term.min(short_term)
    }

    /// Stability after a review on another day.
    pub fn next_stability(
        &self,
        difficulty: f64,
        stability: f64,
        retrievability: f64,
        rating: Rating,
    ) -> f64 {
        let next = match rating {
            Rating::Again => self.forget_stability(difficulty, stability, retrievability),
            _ => self.recall_stability(difficulty, stability, retrievability, rating),
        };
        next.max(STABILITY_MIN)
    }

    /// Whole days until recall drops to the desired retention, within 1..=maximum interval.
    pub fn next_interval_days(&self, stability: f64) -> u32 {
        let raw = (stability / self.factor) * (self.desired_retention.powf(1.0 / self.decay) - 1.0);
        let rounded = round_half_even(raw);
        let capped = rounded.max(1.0).min(f64::from(self.maximum_interval_days));
        // `capped` is a whole number within 1..=36 500 (or NaN-free by construction).
        capped as u32
    }

    /// Inclusive range the fuzz picks from for an interval of `interval_days` (≥ 3).
    pub fn fuzz_range(&self, interval_days: u32) -> (u32, u32) {
        let days = f64::from(interval_days);
        let mut delta = 1.0;
        for (start, end, factor) in FUZZ_RANGES {
            delta += factor * (days.min(end) - start).max(0.0);
        }
        let low = round_half_even(days - delta) as i64;
        let high = round_half_even(days + delta) as i64;
        let high = high.min(i64::from(self.maximum_interval_days));
        let low = low.max(2).min(high);
        (low as u32, high as u32)
    }

    /// Reference fuzz with the uniform sample `uniform` ∈ [0, 1) instead of `random()`.
    /// Intervals under 2.5 days are left alone. Like the reference, a sample close to 1 may land
    /// one day above the range; only the maximum interval is enforced afterwards.
    pub fn fuzzed_interval_days(&self, interval_days: u32, uniform: f64) -> u32 {
        if f64::from(interval_days) < 2.5 {
            return interval_days;
        }
        let (low, high) = self.fuzz_range(interval_days);
        let sample = uniform * f64::from(high - low + 1) + f64::from(low);
        let fuzzed = round_half_even(sample).min(f64::from(self.maximum_interval_days));
        fuzzed as u32
    }
}

/// (start, end, factor) of the reference `FUZZ_RANGES`.
const FUZZ_RANGES: [(f64, f64, f64); 3] = [
    (2.5, 7.0, 0.15),
    (7.0, 20.0, 0.1),
    (20.0, f64::INFINITY, 0.05),
];

fn grade(rating: Rating) -> usize {
    match rating {
        Rating::Again => 1,
        Rating::Hard => 2,
        Rating::Good => 3,
        Rating::Easy => 4,
    }
}

fn clamp_difficulty(difficulty: f64) -> f64 {
    difficulty.clamp(DIFFICULTY_MIN, DIFFICULTY_MAX)
}

/// Python's `round()`: halves go to the even neighbour.
fn round_half_even(value: f64) -> f64 {
    value.round_ties_even()
}

/// FSRS memory state of a reviewed card.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MemoryState {
    pub stability: f64,
    pub difficulty: f64,
}

/// Position in the reference state machine. A new card is `Learning { step: 0 }` without memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Learning { step: u32 },
    Review,
    Relearning { step: u32 },
}

/// Time until the card is due again: a learning step in seconds or a review interval in days.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NextInterval {
    Seconds(u32),
    Days(u32),
}

impl NextInterval {
    pub fn seconds(self) -> i64 {
        match self {
            Self::Seconds(seconds) => i64::from(seconds),
            Self::Days(days) => i64::from(days) * 86_400,
        }
    }
}

/// Everything the state machine needs about a card before the rating.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReviewInput {
    pub phase: Phase,
    /// `None` before the first rating.
    pub memory: Option<MemoryState>,
    /// Whole days since the previous rating (`None` before the first one). Negative values (a
    /// clock moved backwards) count as the same day.
    pub elapsed_days: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReviewOutput {
    pub phase: Phase,
    pub memory: MemoryState,
    pub interval: NextInterval,
}

/// Scheduler policy of one preset: parameters, learning and relearning steps and fuzz.
#[derive(Debug, Clone, PartialEq)]
pub struct SchedulerConfig {
    pub parameters: FsrsParameters,
    pub learning_steps_seconds: Vec<u32>,
    pub relearning_steps_seconds: Vec<u32>,
    pub fuzz: bool,
}

impl SchedulerConfig {
    /// Builds the policy of a preset; steps are stored in minutes.
    pub fn from_options(options: &crate::deck::DeckConfigOptions) -> ValidationResult<Self> {
        let parameters = FsrsParameters::new(
            &options.weights,
            options.desired_retention,
            options.maximum_interval_days,
        )?;
        let to_seconds = |minutes: &Vec<u32>| minutes.iter().map(|m| m * 60).collect();
        Ok(Self {
            parameters,
            learning_steps_seconds: to_seconds(&options.learning_steps_minutes),
            relearning_steps_seconds: to_seconds(&options.relearning_steps_minutes),
            fuzz: options.fuzz,
        })
    }

    /// Stable fingerprint of everything that changes scheduling (`review_logs.parameters_hash`):
    /// 32 hex characters of SHA-256 over the exact bit patterns.
    pub fn parameters_hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(FSRS_VERSION.as_bytes());
        for weight in &self.parameters.weights {
            hasher.update(weight.to_bits().to_le_bytes());
        }
        hasher.update(self.parameters.desired_retention.to_bits().to_le_bytes());
        hasher.update(self.parameters.maximum_interval_days.to_le_bytes());
        for steps in [&self.learning_steps_seconds, &self.relearning_steps_seconds] {
            hasher.update((steps.len() as u32).to_le_bytes());
            for step in steps {
                hasher.update(step.to_le_bytes());
            }
        }
        hasher.update([u8::from(self.fuzz)]);
        hasher.finalize()[..16]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    /// One rating, as the reference `Scheduler.review_card`. `fuzz_uniform` ∈ [0, 1) replaces
    /// the reference's `random()` and is ignored when fuzz is off or the card is not in review.
    pub fn review(&self, input: &ReviewInput, rating: Rating, fuzz_uniform: f64) -> ReviewOutput {
        let params = &self.parameters;
        let same_day = input.elapsed_days.is_some_and(|days| days < 1);
        let elapsed = input.elapsed_days.unwrap_or(0).max(0) as f64;
        let memory = match (input.memory, input.phase) {
            (None, _) => MemoryState {
                stability: params.initial_stability(rating),
                difficulty: params.initial_difficulty(rating),
            },
            (Some(memory), _) if same_day => MemoryState {
                stability: params.short_term_stability(memory.stability, rating),
                difficulty: params.next_difficulty(memory.difficulty, rating),
            },
            (Some(memory), _) => {
                let retrievability = params.retrievability(elapsed, memory.stability);
                MemoryState {
                    stability: params.next_stability(
                        memory.difficulty,
                        memory.stability,
                        retrievability,
                        rating,
                    ),
                    difficulty: params.next_difficulty(memory.difficulty, rating),
                }
            }
        };
        let review_interval = || NextInterval::Days(params.next_interval_days(memory.stability));
        let (phase, interval) = match input.phase {
            Phase::Learning { step } => self.step_through(
                &self.learning_steps_seconds,
                step,
                rating,
                review_interval,
                |step| Phase::Learning { step },
            ),
            Phase::Relearning { step } => self.step_through(
                &self.relearning_steps_seconds,
                step,
                rating,
                review_interval,
                |step| Phase::Relearning { step },
            ),
            Phase::Review => match rating {
                Rating::Again if !self.relearning_steps_seconds.is_empty() => (
                    Phase::Relearning { step: 0 },
                    NextInterval::Seconds(self.relearning_steps_seconds[0]),
                ),
                _ => (Phase::Review, review_interval()),
            },
        };
        let interval = match (phase, interval) {
            (Phase::Review, NextInterval::Days(days)) if self.fuzz => {
                NextInterval::Days(params.fuzzed_interval_days(days, fuzz_uniform))
            }
            _ => interval,
        };
        ReviewOutput {
            phase,
            memory,
            interval,
        }
    }

    /// Learning or relearning steps (reference rules, including the Hard averages).
    fn step_through(
        &self,
        steps: &[u32],
        step: u32,
        rating: Rating,
        review_interval: impl Fn() -> NextInterval,
        phase: impl Fn(u32) -> Phase,
    ) -> (Phase, NextInterval) {
        let index = step as usize;
        let graduate = || (Phase::Review, review_interval());
        if steps.is_empty() || (index >= steps.len() && rating != Rating::Again) {
            return graduate();
        }
        match rating {
            Rating::Again => (phase(0), NextInterval::Seconds(steps[0])),
            Rating::Hard => {
                let seconds = match (index, steps.len()) {
                    (0, 1) => steps[0] * 3 / 2,
                    (0, _) => (steps[0] + steps[1]) / 2,
                    _ => steps[index],
                };
                (phase(step), NextInterval::Seconds(seconds))
            }
            Rating::Good if index + 1 == steps.len() => graduate(),
            Rating::Good => (phase(step + 1), NextInterval::Seconds(steps[index + 1])),
            Rating::Easy => graduate(),
        }
    }
}

/// Anki's leech rule: the threshold lapse and then every half threshold (at least every lapse).
pub fn is_leech(lapses: i64, threshold: u32) -> bool {
    let threshold = i64::from(threshold);
    if threshold == 0 || lapses < threshold {
        return false;
    }
    (lapses - threshold) % (threshold / 2).max(1) == 0
}

/// Uniform sample in [0, 1) for the fuzz of one rating: SHA-256 of the card GUID, the card
/// version the rating is made against and the parameters hash. Preview and commit compute the
/// same value; every committed write bumps the version, so consecutive reviews differ.
pub fn fuzz_uniform(card_guid: &str, card_version: i64, parameters_hash: &str) -> f64 {
    let mut hasher = Sha256::new();
    hasher.update(card_guid.as_bytes());
    hasher.update([0]);
    hasher.update(card_version.to_le_bytes());
    hasher.update([0]);
    hasher.update(parameters_hash.as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    (u64::from_le_bytes(bytes) >> 11) as f64 / (1u64 << 53) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defaults() -> FsrsParameters {
        FsrsParameters::new(&DEFAULT_WEIGHTS, 0.9, MAX_INTERVAL_DAYS).unwrap()
    }

    fn config(learning: &[u32], relearning: &[u32]) -> SchedulerConfig {
        SchedulerConfig {
            parameters: defaults(),
            learning_steps_seconds: learning.to_vec(),
            relearning_steps_seconds: relearning.to_vec(),
            fuzz: false,
        }
    }

    // KMP specification (kmp-fsrs-spec.json "exactChecks"), against the reference values.

    #[test]
    fn retrievability_at_stability_is_ninety_percent() {
        let p = defaults();
        for s in [1.0, 5.0, 10.0, 50.0, 100.0] {
            assert!((p.retrievability(s, s) - 0.9).abs() < 1e-12, "S = {s}");
        }
        assert!((p.retrievability(0.0, 10.0) - 1.0).abs() < 1e-12);
        assert_eq!(p.retrievability(5.0, 0.0), 0.0);
        assert_eq!(p.retrievability(5.0, -1.0), 0.0);
    }

    #[test]
    fn interval_at_ninety_percent_is_the_stability() {
        let p = defaults();
        for s in [1.0, 5.0, 10.0, 50.0] {
            assert!(
                (f64::from(p.next_interval_days(s)) - s).abs() <= 1.0,
                "S = {s}"
            );
        }
        assert_eq!(p.next_interval_days(0.001), 1);
        assert_eq!(p.next_interval_days(1e6), MAX_INTERVAL_DAYS);
    }

    #[test]
    fn initial_values_come_from_the_weights() {
        let p = defaults();
        for (rating, index) in [
            (Rating::Again, 0),
            (Rating::Hard, 1),
            (Rating::Good, 2),
            (Rating::Easy, 3),
        ] {
            assert_eq!(p.initial_stability(rating), DEFAULT_WEIGHTS[index]);
        }
        assert!((p.initial_difficulty(Rating::Again) - DEFAULT_WEIGHTS[4]).abs() < 1e-12);
    }

    #[test]
    fn parameters_are_validated_like_presets() {
        assert_eq!(
            FsrsParameters::new(&[0.2, 1.0], 0.9, 100)
                .unwrap_err()
                .reason,
            "count"
        );
        for retention in [0.5, 1.01] {
            assert_eq!(
                FsrsParameters::new(&DEFAULT_WEIGHTS, retention, 100)
                    .unwrap_err()
                    .field,
                "desiredRetention"
            );
        }
        let mut weights = DEFAULT_WEIGHTS;
        weights[16] = 0.5; // the easy bonus must be at least 1
        assert_eq!(
            FsrsParameters::new(&weights, 0.9, 100).unwrap_err().reason,
            "outOfRange"
        );
        assert!(FsrsParameters::new(&DEFAULT_WEIGHTS, 0.9, 0).is_err());
    }

    #[test]
    fn rounding_matches_python() {
        assert_eq!(round_half_even(2.5), 2.0);
        assert_eq!(round_half_even(3.5), 4.0);
        assert_eq!(round_half_even(-0.5), -0.0);
        assert_eq!(round_half_even(2.4999), 2.0);
    }

    #[test]
    fn hard_on_the_first_step_averages_or_stretches_the_step() {
        let new = ReviewInput {
            phase: Phase::Learning { step: 0 },
            memory: None,
            elapsed_days: None,
        };
        let two = config(&[60, 600], &[600]).review(&new, Rating::Hard, 0.0);
        assert_eq!(two.interval, NextInterval::Seconds(330));
        let one = config(&[600], &[600]).review(&new, Rating::Hard, 0.0);
        assert_eq!(one.interval, NextInterval::Seconds(900));
        assert_eq!(one.phase, Phase::Learning { step: 0 });
    }

    #[test]
    fn without_steps_cards_go_straight_to_review() {
        let new = ReviewInput {
            phase: Phase::Learning { step: 0 },
            memory: None,
            elapsed_days: None,
        };
        let out = config(&[], &[]).review(&new, Rating::Again, 0.0);
        assert_eq!(out.phase, Phase::Review);
        assert!(matches!(out.interval, NextInterval::Days(1)));
    }

    #[test]
    fn a_step_beyond_the_current_steps_graduates_unless_again() {
        let input = ReviewInput {
            phase: Phase::Learning { step: 5 },
            memory: Some(MemoryState {
                stability: 3.0,
                difficulty: 5.0,
            }),
            elapsed_days: Some(0),
        };
        let cfg = config(&[60, 600], &[600]);
        assert_eq!(cfg.review(&input, Rating::Hard, 0.0).phase, Phase::Review);
        assert_eq!(
            cfg.review(&input, Rating::Again, 0.0).phase,
            Phase::Learning { step: 0 }
        );
    }

    #[test]
    fn fuzz_only_applies_to_review_intervals() {
        let mut cfg = config(&[60, 600], &[600]);
        cfg.fuzz = true;
        let review = ReviewInput {
            phase: Phase::Review,
            memory: Some(MemoryState {
                stability: 40.0,
                difficulty: 5.0,
            }),
            elapsed_days: Some(40),
        };
        let low = cfg.review(&review, Rating::Good, 0.0).interval;
        let high = cfg.review(&review, Rating::Good, 0.99).interval;
        assert_ne!(low, high);
        let lapse = cfg.review(&review, Rating::Again, 0.99).interval;
        assert_eq!(lapse, NextInterval::Seconds(600));
    }

    #[test]
    fn leeches_repeat_every_half_threshold() {
        let marks: Vec<i64> = (0..20).filter(|lapses| is_leech(*lapses, 8)).collect();
        assert_eq!(marks, [8, 12, 16]);
        let every: Vec<i64> = (0..5).filter(|lapses| is_leech(*lapses, 1)).collect();
        assert_eq!(every, [1, 2, 3, 4]);
    }

    #[test]
    fn fuzz_uniform_is_deterministic_and_varies_with_the_version() {
        let a = fuzz_uniform("guid", 1, "hash");
        assert_eq!(a, fuzz_uniform("guid", 1, "hash"));
        assert_ne!(a, fuzz_uniform("guid", 2, "hash"));
        assert_ne!(a, fuzz_uniform("other", 1, "hash"));
        assert!((0.0..1.0).contains(&a));
    }

    #[test]
    fn parameters_hash_changes_with_any_scheduling_input() {
        let base = config(&[60, 600], &[600]);
        let hash = base.parameters_hash();
        assert_eq!(hash.len(), 32);
        assert_eq!(hash, base.clone().parameters_hash());
        let mut steps = base.clone();
        steps.learning_steps_seconds = vec![60];
        let mut fuzz = base.clone();
        fuzz.fuzz = true;
        let mut retention = base.clone();
        retention.parameters = FsrsParameters::new(&DEFAULT_WEIGHTS, 0.91, 36_500).unwrap();
        for other in [steps, fuzz, retention] {
            assert_ne!(other.parameters_hash(), hash);
        }
    }
}
