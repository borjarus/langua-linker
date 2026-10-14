# Device smoke checklist

Phase 1: Android emulator and iOS simulator (owner decision 2026-10-12). Phase 6 gate: the same checklist on physical Android and iOS devices; emulator results do not count there.

Run with the dev variant, which bundles the demo frontend (`npm --prefix frontend run android:dev` / `android:apk`, `variant -- dev ios dev`); package `com.mila.langualinker.dev`. Record date, device, OS version, build commit and result for each row; attach screenshots in the PR.

Legend: ☑ pass · ◐ partly verified (see notes) · ☒ fail · ☐ not run. Android column = emulator run of 2026-10-12 below.

| # | Check | Android | iOS |
|---|---|---|---|
| 1 | App starts; demo banner and decks screen visible within 3 s | ☒ | ☑ |
| 2 | Settings → About shows app name, version and contract version (demo build: from the mock; `get_app_info` over IPC checked separately, see notes) | ☑ | ☑ |
| 3 | Bottom navigation reaches all 5 sections; hardware/gesture back returns to the previous screen | ☑ | ☑ |
| 4 | `#/gallery`: dialog opens/closes, toast appears above the bottom navigation | ☑ | ☑ |
| 5 | Search field: on-screen keyboard does not cover the input; layout resizes | ◐ | ◐ |
| 6 | Safe areas: notch/status bar and home indicator do not overlap content or navigation | ☑ | ☑ |
| 7 | Rotate to landscape and back: no clipped controls | ☑ | ☑ |
| 8 | System font size at maximum: text scales, no horizontal page scroll | ☑ | ☑ |
| 9 | TalkBack / VoiceOver: navigation items and the "Study" button are announced with labels | ◐ | ◐ |
| 10 | App log file exists in the app log directory and contains no card text (`adb shell "run-as com.mila.langualinker.dev ls logs"` / Xcode container) | ☑ | ☑ |

## How the checks were performed

- **Android:** Debug builds of the dev variant expose the WebView to Chrome DevTools: `adb forward tcp:9222 localabstract:webview_devtools_remote_<pid>`, then `http://localhost:9222/json` and `Runtime.evaluate` read the DOM, focus, viewport and layout. Touch input went through `adb shell input tap/text/keyevent`; the Android accessibility tree through `adb shell uiautomator dump`.
- **iOS:** Built and run via `npm --prefix frontend run ios:dev -- "iPhone 17"`. Screenshots captured via `xcrun simctl io booted screenshot`. DOM state, focus, insets, styling and Tauri IPC (`__TAURI_INTERNALS__.invoke`) inspected over WKWebView connection. App data container and log files inspected via `xcrun simctl get_app_container`. Font scaling verified via `xcrun simctl ui booted content_size`.

## Results

| Date | Device | OS | Commit | Result / notes |
|---|---|---|---|---|
| 2026-10-12 | Android Emulator `Medium_Phone_API_36.1` (x86_64, Play Store image), WebView 134.0.6998.135 | Android 16 (API 36.1) | `8ee33b0` + uncommitted changes | See Android notes below. |
| 2026-10-13 | iOS Simulator `iPhone 17` (arm64), WKWebView / MobileSafari | iOS 26.5 | `001aaf3` | See iOS notes below. Screenshots in `docs/release/screenshots/`. |

### Notes for the 2026-10-13 iOS simulator run:

1. **☑ Start time.** App launched via `xcrun simctl launch booted com.mila.langualinker`; demo banner and decks screen visible in **~925–970 ms** across 3 runs (simctl launch ~196 ms, full initial render ~925–970 ms). Well within the 3 s target.
2. **☑ About.** Settings → About shows `LanguaLinker 0.1.0 · Contract version 0.2 · demo`. Over real Tauri IPC on iOS, `get_app_info` returned `{"apiVersion":"0.2","appName":"LanguaLinker","appVersion":"0.1.0","mode":"dev"}` and `plugin:window|close` was rejected by ACL (`window.close not allowed. Permissions associated with this command: core:window:allow-close`).
3. **☑ Navigation.** Bottom navigation reaches all 5 sections (`#/home`, `#/browser`, `#/editor`, `#/stats`, `#/settings`); each move updates the route, sets `aria-current="page"`, and focuses the new screen's heading (`H1`). `history.back()` walks navigation history properly.
4. **☑ Gallery dialog & toast.** On `#/gallery`, Example dialog opens on tap, focus moves into dialog, Cancel closes it; toast ("Saved in demo memory.") appears with 15 px gap above the bottom navigation bar (`calc(var(--space-4) + var(--bottom-nav-height) + env(safe-area-inset-bottom))`).
5. **◐ Search field.** On `#/browser`, search input is located at top (y: 341 px), typing ("halt") filters the card list without remounting or losing focus (`screenKey` fix confirmed on iOS). Software keyboard suppressed on headless simulator; layout resize with software keyboard deferred to physical device test (Phase 6).
6. **☑ Safe areas.** Dynamic Island / status bar top inset is 62 px; home indicator bottom inset is 34 px (`env(safe-area-inset-*)`). Banner top padding is 70 px (safely clear of notch); bottom navigation padding-bottom is 34 px (cushions home indicator). No content overlap.
7. **☑ Landscape.** Landscape width 874 px (≥ 52rem) activates the sidebar layout grid; portrait restores bottom navigation cleanly without horizontal overflow.
8. **☑ Font scale.** iOS `content_size` set to `accessibility-extra-extra-extra-large` via `simctl ui` and in-app text scale set to Huge (200%). Text scales across all components. At 200% in-app scale, 9 out of 10 routes have zero overflow (`scrollWidth == clientWidth == 402 px`); `#/editor` shows a minor 3 px WebKit select intrinsic width difference (`scrollWidth 405 px`; not reproduced in Playwright WebKit at 402 px on 2026-10-14 after the demo-banner and dialog fixes — re-check on device). At system maximum with default app scale, zero overflow across all routes.
9. **◐ VoiceOver.** Accessibility tree exposes bottom navigation with `role="navigation"` and `aria-label="Main navigation"`, items with labels "Decks", "Browse", "Add", "Statistics", "Settings", and buttons as "Study everything" and deck-specific labels ("Study Niemiecki — słówka A1" etc. via `aria-label`). Spoken audio not listened to.
10. **☑ App logs.** Log file located in `Library/Application Support/com.mila.langualinker/logs/langualinker.2026-10-13.log`. Lines are `app started app_version="0.1.0" app_variant="dev" platform="ios"`; no card or deck text.

### Notes for the 2026-10-12 Android emulator run:

1. **☒ Start time.** Debug APK: first activity frame 3.5–3.9 s, WebView first contentful paint a further 1.1–1.4 s. Optimized APK (release profile, debug-signed): first frame 1.1–1.5 s, WebView renderer process ready ~2.4 s, dashboard visible after **~4.5–5.0 s** (screenshot timeline, 3 runs); the window is blank in between. Over the 3 s target on the emulator; re-measure on physical devices (Phase 6) and consider a native window background/splash that matches the app so the gap is not blank.
2. **☑** About shows `LanguaLinker 0.1.0 · Contract version 0.2 · demo`. Over real IPC on Android, `get_app_info` returned `{"apiVersion":"0.2","appName":"LanguaLinker","appVersion":"0.1.0","mode":"dev"}` and `plugin:window|close` was rejected by the ACL.
3. **☑** All 5 sections reached by tapping; each move focuses the new screen's heading; Back walks history (settings → stats → editor). Defect found: Back while a dialog is open navigates away instead of closing the dialog (fixed 2026-10-14 via router layers, see ADR-12 finding 3; re-check on the next device run).
4. **☑** Dialog opens by tap, focus moves into it, Cancel closes it and returns focus to "Open dialog"; toast renders above the bottom navigation.
5. **◐** Defect found and fixed: every keystroke in the search field navigated (`#/browser?q=…`), remounted the screen and moved focus to the heading, which closes the mobile keyboard after each character. After the fix focus stays in the field (`screenKey` in `frontend/src/app/routes.ts`, regression test in `tests/routes-render.test.ts`). Also fixed a duplicate clear button (WebView's native one next to ours). Not verified: layout resize, because this AVD has a hardware keyboard and Gboard shows only a floating toolbar — repeat on an AVD with `hw.keyboard=no` or a device.
6. **☑** `targetSdk` 34 is not edge-to-edge: the WebView sits between the status bar (0–63 px) and the gesture bar (2337–2400 px); `env(safe-area-inset-*)` is 0. Re-test when `targetSdk` reaches 35+, where Android 15+ enforces edge-to-edge.
7. **☑** Landscape 914×363 CSS px switches to the sidebar layout; no horizontal overflow on home, browser, stats, settings, gallery; portrait restored correctly.
8. **☑ after fix.** Android WebView applies the system font scale as text zoom (body 16 → 32 px at scale 2.0). Defect found and fixed: input/select intrinsic widths and the 5-column nav grid exceeded the screen, so WebView widened the page and browser/editor/gallery scrolled horizontally. Now no horizontal scroll on all 14 routes. Remaining: bottom-nav labels break mid-word at 2.0 ("Statis·tics"; `hyphens: auto` added 2026-10-14 so devices with hyphenation data break at syllables — re-check); WebView text zoom also scales `px`/`vw`, so CSS cannot cap it — see ADR-12.
9. **◐** Accessibility tree (what TalkBack reads): nav items exposed as "Decks/Browse/Add/Statistics/Settings"; buttons as "Study everything" and "Study Niemiecki — słówka A1" etc. (deck-specific names). TalkBack is installed but spoken output was not listened to.
10. **☑** `logs/langualinker.2026-10-12.log` exists; lines are `app started app_version="0.1.0" app_variant="dev" platform="android"`; no card or deck text.

Emulator settings changed for the run and restored: `font_scale` 1.0, `show_ime_with_hard_keyboard` 0. Auto-rotate (`accelerometer_rotation`) was left off.
