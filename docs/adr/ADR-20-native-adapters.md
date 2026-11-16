# ADR-20: Native adapters — reminders, speech, reminder deep links, lifecycle

- Status: Proposed (2026-11-16); implemented in Phase 15 on Android. Acceptance after the physical Android matrix (grant/deny/revoke, scheduled delivery, warm and cold tap, reboot, zone change) and an iOS run (simulator now, iPhone when available).
- Plan references: Phase 15 steps 1–12 and DoD; ADR-12 (native capability probes); ADR-17 (settings document, `notifications`, `reminderTime`); D-01 (desktop reminders out of scope)

## Context

ADR-12 showed: `tauri-plugin-notification` =2.5.1 permission, delivery and scheduling work on the Android emulator and iOS simulator; the Android WebView has no Web Speech API (iOS WKWebView does); the plugin's tap event (`actionPerformed`) reaches only JS listeners that exist at that moment, so a tap that cold-starts the app is lost. Settings (Phase 12) already store the reminder time and expose `notifications: PermissionState`, but it was always `unsupported`, and `requestNotificationPermission` was the last pending catalogue command.

## Decision

### Reminders (steps 2–7)

1. **Planner in the application layer** (`application::reminders`): no platform code. One reminder per local day at `reminderTime` in the collection time zone (DST gap → first existing minute after it; repeated time → first occurrence). Rolling window of 7 days (iOS keeps at most 64 pending requests).
2. **No material, no reminder**: a day gets one only if review cards (not suspended; buried ones from the next study day) are due by the end of its study day by their stored due dates, or new cards are waiting. **Done today**: today's reminder is dropped when the daily goal is met and no review is due.
3. **Text**: generic title; body with "about N cards" — a forecast from the stored due dates at planning time, not a promise — or "new words are waiting". In the UI language (PL/EN).
4. **Platform sync** (`src-tauri/src/reminders.rs`): cancel our pending ids (15000–15099), then schedule one-shot `Schedule::At` notifications with `allowWhileIdle` on the Android channel `study-reminders` (users can mute it separately). No `SCHEDULE_EXACT_ALARM`: the plugin falls back to inexact `setAndAllowWhileIdle`, enough for a habit reminder (step 5 — exact alarms only when justified). The plugin's boot receiver restores alarms after a reboot.
5. **When the plan is recomputed**: app start, return to the foreground (with the outbox check), the app going to the background (`WindowEvent::Suspended`), a settings change, a granted permission. Rust is never assumed alive after a kill: the window always covers the next 7 days, so a user who does not open the app for a week stops getting reminders — intended (no nagging), documented.
6. **Language**: the text follows the stored UI language. The shown language starts from the device language, so the WebView stores it in the settings at start when they differ (seen on the device: English UI, Polish reminder).
7. **Permission** (step 3): `requestNotificationPermission` asks the OS only while the state is `prompt`; afterwards the OS decides and the settings screen points to the system settings. The summary's reminder offer (after the first completed session) saves the time and then asks — the prompt follows a choice the learner just made. `get_settings`/`update_settings` read the permission from the OS each time, and the WebView reads the settings again when the app returns to the foreground, so a change in the system settings shows on return (found on the device: before that the screen kept `denied`). Desktop: `unsupported` (the plugin cannot schedule there; D-01), and the offer is not shown.
8. **Quiet hours/snooze** (step 4): not separate settings. There is exactly one reminder a day at a time the learner picks, so the chosen time is the quiet-hours rule; snooze would need notification actions and a second scheduling path for little value — left for v1.1 if beta feedback asks for it.

### Reminder tap → deep link (steps 8, 12)

9. Every reminder carries `extra.url = "langualinker://app/home"`. Our Kotlin `NativePlugin` reads the tap intent at a cold start (in `load`) and on `onNewIntent` (warm) and keeps the URL until the WebView takes it with `take_launch_url` (at start and on every `visibilitychange` → visible). This sidesteps the lost cold-start event of the plugin's JS API and needs no `notification:*` permission in the WebView. Found on the device: `tauri-plugin-notification` 2.5.1 never fills the notification JSON of the tap intent (`sourceJson` stays `null`), only `NotificationId`; so a reminder id (15000–15099) maps to the reminder URL, and the JSON path stays for a fixed plugin.
10. The WebView validates with the existing deep-link adapter (`deepLinkToPath`: scheme `langualinker:`, host `app`, allowed routes only, query dropped) and navigates through the router, so leave guards (an open session) and focus handling apply. Anything else is ignored. The target is the dashboard: it shows the resume card and "Study everything", so a missing deck or a changed queue never leads to a dead end. A custom URL scheme registered with the OS (`tauri-plugin-deep-link`) is not added: nothing outside the app links into it yet.

### Speech (step 10)

11. Android: `NativePlugin` wraps `android.speech.tts.TextToSpeech` — `speech_status { lang }` (engine present, language available, an installed offline voice), `speak { text, lang, rate }` (rate 0.5–1.5, text ≤ 1000 characters, `QUEUE_FLUSH`), `stop_speech`. Transient audio focus (`MAY_DUCK`, usage *assistance accessibility*, content *speech*); losing focus (a call, navigation prompt) stops speech; `onPause` stops it (no speech in the background).
12. The WebView `SpeechService` asks the native status for `de`, `en`, `es` at start; where there is a native engine it is used, otherwise the WebView engine (iOS, desktop) — the proven fallback from ADR-12. The service also stops speech when the page becomes hidden.
13. The commands are platform adapters, not `AppApi`: they live in a separate capability `platform` (every variant), so `default` still grants exactly the contract catalogue. Off Android they answer "no engine" / `null`.

### Lifecycle (step 9)

14. Background: the study timer already pauses (Phase 3/10), speech stops (10–11), reminders are recomputed (5). Foreground: queries refresh through `collection_changed`, the outbox runs, the reminder window moves, a pending launch URL is opened. Session checkpoints are durable since Phase 10.

### Files (step 11)

15. Unchanged from Phases 8/12/14: the system picker (`content://` on Android, security-scoped URLs on iOS) for import, a save-a-copy dialog for backups and exports. A native share sheet (`ACTION_SEND` + `FileProvider`) is not added in this phase; saving a copy covers the backup → reinstall → restore path.

## Consequences

- All catalogue commands now have a backend (`commands::pending()` is empty).
- The notification permission is OS state, never stored; the settings document keeps only the time.
- `NativePlugin.kt` lives in the generated Android project (`src-tauri/gen/android/...`), which is tracked in git; regenerating the project must keep it.
- iOS: reminders use the same plugin path (untested on a device); tap routing on iOS still relies on the dashboard being the start screen until a Swift counterpart of `NativePlugin` exists; speech uses WKWebView.
