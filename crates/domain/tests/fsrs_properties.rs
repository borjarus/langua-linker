//! Property tests of the FSRS-6 port (plan Phase 9 DoD): finiteness, bounds, the maximum
//! interval, and the monotonicity the reference policy guarantees — over arbitrary weights within
//! the reference bounds, not only the defaults. Also the KMP specification's ordering checks.

use langualinker_domain::fsrs::{
    DEFAULT_WEIGHTS, DIFFICULTY_MAX, DIFFICULTY_MIN, FsrsParameters, MemoryState, NextInterval,
    Phase, ReviewInput, STABILITY_MIN, SchedulerConfig, WEIGHT_COUNT, WEIGHT_LOWER_BOUNDS,
    WEIGHT_UPPER_BOUNDS,
};
use langualinker_domain::review::Rating;
use proptest::prelude::*;

const RATINGS: [Rating; 4] = [Rating::Again, Rating::Hard, Rating::Good, Rating::Easy];

fn weights() -> impl Strategy<Value = Vec<f64>> {
    (0..WEIGHT_COUNT)
        .map(|i| WEIGHT_LOWER_BOUNDS[i]..=WEIGHT_UPPER_BOUNDS[i])
        .collect::<Vec<_>>()
}

fn parameters() -> impl Strategy<Value = FsrsParameters> {
    (weights(), 0.70f64..=0.99, 1u32..=36_500)
        .prop_map(|(w, retention, max)| FsrsParameters::new(&w, retention, max).unwrap())
}

fn rating() -> impl Strategy<Value = Rating> {
    prop::sample::select(RATINGS.to_vec())
}

fn phase() -> impl Strategy<Value = Phase> {
    prop_oneof![
        (0u32..4).prop_map(|step| Phase::Learning { step }),
        Just(Phase::Review),
        (0u32..3).prop_map(|step| Phase::Relearning { step }),
    ]
}

fn defaults() -> FsrsParameters {
    FsrsParameters::new(&DEFAULT_WEIGHTS, 0.9, 36_500).unwrap()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn every_review_stays_finite_and_in_bounds(
        params in parameters(),
        phase in phase(),
        stability in STABILITY_MIN..=40_000.0f64,
        difficulty in DIFFICULTY_MIN..=DIFFICULTY_MAX,
        has_memory in any::<bool>(),
        elapsed in prop::option::of(-3i64..=40_000),
        rating in rating(),
        fuzz in any::<bool>(),
        uniform in 0.0f64..1.0,
    ) {
        let config = SchedulerConfig {
            parameters: params.clone(),
            learning_steps_seconds: vec![60, 600],
            relearning_steps_seconds: vec![600],
            fuzz,
        };
        let input = ReviewInput {
            phase,
            memory: has_memory.then_some(MemoryState { stability, difficulty }),
            elapsed_days: if has_memory { elapsed } else { None },
        };
        let out = config.review(&input, rating, uniform);
        prop_assert!(out.memory.stability.is_finite());
        prop_assert!(out.memory.stability >= STABILITY_MIN);
        prop_assert!((DIFFICULTY_MIN..=DIFFICULTY_MAX).contains(&out.memory.difficulty));
        match out.interval {
            NextInterval::Days(days) => {
                prop_assert!(days >= 1);
                prop_assert!(days <= params.maximum_interval_days());
            }
            NextInterval::Seconds(seconds) => prop_assert!(seconds > 0),
        }
    }

    #[test]
    fn retrievability_is_a_probability_that_decays(
        params in parameters(),
        stability in STABILITY_MIN..=40_000.0f64,
        t1 in 0.0f64..10_000.0,
        dt in 0.0f64..10_000.0,
    ) {
        let r1 = params.retrievability(t1, stability);
        let r2 = params.retrievability(t1 + dt, stability);
        prop_assert!((0.0..=1.0).contains(&r1));
        prop_assert!(r2 <= r1);
    }

    #[test]
    fn intervals_grow_with_stability_and_respect_the_cap(
        params in parameters(),
        s1 in STABILITY_MIN..=100_000.0f64,
        ds in 0.0f64..=100_000.0,
    ) {
        let i1 = params.next_interval_days(s1);
        let i2 = params.next_interval_days(s1 + ds);
        prop_assert!(i1 <= i2);
        prop_assert!((1..=params.maximum_interval_days()).contains(&i2));
    }

    #[test]
    fn recall_never_lowers_stability_and_follows_the_rating_order(
        params in parameters(),
        stability in STABILITY_MIN..=40_000.0f64,
        difficulty in DIFFICULTY_MIN..=DIFFICULTY_MAX,
        retrievability in 0.0f64..=1.0,
    ) {
        let hard = params.recall_stability(difficulty, stability, retrievability, Rating::Hard);
        let good = params.recall_stability(difficulty, stability, retrievability, Rating::Good);
        let easy = params.recall_stability(difficulty, stability, retrievability, Rating::Easy);
        prop_assert!(hard >= stability && good >= stability && easy >= stability);
        prop_assert!(hard <= good && good <= easy);
        let again = params.forget_stability(difficulty, stability, retrievability);
        prop_assert!(again <= stability);
    }

    #[test]
    fn difficulty_moves_with_the_rating(
        params in parameters(),
        difficulty in DIFFICULTY_MIN..=DIFFICULTY_MAX,
    ) {
        let again = params.next_difficulty(difficulty, Rating::Again);
        let hard = params.next_difficulty(difficulty, Rating::Hard);
        let good = params.next_difficulty(difficulty, Rating::Good);
        let easy = params.next_difficulty(difficulty, Rating::Easy);
        prop_assert!(again >= hard && hard >= good && good >= easy);
    }

    #[test]
    fn higher_desired_retention_never_lengthens_intervals(
        w in weights(),
        stability in STABILITY_MIN..=40_000.0f64,
        r1 in 0.70f64..=0.99,
        dr in 0.0f64..=0.29,
    ) {
        let r2 = (r1 + dr).min(0.99);
        let low = FsrsParameters::new(&w, r1, 36_500).unwrap();
        let high = FsrsParameters::new(&w, r2, 36_500).unwrap();
        prop_assert!(high.next_interval_days(stability) <= low.next_interval_days(stability));
    }

    #[test]
    fn fuzz_stays_near_the_interval(
        params in parameters(),
        days in 1u32..=36_500,
        uniform in 0.0f64..1.0,
    ) {
        let days = days.min(params.maximum_interval_days());
        let fuzzed = params.fuzzed_interval_days(days, uniform);
        prop_assert!(fuzzed <= params.maximum_interval_days());
        if days < 3 {
            prop_assert_eq!(fuzzed, days);
        } else {
            let (low, high) = params.fuzz_range(days);
            prop_assert!(low <= days.max(low) && fuzzed >= low && fuzzed <= high + 1);
            prop_assert!(f64::from(fuzzed.abs_diff(days)) <= 2.0 + f64::from(days) * 0.1);
        }
    }
}

// KMP specification (kmp-fsrs-spec.json "properties"), at the default weights.

#[test]
fn kmp_initial_value_orderings_hold() {
    let p = defaults();
    let s: Vec<f64> = RATINGS.iter().map(|r| p.initial_stability(*r)).collect();
    assert!(s.windows(2).all(|w| w[0] < w[1]));
    let d: Vec<f64> = RATINGS.iter().map(|r| p.initial_difficulty(*r)).collect();
    assert!(d.windows(2).all(|w| w[0] > w[1]));
    assert!(d.iter().all(|d| (1.0..=10.0).contains(d)));
}

#[test]
fn kmp_difficulty_checks_hold() {
    let p = defaults();
    assert!((p.next_difficulty(5.0, Rating::Good) - 5.0).abs() < 0.5);
    assert!(p.next_difficulty(5.0, Rating::Again) > 5.0);
    assert!(p.next_difficulty(5.0, Rating::Easy) < 5.0);
    assert!(p.next_difficulty(1.0, Rating::Easy) >= 1.0);
    assert!(p.next_difficulty(10.0, Rating::Again) <= 10.0);
    assert!(
        (p.next_difficulty(9.9, Rating::Again) - 9.9).abs()
            < (p.next_difficulty(5.0, Rating::Again) - 5.0).abs()
    );
}

#[test]
fn kmp_stability_checks_hold() {
    let p = defaults();
    assert!(
        p.recall_stability(5.0, 20.0, 0.1, Rating::Good)
            > p.recall_stability(5.0, 20.0, 0.9, Rating::Good)
    );
    assert!(
        p.recall_stability(8.0, 10.0, 0.9, Rating::Good)
            < p.recall_stability(2.0, 10.0, 0.9, Rating::Good)
    );
    assert!(p.forget_stability(5.0, 20.0, 0.9) <= 20.0);
    assert!(p.next_stability(10.0, 0.1, 0.9, Rating::Again) >= STABILITY_MIN);
    assert!(p.forget_stability(5.0, 100.0, 0.5) > p.forget_stability(5.0, 10.0, 0.5));
    for rating in [Rating::Good, Rating::Easy] {
        assert!(p.short_term_stability(5.0, rating) >= 5.0);
    }
    assert!(p.short_term_stability(2.0, Rating::Easy) >= p.short_term_stability(2.0, Rating::Good));
    for rating in RATINGS {
        assert!(p.short_term_stability(0.1, rating) >= STABILITY_MIN);
    }
    let shorter = FsrsParameters::new(&DEFAULT_WEIGHTS, 0.95, 36_500).unwrap();
    assert!(shorter.next_interval_days(20.0) < p.next_interval_days(20.0));
}

#[test]
fn kmp_scheduler_transitions_hold_under_the_reference_policy() {
    let config = SchedulerConfig {
        parameters: defaults(),
        learning_steps_seconds: vec![60, 600],
        relearning_steps_seconds: vec![600],
        fuzz: false,
    };
    let new = ReviewInput {
        phase: Phase::Learning { step: 0 },
        memory: None,
        elapsed_days: None,
    };
    for rating in [Rating::Again, Rating::Hard, Rating::Good] {
        assert!(matches!(
            config.review(&new, rating, 0.0).phase,
            Phase::Learning { .. }
        ));
    }
    let easy = config.review(&new, Rating::Easy, 0.0);
    assert_eq!(easy.phase, Phase::Review);
    assert!(matches!(easy.interval, NextInterval::Days(days) if days >= 1));

    let review = ReviewInput {
        phase: Phase::Review,
        memory: Some(MemoryState {
            stability: 20.0,
            difficulty: 5.0,
        }),
        elapsed_days: Some(20),
    };
    let lapse = config.review(&review, Rating::Again, 0.0);
    assert_eq!(lapse.phase, Phase::Relearning { step: 0 });
    assert!(lapse.memory.stability < 20.0);
    let good = config.review(&review, Rating::Good, 0.0);
    assert!(matches!(good.interval, NextInterval::Days(days) if days >= 20));

    // Easy path ends with more stability than the Good path after two reviews.
    let two = |rating: Rating| {
        let first = config.review(&new, rating, 0.0);
        let second = config.review(
            &ReviewInput {
                phase: first.phase,
                memory: Some(first.memory),
                elapsed_days: Some(first.interval.seconds() / 86_400),
            },
            rating,
            0.0,
        );
        second.memory.stability
    };
    assert!(two(Rating::Easy) > two(Rating::Good));
}
