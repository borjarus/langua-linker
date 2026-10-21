# Device smoke checklist

Phase 1: Android emulator and iOS simulator (owner decision 2026-10-12). Phase 6 gate: the same checklist on physical Android and iOS devices; emulator results do not count there.

Step-by-step guide for a run (setup, adb commands, Phase 5 scenarios, cleanup): [emulator-testing.md](emulator-testing.md).

Run with the dev variant, which bundles the demo frontend (`npm --prefix frontend run android:dev` / `android:apk`, `variant -- dev ios dev`); package `com.mila.langualinker.dev`. Record date, device, OS version, build commit and result for each row; attach screenshots in the PR.

Legend: ☑ pass · ◐ partly verified (see notes) · ☒ fail · ☐ not run. Android column = emulator run of 2026-10-12 (rows 1–10) and 2026-10-16 (rows 11–16) below.

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
| 11 | Phase 3 · Study: "Show answer" moves TalkBack/VoiceOver focus to the answer; rating buttons are announced with their interval; after a rating focus lands on the next question | ☑ | ☐ |
| 12 | Phase 3 · Study and summary at maximum system font: card, rating row, toolbar and rating distribution are not clipped; no horizontal page scroll | ☑ | ☐ |
| 13 | Phase 3 · Word sheet and card-actions sheet dock to the bottom above the home indicator; hardware/gesture Back closes the sheet, not the screen | ☑ | ☐ |
| 14 | Phase 3 · Back during an open session asks "Leave the session?"; Keep studying stays, Leave returns to the decks with "Resume session" | ☑ | ☐ |
| 15 | Phase 3 · TTS button: iOS speaks with an installed voice; Android WebView shows "Speech is not available on this device" (ADR-12) | ☑ | ☐ |
| 16 | Phase 3 · Onboarding (demo scenario "New user") → study → End session → summary → decks completes with touch only | ☑ | ☐ |
| 17 | Phase 5 · Onboarding wizard (native → target → goal → decks → ready): focus/TalkBack on each step title; decks prepared; from Settings it returns there ([guide 4.1](emulator-testing.md#41-onboarding-scenario-nowy-użytkownik), [4.10](emulator-testing.md#410-re-onboarding-from-settings)) | ☐ | ☐ |
| 18 | Phase 5 · Statistics: filters, every chart with tooltip by touch and a table view, empty and error states; no horizontal scroll at maximum font ([4.2](emulator-testing.md#42-statistics)) | ☐ | ☐ |
| 19 | Phase 5 · Settings: each change shows "Saved"; language switch reaches navigation and banner; TTS off hides the listen button ([4.3](emulator-testing.md#43-settings)) | ☐ | ☐ |
| 20 | Phase 5 · AI: consent can be declined and study still works; HTTPS-only endpoint; API key field clears and the key is never shown ([4.4](emulator-testing.md#44-ai-and-associations)) | ☐ | ☐ |
| 21 | Phase 5 · Privacy: reset needs the phrase or a fresh safety backup; delete everything leads to onboarding; Back does not dismiss the dialog mid-way ([4.5](emulator-testing.md#45-privacy-reset-and-deletion)) | ☐ | ☐ |
| 22 | Phase 5 · About and licences: versions, redacted diagnostic log; record whether "Copy log" works in the WebView ([4.6](emulator-testing.md#46-about-and-licenses)) | ☐ | ☐ |
| 23 | Phase 5 · Import & export: dry run counts and row errors, delimiter/mapping/duplicates, import result, APKG instructions, closed picker, export + share ([4.7](emulator-testing.md#47-import-and-export)) | ☐ | ☐ |
| 24 | Phase 5 · Backup & restore: create, retention, folder, corrupt/newer file rejected, restore with reopen, failed restore offers the safety copy; dialog scrolls at 200% ([4.8](emulator-testing.md#48-backup-and-restore)) | ☐ | ☐ |
| 25 | Phase 5 · Declined AI, telemetry and notifications do not block a study rating ([4.9](emulator-testing.md#49-declining-ai-telemetry-and-notifications-does-not-block-study)) | ☐ | ☐ |
| 26 | Phase 5 · All Phase 5 screens in light and dark, at 200% system and in-app text, landscape, with TalkBack/VoiceOver and Back ([section 5](emulator-testing.md#5-criteria-for-each-screen)) | ☐ | ☐ |

## How the checks were performed

- **Android:** Debug builds of the dev variant expose the WebView to Chrome DevTools: `adb forward tcp:9222 localabstract:webview_devtools_remote_<pid>`, then `http://localhost:9222/json` and `Runtime.evaluate` read the DOM, focus, viewport and layout. Touch input went through `adb shell input tap/text/keyevent`; the Android accessibility tree through `adb shell uiautomator dump`.
- **Android, Phase 3 (2026-10-16):** TalkBack enabled with `adb shell settings put secure enabled_accessibility_services com.google.android.marvin.talkback/com.google.android.marvin.talkback.TalkBackService`. Evidence is TalkBack's own accessibility-focus ring in screenshots plus the Android accessibility tree (`uiautomator dump`, what TalkBack reads, including WebView content); spoken audio was not listened to. Taps were real `adb shell input tap` events at element centres located over DevTools; note that injected taps bypass TalkBack's explore-by-touch, so they activate elements directly instead of only moving focus — swipe/double-tap navigation is not covered by this run.
- **iOS:** Built and run via `npm --prefix frontend run ios:dev -- "iPhone 17"`. Screenshots captured via `xcrun simctl io booted screenshot`. DOM state, focus, insets, styling and Tauri IPC (`__TAURI_INTERNALS__.invoke`) inspected over WKWebView connection. App data container and log files inspected via `xcrun simctl get_app_container`. Font scaling verified via `xcrun simctl ui booted content_size`.

## Results

| Date | Device | OS | Commit | Result / notes |
|---|---|---|---|---|
| 2026-10-12 | Android Emulator `Medium_Phone_API_36.1` (x86_64, Play Store image), WebView 134.0.6998.135 | Android 16 (API 36.1) | `8ee33b0` + uncommitted changes | See Android notes below. |
| 2026-10-16 | Android Emulator `Medium_Phone_API_36.1` (x86_64, Play Store image), WebView 134.0.6998.135, TalkBack (Android Accessibility Suite) | Android 16 (API 36.1) | `144f051` (dev debug APK, x86_64) | Phase 3 rows 11–16, see notes below. Screenshots `docs/release/screenshots/android-phase3-*.png`. |
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

### Notes for the 2026-10-16 Android emulator run (Phase 3, rows 11–16):

11. **☑ TalkBack focus in study.** After "Show answer" TalkBack's focus ring sits on the answer (`android-phase3-11-answer-focus.png`; DOM focus `div.back`). After a rating it sits on the next question (`android-phase3-11-next-question-focus.png`). Accessibility tree: card region "Question"; rating group "How well did you remember?" with buttons "Again — next review in 1 minute", "Hard — next review in 6 minutes", "Good — next review in 10 minutes", "Easy — next review in 4 days"; the TTS button is exposed as disabled with its reason as the label. Not covered: swipe navigation order and spoken output.
12. **☑ after fix. Maximum system font (`font_scale` 2.0, body 32 px).** Dashboard, study (answer revealed) and summary: no horizontal page scroll, no control outside the viewport, no clipped control text (DevTools check over `main` buttons, links, meters, card sides, rating rows). Defect found: the rating row kept three 121 px columns and broke intervals mid-word ("minute/s", `android-phase3-12-ratings-font-200-before.png`). Cause: Android WebView text zoom enlarges drawn text but not CSS lengths — `getComputedStyle` reports 32 px while `1em` and `1rem` both lay out as 16 px — so no `rem`/`em` column minimum can follow it. Fix: the rating row is a wrapping flex row whose items' minimum is their min-content (longest real word); re-run on a rebuilt APK shows two 186 px columns with "10 minutes" on one line (`android-phase3-12-ratings-font-200.png`). Still open from row 8: bottom-nav labels break mid-word ("Brows/e", "Statis/tics"). Rating distribution and Again list on the summary wrap correctly.
13. **☑ Sheets.** "More card actions" and the word sheet ("gemütlich": Word · linked word, translation, grammar tips, associations) dock to the bottom edge; TalkBack focus moves to the sheet's Close button; hardware Back closes the sheet and stays on the study screen (`android-phase3-13-*.png`). Fixed in the same run: the word sheet showed an expanded "Grammar tips (0)" section for a word without tips; empty sections are now left out (re-checked: "gemütlich" shows only "Associations 1", `android-phase3-13-word-sheet-font-200.png`). After a clean start at font scale 2.0, Back on the word sheet closes the sheet and stays on the study screen. Observation: once, after changing the system font while the app was running, Back closed the app instead (activity recreated with a fresh WebView history); not reproduced after a clean start — re-check on a device. Safe area: `targetSdk` 34 is not edge-to-edge, so the sheet ends above the gesture bar (see row 6).
14. **☑ Leave guard.** Back during a session opens "Leave the session?"; "Keep studying" closes it and restores `#/study/all?session=…`; Back again and "Leave session" returns to the decks with "Unfinished session · All decks · 1 review · 19 cards left · Resume session".
15. **☑ TTS (Android).** The WebView has no `speechSynthesis`/`SpeechSynthesisUtterance`; the button reads "Speech is not available on this device" with `aria-disabled="true"` and `data-availability="unsupported"`, as ADR-12 expects. iOS still to run.
16. **☑ Touch flow.** Scenario "New user" (set over DevTools as test setup) → "Get started" → decks → "Study everything" → 5 ratings (Again, Good, Easy, Hard, Good) → "End session" → summary "All decks · 5 reviews", 23 s, 80% without "Again" (`android-phase3-16-summary.png`) → "Go to decks" with no resume card. All steps by real taps.

Fixed in the same run: the keyboard hint "Space shows the answer, 1–4 rate" is hidden on touch-only devices (`(hover: none) and (pointer: coarse)`; verified `display: none` on the emulator). Found while driving the run: reloading the page during a session (possible in the browser demo, not in the native shell) showed the browser's "Changes you made may not be saved" dialog although ratings are saved; the study guard no longer blocks `beforeunload` (router guard option `blocksUnload: false`; the editor draft guard still does). Emulator settings changed for the run and restored: TalkBack off, `accessibility_enabled` 0, `font_scale` 1.0.

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
