//! Local FSRS-6 optimizer (plan Phase 18 step 2, ADR-22).
//!
//! Fits the 21 weights to the learner's own review history on the device. The model is the
//! scheduler's memory model ([`crate::fsrs`]) written once over a generic number type: plain
//! `f64` for evaluation (identical results to the scheduler) and forward-mode dual numbers for
//! exact gradients. No tensor library: the whole history of a large collection is a few hundred
//! thousand small steps, cheap enough for a phone.
//!
//! Method: binary cross-entropy of the predicted recall against "not Again" on every review made
//! on a later study day than the previous one, Adam with a cosine learning rate, the initial
//! stabilities w0–w3 in log space (their range is 0.001–100), an L2 pull towards the default
//! weights scaled by the dataset size, and clamping to the reference bounds after every step.
//! Whole cards are held out for validation (no card contributes to both sides), so the
//! comparison with the current weights is measured on answers the training never saw.

use std::f64::consts::{E, PI};
use std::ops::{Add, Div, Mul, Neg, Sub};

use sha2::{Digest, Sha256};

use crate::fsrs::{
    DEFAULT_WEIGHTS, DIFFICULTY_MAX, DIFFICULTY_MIN, FsrsParameters, STABILITY_MIN, WEIGHT_COUNT,
    WEIGHT_LOWER_BOUNDS, WEIGHT_UPPER_BOUNDS,
};
use crate::review::{Rating, ReviewKind};

/// Recorded with every run; a change of method gets a new version.
pub const OPTIMIZER_VERSION: &str = "langualinker-fsrs6-adam-1";

/// Usable reviews below which the optimizer refuses: too few for a held-out comparison.
pub const MINIMUM_REVIEWS: u32 = 200;
/// Usable reviews below which a result is shown with a "few reviews" warning. A UX heuristic
/// (plan Phase 18 step 2.4), not an FSRS requirement; the held-out check is what protects.
pub const RECOMMENDED_REVIEWS: u32 = 1000;

/// Share of cards (by a hash of the card id) held out for validation: 1 in `VALIDATION_EVERY`.
const VALIDATION_EVERY: u64 = 5;
/// Reviews per optimizer step.
const BATCH_TARGETS: usize = 512;
const MIN_EPOCHS: usize = 5;
const MAX_EPOCHS: usize = 200;
/// Small datasets get more epochs so the optimizer still takes this many steps.
const MIN_STEPS: usize = 200;
const LEARNING_RATE: f64 = 0.04;
const ADAM_BETA1: f64 = 0.9;
const ADAM_BETA2: f64 = 0.999;
const ADAM_EPSILON: f64 = 1e-8;
/// Strength of the pull towards the defaults, per review in the training set.
const PRIOR_STRENGTH: f64 = 1.0;
/// Spread of the prior: log space for w0–w3, otherwise a quarter of the bound range.
const LOG_PRIOR_SCALE: f64 = 1.5;
/// Predictions are kept inside (ε, 1 − ε) so a confident miss has a finite loss.
const PROBABILITY_EPSILON: f64 = 1e-6;
/// The proposal is recommended only when it beats the current weights by at least this much
/// held-out log loss per review.
const IMPROVEMENT_MARGIN: f64 = 1e-4;
const CALIBRATION_BINS: usize = 20;

// ---------------------------------------------------------------- dataset

/// One review log row as the optimizer needs it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoggedReview {
    pub card: i64,
    pub rating: Rating,
    pub kind: ReviewKind,
    /// Study day of the review in the collection calendar (the scheduler's elapsed-day rule).
    pub day: i64,
    /// Instant in milliseconds; orders reviews of one card.
    pub at: i64,
    /// Undone ratings stay in the log for auditing but are not answers.
    pub undone: bool,
}

/// A card's answers in order: rating and whole study days since the previous answer (0 first).
#[derive(Debug, Clone, PartialEq)]
pub struct CardHistory {
    pub card: i64,
    pub reviews: Vec<(Rating, i64)>,
}

impl CardHistory {
    /// Reviews the model predicts: every answer after the first on a later study day.
    pub fn targets(&self) -> usize {
        self.reviews
            .iter()
            .skip(1)
            .filter(|(_, elapsed)| *elapsed >= 1)
            .count()
    }

    fn in_validation(&self) -> bool {
        mix(self.card as u64).is_multiple_of(VALIDATION_EVERY)
    }
}

/// What went into a dataset and what was left out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DatasetStats {
    /// Every log row read.
    pub reviews: u32,
    /// Cards with a usable history.
    pub cards: u32,
    /// Reviews the model is trained and validated on ([`CardHistory::targets`]).
    pub usable: u32,
    pub excluded_undone: u32,
    /// Manual rescheduling (imported "set due date"/"forget" rows) is not an answer.
    pub excluded_manual: u32,
    /// Reviews of cards whose history does not start with the first learning answer (imported
    /// mid-way): their memory state cannot be reconstructed.
    pub excluded_incomplete: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Dataset {
    pub cards: Vec<CardHistory>,
    pub stats: DatasetStats,
    /// SHA-256 (32 hex) over the usable histories: two runs on the same answers share it.
    pub fingerprint: String,
}

/// Builds chronological card histories from log rows (any order). Undone and manual rows are
/// dropped; a card whose first remaining answer is not a learning answer is dropped whole. The
/// parameters a review was scheduled with do not matter: an answer stays an answer when the
/// weights change (plan Phase 18 step 2.6).
pub fn build_dataset(mut logs: Vec<LoggedReview>) -> Dataset {
    let mut stats = DatasetStats {
        reviews: count(logs.len()),
        ..DatasetStats::default()
    };
    logs.sort_by_key(|log| (log.card, log.at));
    let mut cards = Vec::new();
    let mut start = 0;
    while start < logs.len() {
        let card = logs[start].card;
        let end = start
            + logs[start..]
                .iter()
                .take_while(|log| log.card == card)
                .count();
        let mut kept = Vec::with_capacity(end - start);
        for log in &logs[start..end] {
            if log.undone {
                stats.excluded_undone += 1;
            } else if log.kind == ReviewKind::Manual {
                stats.excluded_manual += 1;
            } else {
                kept.push(*log);
            }
        }
        start = end;
        match kept.first() {
            None => continue,
            Some(first) if first.kind != ReviewKind::Learning => {
                stats.excluded_incomplete += count(kept.len());
                continue;
            }
            Some(_) => {}
        }
        let mut previous_day = kept[0].day;
        let reviews: Vec<(Rating, i64)> = kept
            .iter()
            .enumerate()
            .map(|(index, log)| {
                let elapsed = if index == 0 {
                    0
                } else {
                    (log.day - previous_day).max(0)
                };
                previous_day = log.day;
                (log.rating, elapsed)
            })
            .collect();
        cards.push(CardHistory { card, reviews });
    }
    stats.cards = count(cards.len());
    stats.usable = count(cards.iter().map(CardHistory::targets).sum());
    let fingerprint = fingerprint(&cards);
    Dataset {
        cards,
        stats,
        fingerprint,
    }
}

fn fingerprint(cards: &[CardHistory]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(OPTIMIZER_VERSION.as_bytes());
    for card in cards {
        hasher.update(card.card.to_le_bytes());
        hasher.update((card.reviews.len() as u64).to_le_bytes());
        for (rating, elapsed) in &card.reviews {
            hasher.update([rating.db_code() as u8]);
            hasher.update(elapsed.to_le_bytes());
        }
    }
    hasher.finalize()[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// splitmix64: a fixed, well-mixed hash for the split and the shuffle.
fn mix(value: u64) -> u64 {
    let mut z = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

// ---------------------------------------------------------------- numbers

/// What the memory model needs from a number: `f64` and [`Dual`].
trait Real:
    Copy
    + Add<Output = Self>
    + Sub<Output = Self>
    + Mul<Output = Self>
    + Div<Output = Self>
    + Neg<Output = Self>
{
    fn constant(value: f64) -> Self;
    fn value(self) -> f64;
    fn exp(self) -> Self;
    fn ln(self) -> Self;
    /// `self` to the power `exponent`; `self` > 0.
    fn pow(self, exponent: Self) -> Self;

    /// `max(self, floor)`; the floor is a constant, so the gradient there is zero.
    fn at_least(self, floor: f64) -> Self {
        if self.value() < floor {
            Self::constant(floor)
        } else {
            self
        }
    }

    fn at_most(self, ceiling: f64) -> Self {
        if self.value() > ceiling {
            Self::constant(ceiling)
        } else {
            self
        }
    }

    fn smaller(self, other: Self) -> Self {
        if other.value() < self.value() {
            other
        } else {
            self
        }
    }
}

impl Real for f64 {
    fn constant(value: f64) -> Self {
        value
    }
    fn value(self) -> f64 {
        self
    }
    // Same calls as `fsrs.rs`, so evaluation equals the scheduler bit for bit.
    fn exp(self) -> Self {
        E.powf(self)
    }
    fn ln(self) -> Self {
        f64::ln(self)
    }
    fn pow(self, exponent: Self) -> Self {
        self.powf(exponent)
    }
}

/// Forward-mode dual number: a value and its gradient with respect to every weight.
#[derive(Clone, Copy, Debug)]
struct Dual {
    value: f64,
    gradient: [f64; WEIGHT_COUNT],
}

impl Dual {
    /// A weight whose derivative with respect to the optimised variable is `slope`.
    fn variable(value: f64, index: usize, slope: f64) -> Self {
        let mut gradient = [0.0; WEIGHT_COUNT];
        gradient[index] = slope;
        Self { value, gradient }
    }

    /// Chain rule: a function with value `value` and derivative `slope` at `self`.
    fn chain(self, value: f64, slope: f64) -> Self {
        let mut gradient = self.gradient;
        for g in &mut gradient {
            *g *= slope;
        }
        Self { value, gradient }
    }

    fn combine(self, other: Self, value: f64, a: f64, b: f64) -> Self {
        let mut gradient = self.gradient;
        for (g, o) in gradient.iter_mut().zip(other.gradient) {
            *g = a * *g + b * o;
        }
        Self { value, gradient }
    }
}

impl Add for Dual {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        self.combine(other, self.value + other.value, 1.0, 1.0)
    }
}

impl Sub for Dual {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        self.combine(other, self.value - other.value, 1.0, -1.0)
    }
}

impl Mul for Dual {
    type Output = Self;
    fn mul(self, other: Self) -> Self {
        self.combine(other, self.value * other.value, other.value, self.value)
    }
}

impl Div for Dual {
    type Output = Self;
    fn div(self, other: Self) -> Self {
        let value = self.value / other.value;
        self.combine(other, value, 1.0 / other.value, -value / other.value)
    }
}

impl Neg for Dual {
    type Output = Self;
    fn neg(self) -> Self {
        self.chain(-self.value, -1.0)
    }
}

impl Real for Dual {
    fn constant(value: f64) -> Self {
        Self {
            value,
            gradient: [0.0; WEIGHT_COUNT],
        }
    }
    fn value(self) -> f64 {
        self.value
    }
    fn exp(self) -> Self {
        let value = E.powf(self.value);
        self.chain(value, value)
    }
    fn ln(self) -> Self {
        self.chain(self.value.ln(), 1.0 / self.value)
    }
    fn pow(self, exponent: Self) -> Self {
        // d(b^e) = b^e · (e'·ln b + e·b'/b)
        let value = self.value.powf(exponent.value);
        self.combine(
            exponent,
            value,
            value * exponent.value / self.value,
            value * self.value.ln(),
        )
    }
}

// ---------------------------------------------------------------- model

/// The scheduler's memory model (`fsrs.rs`, same formulas and operation order) over [`Real`].
struct Model<T: Real> {
    w: [T; WEIGHT_COUNT],
    decay: T,
    factor: T,
}

fn c<T: Real>(value: f64) -> T {
    T::constant(value)
}

fn grade(rating: Rating) -> f64 {
    rating.db_code() as f64
}

impl<T: Real> Model<T> {
    fn new(w: [T; WEIGHT_COUNT]) -> Self {
        let decay = -w[20];
        let factor = c::<T>(0.9).pow(c::<T>(1.0) / decay) - c(1.0);
        Self { w, decay, factor }
    }

    fn retrievability(&self, elapsed_days: f64, stability: T) -> T {
        (c::<T>(1.0) + self.factor * c(elapsed_days.max(0.0)) / stability).pow(self.decay)
    }

    fn raw_initial_difficulty(&self, rating: Rating) -> T {
        let w = &self.w;
        w[4] - (w[5] * c(grade(rating) - 1.0)).exp() + c(1.0)
    }

    fn clamp_difficulty(difficulty: T) -> T {
        difficulty.at_least(DIFFICULTY_MIN).at_most(DIFFICULTY_MAX)
    }

    fn next_difficulty(&self, difficulty: T, rating: Rating) -> T {
        let w = &self.w;
        let target = self.raw_initial_difficulty(Rating::Easy);
        let delta = -(w[6] * c(grade(rating) - 3.0));
        let damped = difficulty + (c::<T>(10.0) - difficulty) * delta / c(9.0);
        Self::clamp_difficulty(w[7] * target + (c::<T>(1.0) - w[7]) * damped)
    }

    fn short_term_stability(&self, stability: T, rating: Rating) -> T {
        let w = &self.w;
        let mut increase =
            (w[17] * (c::<T>(grade(rating) - 3.0) + w[18])).exp() * stability.pow(-w[19]);
        if matches!(rating, Rating::Good | Rating::Easy) {
            increase = increase.at_least(1.0);
        }
        (stability * increase).at_least(STABILITY_MIN)
    }

    fn next_stability(&self, difficulty: T, stability: T, retrievability: T, rating: Rating) -> T {
        let w = &self.w;
        let next = if rating == Rating::Again {
            let long_term = w[11]
                * difficulty.pow(-w[12])
                * ((stability + c(1.0)).pow(w[13]) - c(1.0))
                * ((c::<T>(1.0) - retrievability) * w[14]).exp();
            let short_term = stability / (w[17] * w[18]).exp();
            long_term.smaller(short_term)
        } else {
            let hard_penalty = if rating == Rating::Hard {
                w[15]
            } else {
                c(1.0)
            };
            let easy_bonus = if rating == Rating::Easy {
                w[16]
            } else {
                c(1.0)
            };
            stability
                * (c::<T>(1.0)
                    + w[8].exp()
                        * (c::<T>(11.0) - difficulty)
                        * stability.pow(-w[9])
                        * (((c::<T>(1.0) - retrievability) * w[10]).exp() - c(1.0))
                        * hard_penalty
                        * easy_bonus)
        };
        next.at_least(STABILITY_MIN)
    }

    /// Walks one card's answers. `visit` sees every target: predicted recall and whether the
    /// card was recalled. Returns the final (stability, difficulty).
    fn replay(&self, history: &CardHistory, mut visit: impl FnMut(T, bool)) -> (T, T) {
        let w = &self.w;
        let (first, _) = history.reviews[0];
        let mut stability = w[first.db_code() as usize - 1].at_least(STABILITY_MIN);
        let mut difficulty = Self::clamp_difficulty(self.raw_initial_difficulty(first));
        for &(rating, elapsed) in &history.reviews[1..] {
            if elapsed < 1 {
                stability = self.short_term_stability(stability, rating);
            } else {
                let retrievability = self.retrievability(elapsed as f64, stability);
                visit(retrievability, rating != Rating::Again);
                stability = self.next_stability(difficulty, stability, retrievability, rating);
            }
            difficulty = self.next_difficulty(difficulty, rating);
        }
        (stability, difficulty)
    }
}

fn log_loss<T: Real>(predicted: T, recalled: bool) -> T {
    let p = predicted
        .at_least(PROBABILITY_EPSILON)
        .at_most(1.0 - PROBABILITY_EPSILON);
    if recalled {
        -p.ln()
    } else {
        -(c::<T>(1.0) - p).ln()
    }
}

/// Final memory state of a card under `weights`, as the scheduler would have computed it from
/// the same answers (with the scheduler's elapsed days).
pub fn replay_memory(history: &CardHistory, weights: &[f64; WEIGHT_COUNT]) -> (f64, f64) {
    Model::new(*weights).replay(history, |_, _| {})
}

// ---------------------------------------------------------------- evaluation

/// Quality of a weight vector on a set of cards.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Evaluation {
    /// Mean binary cross-entropy per review (lower is better).
    pub log_loss: f64,
    /// Calibration error: RMSE between predicted and actual recall over 20 bins of the
    /// prediction, weighted by reviews.
    pub rmse: f64,
    pub reviews: u32,
}

pub fn evaluate<'a>(
    cards: impl IntoIterator<Item = &'a CardHistory>,
    weights: &[f64; WEIGHT_COUNT],
) -> Evaluation {
    let model = Model::new(*weights);
    let mut loss = 0.0;
    let mut reviews = 0usize;
    let mut bins = [(0.0_f64, 0.0_f64, 0usize); CALIBRATION_BINS];
    for card in cards {
        model.replay(card, |predicted, recalled| {
            loss += log_loss(predicted, recalled);
            reviews += 1;
            let bin = ((predicted * CALIBRATION_BINS as f64) as usize).min(CALIBRATION_BINS - 1);
            bins[bin].0 += predicted;
            bins[bin].1 += f64::from(u8::from(recalled));
            bins[bin].2 += 1;
        });
    }
    if reviews == 0 {
        return Evaluation {
            log_loss: 0.0,
            rmse: 0.0,
            reviews: 0,
        };
    }
    let squared: f64 = bins
        .iter()
        .filter(|(_, _, n)| *n > 0)
        .map(|(predicted, actual, n)| {
            let n = *n as f64;
            n * (predicted / n - actual / n).powi(2)
        })
        .sum();
    Evaluation {
        log_loss: loss / reviews as f64,
        rmse: (squared / reviews as f64).sqrt(),
        reviews: count(reviews),
    }
}

/// Expected workload of the current cards if every next interval came from `weights`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Workload {
    /// Steady-state reviews per day: the sum of 1 / next interval over the cards.
    pub daily_reviews: f64,
    /// Median next interval in days (0 without cards).
    pub median_interval_days: f64,
}

/// Effect preview (plan Phase 18 DoD): no card is changed; the memory state of every card is
/// replayed from its answers and its next interval computed at the preset's retention.
pub fn workload(
    cards: &[CardHistory],
    weights: &[f64; WEIGHT_COUNT],
    desired_retention: f64,
    maximum_interval_days: u32,
) -> Workload {
    let Ok(parameters) = FsrsParameters::new(weights, desired_retention, maximum_interval_days)
    else {
        return Workload {
            daily_reviews: 0.0,
            median_interval_days: 0.0,
        };
    };
    let model = Model::new(*weights);
    let mut intervals: Vec<u32> = cards
        .iter()
        .map(|card| {
            let (stability, _) = model.replay(card, |_, _| {});
            parameters.next_interval_days(stability)
        })
        .collect();
    intervals.sort_unstable();
    let daily_reviews = intervals.iter().map(|days| 1.0 / f64::from(*days)).sum();
    let median_interval_days = match intervals.len() {
        0 => 0.0,
        n if n % 2 == 1 => f64::from(intervals[n / 2]),
        n => (f64::from(intervals[n / 2 - 1]) + f64::from(intervals[n / 2])) / 2.0,
    };
    Workload {
        daily_reviews,
        median_interval_days,
    }
}

// ---------------------------------------------------------------- training

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptimizeError {
    /// Fewer than [`MINIMUM_REVIEWS`] usable reviews, or none held out.
    NotEnoughReviews,
    /// The progress callback asked to stop.
    Cancelled,
}

/// Metrics of one weight vector: held-out quality and workload.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    pub validation: Evaluation,
    pub workload: Workload,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Optimized {
    pub weights: [f64; WEIGHT_COUNT],
    pub training_reviews: u32,
    pub validation_reviews: u32,
    pub current: Metrics,
    pub proposed: Metrics,
    /// The proposal predicts the held-out answers better than the current weights.
    pub recommended: bool,
    /// Fewer usable reviews than [`RECOMMENDED_REVIEWS`].
    pub few_reviews: bool,
}

/// What the optimizer is compared against and previews with.
#[derive(Debug, Clone, PartialEq)]
pub struct OptimizeRequest<'a> {
    pub dataset: &'a Dataset,
    pub current_weights: [f64; WEIGHT_COUNT],
    pub desired_retention: f64,
    pub maximum_interval_days: u32,
}

/// Trains on the training cards from the default weights, then compares the result with the
/// current weights on the held-out cards. Deterministic for the same dataset. `progress(done,
/// total)` is called after every step and at the end; returning `false` cancels.
pub fn optimize(
    request: &OptimizeRequest<'_>,
    mut progress: impl FnMut(u64, u64) -> bool,
) -> Result<Optimized, OptimizeError> {
    let dataset = request.dataset;
    if dataset.stats.usable < MINIMUM_REVIEWS {
        return Err(OptimizeError::NotEnoughReviews);
    }
    let (validation, training): (Vec<&CardHistory>, Vec<&CardHistory>) = dataset
        .cards
        .iter()
        .filter(|card| card.targets() > 0)
        .partition(|card| card.in_validation());
    let training_reviews: usize = training.iter().map(|card| card.targets()).sum();
    let validation_reviews: usize = validation.iter().map(|card| card.targets()).sum();
    if training_reviews == 0 || validation_reviews == 0 {
        return Err(OptimizeError::NotEnoughReviews);
    }

    let batches_per_epoch = training_reviews.div_ceil(BATCH_TARGETS).max(1);
    let epochs = MIN_STEPS
        .div_ceil(batches_per_epoch)
        .clamp(MIN_EPOCHS, MAX_EPOCHS);
    let total = (epochs * training_reviews) as u64;
    let mut trainer = Trainer::new(training_reviews);
    let mut order: Vec<&CardHistory> = training;
    let mut seed = 0x5EED_u64;
    let mut done = 0u64;
    let total_steps = epochs * batches_per_epoch;
    let mut step = 0usize;
    for _ in 0..epochs {
        shuffle(&mut order, &mut seed);
        let mut batch_start = 0;
        while batch_start < order.len() {
            let mut targets = 0;
            let mut batch_end = batch_start;
            while batch_end < order.len() && targets < BATCH_TARGETS {
                targets += order[batch_end].targets();
                batch_end += 1;
            }
            let rate = cosine_rate(step, total_steps);
            trainer.step(&order[batch_start..batch_end], rate);
            step += 1;
            done += targets as u64;
            batch_start = batch_end;
            if !progress(done.min(total), total) {
                return Err(OptimizeError::Cancelled);
            }
        }
    }
    let weights = trainer.weights();

    let metrics = |weights: &[f64; WEIGHT_COUNT]| Metrics {
        validation: evaluate(validation.iter().copied(), weights),
        workload: workload(
            &dataset.cards,
            weights,
            request.desired_retention,
            request.maximum_interval_days,
        ),
    };
    let current = metrics(&request.current_weights);
    let proposed = metrics(&weights);
    progress(total, total);
    Ok(Optimized {
        weights,
        training_reviews: count(training_reviews),
        validation_reviews: count(validation_reviews),
        current,
        proposed,
        recommended: proposed.validation.log_loss
            < current.validation.log_loss - IMPROVEMENT_MARGIN,
        few_reviews: dataset.stats.usable < RECOMMENDED_REVIEWS,
    })
}

fn shuffle<T>(items: &mut [T], seed: &mut u64) {
    for index in (1..items.len()).rev() {
        *seed = mix(*seed);
        let other = (*seed % (index as u64 + 1)) as usize;
        items.swap(index, other);
    }
}

fn cosine_rate(step: usize, total: usize) -> f64 {
    let progress = step as f64 / total.max(1) as f64;
    LEARNING_RATE * 0.5 * (1.0 + (PI * progress.min(1.0)).cos())
}

/// Adam over the optimised variables: ln w for w0–w3, w itself for the rest.
struct Trainer {
    variables: [f64; WEIGHT_COUNT],
    prior: [f64; WEIGHT_COUNT],
    prior_scale: [f64; WEIGHT_COUNT],
    /// Prior gradient factor: strength / training reviews.
    prior_weight: f64,
    first_moment: [f64; WEIGHT_COUNT],
    second_moment: [f64; WEIGHT_COUNT],
    steps: i32,
}

const LOG_SPACE: usize = 4;

impl Trainer {
    fn new(training_reviews: usize) -> Self {
        let mut prior = DEFAULT_WEIGHTS;
        let mut prior_scale = [0.0; WEIGHT_COUNT];
        for index in 0..WEIGHT_COUNT {
            if index < LOG_SPACE {
                prior[index] = DEFAULT_WEIGHTS[index].ln();
                prior_scale[index] = LOG_PRIOR_SCALE;
            } else {
                prior_scale[index] =
                    (WEIGHT_UPPER_BOUNDS[index] - WEIGHT_LOWER_BOUNDS[index]) / 4.0;
            }
        }
        Self {
            variables: prior,
            prior,
            prior_scale,
            prior_weight: PRIOR_STRENGTH / training_reviews.max(1) as f64,
            first_moment: [0.0; WEIGHT_COUNT],
            second_moment: [0.0; WEIGHT_COUNT],
            steps: 0,
        }
    }

    fn weights(&self) -> [f64; WEIGHT_COUNT] {
        let mut weights = self.variables;
        for weight in weights.iter_mut().take(LOG_SPACE) {
            *weight = weight.exp();
        }
        clamp_weights(&mut weights);
        weights
    }

    fn duals(&self) -> [Dual; WEIGHT_COUNT] {
        let weights = self.weights();
        std::array::from_fn(|index| {
            // d w / d ln w = w
            let slope = if index < LOG_SPACE {
                weights[index]
            } else {
                1.0
            };
            Dual::variable(weights[index], index, slope)
        })
    }

    /// Mean log loss gradient over the batch plus the prior, then one Adam update.
    fn step(&mut self, batch: &[&CardHistory], rate: f64) {
        let model = Model::new(self.duals());
        let mut gradient = [0.0; WEIGHT_COUNT];
        let mut reviews = 0usize;
        for card in batch {
            model.replay(card, |predicted, recalled| {
                let loss = log_loss(predicted, recalled);
                for (g, d) in gradient.iter_mut().zip(loss.gradient) {
                    *g += d;
                }
                reviews += 1;
            });
        }
        if reviews == 0 {
            return;
        }
        for (index, g) in gradient.iter_mut().enumerate() {
            *g /= reviews as f64;
            let distance = (self.variables[index] - self.prior[index]) / self.prior_scale[index];
            *g += self.prior_weight * 2.0 * distance / self.prior_scale[index];
        }
        self.steps += 1;
        let correction1 = 1.0 - ADAM_BETA1.powi(self.steps);
        let correction2 = 1.0 - ADAM_BETA2.powi(self.steps);
        for (index, &g) in gradient.iter().enumerate() {
            if !g.is_finite() {
                continue;
            }
            self.first_moment[index] =
                ADAM_BETA1 * self.first_moment[index] + (1.0 - ADAM_BETA1) * g;
            self.second_moment[index] =
                ADAM_BETA2 * self.second_moment[index] + (1.0 - ADAM_BETA2) * g * g;
            let m = self.first_moment[index] / correction1;
            let v = self.second_moment[index] / correction2;
            self.variables[index] -= rate * m / (v.sqrt() + ADAM_EPSILON);
        }
        // Back inside the reference bounds.
        let weights = self.weights();
        for (index, (variable, weight)) in self.variables.iter_mut().zip(weights).enumerate() {
            *variable = if index < LOG_SPACE {
                weight.ln()
            } else {
                weight
            };
        }
    }
}

fn clamp_weights(weights: &mut [f64; WEIGHT_COUNT]) {
    for (index, weight) in weights.iter_mut().enumerate() {
        *weight = weight.clamp(WEIGHT_LOWER_BOUNDS[index], WEIGHT_UPPER_BOUNDS[index]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fsrs::{MAX_INTERVAL_DAYS, MemoryState, Phase, ReviewInput, SchedulerConfig};

    /// Deterministic uniform samples in [0, 1).
    struct Samples(u64);

    impl Samples {
        fn next(&mut self) -> f64 {
            self.0 = mix(self.0);
            (self.0 >> 11) as f64 / (1u64 << 53) as f64
        }
    }

    fn log(card: i64, rating: Rating, kind: ReviewKind, day: i64, at: i64) -> LoggedReview {
        LoggedReview {
            card,
            rating,
            kind,
            day,
            at,
            undone: false,
        }
    }

    /// Learners whose memory follows `truth`: a first answer, a same-day step, then reviews at
    /// roughly the scheduled interval (0.5–1.6×) answered by sampling the true recall.
    fn simulate(truth: &[f64; WEIGHT_COUNT], cards: i64, reviews: usize, seed: u64) -> Dataset {
        let parameters = FsrsParameters::new(truth, 0.9, MAX_INTERVAL_DAYS).unwrap();
        let model = Model::new(*truth);
        let mut samples = Samples(seed);
        let mut logs = Vec::new();
        for card in 0..cards {
            let first = match samples.next() {
                x if x < 0.2 => Rating::Again,
                x if x < 0.3 => Rating::Hard,
                x if x < 0.9 => Rating::Good,
                _ => Rating::Easy,
            };
            let mut day = 0;
            let mut at = card * 1_000_000;
            logs.push(log(card, first, ReviewKind::Learning, day, at));
            at += 1;
            logs.push(log(card, Rating::Good, ReviewKind::Learning, day, at));
            let mut history = CardHistory {
                card,
                reviews: vec![(first, 0), (Rating::Good, 0)],
            };
            for _ in 0..reviews {
                let (stability, _) = model.replay(&history, |_, _| {});
                let interval = f64::from(parameters.next_interval_days(stability));
                let elapsed = ((interval * (0.5 + 1.1 * samples.next())).round() as i64).max(1);
                let recall = model.retrievability(elapsed as f64, stability);
                let rating = if samples.next() < recall {
                    if samples.next() < 0.15 {
                        Rating::Hard
                    } else if samples.next() < 0.1 {
                        Rating::Easy
                    } else {
                        Rating::Good
                    }
                } else {
                    Rating::Again
                };
                day += elapsed;
                at += 1;
                logs.push(log(card, rating, ReviewKind::Review, day, at));
                history.reviews.push((rating, elapsed));
            }
        }
        build_dataset(logs)
    }

    fn truth() -> [f64; WEIGHT_COUNT] {
        let mut weights = DEFAULT_WEIGHTS;
        weights[0] = 0.6;
        weights[2] = 5.0;
        weights[8] = 1.4;
        weights[9] = 0.25;
        weights[11] = 1.9;
        weights[20] = 0.35;
        weights
    }

    #[test]
    fn replay_matches_the_scheduler() {
        let config = SchedulerConfig {
            parameters: FsrsParameters::new(&truth(), 0.9, MAX_INTERVAL_DAYS).unwrap(),
            learning_steps_seconds: vec![60, 600],
            relearning_steps_seconds: vec![600],
            fuzz: false,
        };
        let mut samples = Samples(7);
        for card in 0..200 {
            let mut history = CardHistory {
                card,
                reviews: Vec::new(),
            };
            let mut input = ReviewInput {
                phase: Phase::Learning { step: 0 },
                memory: None,
                elapsed_days: None,
            };
            let mut memory = MemoryState {
                stability: 0.0,
                difficulty: 0.0,
            };
            for index in 0..12 {
                let rating = Rating::ALL[(samples.next() * 4.0) as usize];
                let elapsed = if index == 0 {
                    0
                } else {
                    (samples.next() * 40.0) as i64 - 3
                };
                if index > 0 {
                    input.elapsed_days = Some(elapsed);
                }
                let out = config.review(&input, rating, 0.0);
                history.reviews.push((rating, elapsed.max(0)));
                input.phase = out.phase;
                input.memory = Some(out.memory);
                memory = out.memory;
            }
            let (stability, difficulty) = replay_memory(&history, config.parameters.weights());
            assert_eq!(stability, memory.stability, "card {card}");
            assert_eq!(difficulty, memory.difficulty, "card {card}");
        }
    }

    #[test]
    fn gradients_match_finite_differences() {
        let dataset = simulate(&truth(), 40, 6, 3);
        let at = {
            let mut weights = DEFAULT_WEIGHTS;
            // Away from clamps so the loss is smooth around the point.
            weights[7] = 0.2;
            weights
        };
        let duals: [Dual; WEIGHT_COUNT] =
            std::array::from_fn(|index| Dual::variable(at[index], index, 1.0));
        let model = Model::new(duals);
        let mut analytic = [0.0; WEIGHT_COUNT];
        for card in &dataset.cards {
            model.replay(card, |predicted, recalled| {
                let loss = log_loss(predicted, recalled);
                for (a, g) in analytic.iter_mut().zip(loss.gradient) {
                    *a += g;
                }
            });
        }
        let total = |weights: &[f64; WEIGHT_COUNT]| -> f64 {
            let model = Model::new(*weights);
            let mut sum = 0.0;
            for card in &dataset.cards {
                model.replay(card, |predicted, recalled| {
                    sum += log_loss(predicted, recalled)
                });
            }
            sum
        };
        for index in 0..WEIGHT_COUNT {
            let h = 1e-6 * at[index].abs().max(1e-3);
            let mut up = at;
            up[index] += h;
            let mut down = at;
            down[index] -= h;
            let numeric = (total(&up) - total(&down)) / (2.0 * h);
            let tolerance = 1e-4 * numeric.abs().max(1.0);
            assert!(
                (analytic[index] - numeric).abs() <= tolerance,
                "w{index}: analytic {} numeric {numeric}",
                analytic[index]
            );
        }
    }

    #[test]
    fn dataset_filters_undone_manual_and_incomplete_histories() {
        let mut undone = log(1, Rating::Again, ReviewKind::Review, 3, 30);
        undone.undone = true;
        let logs = vec![
            log(1, Rating::Good, ReviewKind::Review, 9, 90),
            log(1, Rating::Good, ReviewKind::Learning, 0, 0),
            undone,
            log(1, Rating::Hard, ReviewKind::Learning, 0, 10),
            log(1, Rating::Good, ReviewKind::Manual, 5, 50),
            log(1, Rating::Again, ReviewKind::Review, 4, 40),
            // Imported mid-way: starts with a review.
            log(2, Rating::Good, ReviewKind::Review, 2, 20),
            log(2, Rating::Good, ReviewKind::Review, 8, 80),
        ];
        let dataset = build_dataset(logs);
        assert_eq!(
            dataset.cards,
            vec![CardHistory {
                card: 1,
                reviews: vec![
                    (Rating::Good, 0),
                    (Rating::Hard, 0),
                    (Rating::Again, 4),
                    (Rating::Good, 5),
                ],
            }]
        );
        assert_eq!(
            dataset.stats,
            DatasetStats {
                reviews: 8,
                cards: 1,
                usable: 2,
                excluded_undone: 1,
                excluded_manual: 1,
                excluded_incomplete: 2,
            }
        );
    }

    #[test]
    fn a_clock_moved_backwards_counts_as_the_same_day() {
        let dataset = build_dataset(vec![
            log(1, Rating::Good, ReviewKind::Learning, 5, 0),
            log(1, Rating::Good, ReviewKind::Review, 3, 10),
        ]);
        assert_eq!(dataset.cards[0].reviews[1], (Rating::Good, 0));
        assert_eq!(dataset.stats.usable, 0);
    }

    #[test]
    fn the_split_keeps_whole_cards_on_one_side() {
        let dataset = simulate(&truth(), 1000, 3, 11);
        let held_out = dataset.cards.iter().filter(|c| c.in_validation()).count();
        // 1 in 5 by hash, within a generous margin.
        assert!((150..=250).contains(&held_out), "{held_out}");
        // The same card always lands on the same side.
        for card in &dataset.cards {
            assert_eq!(
                card.in_validation(),
                mix(card.card as u64).is_multiple_of(5)
            );
        }
    }

    #[test]
    fn too_few_reviews_are_refused() {
        let dataset = simulate(&truth(), 20, 3, 5);
        assert!(dataset.stats.usable < MINIMUM_REVIEWS);
        let request = OptimizeRequest {
            dataset: &dataset,
            current_weights: DEFAULT_WEIGHTS,
            desired_retention: 0.9,
            maximum_interval_days: MAX_INTERVAL_DAYS,
        };
        assert_eq!(
            optimize(&request, |_, _| true),
            Err(OptimizeError::NotEnoughReviews)
        );
    }

    #[test]
    fn training_recovers_the_learners_memory_better_than_the_defaults() {
        let truth = truth();
        let dataset = simulate(&truth, 1500, 6, 42);
        let request = OptimizeRequest {
            dataset: &dataset,
            current_weights: DEFAULT_WEIGHTS,
            desired_retention: 0.9,
            maximum_interval_days: MAX_INTERVAL_DAYS,
        };
        let mut last = (0, 0);
        let result = optimize(&request, |done, total| {
            assert!(done >= last.0 && done <= total);
            last = (done, total);
            true
        })
        .unwrap();
        assert_eq!(last.0, last.1);
        assert!(result.recommended);
        assert!(!result.few_reviews);
        let validation: Vec<&CardHistory> = dataset
            .cards
            .iter()
            .filter(|card| card.in_validation())
            .collect();
        let ideal = evaluate(validation.iter().copied(), &truth);
        let proposed = result.proposed.validation;
        let current = result.current.validation;
        assert!(
            proposed.log_loss < current.log_loss,
            "{proposed:?} vs {current:?}"
        );
        // Close to the loss of the weights that generated the answers.
        assert!(
            proposed.log_loss - ideal.log_loss < 0.25 * (current.log_loss - ideal.log_loss),
            "proposed {proposed:?}, ideal {ideal:?}, defaults {current:?}"
        );
        assert!(proposed.rmse < current.rmse);
        assert_eq!(
            result.training_reviews + result.validation_reviews,
            dataset.stats.usable
        );
        crate::fsrs::validate_weights(&result.weights).unwrap();
        // The proposal changes the workload preview; both are finite and positive.
        assert!(result.proposed.workload.daily_reviews > 0.0);
        assert!(result.current.workload.median_interval_days >= 1.0);
    }

    #[test]
    fn already_fitted_weights_are_not_replaced() {
        let truth = truth();
        let dataset = simulate(&truth, 600, 5, 9);
        let request = OptimizeRequest {
            dataset: &dataset,
            current_weights: truth,
            desired_retention: 0.9,
            maximum_interval_days: MAX_INTERVAL_DAYS,
        };
        let result = optimize(&request, |_, _| true).unwrap();
        // Held-out answers cannot be predicted much better than by the true weights.
        assert!(
            result.proposed.validation.log_loss > result.current.validation.log_loss - 0.005,
            "{result:?}"
        );
    }

    #[test]
    fn optimizing_is_deterministic_and_cancellable() {
        let dataset = simulate(&truth(), 300, 4, 1);
        let request = OptimizeRequest {
            dataset: &dataset,
            current_weights: DEFAULT_WEIGHTS,
            desired_retention: 0.9,
            maximum_interval_days: MAX_INTERVAL_DAYS,
        };
        let first = optimize(&request, |_, _| true).unwrap();
        let second = optimize(&request, |_, _| true).unwrap();
        assert_eq!(first, second);
        assert_eq!(
            dataset.fingerprint,
            simulate(&truth(), 300, 4, 1).fingerprint
        );
        let mut calls = 0;
        let cancelled = optimize(&request, |_, _| {
            calls += 1;
            calls < 3
        });
        assert_eq!(cancelled, Err(OptimizeError::Cancelled));
    }

    #[test]
    fn workload_follows_the_retention() {
        let dataset = simulate(&truth(), 200, 4, 2);
        let relaxed = workload(&dataset.cards, &DEFAULT_WEIGHTS, 0.8, MAX_INTERVAL_DAYS);
        let strict = workload(&dataset.cards, &DEFAULT_WEIGHTS, 0.95, MAX_INTERVAL_DAYS);
        assert!(strict.daily_reviews > relaxed.daily_reviews);
        assert!(strict.median_interval_days < relaxed.median_interval_days);
        let none = workload(&[], &DEFAULT_WEIGHTS, 0.9, MAX_INTERVAL_DAYS);
        assert_eq!(none.daily_reviews, 0.0);
    }
}
