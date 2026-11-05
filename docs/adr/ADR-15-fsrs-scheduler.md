# ADR-15: FSRS-6 port, scheduler policy and study use cases

- Status: Proposed (2026-11-04); implemented in Phase 9. Acceptance after the Phase 10 device run (real ratings on Android).
- Plan references: Phase 9 steps 1–18 and DoD; §4.3 (session state machine); Phase 10 (wiring to the existing GUI)

## Context

Phase 10 connects the accepted study screens to real scheduling. Before that, scheduling must be shown correct on its own: the FSRS-6 memory model, the card state machine with learning steps, daily limits over a deck tree, sibling burying, leeches, study days in a time zone, and transactional rating with retry and undo. The KMP app used `f32`, whole-day arithmetic and a minimum stability of 0.1 (`tests/fixtures/fsrs/kmp-fsrs-spec.json`); its tests are a checklist, not numeric truth. A crate name ("fsrs") does not prove FSRS-6 behaviour, and the Rust `fsrs` crate also ships the optimizer (Burn), which v1 does not need on the device.

## Decision

### Reference and port (`crates/domain/src/fsrs.rs`)

1. **Own port of py-fsrs 6.3.0** (PyPI `fsrs==6.3.0`, MIT, wheel SHA-256 `deb2b613c917651a48ec90dfde3b1c8808d136f8ae78ffdcc00a5ddb216db78e`). Same formulas and operation order; `f64` throughout; `review_logs.fsrs_version = "fsrs-6/py-fsrs-6.3.0"`. No external crate: the scheduler is ~400 lines, needs no tensor library on mobile, and is checked against the reference numerically.
2. **Conformance vectors** come from the reference itself: `scripts/fsrs/generate_vectors.py` writes `tests/fixtures/fsrs/vectors-py-fsrs-6.3.0.json` (locked in `scripts/fixtures.lock.json`). 44 scenarios / 1 219 ratings: all-Good, all-Again, alternating, mature lapse, same-day reviews, 2-year gaps, Hard/Easy mixes, late reviews and 200 seeded early/on-time/late reviews, each with the default, an optimized, the lower-bound and the upper-bound weights; plus retention 0.70/0.99, maximum interval 30, one/three/no/day-long learning steps. Also 160 fuzz samples, 196 retrievability and 200 interval values.
3. **Tolerance.** S and D within 1e-9 relative + 1e-12 absolute (`powf` may differ from CPython's `pow` in the last ulp); state, step, interval and fuzz must match exactly. Intervals use Python's round-half-to-even.
4. **Parameters.** 21 weights within the reference bounds (`LOWER/UPPER_BOUNDS_PARAMETERS`, now also enforced by `DeckConfigOptions::validate`), retention 0.70–0.99, maximum interval 1–36 500 days. Minimum stability is the reference's 0.001, not KMP's 0.1. The defaults equal both the reference and the KMP specification. `parameters_hash` (32 hex, SHA-256 over the exact bits of weights, retention, maximum interval, steps and fuzz) is stored in every log.
5. **State machine** exactly as the reference: a new card is Learning step 0 without memory; same-day reviews (elapsed < 1 day) use the short-term formula (w17–w19); Hard on the first step waits (step₀ + step₁)/2, or 1.5 × step₀ with one step; Good on the last step and Easy graduate; Again in review enters relearning (or stays in review without relearning steps); a step beyond the current step list graduates. Default steps are the reference's: learning [1 m, 10 m], relearning [10 m].
6. **Fuzz** is the reference's range function with the uniform sample replaced by SHA-256(card GUID, card version, parameters hash). Preview and commit therefore compute the same interval; every committed write bumps the card version, so consecutive reviews get different samples. The plan named the operation id as a seed input, but the preview happens before an operation id exists; the version gives the same per-review variation and keeps preview = commit.

### Study days and due times (`calendar.rs`, `scheduling.rs`)

7. **Study day** = local date of (instant − cutoff) in the collection's IANA zone (`collection.time_zone`, `day_cutoff_minutes`, default 04:00). Days are calendar days: across DST a study day lasts 23 or 25 hours. A cutoff inside a DST gap starts the day at the first existing minute after it; an ambiguous cutoff at the first occurrence. An unknown stored zone falls back to UTC with a warning.
8. **Elapsed days** for FSRS = difference in study days (Anki's rule), not 24-hour blocks; a clock that moved backwards counts as the same day. The conformance harness feeds the reference's 24-hour arithmetic to the same core, so both are tested.
9. **Due times.** A review interval of *d* days makes the card due at the start of the study day *d* days after today; a learning step is due exactly *step* after the rating, in the day-learning queue when that is on a later study day. Instants stay UTC milliseconds, so travelling (a zone change) moves day labels, never due instants.

### Queue (`crates/domain/src/queue.rs`)

10. **Limits over the tree.** A deck's daily limits cap its cards and its subdecks'. A card is admitted only when every deck from its own up to the studied deck has room; admitting it uses room in all of them ("all decks" walks up to the top-level deck). Done-today counts come from committed, valid, non-imported logs: introduced new cards against the new limit, reviews of review or day-learning cards against the review limit. Learning steps are never limited. Each card has one deck, so "all decks" needs no dedupe.
11. **Order.** Intraday learning due now → reviews (by due time) mixed with new cards (deck tree order; by position, or a per-day permutation for `Random`) → learning cards due within 20 minutes when nothing else is left. A new card is shown after every *(reviews + new) / new − 1* reviews of what remains.
12. **Siblings, burying, suspension.** With `bury_siblings`, answering a card buries its new siblings and review/day-learning siblings due today until the next study day, and the queue gathers at most one card per note (also skipping notes rated today). Suspended, buried, archived and deleted cards are never candidates. Expired burials are lifted at session start and before counting.
13. **Leeches** follow Anki: at `leech_threshold` lapses and every half threshold after; action `Tag` adds the note tag `leech`, `Suspend` suspends the card.
14. **Nothing due.** The view reports which limit hides more cards; `extend_today_limits` raises one deck's limits for today only (setting `study.limitExtensions`). Custom-study/filtered decks stay out of scope (v1 non-goal).

### Use cases (`crates/application/src/study.rs`, schema v3)

15. **Session.** `start_study` finishes any open session and starts one for a deck, all decks, or chosen cards (a restricted session without limits, its learning cards come back). `resume_study`, `change_session` (pause/resume/end), `study_view`, `due_counts` (GetDue), `preview`, `rate_card` (ScheduleReview), `undo_review`, `extend_today_limits`, `session_stats` (GetStudyStats). A session with nothing left today, not even a learning card, finishes itself; undo reopens it.
16. **Config changes.** Ratings use the deck's current preset. The session records the preset id/version it started with (audit). The preview token is SHA-256 over card id + version, preset id + version, parameters hash, session and study day: a rating against an outdated preview fails with `PREVIEW_STALE` and the UI fetches a new preview — never a silent different interval.
17. **One transaction per rating.** Validation (operation id, session active and containing the card, card version, token) → FSRS → card + leech action + buried siblings + review log (full before/after snapshots, duration capped at 60 s) + undo entry + session item/active time/current card. Any failure rolls back everything (tested with a failing trigger on `review_logs`).
18. **Idempotency.** A rating is keyed by its unique `review_logs.operation_id`: a retry returns the first result (`replayed`), never a second rating; an id used for another card, session or operation is `VALIDATION_FAILED reused`. Start, undo and extension are recorded in `operations`.
19. **Undo** reverses only the latest rating of the session, once: it refuses when the card, a buried sibling or the leech-tagged note changed since (`VERSION_CONFLICT`), then restores the card's full snapshot, the siblings and the tag, invalidates the log (kept for audit, excluded from every count) and makes the card current again. Only audit metadata (version, `updated_at`, `usn`) differs afterwards.
20. **Schema v3** (`0003_scheduler.sql`): `study_sessions.restricted`, indexes for session logs, undo entries and buried cards. `review_undo_entries.sibling_changes` holds `{siblings: [{cardId, before, afterVersion}], leechTagged}`; v2's bare `[]` still reads.

## Consequences

- Rust is the only scheduler; the UI shows what `preview` returns and sends the token back. Phase 10 maps these results to the existing DTOs (`StudySessionDto`, `RatingPreviewDto`, `ReviewResultDto`, `SessionSummaryDto`) and replaces the dashboard counts with `due_counts` and a zone-aware streak; the dashboard already uses the zone for today's boundaries.
- Updating the reference means regenerating the vectors with the new pinned version, reviewing the diff and changing `FSRS_VERSION`; old logs keep their version and hash.
- The full queue for all decks takes ≈ 9 ms (one deck ≈ 2.4 ms) on the 50 000-card / 500 000-log collection (release, development PC, `cargo bench -p langualinker-infrastructure --bench due_query -- scheduler`); the device measurement is part of Phase 10.
- Not covered: the optimizer (Phase 18), "new cards ignore the review limit" and separate bury switches per card kind (one `bury_siblings` switch), user bury/suspend commands (the scheduler honours them; the actions arrive with the card actions in Phases 10–11).

## Verification

`cargo test -p langualinker-domain` (conformance 7, properties 11 × 512 cases plus the KMP checks, calendar/scheduling/queue units), `cargo test -p langualinker-infrastructure --test study` (16 integration tests: rating flow, retry, stale preview/version, rollback, undo with siblings and counters, undo conflict, leech tag/suspend, parent/child limits, suspended/buried, interleaving and overdue, chosen cards, limit extension, time zone, restart/pause/end, foreign/paused session, snapshots), migration fixture v2 → v3.
