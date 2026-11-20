# Device smoke checklist

Phase 1: Android emulator and iOS simulator (owner decision 2026-10-12). Phase 6 gate: the same checklist on a physical Android device and on the iOS simulator (owner decision 2026-10-29: no iPhone available for testing; the physical iPhone run is carried over), plus rows G1–G8 of [phase6-gate.md](phase6-gate.md#step-7--physical-device-protocol); Android emulator results do not count there.

Step-by-step guides: emulator and simulator — [emulator-testing.md](emulator-testing.md); physical Android device — [android-device-testing.md](android-device-testing.md).

Run with the dev variant, which bundles the demo frontend (`npm --prefix frontend run android:dev` / `android:apk`, `variant -- dev ios dev`); package `com.mila.langualinker.dev`. Record date, device, OS version, build commit and result for each row; attach screenshots in the PR.

Legend: ☑ pass · ◐ partly verified (see notes) · ☒ fail · ☐ not run. Android column = physical-device run of 2026-10-30 (Phase 6 gate, all rows passed); the earlier emulator runs of 2026-10-12 (rows 1–10) and 2026-10-16 (rows 11–16) are described in the notes below.

| # | Check | Android | iOS |
|---|---|---|---|
| 1 | App starts; demo banner and decks screen visible within 3 s | ☑ | ☑ |
| 2 | Settings → About shows app name, version and contract version (demo build: from the mock; `get_app_info` over IPC checked separately, see notes) | ☑ | ☑ |
| 3 | Bottom navigation reaches all 5 sections; hardware/gesture back returns to the previous screen | ☑ | ☑ |
| 4 | `#/gallery`: dialog opens/closes, toast appears above the bottom navigation | ☑ | ☑ |
| 5 | Search field: on-screen keyboard does not cover the input; layout resizes | ☑ | ◐ |
| 6 | Safe areas: notch/status bar and home indicator do not overlap content or navigation | ☑ | ☑ |
| 7 | Rotate to landscape and back: no clipped controls | ☑ | ☑ |
| 8 | System font size at maximum: text scales, no horizontal page scroll | ☑ | ☑ |
| 9 | TalkBack / VoiceOver: navigation items and the "Study" button are announced with labels | ☑ | ◐ |
| 10 | App log file exists in the app log directory and contains no card text (`adb shell "run-as com.mila.langualinker.dev ls logs"` / Xcode container) | ☑ | ☑ |
| 11 | Phase 3 · Study: "Show answer" moves TalkBack/VoiceOver focus to the answer; rating buttons are announced with their interval; after a rating focus lands on the next question | ☑ | ☐ |
| 12 | Phase 3 · Study and summary at maximum system font: card, rating row, toolbar and rating distribution are not clipped; no horizontal page scroll | ☑ | ☐ |
| 13 | Phase 3 · Word sheet and card-actions sheet dock to the bottom above the home indicator; hardware/gesture Back closes the sheet, not the screen | ☑ | ☐ |
| 14 | Phase 3 · Back during an open session asks "Leave the session?"; Keep studying stays, Leave returns to the decks with "Resume session" | ☑ | ☐ |
| 15 | Phase 3 · TTS button: iOS speaks with an installed voice; Android WebView shows "Speech is not available on this device" (ADR-12) | ☑ | ☐ |
| 16 | Phase 3 · Onboarding (demo scenario "New user") → study → End session → summary → decks completes with touch only | ☑ | ☐ |
| 17 | Phase 5 · Onboarding wizard (native → target → goal → decks → ready): focus/TalkBack on each step title; decks prepared; from Settings it returns there ([guide 4.1](emulator-testing.md#41-onboarding-scenario-nowy-użytkownik), [4.10](emulator-testing.md#410-re-onboarding-from-settings)) | ☑ | ☐ |
| 18 | Phase 5 · Statistics: filters, every chart with tooltip by touch and a table view, empty and error states; no horizontal scroll at maximum font ([4.2](emulator-testing.md#42-statistics)) | ☑ | ☐ |
| 19 | Phase 5 · Settings: each change shows "Saved"; language switch reaches navigation and banner; TTS off hides the listen button ([4.3](emulator-testing.md#43-settings)) | ☑ | ☐ |
| 20 | Phase 5 · AI: consent can be declined and study still works; HTTPS-only endpoint; API key field clears and the key is never shown ([4.4](emulator-testing.md#44-ai-and-associations)) | ☑ | ☐ |
| 21 | Phase 5 · Privacy: reset needs the phrase or a fresh safety backup; delete everything leads to onboarding; Back does not dismiss the dialog mid-way ([4.5](emulator-testing.md#45-privacy-reset-and-deletion)) | ☑ | ☐ |
| 22 | Phase 5 · About and licences: versions, redacted diagnostic log; record whether "Copy log" works in the WebView ([4.6](emulator-testing.md#46-about-and-licenses)) | ☑ | ☐ |
| 23 | Phase 5 · Import & export: dry run counts and row errors, delimiter/mapping/duplicates, import result, APKG instructions, closed picker, export + share ([4.7](emulator-testing.md#47-import-and-export)) | ☑ | ☐ |
| 24 | Phase 5 · Backup & restore: create, retention, folder, corrupt/newer file rejected, restore with reopen, failed restore offers the safety copy; dialog scrolls at 200% ([4.8](emulator-testing.md#48-backup-and-restore)) | ☑ | ☐ |
| 25 | Phase 5 · Declined AI, telemetry and notifications do not block a study rating ([4.9](emulator-testing.md#49-declining-ai-telemetry-and-notifications-does-not-block-study)) | ☑ | ☐ |
| 26 | Phase 5 · All Phase 5 screens in light and dark, at 200% system and in-app text, landscape, with TalkBack/VoiceOver and Back ([section 5](emulator-testing.md#5-criteria-for-each-screen)) | ☑ | ☐ |

## How the checks were performed

- **Android:** Debug builds of the dev variant expose the WebView to Chrome DevTools: `adb forward tcp:9222 localabstract:webview_devtools_remote_<pid>`, then `http://localhost:9222/json` and `Runtime.evaluate` read the DOM, focus, viewport and layout. Touch input went through `adb shell input tap/text/keyevent`; the Android accessibility tree through `adb shell uiautomator dump`.
- **Android, Phase 3 (2026-10-16):** TalkBack enabled with `adb shell settings put secure enabled_accessibility_services com.google.android.marvin.talkback/com.google.android.marvin.talkback.TalkBackService`. Evidence is TalkBack's own accessibility-focus ring in screenshots plus the Android accessibility tree (`uiautomator dump`, what TalkBack reads, including WebView content); spoken audio was not listened to. Taps were real `adb shell input tap` events at element centres located over DevTools; note that injected taps bypass TalkBack's explore-by-touch, so they activate elements directly instead of only moving focus — swipe/double-tap navigation is not covered by this run.
- **iOS:** Built and run via `npm --prefix frontend run ios:dev -- "iPhone 17"`. Screenshots captured via `xcrun simctl io booted screenshot`. DOM state, focus, insets, styling and Tauri IPC (`__TAURI_INTERNALS__.invoke`) inspected over WKWebView connection. App data container and log files inspected via `xcrun simctl get_app_container`. Font scaling verified via `xcrun simctl ui booted content_size`.

## Results

| Date | Device | OS | Commit | Result / notes |
|---|---|---|---|---|
| 2026-10-12 | Android Emulator `Medium_Phone_API_36.1` (x86_64, Play Store image), WebView 134.0.6998.135 | Android 16 (API 36.1) | `8ee33b0` + uncommitted changes | See Android notes below. |
| 2026-10-16 | Android Emulator `Medium_Phone_API_36.1` (x86_64, Play Store image), WebView 134.0.6998.135, TalkBack (Android Accessibility Suite) | Android 16 (API 36.1) | `144f051` (dev debug APK, x86_64) | Phase 3 rows 11–16, see notes below. Screenshots `docs/release/screenshots/android-phase3-*.png`. |
| 2026-10-30 | Physical Android device (Phase 6 gate, step 7) | — | `33c44e0` (dev build) | Rows 1–26 and G1–G8 passed, reported by the product owner; device details not recorded here. |
| 2026-11-02 | Android Emulator `Medium_Phone_API_36.1` (x86_64, Play Store image), WebView 155.0.8059.43 | Android 16 | `33c44e0` + uncommitted Phase 7 changes (dev debug APK, x86_64) | Phase 7 backend: P1–P8 below. Screenshot `docs/release/screenshots/android-phase7-start.png`. |
| 2026-10-13 | iOS Simulator `iPhone 17` (arm64), WKWebView / MobileSafari | iOS 26.5 | `001aaf3` | See iOS notes below. Screenshots in `docs/release/screenshots/`. |
| 2026-11-06 | Physical Samsung Galaxy A56 `SM-A566B` (arm64-v8a), WebView 156.0.8078.47 | Android 16 (API 36) | `8e437d8` (beta debug APK; beta release profile signed with the debug key for H) | Phase 10 run, parts A–E, G, H done remotely over adb; F (TalkBack) and I (iOS) not run. See notes below. Screenshots `docs/release/screenshots/android-phase10-*.png`. |
| 2026-11-10 | Physical Samsung Galaxy A56 `SM-A566B` (arm64-v8a), WebView 156.0.8078.47 | Android 16 (API 36) | `969f939` + Android build fix in `files.rs` (beta debug APK; beta release profile signed with the debug key for the start check) | Phase 11 and 12 run, remotely over adb; Phase 13 left for a later run with a real key. **Two P1 blockers** (start race, Browse paging). See notes below. Screenshots `docs/release/screenshots/android-phase11-12-*.png`. |
| 2026-11-17 | Physical Samsung Galaxy A56 `SM-A566B` (arm64-v8a), WebView 157.0.8103.40 | Android 17 (API 37) | `1cd3db7` + fixes below (beta debug APK `0.5.0-beta.1`, targetSdk 36) | Pre-release check of 0.5.0-beta.1 (Phase 17 changes), remotely over adb. **Two P1 defects found and fixed in the run.** See notes below. |
| 2026-11-18 | Physical Samsung Galaxy A56 `SM-A566B` (arm64-v8a), WebView 157.0.8103.50 | Android 17 (API 37) | `4402bde` + uncommitted Phase 18 changes (beta debug APK, Rust `opt-level 3` for the core crates) | ADR-22 optimizer run on a generated one-year history (owner's choice), restore of deleted bundled decks, progress reset afterwards. See notes below. Screenshots `docs/release/screenshots/android-phase18-*.png`. |
| 2026-11-19 | Physical Samsung Galaxy A56 `SM-A566B` (arm64-v8a), WebView 157.0.8103.50 | Android 17 (API 37) | `b6bf6e4` + fixes below (beta debug APK; re-checked on `0.5.1-beta.1`) | Pre-release check of 0.5.1-beta.1 (grammar tab, levels, reference decks, in-session words), remotely over adb, installed as an update over the 0.5.0-beta.1 collection. **One P1 layout defect and three P2/P3 found and fixed.** See notes below. |
| 2026-11-20 | Physical Samsung Galaxy A56 `SM-A566B` (arm64-v8a), WebView 157.0.8103.50 | Android 17 (API 37) | `8323a53` + fixes below (beta debug APK built without the on-device engine; CI dev APK with the engine) | Pre-release check of 0.5.2-beta.1 (card deck, more word links, English grammar, on-device model settings), remotely over adb, installed as an update over the 0.5.1-beta.1 collection. **One P1 and two P3 found and fixed.** See notes below. |

### 0.5.2-beta.1 physical Android run (2026-11-20)

Driven remotely over Chrome DevTools (`Runtime.evaluate` on the WebView socket; elements found by accessible name) with screenshots over adb. The PC could not build llama.cpp for Android (no CMake/libclang on Windows), so the device build is a **beta debug APK without the on-device engine** (local, uncommitted manifest change), signed with the local debug key and installed **as an update over 0.5.1-beta.1**; the collection was copied to the PC first. The engine build itself was checked by CI: the Mobile builds workflow built the dev APK for all four ABIs with llama.cpp (NDK r27, static libc++), and that APK started on the phone.

| Area | Result | Note |
|---|---|---|
| Update and content | ☑ | Migration 8 → 9 with the `pre-v9` copy; the three sentence decks moved to content 2026.11.20: 1 280 of 1 298 tips and 109 of 109 chapter titles translated (the 18 others are the owner's own tips); 10 060 cards, 115 answers and 16 360 word links unchanged. The `unhandledRejection` WebView error logged on the first start after the migration also appeared after migration 8 on 2026-11-19; not new. |
| Deck on the study card | ☑ | "Deck: Angielski A1 — zdania" above the chips. |
| Words without a link | ☒ → ☑ | IPA in slashes is never offered. **P1:** the new `addWordLink`, `downloadAiModel` and `deleteAiModel` answered `NOT_IMPLEMENTED`: the shell routes catalogue commands missing from `commands::IMPLEMENTED` to the pending handler, and the tests accepted that as consistent. Fixed: the commands are listed, and a test now requires the pending list to be empty. Then "fine" in "– How are you? – I'm fine, and you?" was linked to a new word "fine — w porządku, dobrze" (English word deck), shows "your choice" and survives a restart. **P3:** "m" of "I'm" (with "I" linked) was offered as a word; fixed: a word glued to a link is a fragment. **P3:** a tappable word was a `<button>`, an atomic box after which the line may break, so ", and" started a line; fixed: words are inline `span role="button"` with Enter/Space. |
| Grammar in the app language | ☑ | English: chapter "1. Useful phrases to get started" and English tips, examples and IPA unchanged; Polish: the original texts. App language restored to English. |
| On-device model settings | ☑ | Demo build (dev APK): provider "On this device" with the model's name and size, Wi-Fi note, simulated download to "Downloaded and verified", remove. Beta build without engine: the provider is not listed; `downloadAiModel` answers `PERMISSION_DENIED { aiProvider }`. A download icon replaced the refresh icon (P3). |
| App icon | ☑ | New "LL" icon generated for desktop, Android (with an adaptive icon on a #1D5A48 background) and iOS from `src-tauri/app-icon.svg`. |

State left on the phone (owner's collection): new word "fine" in "Angielski A1 — słowa" linked from that sentence; app language English as before; the dev APK installed for the run was removed.

### 0.5.1-beta.1 physical Android run (2026-11-19)

Driven remotely as before: Chrome DevTools reads the screen, taps are real `adb shell input tap` events at element centres (the WebView starts 101 px below the top of the screen, under the status bar), text through `adb shell input text`, native pickers through `uiautomator dump`. Installed **as an update over 0.5.0-beta.1** (collection with three sentence decks at content 2026.11.18, the word decks deleted by the owner, 42 answers); the collection was copied to the PC first.

| Area | Result | Note |
|---|---|---|
| Update and content | ☑ | Schema v8; the three sentence decks moved to content 2026.11.19 on the first start: levels (A1, Spanish A1–A2), 109 chapters, answers and progress unchanged. The 12 archived German cards are the ones archived on 2026-11-10. |
| Restore word decks ("Set up languages and decks again") | ☑ | The three word decks came back (302 / 670 / 1 274 cards) next to the installed sentence decks ("Already in your collection"). |
| Bottom navigation | ☒ → ☑ | **P1:** with the sixth tab (Grammar) the grid kept five columns: "Settings" wrapped to a second row and took ≈ 90 px of every screen. Fixed: one auto-sized column per tab. In Polish the labels then broke with hyphens ("Przeglą-daj", "Ustawie-nia"): the label is 8 % smaller and the side padding 2 px (rem-based, it still follows the text size). Checked live on the phone (EN and PL), then on the rebuilt APK. |
| Grammar tab | ◐ → ☑ | Deck picker with levels, 420 tips / 36 chapters (English), chapters expand, a tip opens its card. **P2:** Back from the card restored the scroll position but collapsed the chapter and reset deck and search, so the reader landed elsewhere. Fixed: the screen keeps deck, search and open chapters for the next visit. |
| Reference deck | ☑ | "Include in study sessions" off → "Reference only" chip, counts 0, no Study button, explanatory text, toast "Deck saved."; sentences still open its words in the word sheet (with the extended association). The checkbox used the platform blue (P3): now the theme colour. |
| Study card | ☑ | Association-count chip above the card; after the answer it opens and scrolls to the section. Four rating buttons in one row. Back closes the word sheet only. |
| Add a word from a sentence | ◐ → ☑ | A test sentence with an unresolved phrase ("Kuehlschrank", temporary deck): the sheet pre-fills the word, "Add and link" links it without leaving the session ("your choice"). **P2:** the deck offered was the sentence's own deck, not the word deck: the rule "first deck without word links" never matches bundled word decks, which are Linguistic. Fixed: `getEditorData` reports `wordCount` per deck (contract 1.8) and the sheet picks the deck of the language with the most words. The test deck and word were deleted afterwards. |
| Card details | ◐ → ☑ | P3: "Asks: \*\*I am here.\*\*" showed the Markdown marks. Fixed (rich text). |
| Setup deck list | ◐ → ☑ | P3: sentence-deck descriptions read "…gramatycznymi Obie strony karty."; the level chip showed "A1-A2". Descriptions fixed in the content (new installs; installed decks keep their own), level shown as A1–A2. |

State left on the phone (owner's collection): the three word decks restored and set to **Reference only** (the owner had deleted them, so they stay out of sessions but serve the sentence links); app language English as before; 8 answers given in the German sentence deck during the run were kept.

### Phase 18 optimizer run on the Galaxy A56 (2026-11-18)

For the ADR-22 acceptance (time, memory, held-out result on the phone). Driven remotely as in the earlier runs (Chrome DevTools for reading the screen, a real `input tap` for the start). Installed **as an update** over the 0.5.0-beta.1 collection (schema v5 → v6, safety copy `collection.pre-v6.sqlite` written).

**Build.** Beta debug APK (debuggable, so the database could be prepared over `run-as`), with the Rust core crates (`domain`, `application`, `infrastructure`, SQLite) at `opt-level 3` without debug assertions or overflow checks; the shell crate at `opt-level 1` (opt-level 3 ran out of memory on the build PC). The optimizer code is in the core crates, so the times stand for a release build.

**Collection.** The phone held only 81 real answers (minimum 200), all decks deleted. The owner chose to generate the history for the run and reset it afterwards:

1. The six bundled decks were brought back through *Set up languages and decks again* — this also checked the new restore of deleted bundled decks: all six back with 10 060 cards and their earlier progress, decks screen after 2.4 s.
2. A one-year history was generated on the PC for the live cards and written into the pulled database (`history-gen` over `langualinker_domain::optimizer::replay_memory`): 20 new cards a day for 365 days, a first answer and a same-day step, reviews at 0.5–1.6× the scheduled interval answered by sampling the recall of **known weights that differ from the defaults** (w0 0.6, w2 5.0, w8 1.4, w9 0.25, w11 1.9, w20 0.35 — the recovery-test "truth"), a relearning step after each lapse. Result: 7 300 cards, 89 604 answers; on the screen 89 686 logs, **68 524 usable**, 3 undone excluded.

| Check | Result |
|---|---|
| Time (job `started_at` → `finished_at`: read history, train, held-out evaluation, workload, store run) | **287 ms**, 285 ms, 303 ms (3 runs; 54 860 training answers, 274 300 progress units). Desktop reference: 1.0 s for 450 000 answers. |
| Memory | PSS before the start 232 MB, peak **259 MB** right after the run (with the result rendered), 249 MB afterwards (`dumpsys meminfo`, ≈ 0.15 s samples). Runs 2–3 sampled from `/proc/<pid>/status` every ≈ 20 ms: RSS **+10.6 MB** and **+8.4 MB** during the run; `VmHWM` (328 MB, from the start and onboarding) unchanged. |
| Held-out result | Recommended. Log loss **0.3229 → 0.3185**, calibration RMSE **2.7 % → 0.8 %** on 13 664 answers of cards never trained on. Workload preview: 257.9 → 334.9 reviews/day, median interval 140 → 67 days. Proposed weights moved from the defaults towards the generating weights: w0 0.21 → 0.50 (0.6), w2 2.31 → 5.57 (5.0), w8 1.87 → 1.55 (1.4), w9 0.17 → 0.23 (0.25), w20 0.15 → 0.25 (0.35); w11 1.48 → 1.38 (1.9) did not. |
| Same data, same result | Runs 2 and 3 gave the same metrics (deterministic). |
| Apply / restore | "Use the new weights" → status *Applied*, toast "New weights saved in “Default”"; "Restore previous weights" → *Reverted*, preset weights equal to the weights before the run (parsed values). Review logs and card schedules unchanged by both (counts, sums of due dates and stabilities compared). |
| Clean-up | *Reset progress* (typed confirmation, no in-app backup: it would only hold the generated history; copies of the database before and after are on the test PC): 0 review logs, 10 060 cards new, 6 decks and their notes kept, `integrity_check` ok. The three optimizer runs stayed in *Earlier runs* (by design they survive a progress reset); deleted afterwards, see below. |

Screenshots: `android-phase18-optimizer-start.png`, `-done.png` (the run finished before a progress screenshot could be taken), `-applied.png`, `-reverted.png`.

Open:

- ~~P2 — the comparison table breaks words at phone width~~ (headers "Curre/nt", "Propo/sed", log loss "0.322/9"; cause: the global `overflow-wrap: anywhere` lets table columns shrink to one character). **Fixed the same day**: in the optimizer tables labels wrap at spaces only, values and value headers never (the wrapper scrolls if needed). Checked in Chromium at the phone's CSS width (411 px, demo adapter): no word broken, no horizontal overflow. Not re-checked on the phone: the runs were deleted first.
- The run is on generated answers, not on a learner's own history; the owner accepted this for the measurement, then accepted the screen and ADR-22 (2026-11-18).
- Afterwards, at the owner's request, the three optimizer runs and their `optimize` jobs were deleted from the phone's collection (database edited over `run-as`, app stopped); the 9 answers given on the phone after the reset were kept.

### 0.5.0-beta.1 physical Android run (2026-11-17)

Driven remotely from the PC as in the earlier runs (`adb shell input tap/text` at element centres read over Chrome DevTools). Installed **as an update over the 0.4.0-beta.1 collection** (7 decks, 5 025 notes, 6 149 cards, 41 reviews, unfinished session). The APK was built from a worktree on an NTFS drive: the usual checkout is on exFAT, where `tauri android build` cannot create the `jniLibs` symlink ("Incorrect function").

| Area | Result | Note |
|---|---|---|
| Update 0.4.0-beta.1 → 0.5.0-beta.1 | ☒ → ☑ | Every command failed with "An unexpected error occurred": `migration 1 checksum mismatch`. Migrations were hashed as raw bytes; the worktree was checked out with `core.autocrlf=true`, so the compiled SQL had CRLF. The same applies to the Windows job of the release workflow (an Android backup would not open on Windows). Fixed: the checksum is taken over LF-normalized SQL and the raw CRLF hash is still accepted; `.gitattributes` keeps the migrations LF. After the fix: collection opened, data and the unfinished session intact. |
| External links (Licences) | ☒ → ☑ | The first tap on a licence link froze the app for good (UI and DevTools dead). `on_navigation` runs on the Android main thread inside `shouldOverrideUrlLoading`, and the opener's Kotlin call waits for that thread. Fixed: the link is opened from another thread. After the fix: Chrome opens the page, Back returns to the same screen and scroll position. |
| Theme switch while open | ◐ → ☑ | Light → dark system theme left the status and navigation bars light until a restart (`uiMode` is in `configChanges`). Fixed in `MainActivity.onConfigurationChanged`. |
| Edge-to-edge | ☑ | Portrait, landscape (content clear of the camera cutout and the 3-button bar, no horizontal scroll) and dark: header below the status bar, navigation above the system bar. |
| Keyboard | ☑ | Report comment field: viewport shrinks 748 → 493 px, field and buttons above the keyboard. |
| Licences, Privacy | ☑ | 367 components with licences. Privacy text as in the changelog; no policy link (no `VITE_PRIVACY_POLICY_URL` in this build). |
| Association report | ☑ | Report → reason, comment, full message → Open e-mail → system chooser (Gmail, Outlook, Proton Mail…) and the "Thank you…" toast. Not sent. |
| Study | ☑ | Resume session → Show answer (focus to the answer) → Good (2 → 3 reviews, next card) → Undo (back to the same card) → End session → summary. |
| Backup | ☑ | Create backup (7.4 MB, retention applied); restore preview shows matching counts (not confirmed). |

Open:

- **P2 — the app closes about 1 s after a launch made right after `adb install -r`** (3 of 4 times; once with `FORTIFY: pthread_mutex_lock called on a destroyed mutex`). It happens during the `PACKAGE_REPLACED` broadcast, with a second `MainActivity` started by System UI. It was never seen on an ordinary start (8 starts).
- **P3** — Back does not close the restore preview dialog (Cancel does); the association report quotes the raw Markdown front (`**Danke.**`).

Not run: TalkBack listening, reminder delivery, restore and uninstall/reinstall, iOS.

### Phase 11–12 physical Android run (2026-11-10)

Driven remotely from the PC as in the Phase 10 run: `adb shell input tap/text` (real touch events and the phone keyboard, Samsung Keyboard), coordinates and state read from the WebView over Chrome DevTools, database copies over `run-as`. Not listened to with TalkBack. Wi-Fi on (no network feature was tested). The beta debug APK was installed **as an update over the Phase 10 collection** (schema v3).

| Field | Value |
|---|---|
| Date | 2026-11-10 |
| Device / ABI | Samsung Galaxy A56 `SM-A566B` / arm64-v8a |
| Android / API | 16 / 36 |
| WebView | com.google.android.webview 156.0.8078.47 |
| Phone time zone | CET+0100 |
| Commit | `969f939` + fix below |
| Build | beta debug APK; beta release profile signed with the debug key (start check only) |

**Build.** `969f939` does not compile for Android: `pick_backup_folder` ([files.rs](../../src-tauri/src/files.rs)) calls `blocking_pick_folder`, which the dialog plugin has only on desktop; the mobile early return used `cfg!()`, a run-time value, so the call was still compiled. Fixed with `#[cfg]` blocks (desktop `cargo check` still passes).

| Area | Result | Note / screenshot |
|---|---|---|
| Update v3 → v4 | ☑ | `migration applied schema_version=4`, integrity `ok`; the content job re-prepared the bundled decks (1 492 notes created, 1 updated, 42 unchanged, 12 archived — the old unstudied sentence cards) in ≈ 3 s; all 41 logs, every due date/S/D/reps/flag/suspension/deletion unchanged (only 21 expired burials released); collection zone now `Europe/Warsaw` (Phase 10 defect 1 fixed). |
| P11 browser search | ☑ | Over the real IPC on 1 686 cards: every query p95 ≤ 50 ms (`is:due` = 14 = dashboard); diacritics folded (`haus` finds *Häuser*). **50 000 cards on the phone** (`browser_search_p95_on_fifty_thousand_cards`, aarch64 release): **p50 45 ms, p95 140 ms, max 141 ms** — DoD < 200 ms met. |
| P11 browser list | ☒ | Only the first page (100 rows) ever shows. Scrolling past row 100 requests page 1 with `cursor: "100"` (an offset, [browserController.svelte.ts](../../frontend/src/features/browser/browserController.svelte.ts) `#request`), the backend wants the opaque keyset cursor it returned (`nextCursor`) and answers `VALIDATION_FAILED cursor/format`, and the whole list turns into "Correct the search to see results." (`android-phase11-12-P11-browse-page2-error.png`). Frames up to that point: p50 8.3 ms, p95 16.7 ms, max 33 ms. |
| P11 notes | ☑ | New Basic-and-reversed note → 2 cards, tag, own association, 2 tips saved; tip delete (soft); flag; favourite; editing a **studied** bundled note keeps due/S/D/reps and records the override (`back`); type change → reverse card archived with the warning in the preview, switching back restores the **same card id**; duplicate front warns but saves (`…-duplicate-warning.png`); delete note soft-deletes note and card; empty back blocks saving. |
| P11 decks/presets | ☑ | Create, rename, move under another deck; the child is not offered as a parent and the backend refuses it (`parentId: cycle`); preset editor warns about 3 sharing decks, validates (retention 101, steps `1 abc`), "save as a copy" creates `Default (copy)` (5/day, 95 %) for that deck only. |
| P11 bulk | ☑ | "Select all 1 384 matches" → Suspend → preview → applied in one operation (all `queue -1`), then Unsuspend (all `queue 0`). |
| P12 statistics | ☑ | Real data, matches the database: 39 valid reviews, 7 min (6.9), retention "not enough data", 14 overdue = dashboard, states sum to 1 688. |
| P12 settings | ◐ | Daily goal, language, theme save at once and survive a restart; but after a restore or reinstall the UI language and theme come from WebView `localStorage`, not from the restored settings (see defect 4). |
| P12 backup/share | ☑ | Manual backup: manifest, SHA-256, integrity, counts correct, no key material. *Share* opens the system save dialog; the copy in Downloads has the same size. |
| P12 restore | ☑ | From an in-app backup: preview counts right, safety copy first (holds the newer note), restored tables identical to the backup. **Backup in Downloads → uninstall → install → "Restore from a file…" → notes, cards (GUID, due, S/D, reps/lapses) and review logs identical**; only `jobs`/`operations` differ (`…-restore-from-file-preview.png`). |
| P12 reset/delete | ☑ | Reset progress needs a fresh backup or the phrase; it removes logs, undo and sessions, cards back to New, suspension kept. Delete everything empties the collection, keeps the backups, opens onboarding; restoring the safety copy brings everything back identically. |
| Start (release profile) | ☒ | **5 of 5 cold starts end on "Something went wrong"** on the dashboard (≈ 6 s); *Refresh* works later (`…-REL-startup-error.png`). Same on the debug build (`…-P12-startup-dashboard-error.png`). See defect 1. |

Defects (proposed priority):

1. **P1 (blocks release) — commands fail during the first seconds after start.** `setup` registers `Backend` only after `Backend::open_with` returns, and `ContentService::new` runs `Bundle::load(…, Mode::Dev)` eagerly: it decrypts the whole `decks.7z` (7-Zip AES, 2^19 SHA-256 rounds), unpacks and validates all content. Timed with temporary logs on the debug build: **6.44 s** inside `ContentService::new`, everything else ≈ 85 ms. Until then every command answers the string error "state not managed for field `backend`", shown as "An unexpected error occurred." (dashboard, card details; no retry except *Refresh* on the dashboard). While the phone was locked during the first start after the update, the gap was ≈ 3 min. Suggested: load the bundle lazily or on a background thread, and answer a retryable code (or let the frontend wait for `app-ready`) instead of the transport error.
2. **P1 — Browse shows only the first 100 rows** (above): the controller sends an offset as `cursor`; the mock accepts it, the real backend does not.
3. **P1 (content) — all 1 384 bundled sentence notes have Markdown and IPA in `front`** (`"**Danke.** /ˈdaŋkə/"`), shown literally in study, card details and Browse (`…-P11-sentence-raw-markdown.png`); there is no Markdown renderer in the frontend.
4. **P2 — restored/reinstalled UI language and theme are not applied.** The backend keeps `uiLanguage`/`theme`, but [bootstrap.ts](../../frontend/src/app/bootstrap.ts) reads them only from `localStorage`; after restore on a fresh install the app shows English/light and the Settings screen shows `en`/System while the stored settings say `pl`/dark (`…-P12-restored-settings-not-applied.png`). Consequence: "Delete everything" in the English UI asks for the Polish phrase "USUŃ WSZYSTKO" (the backend picks the phrase from its stored language).
5. **P2 — two automatic backups at once.** Start and the first foreground event both call `start_automatic`, which checks `jobs.running()` and then creates the job (not atomic, [backup.rs](../../crates/infrastructure/src/backup.rs)). Seen twice: once the second job failed with `NOT_FOUND backup`, once both succeeded (two "Automatic" copies at 2:31 PM), which with "keep 3" pushes out older copies.
6. **P2 — "Choose a folder…" on Android** answers "This action requires permission." The folder target is desktop-only by design; the button and the hint "survives uninstalling" should not be offered on mobile (or should point to *Share*) (`…-P12-choose-folder-android.png`).
7. **P2 — picked restore file shown as "1429"** (the content URI document id) instead of its file name.
8. **P2 — search parse error shows an empty token**: "“” is not supported…"; the error details carry `field` and `reason` but no `token` (`…-P11-search-parse-error.png`).
9. **P3** — bulk preview "Notes: 1384" not number-formatted; Browse row "Learning· Suspended" missing a space; reset dialog keeps showing the previous "Latest backup" time after the safety copy; the safety copy made from the reset/delete dialog is listed as *Manual*; "Delete everything" keeps the theme although it says every setting except the language is reset; reset leaves a soft-deleted card with its old schedule; toasts are announced twice in the DOM. Phase 10 defect 6 (beta shows *Component gallery* / *Native diagnostics*) is still open.

Not run: Phase 13 (left for a run with a real API key), TalkBack listening, iOS.

**Fixed the same day and re-checked on the phone** (uncommitted with the run):

- Defect 1: `ContentService` loads the bundle on first use (shared `OnceLock`) and the start-up check for pending/newer bundled decks runs on its own thread, so `setup` registers the backend right after opening the collection; the Tauri adapter also waits (up to 30 s, every 150 ms) when a command meets "state not managed" — such a command never ran. Debug build, 5 cold starts: dashboard with data **410–781 ms** after the WebView started, no error (before: error after ≈ 6.5 s). Release profile: no error in 5 starts (measured only coarsely with `uiautomator`). Only commands that need the bundled catalog (`getOnboardingState`, deck catalog/import) still wait for it on the first call (≈ 4 s on the debug build).
- Defect 2: the browser pages with the `nextCursor` of the previous page; a jump to the end loads the pages in order. Flinging Browse reaches row 1 687 of 1 688, no error, no blank rows (`android-phase11-12-FIX-browse-past-100.png`).
- New: **Google Gemini** as a BYOK provider (`gemini-3.8-flash`, `gemini-3.7-flash`, `gemini-3-flash-preview`; ADR-18). On the phone: selectable, models listed, a key is saved to the Android Keystore (field cleared, not in the DOM, the database or the log; still there after a restart) and can be removed (`android-phase11-12-FIX-gemini-key-saved.png`). No real request was sent.

### Phase 10 physical Android run (2026-11-06)

Plan: [phase10-device-testing.md](phase10-device-testing.md). The run was driven remotely from the PC: taps with `adb shell input tap` (real Android touch events, coordinates read from the WebView DOM over Chrome DevTools), not with fingers; TalkBack was not listened to. Airplane mode on for the whole run (Wi-Fi off). The UI ran in **English**: the UI language follows the phone language and the settings screen cannot change it yet (`get_settings` is Phase 12), so the labels below are the English ones.

| Field | Value |
|---|---|
| Date | 2026-11-06 |
| Device / ABI | Samsung Galaxy A56 `SM-A566B` / arm64-v8a |
| Android / API | 16 / 36 |
| WebView | com.google.android.webview 156.0.8078.47 |
| Phone time zone | CET+0100 |
| Commit | `8e437d8` |
| Build | beta debug APK (A–G); beta release profile signed with the debug key (H) |

| Row | Result | Note / screenshot |
|---|---|---|
| A1–A7 | ◐ | No automatic onboarding on a fresh install: the app opens on the empty decks screen and onboarding is reached through "Add decks" (the shell never calls `getOnboardingState`). Onboarding PL → DE → goal 20 → both DE decks → "Get started": decks screen after **≈ 1.6 s** (DOM polling, ±0.3 s; recording kept, no ffmpeg for frame stepping); **peak PSS 274 MB**. "Due today" = 0 after the import is correct (`due_today = review + learning`, [dashboard.rs](../../crates/application/src/dashboard.rs)); the plan expectation "> 0" is wrong. A6 ☑ (restart: decks straight away, log `status="current"`, no second import). The log has no entries for a successful preparation job, so A5 cannot be read from the log. A7 not run. |
| B1–B9 | ☑ | Valid ratings: 20 / undone: 1 / duplicates: 0; order and ratings match the rating log one to one; double tap at #7 → one log; undo at #12 restores the card; summary 20 ratings, A4/H3/G9/E4, 2 min 11 s, 3 "Again" cards; streak 1; counters identical after restart; due dates: Again 1–2 min, Hard 6 min, Good 10 min, Easy 6–10 days; learning cards back after 10 min. `android-phase10-B5-summary.png`, `android-phase10-B6-dashboard.png`. |
| C1–C6 | ◐ | Leave / resume on the same card ☑, pause → kill → "Session paused" + "Continue" → same card ☑, 2 min in background not counted (`duration_ms` 24 444, `active_ms` 24 444) ☑, "Don't keep activities" ☑, summary of the session only ☑. **Defect:** the dashboard is not refreshed after a session is started (and after "Study everything" replaced an open session): after "Leave session" the "Unfinished session" card is missing or shows the previous, finished session, and "Resume session" then opens "No more cards in this session" (`android-phase10-C2-resume-empty.png`); "Refresh" or a restart shows the right session. |
| D1–D4 | ☑ | Kills: 12 (tap and `am force-stop` in one `adb shell`, 0–0.6 s apart). All 12 landed after COMMIT (next card on screen after restart), each gave exactly one log; valid logs 37 = 20 + 3 + 2 + 12; no duplicate `operation_id`. D1 (kill before rating): same card, no log. D4 (undo + kill): undo kept (`invalidated_at` set), card back. |
| E1–E10 | ◐ | Bury, suspend, flag (rating afterwards fine), delete, word sheet with real content (Back closes the sheet), favourite (exactly one in `associations`), AI generation "not available yet", speech button `aria-disabled` ☑; database: `queue -3` + `buried_until`, `queue -1`, `flags 1`, `deleted_at` ☑. E4: editor shows "This feature is not available yet", but Back asks "Discard changes? You have unsaved changes" with nothing edited (`android-phase10-E4-discard-dialog.png`). |
| F1–F10 | — | Not run (needs listening to TalkBack). |
| G | ☑ | scheduler all decks: **4.09 ms**; one deck: **0.75 ms**; due 200: **8.43 ms** (`due_queue` group: all decks 4.14 ms, one deck 0.92 ms, one deck 200: 0.41 ms, new 20: 0.41 ms); budget 50 ms. |
| H | ☑ | T1+T2 median: **1.36 s** (tries: 1.44, 1.30, 1.46, 1.32, 1.36; T1 1.00–1.13 s, T2 0.30–0.37 s); resume: 1.40, 1.50, 1.50 s; blank window: yes, a white frame for ≈ 0.35–0.7 s before the app background. Measured on the phone: `input tap` on the launcher icon, then `screencap` in a loop checking two pixels (≈ 0.35 s per sample, so values are rounded up). The release APK is written as `app-universal-release.apk`, not `…-unsigned.apk` as in the plan. |
| I | ☑ | iOS Simulator (`iPhone 17`, iOS 26.5 runtime, app variant: `beta com.mila.langualinker`). Onboarding & import: **≈ 1.08 s** (86 words + 12 sentences). Part B: 20 ratings (A:4, H:3, G:9, E:4), double-tap (#7) preserved 1 log, undo (#12) marked `invalidated_at`, summary matches, restart keeps streak (1) and counters, due dates: Again ~1 min, Hard ~6 min, Good ~10 min, Easy 8–10 d. Part C: Leave session & resume on same card (`sein`) ☑; pause → kill process → restart shows `[ Paused ]` unfinished session → Continue restores exact card `sein` ☑. Part E: Bury (`queue -3`, `buried_until`), Suspend (`queue -1`), Delete (`deleted_at` set), WordSheet (`ins`: base form, translation, grammar tips, associations, backdrop close restores study) ☑; Speech button active (ADR-12 native WKWebView TTS enabled) ☑. Part F: Accessible structure, `role="group"` `aria-labelledby` for rating group, `aria-label` with intervals (`"{{rating}} — następna powtórka za {{interval}}"`), focus moved to question/answer on phase transitions ☑. Database integrity: 33 valid logs, 1 undone, 0 duplicates, 0 duration > 60 s. Screenshots: `ios-phase10-B5-summary.png`, `ios-phase10-B6-dashboard.png`, `ios-phase10-E4-editor.png`, `ios-phase10-E4-discard-dialog.png`, `ios-phase10-E5-sentence-card.png`, `ios-phase10-E5-wordsheet.png`, `ios-phase10-F10-dashboard.png`. |
| J | — | Not run (optional). |

Defects (proposed priority):

1. **P1 — collection time zone is always `UTC`.** Nothing sets `collection.time_zone` (migration default `'UTC'`), so on a Polish phone the study day, buried-until and streak change at 05:00 local time (06:00 in summer) instead of 04:00. Part J step 5 assumes the phone zone is saved when the collection is created.
2. **P1 — stale "Unfinished session" on the dashboard** (C above).
3. **P1 (release) — `liblangualinker_lib.so` not 16 KB aligned.** Android 16 shows "This app isn't 16 KB compatible … LOAD segment not aligned" on every cold start of the debuggable build (`android-phase10-X-16kb-alignment.png`); required by Google Play for apps targeting Android 15+ and needed on 16 KB page devices.
4. **P2 — no onboarding on first start** (A above); the UI language does not follow the native language chosen in onboarding.
5. **P2 — new-card counter.** Before the first session the words deck showed "New 10" (sentences 12), but the all-decks session served 19 new word cards and no sentence cards (deck limit `new_per_day` 20). Probably the counter counts a 20-card candidate window after sibling burial (`bury_siblings`); to be confirmed.
6. **P2 — beta build shows "Component gallery" and "Native diagnostics"** in Settings: [SettingsScreen.svelte](../../frontend/src/features/settings/SettingsScreen.svelte) hides `devOnly` links only for `release`, not for `beta`.
7. **P2 — "Session resumed — N cards already rated."** does not update while studying (says 13 after 14 ratings).
8. **P2 — editor "Discard changes?"** with nothing edited (E4).
9. **P2 — Svelte 5 `$derived` state mutation prevents flag selection in `CardActions.svelte` and accordion expansion in `Disclosure.svelte`:**
   - In [CardActions.svelte](../../frontend/src/features/study/CardActions.svelte#L25), `let flag = $derived<CardFlag>(card.flag)` is bound via `bind:group={flag}` to radio inputs. Mutating a `$derived` rune is rejected at runtime in Svelte 5, so picking a flag does not update `flag` (it remains `card.flag` = `'none'`), and tapping "Set flag" only ever sends `'none'` without persisting any flag.
   - In [Disclosure.svelte](../../frontend/src/components/Disclosure.svelte#L16), `let open = $derived(expanded)` with `onclick={() => (open = !open)}` cannot be toggled because assigning to `$derived` fails at runtime, leaving collapsed accordions stuck collapsed.
10. **P2 — Top safe-area touch boundary on iOS (`Screen.svelte`):**
    - On iPhone 17 (402 × 874 pt), the top safe area / Dynamic Island spans y = 0 to ~50 pt. The `<` back button in [Screen.svelte](../../frontend/src/features/shared/Screen.svelte#L30) renders at y ≈ 40 pt without `env(safe-area-inset-top)` padding, causing native touches to be intercepted by the iOS status bar rather than the web back button.
11. **P2 — Bottom deck card "Study" button overlap with fixed bottom navigation on iOS:**
    - When the decks list is scrolled to the bottom, Deck 2's "Study" button sits at y ≈ 790 pt, partially overlapping the tap targets of the fixed bottom navigation bar (Statistics / Settings tabs).
12. **P2 — Missing `onfavorite` callback in `WordDetails.svelte`:**
    - [WordDetails.svelte](../../frontend/src/features/study/WordDetails.svelte#L26) embeds `<AssociationPanel associations={note.associations} expanded />` without `onfavorite`, so associations shown inside the sentence WordSheet do not render the "Mark as favorite" button.

Not part of the app: the default `JAVA_HOME` on the build PC (JDK 25) breaks the Gradle/Kotlin build; the APK was built with JDK 17. Also observed: Xiaomi/HyperOS refuses both `adb install` and `input` until "Install via USB" and "USB debugging (Security settings)" are on; the run moved to the Samsung.

### Phase 7 Android emulator run (2026-11-02)

Checks of the SQLite backend ([ADR-09](../adr/ADR-09-sqlite-persistence.md)). The dev frontend still runs on the demo mock, so the backend was driven over real IPC (`__TAURI_INTERNALS__.invoke` via DevTools); the database file was pulled with `adb exec-out run-as … cat`, edited on the PC with Python `sqlite3` and put back with `adb push` + `run-as cp` (piping into `run-as sh -c 'cat > …'` corrupts binaries from Windows). An emulator run does not count as the physical device run.

| # | Check | Result |
|---|---|---|
| P1 | Fresh install creates `collection.sqlite` in the app data directory (`/data/user/0/com.mila.langualinker.dev`) in WAL mode; log `migration applied schema_version=1`, then `status="current"` on later starts | ☑ `-wal`/`-shm` next to the file; after a fresh start the main file is 4 KB and the schema sits in the 309 KB WAL until a checkpoint |
| P2 | `get_app_info` / `get_about_info` / `get_diagnostics_report` over IPC; a v1 command without a backend answers `NOT_IMPLEMENTED`; `plugin:window\|close` rejected by the ACL | ☑ `schemaVersion: 1`, `platform: "android"`; `get_dashboard` → `NOT_IMPLEMENTED { operation: "getDashboard" }` |
| P3 | SecretStore (`probe_secret_store`, dev only): write, read back, overwrite, delete | ☑ all 10 steps ok on Android SharedPreferences/KeyStore |
| P4 | Jobs left `running`/`queued` by a killed process (rows injected, app force-stopped) are failed at startup | ☑ log `jobs interrupted by the previous run count=2`; both jobs `failed`, `INTERNAL { reason: "interrupted" }`, sequence raised by 1, progress kept; `cancel_job` on a finished job leaves it unchanged; unknown id → `NOT_FOUND`, malformed → `VALIDATION_FAILED` |
| P5 | Newer schema (`schema_migrations` v2, `user_version` 2) | ☑ data commands → `SCHEMA_TOO_NEW { found: 2, supported: 1 }`; file byte-identical after start (SHA-256), no WAL created; `get_about_info` answers with `schemaVersion: null` |
| P6 | Changed checksum of an applied migration | ☑ collection refused (`INTERNAL`, no text over IPC; log `collection could not be opened code="INTERNAL"`) |
| P7 | Infrastructure tests on device storage: release test binaries for `x86_64-linux-android` run from `/data/local/tmp` | ☑ 86 + 6 passed, including `upgrade_of_a_file_writes_a_safety_copy_first` (`VACUUM INTO`) and `uncommitted_work_is_gone_after_reopening_the_file` |
| P8 | Due query, 50 000 cards / 500 000 logs (`due_query_budget_on_the_plan_collection`, release) | ☑ worst of 20: 17.3, 27.5, 25.4 ms (3 runs) against 50 ms; seeding 7.0–8.1 s |

Start time (debug APK): `am start -W` TotalTime 1.6–2.3 s over 5 cold starts; opening the collection takes 0.22–0.28 s after `app started` (fresh install including migration v1: 0.28 s). Carried over: due query and start time on a physical device, iOS simulator.

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
