# ADR-07: SecretStore — OS secret storage through `keyring-core`

- Status: Proposed — spike passed on the Android emulator (2026-10-13); iOS simulator pending
- Date: 2026-10-13
- Plan references: Phase 1 step 8, Phase 7 step 26, Phase 13 step 6, §3 (Secrets row), final DoD "SecretStore device-tested"

## Context

The app must keep a small number of secrets (BYOK LLM API key, later a proxy token) outside the database, backups and logs, in the platform store: Android Keystore and iOS Keychain. Tauri 2 has no official plugin for this; Stronghold is an encrypted file vault, not the OS store. The plan allowed a custom Kotlin/Swift plugin if no wrapper met the requirements. Phase 1 targets are the Android emulator and iOS simulator (owner decision 2026-10-12); physical devices are the Phase 6 gate.

## Decision (proposed)

Use the `keyring-core` 1.x API with per-platform stores from the same project (open-source-cooperative, MIT/Apache-2.0), pinned exactly:

| Target | Crate | Store |
|---|---|---|
| Android | `android-native-keyring-store` =1.0.0 | AES-GCM values in `SharedPreferences` (`keyring-default.xml`), key in Android Keystore; JNI context from Tauri via `ndk-context` — no Kotlin code |
| iOS | `apple-native-keyring-store` =1.0.2, feature `protected` | Keychain "protected data" store, app's default access group |
| macOS (development) | same crate, feature `keychain` | legacy login keychain |
| Windows (development) | `windows-native-keyring-store` =1.1.0 | Credential Manager |
| Linux (CI) | — | `Unsupported`; tests use `keyring_core::mock` |

Implementation: `crates/infrastructure/src/secrets.rs` (`SecretStore` with `set`/`get`/`has`/`delete`; service = app identifier). `get` is Rust-only: no IPC command returns a secret to JavaScript (plan Phase 7 step 26). Errors never carry secret bytes (`BadEncoding` payload is dropped; unit-tested).

No custom plugin is needed unless the iOS simulator or Phase 6 devices fail.

## Spike

Dev-variant diagnostics screen `#/diagnostics` → "Run test" calls `probe_secret_store` (`src-tauri/src/diagnostics.rs`): marker read → cleanup → absent → write → read back → overwrite with non-ASCII → read → delete → absent → marker write. The marker left behind reports persistence on the next launch. The command is granted only by the `diagnostics` capability, which only `tauri.dev.conf.json` enables; it also refuses to run outside the dev variant. Beta/release bundles are checked to contain neither the screen nor the command name.

| Check | Android emulator | iOS simulator | Desktop |
|---|---|---|---|
| Round trip (all 10 steps) | pass (2026-10-13) | not run | not run (Windows backend compiles; IPC denial tested) |
| Survives app restart | pass (force-stop → relaunch: marker found) | not run | — |
| No plaintext value in app files | pass: `keyring-default.xml` holds only ciphertext; entry names are plaintext | not run | — |
| No value in logs/IPC report | pass (unit tests + `grep` of app data) | not run | — |

Android run: emulator `Medium_Phone_API_36.1` (Android 16, x86_64), dev debug APK, backend reported as "Android SharedPreferences/KeyStore".

## Open

1. **iOS simulator** (owner's Mac): `npm --prefix frontend run ios:dev`, Settings → Native diagnostics → Run test; force-quit, relaunch, run again (expect "Survived a restart: Yes"). If the first step fails with `errSecMissingEntitlement` (-34018), add a `keychain-access-groups` entitlement or fall back to a thin Swift plugin.
2. Entry names (`<name>@<service>`) are stored in plaintext. They must stay generic (`llm-api-key`), never user data.
3. Service is the configured identifier; mobile builds use the base identifier for all variants (Android variants are still isolated by `applicationIdSuffix`; iOS variants share the bundle id until suffixes exist there).
4. Backup exclusion: Android `SharedPreferences` are included in Auto Backup by default — before Phase 13 stores a real key, exclude `keyring-default.xml` in the backup rules (the Keystore key does not migrate, so a restored ciphertext would be unreadable anyway). iOS Keychain items use the crate's default accessibility; confirm `ThisDeviceOnly` behaviour on devices in Phase 6.
5. Physical Android/iOS devices at the Phase 6 gate.
