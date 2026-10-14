# ADR-12: Native capabilities — WebView, notifications, TTS, files

- Status: Proposed — WebView spike done on the Android emulator (2026-10-12) and iOS simulator (2026-10-13); notification, TTS, file and share probes implemented on `#/diagnostics` (2026-10-14), not yet run on the emulator/simulator
- Date: 2026-10-12
- Plan references: Phase 1 steps 9–10, Phase 15, D-11; SecretStore is ADR-07

## Context

The app is a Tauri 2 shell around a Svelte WebView. Phase 1 must show, per target, whether each native capability works with stock Tauri, needs a plugin (official, community or our own Kotlin/Swift), or is deferred. Phase 1 targets are the Android emulator and iOS simulator; physical devices are the Phase 6 gate.

## Results per target

| Capability | Android emulator | iOS simulator | Desktop | Outcome / approach |
|---|---|---|---|---|
| WebView (shell, focus, keyboard, safe areas, rotation, text scale) | works, with fixes below; keyboard resize and start time open | works (start ~0.95 s, safe areas 62/34 px, landscape, max text); software keyboard not shown on the simulator; `#/editor` 3 px overflow at 200% in-app scale | not re-checked | no plugin; CSS and app-level fixes |
| Notifications (permission, send, schedule, tap) | probe ready, not run | probe ready, not run | probe ready, not run | official `tauri-plugin-notification` =2.5.1 |
| TTS (de-DE, en-US, offline, rate, stop) | probe ready, not run | probe ready, not run | probe ready, not run | WebView `speechSynthesis` first; otherwise a plugin (community `tauri-plugin-tts` or own `TextToSpeech` / `AVSpeechSynthesizer`) |
| Files (open CSV/APKG, save backup) | probe ready, not run | probe ready, not run | probe ready, not run | official `tauri-plugin-dialog` =2.8.1 + `tauri-plugin-fs` =2.6.0, used from Rust |
| Share sheet | probe ready, not run | probe ready, not run | probe ready, not run | Web Share API first; otherwise a plugin (community `tauri-plugin-sharekit` or own) |

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
3. **Back with an open modal.** Hardware Back navigates away instead of closing the dialog/sheet. Open: needs app-level back handling (for example a router guard registered by open modals that closes them and cancels the move).
4. **Safe areas.** `targetSdk` 34 is not edge-to-edge, so the WebView sits between the system bars and `env(safe-area-inset-*)` is 0. Re-test when the Play target-API requirement moves us to `targetSdk` 35+, where Android 15+ enforces edge-to-edge; if WebView does not report the insets, `MainActivity` must pass `WindowInsets` to CSS.
5. **Start time.** Optimized build on the emulator: first frame ~1.1–1.5 s, content after ~4.5–5.0 s, blank window in between (WebView renderer start + page load). Measure on devices; a native window background matching the app removes the blank flash cheaply.
6. **Keyboard resize.** Not verified: the AVD has a hardware keyboard, so only Gboard's floating toolbar appears. Repeat on an AVD with `hw.keyboard=no` or a device.
7. **Diagnostics.** Debug builds expose the WebView to Chrome DevTools (`webview_devtools_remote_<pid>`), which made DOM/layout checks scriptable over `adb`; release builds do not.

## Consequences

- WebView needs no plugin; the fixes above are in the frontend and covered by tests.
- Registering the notification plugin adds `POST_NOTIFICATIONS`, `RECEIVE_BOOT_COMPLETED` and `WAKE_LOCK` to the Android manifest of every variant. Phase 15 needs them anyway; the store privacy declarations must list them.
- Each remaining spike runs on the dev-variant-only diagnostics screen (`#/diagnostics`, `src-tauri/src/diagnostics.rs`, `diagnostics` capability — introduced by the SecretStore spike, ADR-07) and records its row in the table above before this ADR is accepted.
- iOS simulator WebView results: [device-smoke.md](../release/device-smoke.md) (2026-10-13). iOS results for the other spikes and physical devices (Phase 6) are still required.
