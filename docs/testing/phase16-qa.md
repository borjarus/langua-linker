# Phase 16 — QA, security and data-loss report

Status of plan Phase 16 (instrumentation, QA, security, performance) as of 2026-11-17. What is automated runs in the normal test commands; what needs a device or a person is listed as open with the way to close it.

## Test layers (step 2)

| Layer | Where | What it covers |
|---|---|---|
| Rust unit | each crate `src/**` | FSRS (conformance vectors, properties), scheduler, queue, calendar, settings, stats, reminders planner, CSV/APKG codec, telemetry summary |
| Rust integration on real SQLite files | `crates/infrastructure/tests/*.rs` | study/undo/limits, management, content import, backup/restore/reset, transfer, LLM outbox and HTTP client, jobs, file logging |
| Property-based fuzz | `crates/infrastructure/tests/fuzz.rs`, `crates/domain/tests/fsrs_properties.rs`, CSV in `interop` | search language, backup archives, content archive, study state machine |
| Data-loss drill (real process kills) | `crates/infrastructure/tests/data_loss.rs` | session, import, migration, restore, disk full |
| Budgets (ignored, release) | `crates/infrastructure/tests/budgets.rs`, `management.rs`, `transfer.rs`, `benches/` | [performance-budgets.md](performance-budgets.md) |
| Shell IPC (mock runtime) | `src-tauri/src/lib.rs` tests | every catalogue command answers, capabilities, onboarding → 20 ratings → restart, settings/backup/restore/reset, AI, metrics consent, release security surface |
| Contract | `crates/contracts/tests`, `frontend/tests/contract*.test.ts` | frozen v1 catalogue and samples on both sides, adapter conformance |
| Vitest + Testing Library | `frontend/tests/*.test.ts` | components, controllers (rerender, cleanup), router, i18n, platform/metrics bridges, renderer and deep-link fuzz |
| Browser E2E (Playwright, demo adapter) | `frontend/e2e/*.spec.ts` | flows and layout on the mock; axe accessibility checks |
| Device acceptance | `docs/release/*.md` | Android physical runs (Phases 10, 15); iOS simulator |

Packaged IPC E2E and native harnesses (step 7–8: WebDriver for the Tauri app, UIAutomator/XCTest) are **not** set up: Playwright on the demo adapter does not prove the WebView/IPC path, the shell IPC tests prove the command layer on the mock runtime, and the device runs are manual. Open.

## Fuzzing (step 10) and what it found

| Target | Property | Cases |
|---|---|---|
| Search language (`parse_query`, `fts_query`) | parses or is a validation error; FTS expression well-formed; odd words run on the real FTS5 index | 2 000 + fixed list |
| Backup archive (`validate_file`) | flips/truncation/insertion/appending are refused with a typed error; a changed database part never validates; random bytes never validate | 96 + 96 |
| Content archive (`unpack`) | damaged or foreign archives fail cleanly; no path-like names | 24 + 48 |
| Study session state machine | random rate/retry/undo/pause/resume/wait: session count = valid logs, every card's `reps` = its valid logs, undo restores the exact card state, a retried rating changes nothing | 64 (1 000 in the long run) |
| Inline Markdown renderer (TS) | never throws, runs ordered and in range, only marker characters dropped | 3 000 |
| Deep links and hash routes (TS) | only `langualinker://app/<allowed route>`; no query, fragment, `..` or foreign scheme in the result | 3 000 + 3 000 |

Findings, both fixed in this phase:

1. **Undo restored FSRS stability off by one ULP.** Review-log snapshots are JSON; `serde_json` without `float_roundtrip` parses some `f64` values to a neighbouring double. Undo is promised to restore the exact snapshot. Fix: `float_roundtrip` on the workspace `serde_json`.
2. **A search word containing NUL failed with `INTERNAL`.** FTS5 ends a string literal at NUL ("unterminated string"). Fix: `fts_query` turns control characters into spaces (unit test added).

Proptest stores the seeds of failures in `crates/infrastructure/tests/fuzz.proptest-regressions` (checked in; they run first).

## Data-loss drill (step 20, DoD)

The test binary starts itself again as a child process and the parent ends it with `TerminateProcess`/`SIGKILL` (or the child aborts at an exact point). The parent then reopens the file and checks `PRAGMA integrity_check`, that every rating the child reported as committed is still there (same log rows), that each card's `reps` equals its valid log, and that the interrupted operation is all-or-nothing. Desktop run, 2026-11-17:

| Operation | Fault | Result |
|---|---|---|
| Study session | killed after 1, 2, 3, 5, 8 reported ratings | all reported ratings present; no half-applied rating |
| CSV import (3k rows debug / 20k release) | killed 0, 30, 120, 400, 1 500 ms after the job started | 4 × nothing imported, 1 × everything; ratings unchanged |
| Migration v3 → v4 | process aborts inside the migration transaction after its statements | schema stays v3, data identical, safety copy present; next start migrates and keeps the ratings |
| Restore of an 8k-card backup | killed 0, 100, 400, 1 000, 3 000 ms after start | 2 × old collection intact, 3 × restored collection; never mixed |
| Disk full during import | `max_page_count` cap on the worker connection | job fails with `STORAGE_FULL`; collection identical |
| Corrupt backup | fuzz above + `backup.rs` | refused before anything is replaced |

Open: the same drill on a phone (Android low-memory kill, iOS jetsam) and a real full device storage.

## Security audit (step 18)

| Area | Finding | Status |
|---|---|---|
| CSP | `default-src 'self'`; no `script-src` override (no inline, no eval, no CDN); `connect-src ipc: http://ipc.localhost` only; `img-src 'self' asset:`; `style-src 'self' 'unsafe-inline'` (Svelte style attributes) | ✅ enforced by `release_security_surface_is_minimal` |
| Capabilities | release: `default` (exactly the contract catalogue + `core:event:default`), `platform` (speech, launch URL), `metrics` (3 commands); no fs/shell/http/dialog/notification permissions in the WebView. Diagnostics probes only in dev | ✅ tests |
| Custom commands | every input parsed with `deny_unknown_fields` DTOs; ids validated; paths never come from the WebView (picker tokens) | ✅ (Phase 7–15 tests) |
| Network endpoints | only Rust talks to the network: the AI provider over rustls with the Mozilla roots; OpenAI-compatible endpoints must be HTTPS | ✅ (ADR-18) |
| HTML / rendering | card text is rendered as Svelte text nodes (inline Markdown → styled runs, no HTML) | ✅ fuzzed |
| Import | CSV row/column/field limits and formula guard; APKG import not shipped (v1.1); content archive names checked | ✅ |
| Logs and metrics | field allow-list redaction; metrics are typed numbers; WebView errors cross IPC as a kind only | ✅ tests |
| Devtools / debug | `tauri` built without `devtools`; no window `devtools`; release frontend bundle checked by `verify-release-bundle.mjs` (no demo adapter, gallery, diagnostics) | ✅ |
| Rust dependencies | `cargo audit` / `cargo deny` are not installed in this environment | ⬜ run in CI (`cargo install cargo-audit && cargo audit`) |
| npm dependencies | `npm audit --omit=dev`: 0 vulnerabilities. Dev tooling: 10 moderate (vite dev-server `fs.deny` bypass, vitest mocker, minimatch ReDoS via typescript-eslint) — not shipped | ⬜ upgrade dev tooling |
| Licences | attributions screen (Phase 5/8) lists bundled content and libraries | ✅ generated Rust/JS/native licence report with allowlist, checked in CI (Phase 17, ADR-21) |

## Release contents (step 21)

- Demo adapter, fixtures, gallery, diagnostics screen and probe commands are absent from `dist-release` (`npm run verify:release`).
- `diagnostics` capability only in `tauri.dev.conf.json`; probe commands also refuse to run outside the dev variant.
- The content password is compiled in from `.env`/environment, never from the repository.
- Every release screen uses the real `AppApi` (`adapterSwap.test.ts`); the demo runs only in the browser build.

## Accessibility and localisation (step 19)

Automated: axe checks in Playwright, i18n lint (no literal UI strings), PL/EN key parity and plurals, 200 % text scale screens in the UI matrix ([ui-test-matrix.md](ui-test-matrix.md)). Open for a person on a device: long German words at 200 %, orientation change, safe areas and the software keyboard, TalkBack/VoiceOver pass, contrast in both themes. RTL is not a v1 language; layout uses logical properties.

## Svelte checks (step 11)

`svelte-check --fail-on-warnings` is clean; controllers are tested for rerender and cleanup (`*Controllers.svelte.test.ts`, `query.svelte.test.ts`); new global listeners (`watchClientErrors`, launch links, visibility) return their cleanup and are removed when the root unmounts.

## Open items for the DoD

- Budgets on Galaxy A56 and an iPhone; size/RSS/battery limits into an ADR.
- Drill on phones; beta metric counting and crash symbols with D-08.
- Packaged-app E2E and native harnesses; TalkBack/VoiceOver pass.
- `cargo audit` in CI; dev-tool upgrades; licence report.
