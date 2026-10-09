# LanguaLinker

Rust/Tauri 2 desktop and mobile shell with Svelte 5 + TypeScript frontend. This repository starts phases 0 and 1 of the implementation plan; it deliberately contains no flashcard domain behavior yet.

## Phase status

- Phase 0: KMP audit recorded in [docs/migration/kmp-audit.md](docs/migration/kmp-audit.md). KMP assets remain source-of-truth references until ported and revalidated.
- Phase 1: Cargo workspace, Tauri shell, Svelte/TypeScript frontend, Nix development shell, and an IPC smoke command/event are present.
- Native SecretStore, notifications, TTS, file access, Android/iOS builds, and production CSP capabilities remain spikes, not completed features.

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

## Quality commands

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm --prefix frontend run check
npm --prefix frontend run build
```

## Repository layout

- `crates/domain`: future business rules; no Tauri, SQL, or network dependency.
- `crates/application`: future use cases and ports.
- `crates/infrastructure`: future adapter implementations.
- `crates/contracts`: IPC DTOs and error contract.
- `src-tauri`: Tauri composition root and commands.
- `frontend`: Svelte 5 + TypeScript SPA.
- `docs/migration`: KMP migration evidence and decisions.