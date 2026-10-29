# Phase 6 — GUI acceptance gate

The gate decides whether Phase 7 (Rust core, SQLite, IPC) may start. It accepts the interface and contract v1.0 on demo data — **a GUI milestone, not a working domain**: no FSRS, no SQLite, nothing stored durably.

Opened 2026-10-26 at commit `71c68cc` + Phase 6 changes. Status: **open — automated part complete; product-owner review (step 2) and the physical-device run (step 7) pending.**

**Owner decision 2026-10-29:** no iPhone is available for testing, so step 7 runs on a **physical Android device** and on the **iOS simulator** only. The physical iPhone run is carried over (see [Carried over: physical iPhone](#carried-over-physical-iphone)). Android guide: [android-device-testing.md](android-device-testing.md).

## Steps

| # | Step | Status | Evidence |
|---|---|---|---|
| 1 | Routes/dialogs/CTAs/keys/Back reviewed; placeholders removed | ☑ | `Screen` lost its unused phase placeholder; prototype notices render only with demo services; two mock-specific strings made neutral. [tests/guiAudit.test.ts](../../frontend/tests/guiAudit.test.ts): every production route has a UI entry point. Every route renders ([tests/routes-render.test.ts](../../frontend/tests/routes-render.test.ts)), Back/dialog layers ([tests/router.svelte.test.ts](../../frontend/tests/router.svelte.test.ts), [e2e/gallery.spec.ts](../../frontend/e2e/gallery.spec.ts)). Deferred by design: real file picker, share sheet, notifications, SecretStore (Phases 12–15). |
| 2 | Product-owner review of appearance, behaviour and data; prioritized blocker backlog | ☐ owner | Procedure and backlog below; decision recorded in [Owner acceptance](#owner-acceptance). |
| 3 | Scenarios: new user, large collection, nothing due, offline, save error, ambiguous, reset, restore | ☑ | [e2e/acceptance.spec.ts](../../frontend/e2e/acceptance.spec.ts) in Chromium and WebKit, 16/16, with the rule-4 layout check after each scenario. The save error is reached with the new banner control "Fail the next save". |
| 4 | Contract v1 frozen, with versioning | ☑ | `API_VERSION` 1.0 (TS and Rust); [crates/contracts/v1/catalogue.json](../../crates/contracts/v1/catalogue.json), [samples.json](../../crates/contracts/v1/samples.json) (70 calls, every callable operation, 2 error payloads, both events); rules in [contracts/README.md](../contracts/README.md#versioning-v1-and-later). |
| 5 | Dual-adapter contract tests; fake mappings in Rust | ☑ | [tests/contractV1.test.ts](../../frontend/tests/contractV1.test.ts): same samples replayed on `MockAppApi` and on `TauriAppApi` over a frozen-payload backend (exact `{ input }` per command, events unchanged). [crates/contracts/tests/contract_v1.rs](../../crates/contracts/tests/contract_v1.rs): Rust reads the same files (version, error codes, catalogue, coverage, transport rules, `ErrorDto`/`AppInfoDto` round trip); `src-tauri` checks the real `get_app_info` against its sample. Per-command Rust DTO checks are added as the DTOs land (checklist rule 2). |
| 6 | Per-screen integration checklist; no silent mock fallback | ☑ draft · ☐ owner approval | [contracts/integration-checklist.md](../contracts/integration-checklist.md). [tests/adapterSwap.test.ts](../../frontend/tests/adapterSwap.test.ts): every screen runs on the production adapter without demo services and without component changes. |
| 7 | Physical TalkBack/VoiceOver/WebView/orientation/keyboard-resize review | ☐ devices | Protocol below: physical Android device + iOS simulator (owner decision 2026-10-29). Android emulator results do not close it. |
| 8 | Screenshot baseline / demo of the whole application | ◐ | [e2e/tour.spec.ts](../../frontend/e2e/tour.spec.ts): 15 screens plus study → summary, PL, light and dark, 360 px, both engines (64 passed; pixels deterministic over two local runs). Baselines are generated on Linux CI only: run the **E2E baselines** workflow, review the artifact, commit `frontend/e2e/__snapshots__/` (also seeds the gallery baselines, never committed so far). |
| 9 | svelte-check/lint/reactivity clean; no React dependencies | ☑ | svelte-check 0 errors / 0 warnings, ESLint, `lint:i18n`, Vitest 293 tests, `verify:beta`/`verify:release`; [tests/guiAudit.test.ts](../../frontend/tests/guiAudit.test.ts) fails on React packages or imports, `$:`, `export let`, `on:` directives, `<slot>`, `createEventDispatcher`, or Tauri imports outside `src/api`/`src/services`. |

## DoD — condition to start Phase 7

- [x] All screens and critical flows are complete on mocks (Phases 3–5 suites, step 3 scenarios).
- [ ] No blocking UX/accessibility issues; owner acceptance recorded — needs steps 2 and 7.
- [x] Contracts, matrix and adapter tests are current and framework-independent (`AppApi`, catalogue, samples and Rust tests import no Svelte).
- [x] Native integrations are feasible based on spikes — ADR-07 (SecretStore) and ADR-12 (files, notifications, TTS, deep links) accepted on emulator and simulator; their physical Android confirmation is part of step 7, physical iOS is carried over.
- [x] Adapter replacement does not require rewriting Svelte components ([tests/adapterSwap.test.ts](../../frontend/tests/adapterSwap.test.ts)).

## Step 2 — owner review procedure

Build the demo once and review every screen with the scenarios in the demo banner:

- Browser: `npm --prefix frontend run dev`, open `http://localhost:5173/#/home`.
- Devices: `npm --prefix frontend run android:dev` (physical Android phone) / `ios:dev` (iOS simulator) — dev variant, demo frontend.

For each screen in the [matrix](../contracts/README.md#ui--backend-matrix-v10-phase-6-gate): appearance (light/dark, 200% text), behaviour (primary action, Back, dialogs, error and empty states — use "Database error", "Endless loading", "Offline" and "Fail the next save"), data (labels, counts, wording in PL and EN). Record each finding as a backlog row with a priority:

- **P0 — blocker:** the gate cannot pass (wrong flow, data loss risk, inaccessible control, misleading wording about what is saved).
- **P1 — before beta:** fix during Phases 7–16, tracked against the screen's checklist row.
- **P2 — polish.**

## Step 7 — physical-device protocol

Run [device-smoke.md](device-smoke.md) rows 1–26 on one physical Android device (WebView ≥ 120, Android 8+) and on the iOS simulator (iOS 16+) with the dev build, plus the rows below. Step-by-step for the Android device: [android-device-testing.md](android-device-testing.md). The iOS column is a simulator run; checks marked *device only* cannot be closed there and are carried over.

| # | Check | Android (device) | iOS (simulator) |
|---|---|---|---|
| G1 | TalkBack / VoiceOver by swipe and double-tap (not injected taps): navigation, study rating group, word sheet, every dialog's first and last control, spoken output listened to | ☐ | ☐ (simulator VoiceOver / Accessibility Inspector; swipe gestures *device only*) |
| G2 | Software keyboard: search, editor fields and the reset phrase stay visible; layout resizes; Back/Done closes the keyboard first | ☐ | ☐ (I/O → Keyboard → untick "Connect Hardware Keyboard") |
| G3 | Orientation: every screen in landscape and back with a dialog or sheet open | ☐ | ☐ |
| G4 | Start time to the dashboard ≤ 3 s (release-profile build) | ☐ | — *device only* |
| G5 | Maximum system font + in-app 200%: no clipped control, no page scroll, bottom-nav labels | ☐ | ☐ |
| G6 | 50 000-card browser fling (ADR-03 frame budget) | ☐ | ☐ (smoothness only; frame budget *device only*) |
| G7 | SecretStore probe and file open/save probe on the device (`#/diagnostics`, ADR-07/12) | ☐ | ☐ |
| G8 | Notifications permission, delivery and tap; TTS with an installed voice | ☐ | ☐ |

### Carried over: physical iPhone

Open until an iPhone is available for testing (owner decision 2026-10-29); they do not block Phase 7: rows 1–26 and G1–G8 on a physical iPhone, in particular G1 by swipe with spoken VoiceOver output, G4 start time, G6 frame budget, G7 Keychain on the device (ADR-07), the software keyboard of a real device (row 5) and safe areas on a notched device (row 6).

Record device, OS, WebView/Safari version, commit and result per row in the device-smoke results table.

## Blocker backlog (from earlier runs; to be prioritized by the owner)

| Item | Source | Proposed priority |
|---|---|---|
| Android start time 4.5–5.0 s on the emulator (blank window before first paint) — re-measure on a device; native splash matching the app | device-smoke Android row 1 | P1 (P0 if > 3 s on the device) |
| Software-keyboard resize never verified (emulator had a hardware keyboard; simulator suppressed it) | device-smoke row 5 | P0 until G2 passes |
| Spoken TalkBack/VoiceOver output never listened to; swipe navigation not covered | device-smoke rows 9, 11 | P0 until G1 passes |
| Bottom-nav labels break mid-word at Android font scale 2.0 ("Statis·tics") | device-smoke row 8/12 | P2 |
| Back once closed the app after a system font change while running (activity recreated) | device-smoke row 13 | P1 — re-check on a device |
| iOS Phase 3 rows 11–16 and all Phase 5 rows 17–26 not run on any target | device-smoke | P0 until step 7 (iOS simulator run) |
| No physical iPhone run | owner decision 2026-10-29 | P1 — carried over until an iPhone is available |
| `#/editor` 3 px select overflow on WebKit at 200% (not reproduced in Playwright) | device-smoke iOS row 8 | P2 — re-check |
| Pixel baselines (gallery and tour) not committed | step 8 | P1 — run the workflow |
| A command not registered in Rust reaches the UI as `INTERNAL` | integration checklist rule 1 | P1 — Phase 7 registers every v1 command with `NOT_IMPLEMENTED` |

## Owner acceptance

| Date | Reviewer | Scope | Decision | Open P0 |
|---|---|---|---|---|
| | | Steps 2 and 6 (screens, data, wording; integration checklist) | ☐ accepted · ☐ accepted with P1/P2 backlog · ☐ rejected | |
| | | Step 7 (physical Android device, iOS simulator) | ☐ passed · ☐ failed | |

Phase 7 starts when both rows are accepted with no open P0.
