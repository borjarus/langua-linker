# Device smoke checklist

Phase 1: Android emulator and iOS simulator (owner decision 2026-10-12). Phase 6 gate: the same checklist on physical Android and iOS devices; emulator results do not count there.

Run with the demo build (`tauri android dev` / `tauri ios dev`, or a debug APK). Record date, device, OS version, build commit and result for each row; attach screenshots in the PR.

| # | Check | Android | iOS |
|---|---|---|---|
| 1 | App starts; demo banner and decks screen visible within 3 s | ☐ | ☐ |
| 2 | Settings → About shows app name, version and contract version (demo build: from the mock; a `--mode dev` build is needed to exercise `get_app_info` over IPC) | ☐ | ☐ |
| 3 | Bottom navigation reaches all 5 sections; hardware/gesture back returns to the previous screen | ☐ | ☐ |
| 4 | `#/gallery`: dialog opens/closes, toast appears above the bottom navigation | ☐ | ☐ |
| 5 | Search field: on-screen keyboard does not cover the input; layout resizes | ☐ | ☐ |
| 6 | Safe areas: notch/status bar and home indicator do not overlap content or navigation | ☐ | ☐ |
| 7 | Rotate to landscape and back: no clipped controls | ☐ | ☐ |
| 8 | System font size at maximum: text scales, no horizontal page scroll | ☐ | ☐ |
| 9 | TalkBack / VoiceOver: navigation items and the "Study" button are announced with labels | ☐ | ☐ |
| 10 | App log file exists in the app log directory and contains no card text (`adb shell run-as com.mila.langualinker ls files/../` / Xcode container) | ☐ | ☐ |

## Results

| Date | Device | OS | Commit | Result / notes |
|---|---|---|---|---|
| 2026-10-12 | Android Emulator `Medium_Phone_API_36.1` (x86_64, Play Store image) | Android 16 (API 36.1) | uncommitted working tree | Debug APK installed, app started, demo dashboard rendered (screenshot). Start time not measured, so row 1 is not ticked yet. Other rows not yet run. |
