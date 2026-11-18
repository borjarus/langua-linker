# ADR-22: Local FSRS-6 optimizer

- Status: Proposed (2026-11-18); implemented in Phase 18 steps 1–2. Acceptance after the owner accepts the optimizer screen and a run on a real collection on the Galaxy A56 (time, memory, held-out result).
- Plan references: Phase 18 steps 1–7 and DoD; ADR-15 (FSRS-6 port, parameters hash), ADR-16 (learning flow), contract v1.5 ([contracts README](../contracts/README.md))

## Context

Presets carry 21 FSRS-6 weights; until now every learner used the defaults (or typed weights by hand). Fitting the weights to the learner's own answers is the main quality lever of FSRS. The plan requires the history to stay on the device, a held-out check without leakage, a bounded and cancellable job, a preview of the effect, a way back, and no rescheduling without consent. ADR-15 kept the scheduler free of a tensor library; the reference optimizer (`fsrs-rs`, BSD-3) is built on Burn, which would add a large dependency tree and native build risk on Android/iOS for one screen.

## Decision

### Algorithm (`crates/domain/src/optimizer.rs`)

1. **Own optimizer, same model as the scheduler.** The FSRS-6 memory model is written once over a generic number type: `f64` reproduces `SchedulerConfig::review` bit for bit (test `replay_matches_the_scheduler`), forward-mode dual numbers with 21 tangents give exact gradients (test against central finite differences for every weight). No new dependency. Version string `langualinker-fsrs6-adam-1`, stored with every run next to `FSRS_VERSION`.
2. **Loss.** Binary cross-entropy of the predicted recall against "not Again" on every answer given on a later study day than the previous one (same-day answers drive the short-term formula w17–w19 but are not predicted, as in the reference trainers). Elapsed days use the scheduler's rule: study days in the collection's calendar (ADR-15 §8).
3. **Training.** Starts from the default weights; Adam (β 0.9/0.999), learning rate 0.04 with cosine decay, batches of ≥ 512 answers (whole cards), at least 5 epochs and at least 200 steps (small datasets get more epochs, at most 200); w0–w3 (initial stabilities, range 0.001–100) are optimised in log space so one learning rate fits all weights; an L2 pull towards the defaults with strength 1 / training answers (log space for w0–w3, a quarter of the bound range elsewhere) keeps small datasets near the defaults; after every step the weights are clamped to the reference bounds. Deterministic: fixed shuffle seed, same dataset → same weights.
4. **Held-out validation without leakage.** Whole cards are split by a fixed hash of the card id (1 in 5 held out), so no card contributes answers to both sides. The proposal and the preset's current weights are both evaluated on the held-out cards: mean log loss and a calibration RMSE (20 bins of the prediction). The proposal is **recommended** only when its held-out log loss is lower by at least 1e-4.
5. **Effect preview.** For every card of the preset the memory state is replayed from its answers under each weight vector and the next interval computed at the preset's retention and maximum interval: estimated reviews per day (Σ 1/interval) and the median interval, current vs proposed. Nothing is written.
6. **Thresholds.** Below 200 usable answers the optimizer refuses (`VALIDATION_FAILED { field: "presetId", reason: "notEnoughReviews" }`); below 1 000 the result is marked `fewReviews` and the screen says so. 1 000 is a UX heuristic (plan step 2.4), not an FSRS requirement — the held-out comparison is what protects the learner.

### Dataset (plan steps 2.5–2.6)

7. Per preset: every log of the live cards in live decks that use the preset, ordered by card and time. **Excluded:** undone ratings (`invalidated_at`, kept in the log), manual rows (`kind = Manual`, imported "set due"/"forget"), and whole cards whose first remaining answer is not a learning answer (history imported mid-way — the memory state cannot be reconstructed). Counts of each exclusion are shown. A clock moved backwards counts as the same day.
8. **Parameters do not split the dataset.** Answers scheduled under other weights (another `parameters_hash`) or imported (`fsrs_version = "import"`) stay in: a weight change does not make an answer less true. Provenance is recorded instead: optimizer and FSRS version and a 32-hex fingerprint of the exact histories used (`dataset_hash`).

### Persistence and use cases (schema v6, `crates/application/src/optimizer.rs`)

9. **`optimizer_runs`** (`0006_optimizer_runs.sql`): preset, status (`proposed`/`applied`/`reverted`), versions, dataset hash, `weights_before` (the preset's weights at the start), `weights_proposed`, a JSON report (dataset counts, training/validation sizes, current/proposed metrics, recommended, fewReviews), created/applied/reverted times. Review logs are never written by the optimizer. Runs survive a progress reset (they hold no answers) and travel with backups.
10. **Job.** `startOptimizer` reads the dataset, refuses below the minimum before a job exists, then starts a job of kind `optimize`; the work persists progress at most ~50 times, checks for a cancel after every step and stores the run only after `begin_commit` — a cancelled run stores nothing. A second start for a preset with a run in progress returns that job. `getOptimizer` names the running job so a screen opened later attaches to it.
11. **Apply / revert with consent.** Both take the preset version the screen showed (`VERSION_CONFLICT` otherwise) and are idempotent by `operationId`. Apply needs status `proposed` and the preset still at `weights_before` (else `VALIDATION_FAILED runId stale|status`); it writes only the weights (a new `parameters_hash` from the next rating) and marks the run applied. Revert needs status `applied` and the preset still at `weights_proposed`, and restores `weights_before` exactly. **No card is rescheduled**: due dates and stored memory states stay; the new weights act from each card's next answer. `collection_changed { decks, cards }` follows.

### GUI first (plan step 1)

12. The screen `/preset/:id/optimizer` (entry from the deck screen and the preset editor) was built against the demo adapter first: dataset and exclusions, start/progress/cancel, held-out comparison and workload preview, weights before/proposed, apply, restore previous weights, run history. The demo adapter follows the same rules (contract samples are recorded from it and replayed through the Rust DTOs).

## Consequences

- Desktop reference (release, development PC): reading 500 000 logs of one preset 0.73 s; training on 50 000 cards / 450 000 answers 1.0 s (5 epochs, 3 511 steps) — `cargo test --release -p langualinker-infrastructure --test budgets optimizer -- --ignored --nocapture`. Memory is the dataset (≈ 16 bytes per answer) plus constant model state. A phone measurement is open.
- Our numbers are not comparable to `fsrs-rs` results bit for bit (different pre-training and regularisation); quality is checked by recovery tests (answers simulated from known weights: the proposal's held-out loss comes within a quarter of the gap between the defaults and the true weights) and by the held-out comparison on every real run.
- Changing the method means a new `OPTIMIZER_VERSION`; old runs keep theirs.
- Not covered: per-deck (instead of per-preset) optimisation, optimising the desired retention, automatic periodic runs, recomputing memory states or rescheduling after apply (would need an explicit consent step and a preview of due-date changes).

## Verification

`cargo test -p langualinker-domain optimizer` (10: scheduler equivalence, gradients, filters, split, refusal, recovery, already-fitted, determinism/cancel, workload), `cargo test -p langualinker-infrastructure --test optimizer` (4: dataset with exclusions, job → proposed → apply → retry → revert with history untouched, stale/unknown run, refusal below the minimum, cancel stores nothing and a running job is shared), shell test `optimizer_commands_answer_in_the_contract_shapes`, `contract_v1.rs` `phase18_samples_round_trip_through_their_rust_dtos`, Vitest `optimizer.test.ts`, migration v5 → v6.
