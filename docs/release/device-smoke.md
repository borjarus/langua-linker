# Device smoke checklist

Phase 1: Android emulator and iOS simulator (owner decision 2026-10-12). Phase 6 gate: the same checklist on physical Android and iOS devices; emulator results do not count there.

Run with the dev variant, which bundles the demo frontend (`npm --prefix frontend run android:dev` / `android:apk`, `variant -- dev ios dev`); package `com.mila.langualinker.dev`. Record date, device, OS version, build commit and result for each row; attach screenshots in the PR.

Legend: ☑ pass · ◐ partly verified (see notes) · ☒ fail · ☐ not run. Android column = emulator run of 2026-10-12 below.

| # | Check | Android | iOS |
|---|---|---|---|
| 1 | App starts; demo banner and decks screen visible within 3 s | ☒ | ☐ |
| 2 | Settings → About shows app name, version and contract version (demo build: from the mock; `get_app_info` over IPC checked separately, see notes) | ☑ | ☐ |
| 3 | Bottom navigation reaches all 5 sections; hardware/gesture back returns to the previous screen | ☑ | ☐ |
| 4 | `#/gallery`: dialog opens/closes, toast appears above the bottom navigation | ☑ | ☐ |
| 5 | Search field: on-screen keyboard does not cover the input; layout resizes | ◐ | ☐ |
| 6 | Safe areas: notch/status bar and home indicator do not overlap content or navigation | ☑ | ☐ |
| 7 | Rotate to landscape and back: no clipped controls | ☑ | ☐ |
| 8 | System font size at maximum: text scales, no horizontal page scroll | ☑ | ☐ |
| 9 | TalkBack / VoiceOver: navigation items and the "Study" button are announced with labels | ◐ | ☐ |
| 10 | App log file exists in the app log directory and contains no card text (`adb shell "run-as com.mila.langualinker.dev ls logs"` / Xcode container) | ☑ | ☐ |

## How the Android rows were checked

Debug builds of the dev variant expose the WebView to Chrome DevTools: `adb forward tcp:9222 localabstract:webview_devtools_remote_<pid>`, then `http://localhost:9222/json` and `Runtime.evaluate` read the DOM, focus, viewport and layout. Touch input went through `adb shell input tap/text/keyevent`; the Android accessibility tree through `adb shell uiautomator dump`.

## Results

| Date | Device | OS | Commit | Result / notes |
|---|---|---|---|---|
| 2026-10-12 | Android Emulator `Medium_Phone_API_36.1` (x86_64, Play Store image), WebView 134.0.6998.135 | Android 16 (API 36.1) | `8ee33b0` + uncommitted changes | See rows below. |

Notes for the 2026-10-12 emulator run:

1. **☒ Start time.** Debug APK: first activity frame 3.5–3.9 s, WebView first contentful paint a further 1.1–1.4 s. Optimized APK (release profile, debug-signed): first frame 1.1–1.5 s, WebView renderer process ready ~2.4 s, dashboard visible after **~4.5–5.0 s** (screenshot timeline, 3 runs); the window is blank in between. Over the 3 s target on the emulator; re-measure on physical devices (Phase 6) and consider a native window background/splash that matches the app so the gap is not blank.
2. **☑** About shows `LanguaLinker 0.1.0 · Contract version 0.2 · demo`. Over real IPC on Android, `get_app_info` returned `{"apiVersion":"0.2","appName":"LanguaLinker","appVersion":"0.1.0","mode":"dev"}` and `plugin:window|close` was rejected by the ACL.
3. **☑** All 5 sections reached by tapping; each move focuses the new screen's heading; Back walks history (settings → stats → editor). Defect found: Back while a dialog is open navigates away instead of closing the dialog (open).
4. **☑** Dialog opens by tap, focus moves into it, Cancel closes it and returns focus to "Open dialog"; toast renders above the bottom navigation.
5. **◐** Defect found and fixed: every keystroke in the search field navigated (`#/browser?q=…`), remounted the screen and moved focus to the heading, which closes the mobile keyboard after each character. After the fix focus stays in the field (`screenKey` in `frontend/src/app/routes.ts`, regression test in `tests/routes-render.test.ts`). Also fixed a duplicate clear button (WebView's native one next to ours). Not verified: layout resize, because this AVD has a hardware keyboard and Gboard shows only a floating toolbar — repeat on an AVD with `hw.keyboard=no` or a device.
6. **☑** `targetSdk` 34 is not edge-to-edge: the WebView sits between the status bar (0–63 px) and the gesture bar (2337–2400 px); `env(safe-area-inset-*)` is 0. Re-test when `targetSdk` reaches 35+, where Android 15+ enforces edge-to-edge.
7. **☑** Landscape 914×363 CSS px switches to the sidebar layout; no horizontal overflow on home, browser, stats, settings, gallery; portrait restored correctly.
8. **☑ after fix.** Android WebView applies the system font scale as text zoom (body 16 → 32 px at scale 2.0). Defect found and fixed: input/select intrinsic widths and the 5-column nav grid exceeded the screen, so WebView widened the page and browser/editor/gallery scrolled horizontally. Now no horizontal scroll on all 14 routes. Remaining: bottom-nav labels break mid-word at 2.0 ("Statis·tics"); WebView text zoom also scales `px`/`vw`, so CSS cannot cap it — see ADR-12.
9. **◐** Accessibility tree (what TalkBack reads): nav items exposed as "Decks/Browse/Add/Statistics/Settings"; buttons as "Study everything" and "Study Niemiecki — słówka A1" etc. (deck-specific names). TalkBack is installed but spoken output was not listened to.
10. **☑** `logs/langualinker.2026-10-12.log` exists; lines are `app started app_version="0.1.0" app_variant="dev" platform="android"`; no card or deck text.

Emulator settings changed for the run and restored: `font_scale` 1.0, `show_ime_with_hard_keyboard` 0. Auto-rotate (`accelerometer_rotation`) was left off.
