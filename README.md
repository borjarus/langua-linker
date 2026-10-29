# LanguaLinker

Rust/Tauri 2 desktop and mobile shell with Svelte 5 + TypeScript frontend. The repository covers phases 0–2 of the implementation plan: toolchain, design system, contracts, mock API and routing. It deliberately contains no flashcard domain behavior yet — every screen runs on demo data.

## Phase status

- Phase 0: done. Fresh database (ADR-08); KMP code is a read-only reference. Audit and field-level manifest in [docs/migration](docs/migration/kmp-manifest.md); reused assets are copied into this repo with checksums. Content rights are still pending ([content/PROVENANCE.md](content/PROVENANCE.md)).
- Phase 1: Cargo workspace, Tauri shell, Svelte/TypeScript frontend, Nix development shell, and an IPC smoke command/event are present.
- Phase 2: design system (`frontend/src/components`, `/gallery`), AppApi contract v0.2 with operation catalogue, deterministic `MockAppApi` with scenarios, typed hash router with all routes, PL/EN i18n, query cache, reference study controller, Vitest suite. See [docs/contracts](docs/contracts/README.md), [docs/frontend/patterns.md](docs/frontend/patterns.md) and ADR-01/02/05 in [docs/adr](docs/adr).
- Phase 3 (prototype on demo data, no FSRS/SQLite): Dashboard → Study → Summary flow, contract v0.3 (`updateStudySession`, `getNoteContent`, `startStudy.cardIds`), demo scenarios `noCards`, `caughtUp`, `preparing`, `aiErrors`, Playwright learning-flow E2E. Device rows 11–16 in [device-smoke.md](docs/release/device-smoke.md) are still open.
- Phase 4 (prototype on demo data): card details, note editor, card browser and deck overview, contract v0.4 (presets, `updateNoteContent`, `deleteNote`, `searchWords`, `previewBulkAction`, note-type templates), ADR-03 (browser virtualization on `@tanstack/svelte-virtual`, accepted), Playwright management E2E. Mobile device runs of these screens are still open.
- Phase 5 (prototype on demo data): statistics, settings, AI, privacy, about, licences, onboarding, import & export, backup & restore; contract v0.5; Playwright Phase 5 E2E. Device runs of these screens are still open.
- Phase 6 (GUI acceptance gate, open): contract v1.0 frozen in [crates/contracts/v1](crates/contracts/v1) with dual-adapter tests (mock and Tauri adapter replay the same samples; Rust checks the same files), per-screen [integration checklist](docs/contracts/integration-checklist.md), acceptance scenarios and application tour in Playwright, prototype notices only in the demo profile. Owner review and the device run (physical Android, iOS simulator — no iPhone for now) are pending: [docs/release/phase6-gate.md](docs/release/phase6-gate.md).
- Not done yet: iOS build, the iOS simulator smoke checklist and the open Android emulator rows 1, 5, 9 ([device-smoke.md](docs/release/device-smoke.md)), physical Android smoke tests (Phase 6 gate; guide: [android-device-testing.md](docs/release/android-device-testing.md)), physical iPhone tests (no device yet), running the notification, TTS, file and share probes on the emulator/simulator (probes are on `#/diagnostics`; WebView and SecretStore spikes are done, see ADR-12 and ADR-07), Playwright/visual snapshots. ADR-01/02/05 were accepted on 2026-10-12. See the status notes in [the plan](docs/Detailed-Implementation-Plan.md).

## Nix environment

Nix is intended for Linux or macOS. On Windows, run it through WSL2 with Nix installed; a WSL shell builds Linux artifacts, not a Windows Tauri bundle.

```powershell
wsl --install
wsl
sh <(curl -L https://nixos.org/nix/install) --daemon
cd /mnt/c/Users/m.puczynski/Documents/projects/rust/langua-linker
nix develop
```

Inside `nix develop`:

```bash
npm --prefix frontend install
npm --prefix frontend run check
cargo check --workspace
```

## Windows native development

For a Windows Tauri build, install Rust through `rustup` and Node.js on Windows. Nix/WSL does not provide a Windows target toolchain.

```powershell
cd C:\Users\m.puczynski\Documents\projects\rust\langua-linker
npm --prefix frontend install
npm --prefix frontend run check
cargo check --workspace
npm --prefix frontend run app:dev
```

`app:dev` starts Vite and launches the native shell as the dev variant (see below). Browser-only frontend smoke test:

```powershell
npm --prefix frontend run dev
```

## Mobile

Android builds on Windows and iOS on a Mac are described in [docs/release/mobile-setup.md](docs/release/mobile-setup.md); the device checklist is [docs/release/device-smoke.md](docs/release/device-smoke.md).

## Profiles and demo mode

The build profile is chosen with Vite's `--mode` (`VITE_APP_PROFILE`):

- `demo` (default for `dev`/`build`): in-memory `MockAppApi`, purple demo banner with a scenario switcher (`happy`, `newUser`, `empty`, `loading`, `error`, `offline`, `quota`, `ambiguous`, `large`). Nothing is persisted; the chosen scenario is remembered in local storage.
- `dev` (`--mode dev`): Tauri adapter plus `/gallery` and `/diagnostics` (native capability probes, ADR-07/ADR-12; the Rust commands are granted only by the dev variant's `diagnostics` capability).
- `beta`/`release` (`build:beta`/`build:release`): only the Tauri adapter; the demo adapter and `/gallery` are not emitted. `verify:beta`/`verify:release` check the bundles.

## App variants

Native builds come in three variants that install side by side, each with its own app data, logs, WebView storage and (later) keychain entries. Always start native builds through `frontend/scripts/tauri-variant.mjs` (`npm --prefix frontend run variant -- <dev|beta|release> <tauri args>`); it selects the config overlays and sets `LL_APP_VARIANT` for Rust and Gradle.

| Variant | Identifier | Name | Frontend profile | Shortcuts |
|---|---|---|---|---|
| dev | `com.mila.langualinker.dev` | LanguaLinker Dev | `demo` (until real backend phases) | `app:dev`, `android:dev`, `android:apk` |
| beta | `com.mila.langualinker.beta` | LanguaLinker Beta | `beta` | `variant -- beta …` |
| release | `com.mila.langualinker` | LanguaLinker | `release` | `variant -- release …` |

- `src-tauri/tauri.conf.json` is the release variant; `tauri.dev.conf.json`/`tauri.beta.conf.json` overlay name, identifier and frontend build.
- Android/iOS projects are generated for the base identifier (the Tauri CLI derives the Kotlin package from it), so mobile builds re-apply it with `tauri.mobile.conf.json`; Gradle adds the `.dev`/`.beta` application id suffix and launcher label from `LL_APP_VARIANT`. iOS bundle-id suffixes are still to be set up on the Mac.
- A build without `LL_APP_VARIANT` (plain `cargo`, Android Studio) is a dev build; release is always explicit. Settings → About shows the variant (`AppInfoDto.mode`).

Useful routes: `#/home`, `#/gallery` (component gallery, demo only), `#/settings` (language, theme, 200% text scale).

## Quality commands

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm --prefix frontend run check
npm --prefix frontend run lint
npm --prefix frontend run lint:i18n
npm --prefix frontend test
npm --prefix frontend run build
npm --prefix frontend run verify:release
```

## Repository layout

- `crates/domain`: future business rules; no Tauri, SQL, or network dependency.
- `crates/application`: future use cases and ports.
- `crates/infrastructure`: future adapter implementations.
- `crates/contracts`: IPC DTOs and error contract.
- `src-tauri`: Tauri composition root and commands.
- `frontend`: Svelte 5 + TypeScript SPA.
- `docs/migration`: KMP migration evidence and decisions ([kmp-manifest.md](docs/migration/kmp-manifest.md)).
- `content/decks`: bundled deck sources (from KMP, rights pending — see [content/PROVENANCE.md](content/PROVENANCE.md)).
- `tests/fixtures`: KMP schema/APKG reference fixtures and FSRS specification; checksums in `scripts/fixtures.lock.json` (`node scripts/verify-fixtures.mjs`).
- `docs/adr`, `docs/contracts`, `docs/frontend`, `docs/testing`: decisions, contract rules and UI→backend matrix, frontend patterns, test matrix.