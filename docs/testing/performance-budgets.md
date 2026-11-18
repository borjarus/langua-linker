# Performance budgets (plan Phase 16 step 3)

Budgets are goals per device class. A row is green only when measured on that device with a release build; deviations need the owner's approval (DoD "budgets measured per device").

## How to measure

| Budget | Command (release build) | Source |
|---|---|---|
| Due queue p95 < 50 ms (50k cards, 500k logs) | `cargo test --release -p langualinker-infrastructure --test budgets due_queue -- --ignored --nocapture` | `crates/infrastructure/tests/budgets.rs` |
| Backend share of cold start (open + migration check + first due counts) p95 < 500 ms | `… --test budgets backend_cold_open -- --ignored --nocapture` | same |
| CSV import 5k rows < 5 s (preview + import) | `… --test budgets csv_import -- --ignored --nocapture` | same |
| Search p95 < 200 ms (50k cards) | `… --test management browser_search_p95 -- --ignored --nocapture` | `crates/infrastructure/tests/management.rs` |
| 20k import / CSV / APKG export | `… --test transfer twenty` | `crates/infrastructure/tests/transfer.rs` |
| Optimizer: history read of one preset (500k logs) < 1 s, training 50k cards / 450k answers < 60 s (ADR-22) | `… --test budgets optimizer -- --ignored --nocapture` | `crates/infrastructure/tests/budgets.rs` |
| Due queue micro-benchmark | `cargo bench -p langualinker-infrastructure --bench due_query` | `crates/infrastructure/benches/due_query.rs` |
| Cold start median < 2 s (whole app) | opt in on the device, start the app ≥ 5 times from killed, read *Privacy → Collected so far* | `report_startup` (ADR-13) |
| Generation p90 < 4 s | same summary after ≥ 10 AI calls | LLM service metrics (ADR-13) |

The fixture is `sqlite::seed::SeedSize::PLAN`: 10 decks, 50 000 cards, 500 000 review logs, in a WAL file on disk (not in memory).

## Results

### Desktop reference — 2026-11-17

Intel Core i7-1355U (13th gen, laptop), 40 GB RAM, Windows 11, NVMe; rustc 1.90.0 release profile, bundled SQLite; collection file on the repository drive.

| Budget | Result | Status |
|---|---|---|
| Due card ids (200), p95 | 16.1 ms (p50 0.6 ms, max 19.6 ms, n 60) | ✅ < 50 ms |
| Scheduler due counts (limits, tree, done-today), p95 | 13.5 ms (p50 10.0 ms, n 60) | ✅ < 50 ms |
| Open + first due counts, p95 | 54 ms (p50 26 ms, n 10) | ✅ < 500 ms |
| CSV import 5 000 rows, preview + import | 0.79 s (preview 12 ms) | ✅ < 5 s |
| Search over 50k cards, p95 | 179 ms (p50 72 ms, max 268 ms, n 70) | ✅ < 200 ms (tight) |
| 20k rows (Phase 14a) | preview 65 ms, import 3.5 s, CSV export 0.5 s, APKG export 1.1 s | — |
| Optimizer (Phase 18, 2026-11-18): history read / training | 0.73 s (500 000 logs) / 1.0 s (450 000 answers, 3 511 steps) | ✅ < 1 s / < 60 s |
| Frontend release bundle | 1.3 MB on disk: JS 581 kB, CSS 58 kB (uncompressed) | baseline |

Search p95 is the closest to its budget; the slow cases are prefix searches sorted by front over all 50k cards. A phone will likely miss it — first candidate for work if the device run confirms (an FTS-first plan with a covering index on the sort key).

### Phones — pending

| Device | Build | Due p95 | Search p95 | Import 5k | Cold start median | Scroll | Battery 30 min |
|---|---|---|---|---|---|---|---|
| Galaxy A56 (Android 17) | beta release | — | — | — | — | — | — |
| iPhone (iOS 18+) | — | — | — | — | — | — | — |

The Rust budgets run on a phone through the test binaries (`cargo ndk … test --no-run`, push with `adb`, run with `--ignored --nocapture`); the app-level numbers come from the metrics summary.

## Limits not yet measured (step 15–17)

Proposed limits for the ADR once the phone baseline exists: AAB ≤ 40 MB download, IPA ≤ 60 MB; RSS during study ≤ 250 MB, peak during a 20k import ≤ 400 MB; scroll ≥ 55 fps p95 frame time ≤ 18 ms in the browser on 50k rows; 30-minute session ≤ 6 % battery without TTS, ≤ 8 % with TTS, no thermal throttling. These are proposals, not results.
