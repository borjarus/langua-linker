# Mobile setup (Phase 1)

## Android — Windows (verified 2026-10-12)

Installed per user, no admin rights needed:

| Component | Version | Location |
|---|---|---|
| JDK | Temurin 17.0.20.1 | `%LOCALAPPDATA%\Programs\Temurin\jdk-17.0.20.1+1` (`JAVA_HOME`) |
| Android SDK | platforms 34, 36; build-tools 34–37 | `%LOCALAPPDATA%\Android\Sdk` (`ANDROID_HOME`) |
| Command-line tools | latest (13114758) | `Sdk\cmdline-tools\latest` |
| NDK | r27d LTS `27.3.13750724` | `NDK_HOME` |
| Rust targets | aarch64/armv7/i686/x86_64-linux-android | rustup (toolchain 1.90.0) |

Build and run:

```powershell
npm --prefix frontend run tauri -- android build --debug --apk --target aarch64
adb install -r src-tauri\gen\android\app\build\outputs\apk\universal\debug\app-universal-debug.apk

# live reload on a USB-connected phone (same Wi-Fi/LAN as the PC for the dev server)
npm --prefix frontend run tauri -- android dev
```

Project specifics:

- `src-tauri` is a library (`langualinker_lib`) with `#[cfg_attr(mobile, tauri::mobile_entry_point)]`.
- `minSdkVersion` 26 (`tauri.conf.json` → `bundle.android`).
- Local change in generated code: `gen/android/buildSrc/.../BuildTask.kt` runs `npm run tauri` in `../frontend`, because the npm project is not next to `src-tauri`. Re-apply it if `gen/android` is regenerated.
- Vite listens on `TAURI_DEV_HOST` (HMR on port 5183) so the phone can reach the dev server.

Known issue on this network: Java downloads (Gradle plugins, SDK packages) intermittently fail with `SSLException: (bad_record_mac) Tag mismatch` while curl works. Workaround for the shell running the build:

```powershell
$env:JAVA_TOOL_OPTIONS = '-Djdk.tls.client.protocols=TLSv1.2 -Dhttps.protocols=TLSv1.2'
```

## iOS — own Mac (to do on the Mac)

Requirements: macOS with Xcode 16+, an Apple ID (free account works for device testing; the provisioning profile expires after 7 days), iPhone with iOS 16+ and Developer Mode enabled.

```bash
# toolchain
xcode-select --install
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh   # rust-toolchain.toml pins 1.90.0
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
brew install node@24 cocoapods

# project
git clone <repo> && cd langua-linker
npm --prefix frontend ci
npm --prefix frontend run tauri -- ios init          # creates src-tauri/gen/apple — commit it
npm --prefix frontend run tauri -- ios dev --open    # select your team in Xcode → Signing
```

- `minimumSystemVersion` 16.0 is set in `tauri.conf.json` → `bundle.iOS`.
- As on Android, check whether the generated Xcode build phase runs `npm run tauri` from `frontend/`; if not, apply the same working-directory change and note it here.
- After `gen/apple` is committed, enable the `ios-simulator` job in `.github/workflows/mobile.yml`.
- Record results in [device-smoke.md](device-smoke.md).
