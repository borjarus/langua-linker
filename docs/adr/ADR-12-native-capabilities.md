# ADR-12: Native capabilities — WebView, notifications, TTS, files

- Status: Proposed — WebView spike done on the Android emulator (2026-10-12); other spikes not started
- Date: 2026-10-12
- Plan references: Phase 1 steps 9–10, Phase 15, D-11; SecretStore is ADR-07

## Context

The app is a Tauri 2 shell around a Svelte WebView. Phase 1 must show, per target, whether each native capability works with stock Tauri, needs a plugin (official, community or our own Kotlin/Swift), or is deferred. Phase 1 targets are the Android emulator and iOS simulator; physical devices are the Phase 6 gate.

## Results per target

| Capability | Android emulator | iOS simulator | Desktop | Outcome / approach |
|---|---|---|---|---|
| WebView (shell, focus, keyboard, safe areas, rotation, text scale) | works, with fixes below; keyboard resize and start time open | not run | not re-checked | no plugin; CSS and app-level fixes |
| Notifications (permission, send, schedule, tap) | not started | not started | not started | candidate: official `tauri-plugin-notification` |
| TTS (de-DE, en-US, offline, rate, stop) | not started | not started | not started | try WebView `speechSynthesis` first; otherwise thin own plugin (`TextToSpeech` / `AVSpeechSynthesizer`, desktop `tts` crate) |
| Files (open CSV/APKG, save backup, share sheet) | not started | not started | not started | candidates: official `tauri-plugin-dialog` + `tauri-plugin-fs`; share via community or own plugin |

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
- Each remaining spike runs as a dev-variant-only diagnostics screen and records its row in the table above before this ADR is accepted.
- iOS results (simulator, Phase 1) and physical devices (Phase 6) are still required.
