# ADR-12: Native capabilities — WebView, notifications, TTS, files

- Status: Accepted — native capability probes confirmed on iOS simulator (2026-10-15) and Android emulator (2026-10-15)
- Date: 2026-10-12 (accepted 2026-10-15)
- Plan references: Phase 1 steps 9–10, Phase 15, D-11; SecretStore is ADR-07

## Context

The app is a Tauri 2 shell around a Svelte WebView. Phase 1 must show, per target, whether each native capability works with stock Tauri, needs a plugin (official, community or our own Kotlin/Swift), or is deferred. Phase 1 targets are the Android emulator and iOS simulator; physical devices are the Phase 6 gate.

## Results per target

| Capability | Android emulator | iOS simulator | Desktop | Outcome / approach |
|---|---|---|---|---|
| WebView (shell, focus, keyboard, safe areas, rotation, text scale) | works, with fixes below; keyboard resize and start time open | works (start ~0.95 s, safe areas 62/34 px, landscape, max text); software keyboard not shown on the simulator; `#/editor` 3 px overflow at 200% in-app scale (not reproduced in Playwright WebKit after the 2026-10-14 fixes; re-check on device) | not re-checked | no plugin; CSS and app-level fixes |
| Notifications (permission, send, schedule, tap) | works: permission prompt (`POST_NOTIFICATIONS`), immediate delivery (id 7001 in status bar), scheduling pending and delivered via `tauri-plugin-notification` =2.5.1 | works: permission prompt ("LanguaLinker Dev Would Like to Send You Notifications"), immediate delivery, scheduling pending and delivered via `tauri-plugin-notification` =2.5.1 | not re-checked | official `tauri-plugin-notification` =2.5.1 |
| TTS (de-DE, en-US, offline, rate, stop) | requires plugin: Android System WebView does not expose `window.speechSynthesis` | works: WKWebView `speechSynthesis` has 68 voices (including German *Anna* and English *Samantha* offline), rate and stop work without a plugin | not re-checked | iOS uses WebView `speechSynthesis` (no plugin); Android requires a native plugin (community `tauri-plugin-tts` or Kotlin `TextToSpeech` bridge in Phase 2) |
| Files (open CSV/APKG, save backup) | works: `probe_file_open` opens `test_deck.csv` via DocumentsUI (`content://`), streams and sniffs text in Rust; `probe_file_save` creates backup file via DocumentsUI, writes and reads back in Rust | works: `probe_file_open` reads file via security-scoped URL and streams in Rust; `probe_file_save` writes placeholder and reads back in Rust | not re-checked | official `tauri-plugin-dialog` =2.8.1 + `tauri-plugin-fs` =2.6.0, used from Rust |
| Share sheet | requires plugin: Android System WebView does not expose `navigator.share` / `navigator.canShare` | works: WKWebView `navigator.share` and `canShare({ files })` work for text and CSV files | not re-checked | iOS uses Web Share API (no plugin); Android requires a native plugin (community `tauri-plugin-sharekit` or Android Intent bridge in Phase 4) |

## Probes (2026-10-14)

All run from the dev-variant `#/diagnostics` screen. Every plugin is registered in every variant, but the WebView reaches it only through the `diagnostics` capability (dev only). Beta/release bundles are checked to contain none of the probe code (`verify-release-bundle.mjs`).

- **Notifications** — JS calls the plugin commands directly (`is_permission_granted`, `request_permission`, `notify`, `cancel`, `get_pending`). The plugin's `sendNotification` drops errors, so it is not used. Steps: check and request permission, send now (id 7001), schedule in 10 s (id 7002, `allowWhileIdle`) and confirm it is pending. While the screen is open, deliveries and taps are logged with their `extra` through `onNotificationReceived` and `onAction`. Run it with the app in the foreground, in the background, and once killed. A tap on a cold start reaches only listeners registered after start-up, so record whether it is lost.
- **Files** — Rust commands `probe_file_open { filtered }` and `probe_file_save` use the dialog plugin's blocking picker on a worker thread and `FsExt::fs().open()`. That is the path the Phase 8 importer and Phase 12 backup will take: a `content://` URI on Android, a security-scoped `file://` URL on iOS (released after use). The open probe streams the whole file and reports the location scheme, size and sniffed type (`text`/`zip`), never the name or contents. The save probe writes a non-ASCII sample and reads it back. Two risks are visible in the plugin source and must be confirmed:
  - Android maps filter extensions to MIME types through `MimeTypeMap`. `apkg` has no MIME type, so the filtered picker may grey out APKG files. "Open any file" is the control.
  - iOS implements save as "export an empty placeholder, return the destination URL". Record whether writing to that URL afterwards works.
- **TTS** — WebView `speechSynthesis`: availability, voices for `de-DE`/`en-US` with `localService` (offline), speak at rate 0.5–1.5, Stop (expect `error:interrupted`/`canceled` or `end`). Repeat in airplane mode.
- **Share** — `navigator.share` / `canShare({ files })` with text and a small CSV file. `AbortError` means the sheet opened and was closed.

Test files for the open probe: a CSV and an `.apkg` (any zip renamed to `.apkg` is enough to check the picker and the sniffing), pushed to the device's Downloads (`adb push deck.csv deck.apkg /sdcard/Download/`) or added to the simulator via Files/drag and drop.

## WebView findings (Android 16 emulator, WebView 134)

Details and measurements: [device-smoke.md](../release/device-smoke.md).

1. **Text scale.** Android WebView applies the system font scale as text zoom and multiplies every computed size, including `px` and `vw` — CSS cannot cap it. At scale 2.0 form controls and the nav grid exceeded the screen and WebView widened the page. Fixed with `min-width: 0; max-width: 100%` on form controls and `minmax(0, 1fr)` grid tracks; all 14 routes now fit. **Proposed decision:** keep honouring the system scale (accessibility) and design for 2.0; the bottom-nav label strategy at large text (labels currently break mid-word) is a Phase 3 UX item. Alternative kept open: `textZoom = 100` in `MainActivity` and the font scale passed to CSS as a variable — more control, but native code and runtime-change handling.
2. **Focus vs. navigation.** Query-only navigation (search text in the URL) remounted the screen and moved focus to the heading, which would close the mobile keyboard after each character. Fixed: `screenKey` decides both remount and focus; screens that own their query update in place.
3. **Back with an open modal.** Hardware Back navigated away instead of closing the dialog/sheet. Fixed 2026-10-14: an open `Dialog`/`Sheet` registers a router layer (`Router.addLayer`); a history-back move while a layer is open is undone and dismisses the topmost layer, as Escape does. Programmatic navigation is unaffected. Covered by Vitest and the Playwright gallery test; not yet re-run on the emulator. Limitation: on the very first history entry Android has nothing to go back to and closes the activity; handling that needs `onBackButtonPress` from `@tauri-apps/api/app`.
4. **Safe areas.** `targetSdk` 34 is not edge-to-edge, so the WebView sits between the system bars and `env(safe-area-inset-*)` is 0. Re-test when the Play target-API requirement moves us to `targetSdk` 35+, where Android 15+ enforces edge-to-edge; if WebView does not report the insets, `MainActivity` must pass `WindowInsets` to CSS.
5. **Start time.** Optimized build on the emulator: first frame ~1.1–1.5 s, content after ~4.5–5.0 s, blank window in between (WebView renderer start + page load). Measure on devices; a native window background matching the app removes the blank flash cheaply.
6. **Keyboard resize.** Not verified: the AVD has a hardware keyboard, so only Gboard's floating toolbar appears. Repeat on an AVD with `hw.keyboard=no` or a device.
7. **Diagnostics.** Debug builds expose the WebView to Chrome DevTools (`webview_devtools_remote_<pid>`), which made DOM/layout checks scriptable over `adb`; release builds do not.

## Probe findings (iOS simulator and Android emulator, 2026-10-15)

The probes were run on the `com.mila.langualinker.dev` variant via `#/diagnostics` on both targets:
- **iOS Simulator:** iPhone 17 (iOS 26.5), WKWebView
- **Android Emulator:** Pixel 9 Pro (API 36 / Android 16), System WebView 134

### 1. Notifications (`tauri-plugin-notification` =2.5.1)
- **iOS Simulator:** Requesting permission triggered the native iOS system dialog ("*LanguaLinker Dev Would Like to Send You Notifications*"). Immediate notification sent and delivered (`id: 7001`). Scheduling in 10 s verified pending and delivered.
- **Android Emulator:** Requesting permission triggered the native Android system dialog ("*Allow LanguaLinker Dev to send you notifications?*"). Immediate notification sent and delivered (`id: 7001`, status bar icon shown, delivery logged via `onNotificationReceived`). Scheduling in 10 s verified pending and delivered.
- **Decision:** Official plugin `tauri-plugin-notification` =2.5.1 satisfies all requirements for Phase 15 notifications.

### 2. Files (`tauri-plugin-dialog` =2.8.1 + `tauri-plugin-fs` =2.6.0)
- **iOS Simulator:** `probe_file_open` opened the native iOS document picker (`UIDocumentPickerViewController`), streamed the chosen file through Rust using a security-scoped URL, and sniffed its content type. `probe_file_save` exported a placeholder file, which Rust opened, wrote non-ASCII test data to, and read back.
- **Android Emulator:** `probe_file_open` opened Android DocumentsUI (Storage Access Framework), selected `test_deck.csv`, and Rust streamed the file via `content://` URI (reporting `Location: content`, `Size: 23 B`, sniffed `text`). `probe_file_save` opened the DocumentsUI save dialog for `langualinker-probe-backup.txt`, and Rust successfully wrote and read back the 75 B test content.
- **Decision:** The official Tauri dialog and filesystem plugins work cleanly from Rust for Phase 8 import and Phase 12 export without needing custom Kotlin/Swift code.

### 3. Text to speech (TTS)
- **iOS Simulator:** WKWebView exposes `window.speechSynthesis` with 68 voices out of the box (including German *Anna* and English *Samantha* offline with `localService: true`). Rate adjustment and cancellation work without any plugin.
- **Android Emulator:** Android System WebView does **not** expose the Web Speech API (`window.speechSynthesis` is `undefined`).
- **Decision:** iOS will use WebView `speechSynthesis` directly (no plugin). Android requires a native plugin (community `tauri-plugin-tts` or a lightweight custom Kotlin bridge to Android's `TextToSpeech` engine) when speech is implemented in Phase 2.

### 4. Share sheet
- **iOS Simulator:** WKWebView supports the Web Share API (`navigator.share` and `navigator.canShare`). Both text sharing and CSV file sharing (`canShare({ files })`) are supported and present the native iOS share sheet.
- **Android Emulator:** Android System WebView does **not** expose `navigator.share` in this context (`Share text: No`, `Share CSV file: No`).
- **Decision:** iOS will use the Web Share API (no plugin). Android requires a native plugin or custom Android Intent bridge (`ACTION_SEND`) when deck/card sharing is implemented in Phase 4.

## Consequences

- All native spikes for Phase 1 are complete and verified across both required mobile targets (iOS simulator and Android emulator). ADR-12 is accepted.
- WebView needs no plugin; the fixes above are in the frontend and covered by tests.
- Registering the notification plugin adds `POST_NOTIFICATIONS`, `RECEIVE_BOOT_COMPLETED` and `WAKE_LOCK` to the Android manifest of every variant. Phase 15 needs them anyway; the store privacy declarations must list them.
- File operations (import/export) will be driven through Rust commands using `tauri-plugin-dialog` and `tauri-plugin-fs`.
- TTS and Share sheet will follow platform-specific paths: stock Web APIs on iOS/desktop, and native plugin/Intent bridges on Android.
- Verification on physical devices remains a Phase 6 gate requirement.
