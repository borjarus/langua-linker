"""Generates FSRS-6 conformance vectors from the pinned reference implementation.

Reference: py-fsrs 6.3.0 (PyPI `fsrs==6.3.0`, MIT), wheel SHA-256 below. Run it in a throwaway
virtual environment, never from inside this repository's directories:

    python -m venv /tmp/fsrsenv
    /tmp/fsrsenv/bin/python -m pip install fsrs==6.3.0
    /tmp/fsrsenv/bin/python -I scripts/fsrs/generate_vectors.py > tests/fixtures/fsrs/vectors-py-fsrs-6.3.0.json
    node scripts/verify-fixtures.mjs --update

The output is deterministic: review times and ratings come from a seeded generator and fuzzing is
disabled in the scenarios; fuzz itself is sampled with a patched uniform source. Rust replays the
file in `crates/domain/tests/fsrs_conformance.rs` (plan Phase 9 step 2, ADR-15).
"""

import json
import random
import sys
from datetime import datetime, timedelta, timezone

import fsrs
from fsrs import Card, Rating, Scheduler, State
from fsrs import scheduler as scheduler_module

REFERENCE = {
    "package": "fsrs",
    "version": "6.3.0",
    "wheel": "fsrs-6.3.0-py3-none-any.whl",
    "wheelSha256": "deb2b613c917651a48ec90dfde3b1c8808d136f8ae78ffdcc00a5ddb216db78e",
    "license": "MIT",
    "source": "https://github.com/open-spaced-repetition/py-fsrs",
}

PARAMETER_SETS = {
    "default": list(scheduler_module.DEFAULT_PARAMETERS),
    # A realistic optimizer output (within the reference bounds), not the defaults.
    "optimized": [
        0.4072, 1.1829, 3.1262, 15.4722, 7.2102, 0.5316, 1.0651, 0.0234, 1.616, 0.1544,
        1.0824, 1.9813, 0.0953, 0.2975, 2.2042, 0.2407, 2.9466, 0.5034, 0.6567, 0.0773, 0.2,
    ],
    "lowerBounds": list(scheduler_module.LOWER_BOUNDS_PARAMETERS),
    "upperBounds": list(scheduler_module.UPPER_BOUNDS_PARAMETERS),
}

START = datetime(2026, 1, 5, 12, 0, tzinfo=timezone.utc)
STEPS_DEFAULT = [60, 600]
RELEARN_DEFAULT = [600]
STATE_NAMES = {State.Learning: "Learning", State.Review: "Review", State.Relearning: "Relearning"}


def scheduler(parameters, retention=0.9, maximum=36500, learning=None, relearning=None):
    return Scheduler(
        parameters=parameters,
        desired_retention=retention,
        learning_steps=[timedelta(seconds=s) for s in (STEPS_DEFAULT if learning is None else learning)],
        relearning_steps=[timedelta(seconds=s) for s in (RELEARN_DEFAULT if relearning is None else relearning)],
        maximum_interval=maximum,
        enable_fuzzing=False,
    )


def run(sched, plan):
    """`plan` yields (rating, seconds after the due time or None for an absolute offset)."""
    card = Card(card_id=1, due=START)
    steps = []
    now = START
    for rating, timing in plan(card_state=lambda: card):
        kind, value = timing
        if kind == "afterDue":
            now = card.due + timedelta(seconds=value)
        elif kind == "afterLast":
            now = (card.last_review or START) + timedelta(seconds=value)
        else:
            raise ValueError(kind)
        card, _ = sched.review_card(card, Rating(rating), review_datetime=now)
        interval = card.due - now
        steps.append({
            "atSeconds": int((now - START).total_seconds()),
            "rating": rating,
            "expect": {
                "state": STATE_NAMES[card.state],
                "step": card.step,
                "stability": card.stability,
                "difficulty": card.difficulty,
                "intervalSeconds": int(interval.total_seconds()),
            },
        })
    return steps


def fixed(ratings, delay=0):
    def plan(card_state):
        for rating in ratings:
            yield rating, ("afterDue", delay)
    return plan


def same_day(card_state):
    yield 3, ("afterDue", 0)          # New → step 1 (10 min)
    yield 3, ("afterDue", 0)          # graduate
    yield 2, ("afterLast", 2 * 3600)  # Review, same day: short-term stability
    yield 3, ("afterLast", 3600)
    yield 4, ("afterLast", 1800)
    yield 1, ("afterDue", 0)          # lapse → relearning
    yield 1, ("afterLast", 300)       # relearning, same day
    yield 2, ("afterLast", 120)
    yield 3, ("afterDue", 0)
    yield 3, ("afterDue", 0)


def two_year_gap(card_state):
    for _ in range(6):
        yield 3, ("afterDue", 0)
    yield 3, ("afterLast", 730 * 86400)
    yield 3, ("afterDue", 0)
    yield 1, ("afterLast", 730 * 86400)
    yield 3, ("afterDue", 0)


def seeded(count, seed):
    rng = random.Random(seed)

    def plan(card_state):
        for _ in range(count):
            rating = rng.choices([1, 2, 3, 4], weights=[15, 15, 55, 15])[0]
            card = card_state()
            if card.state == State.Review and card.last_review is not None:
                interval = (card.due - card.last_review).total_seconds()
                # Early, on time and late reviews, in whole minutes.
                offset = int(rng.uniform(-0.4, 1.5) * interval) // 60 * 60
                yield rating, ("afterDue", offset)
            else:
                yield rating, ("afterDue", rng.randrange(0, 3 * 3600, 60))
    return plan


def mature_lapse(card_state):
    for _ in range(8):
        yield 3, ("afterDue", 0)
    yield 1, ("afterDue", 0)
    for _ in range(4):
        yield 3, ("afterDue", 0)


def scenarios():
    result = []

    def add(name, parameters, plan, **options):
        sched = scheduler(PARAMETER_SETS[parameters], **options)
        result.append({
            "id": f"{name}/{parameters}",
            "parameters": parameters,
            "desiredRetention": sched.desired_retention,
            "maximumIntervalDays": sched.maximum_interval,
            "learningStepsSeconds": [int(s.total_seconds()) for s in sched.learning_steps],
            "relearningStepsSeconds": [int(s.total_seconds()) for s in sched.relearning_steps],
            "reviews": run(sched, plan),
        })

    for parameters in PARAMETER_SETS:
        add("all-good", parameters, fixed([3] * 14))
        add("all-again", parameters, fixed([1] * 10))
        add("alternating", parameters, fixed([1, 3] * 8))
        add("mature-lapse", parameters, mature_lapse)
        add("same-day", parameters, same_day)
        add("two-year-gap", parameters, two_year_gap)
        add("hard-and-easy", parameters, fixed([2, 2, 3, 4, 2, 4, 1, 2, 3, 4]))
        add("late-reviews", parameters, fixed([3, 3, 3, 2, 3, 4, 3], delay=5 * 86400))
        # The cap keeps 200 reviews of the upper-bound set inside datetime range.
        add("seeded-200", parameters, seeded(200, 20261104), maximum=1000)
    add("easy-first", "default", fixed([4, 3, 3]))
    add("retention-0.70", "default", fixed([3] * 10), retention=0.70)
    add("retention-0.99", "default", fixed([3] * 10), retention=0.99)
    add("max-interval-30", "default", fixed([3, 3, 4, 4, 4, 4, 4, 4]), maximum=30)
    add("single-learning-step", "default", fixed([2, 2, 3, 1, 2, 3]), learning=[600], relearning=[600])
    add("three-steps", "default", fixed([3, 2, 1, 3, 3, 3, 3, 1, 3, 3]), learning=[60, 600, 3600], relearning=[300, 1800])
    add("no-steps", "default", fixed([1, 3, 1, 1, 3, 4]), learning=[], relearning=[])
    add("day-steps", "default", fixed([3, 3, 3, 1, 3, 3]), learning=[600, 86400], relearning=[86400 * 2])
    return result


def fuzz_vectors():
    vectors = []
    original = scheduler_module.random
    try:
        for maximum in (36500, 30):
            sched = Scheduler(maximum_interval=maximum, enable_fuzzing=True)
            for days in (1, 2, 3, 4, 5, 7, 10, 15, 20, 21, 30, 45, 100, 365, 1000, 36500):
                for uniform in (0.0, 0.2, 0.5, 0.8, 0.999999):
                    scheduler_module.random = lambda u=uniform: u
                    fuzzed = sched._get_fuzzed_interval(interval=timedelta(days=days))
                    vectors.append({
                        "maximumIntervalDays": maximum,
                        "intervalDays": days,
                        "uniform": uniform,
                        "fuzzedDays": fuzzed.days,
                    })
    finally:
        scheduler_module.random = original
    return vectors


def retrievability_vectors():
    vectors = []
    for name, parameters in PARAMETER_SETS.items():
        sched = scheduler(parameters)
        for stability in (0.001, 0.5, 1.0, 3.7, 10.0, 100.0, 36500.0):
            for elapsed in (0, 1, 2, 5, 10, 100, 1000):
                card = Card(card_id=1, state=State.Review, step=None, stability=stability,
                            difficulty=5.0, due=START, last_review=START)
                vectors.append({
                    "parameters": name,
                    "stability": stability,
                    "elapsedDays": elapsed,
                    "retrievability": sched.get_card_retrievability(card, START + timedelta(days=elapsed)),
                })
    return vectors


def interval_vectors():
    vectors = []
    for name, parameters in PARAMETER_SETS.items():
        for retention in (0.7, 0.8, 0.9, 0.95, 0.99):
            sched = scheduler(parameters, retention=retention)
            for stability in (0.001, 0.4, 0.5, 1.0, 2.5, 3.5, 10.0, 99.5, 1000.0, 100000.0):
                vectors.append({
                    "parameters": name,
                    "desiredRetention": retention,
                    "stability": stability,
                    "intervalDays": sched._next_interval(stability=stability),
                })
    return vectors


def main():
    from importlib.metadata import version

    if version("fsrs") != REFERENCE["version"]:
        raise SystemExit(f"expected fsrs {REFERENCE['version']}, found {version('fsrs')}")
    document = {
        "reference": REFERENCE,
        "start": START.isoformat().replace("+00:00", "Z"),
        "notes": [
            "Each review happens at `atSeconds` after `start` (UTC); elapsed days are whole 24-hour "
            "periods since the previous review, as in the reference.",
            "intervalSeconds is due - review time; learning steps are seconds, review intervals whole days.",
            "Fuzzing is disabled in scenarios; `fuzz` samples the fuzz function with a fixed uniform value.",
        ],
        "parameterSets": PARAMETER_SETS,
        "scenarios": scenarios(),
        "fuzz": fuzz_vectors(),
        "retrievability": retrievability_vectors(),
        "intervals": interval_vectors(),
    }
    json.dump(document, sys.stdout, indent=1)
    sys.stdout.write("\n")


if __name__ == "__main__":
    main()
