# LanguaLinker

Rust/Tauri 2 desktop and mobile shell with Svelte 5 + TypeScript frontend. The repository covers phases 0–2 of the implementation plan: toolchain, design system, contracts, mock API and routing. It deliberately contains no flashcard domain behavior yet — every screen runs on demo data.

## Phase status

- Phase 0: done. Fresh database (ADR-08); KMP code is a read-only reference. Audit and field-level manifest in [docs/migration](docs/migration/kmp-manifest.md); reused assets are copied into this repo with checksums. Content rights are still pending ([content/PROVENANCE.md](content/PROVENANCE.md)).
- Phase 1: Cargo workspace, Tauri shell, Svelte/TypeScript frontend, Nix development shell, and an IPC smoke command/event are present.
- Phase 2: design system (`frontend/src/components`, `/gallery`), AppApi contract v0.2 with operation catalogue, deterministic `MockAppApi` with scenarios, typed hash router with all routes, PL/EN i18n, query cache, reference study controller, Vitest suite. See [docs/contracts](docs/contracts/README.md), [docs/frontend/patterns.md](docs/frontend/patterns.md) and ADR-01/02/05 in [docs/adr](docs/adr).
- Not done yet: iOS build, the full emulator/simulator smoke checklist (Android debug APK builds and starts), physical-device smoke tests (Phase 6 gate), native spikes (SecretStore, notifications, TTS, files, WebView), Playwright/visual snapshots. ADR-01/02/05 were accepted on 2026-10-12. See the status notes in [the plan](docs/Detailed-Implementation-Plan.md).

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
npm --prefix frontend run tauri dev
```

`tauri dev` starts Vite and launches the native shell. Browser-only frontend smoke test:

```powershell
npm --prefix frontend run dev
```

## Mobile

Android builds on Windows and iOS on a Mac are described in [docs/release/mobile-setup.md](docs/release/mobile-setup.md); the device checklist is [docs/release/device-smoke.md](docs/release/device-smoke.md).

## Profiles and demo mode

The build profile is chosen with Vite's `--mode` (`VITE_APP_PROFILE`):

- `demo` (default for `dev`/`build`): in-memory `MockAppApi`, purple demo banner with a scenario switcher (`happy`, `newUser`, `empty`, `loading`, `error`, `offline`, `quota`, `ambiguous`, `large`). Nothing is persisted; the chosen scenario is remembered in local storage.
- `release` (`npm --prefix frontend run build:release`): only the Tauri adapter; the demo adapter and `/gallery` are not emitted. `npm --prefix frontend run verify:release` checks the bundle.

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