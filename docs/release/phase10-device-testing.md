# Phase 10 — device test plan

Step-by-step instructions that close the open Phase 10 DoD items ([plan, phase 10](../Detailed-Implementation-Plan.md#phase-10--connect-learning-to-the-existing-gui)) and the acceptance of [ADR-15](../adr/ADR-15-fsrs-scheduler.md) and [ADR-16](../adr/ADR-16-learning-flow.md). Phone preparation, drivers, `adb` and connection troubleshooting are described in [android-device-testing.md](android-device-testing.md) (sections 1–3) and are not repeated here.

Button and screen labels are quoted as they appear in the **Polish** UI, because the test runs with the app language set to Polish (the onboarding native language is Polish). Quoted labels stay as they are; everything else is in English.

## 0. What this run closes

| ID | DoD item / open point | Where in this plan |
|---|---|---|
| D1 | `[ ]` Offline flow on Android/iOS; TalkBack/VoiceOver | Parts B, C, E, F, I |
| D2 | `[ ]` Cold start → first card under 2 s; first-install content import measured separately | Parts A (import), H (start) |
| D3 | `[x]` Fresh install → onboarding → 20 DE ratings → restart → correct due dates/history — so far checked only over IPC; **confirmation on the device** | Part B |
| D4 | `[x]` Kill before/during/after COMMIT — so far simulated; **confirmation with a real process kill** | Part D |
| D5 | Scheduler queue measurement on the device (ADR-15, ADR-16) | Part G |
| D6 | Streak (collection zone, cutoff, no protection days) on the device | Part J (optional, at the end) |

After the run: tick D1 and D2 in the plan, set ADR-15 and ADR-16 to *Accepted*, and tick the Dashboard/Study/Summary screens in [integration-checklist.md](../contracts/integration-checklist.md) (section 8 of that document).

## 1. The main difference from phase 6: the **beta** variant, not dev

The `dev` variant (`com.mila.langualinker.dev`) runs the demo frontend with `MockAppApi`. It **does not work** for this run because it never touches the real backend. Phase 10 is tested on the **beta** variant:

| Variant | Package | Label | API | Used for |
|---|---|---|---|---|
| beta, debug profile | `com.mila.langualinker.beta` | LanguaLinker Beta | `TauriAppApi` → Rust/SQLite | Parts A–G, I, J (`run-as` and Chrome DevTools work) |
| beta, release profile (signed with the debug key) | `com.mila.langualinker.beta` | LanguaLinker Beta | same | Part H (start time measurement) |

Check that this is the right build: Settings → About shows the variant `beta`, there is **no demo banner** at the top, and there are no "Component gallery" or "Native diagnostics" entries.

## 2. Preparation

### 2.1 Phone and PC

1. Phone and PC prepared per [android-device-testing.md, sections 2–3](android-device-testing.md#2-prepare-the-phone-one-time) (USB debugging, Stay awake, WebView ≥ 120, TalkBack shortcut).
2. Headphones or speaker on — TalkBack speech has to be **listened to**.
3. Python 3 on the PC (the `sqlite3` module is in the standard library) — for checking the database after the run.
4. Record the device data (for the results table):

   ```powershell
   adb shell getprop ro.product.model
   adb shell getprop ro.build.version.release
   adb shell getprop ro.build.version.sdk
   adb shell getprop ro.product.cpu.abi
   adb shell dumpsys webviewupdate | Select-String 'Current WebView package'
   adb shell date +%Z%z           # phone time zone
   git rev-parse --short HEAD
   ```

5. Phone clock: **automatic date and time zone on** (Part J changes them deliberately at the end).

### 2.2 Build and install (beta, debug)

```powershell
npm --prefix frontend ci
# if Gradle reports SSLException (bad_record_mac):
# $env:JAVA_TOOL_OPTIONS = '-Djdk.tls.client.protocols=TLSv1.2 -Dhttps.protocols=TLSv1.2'

npm --prefix frontend run variant -- beta android build --debug --apk --target aarch64
adb uninstall com.mila.langualinker.beta     # fresh install; "not installed" error is OK
adb install src-tauri\gen\android\app\build\outputs\apk\universal\debug\app-universal-debug.apk
```

Phone with `armeabi-v7a`: use `--target armv7`.

**Do not start the app yet** — the first start is measured in Part A.

### 2.3 Helper commands (PowerShell)

Define these at the start of the session:

```powershell
$pkg = 'com.mila.langualinker.beta'
$out = "$env:TEMP\ll-phase10"; New-Item -ItemType Directory -Force $out | Out-Null

function Stop-App  { adb shell am force-stop $pkg }
function Start-App { adb shell monkey -p $pkg 1 | Out-Null }

# Copy the database to the PC (the app must be stopped so the copy is consistent).
# cmd /c is used because the cmd redirection does not corrupt binary data.
function Get-Db([string]$tag) {
  Stop-App
  $dir = "$out\$tag"; New-Item -ItemType Directory -Force $dir | Out-Null
  foreach ($f in 'collection.sqlite','collection.sqlite-wal','collection.sqlite-shm') {
    cmd /c "adb exec-out run-as $pkg cat $f > `"$dir\$f`" 2>nul"
  }
  Get-ChildItem $dir | Format-Table Name, Length
}

function Get-Log {
  $f = adb shell "run-as $pkg find . -name '*.log'" | Select-Object -Last 1
  adb shell "run-as $pkg cat $f"
}
```

If `collection.sqlite` is not in the root of the app data directory, find the path with `adb shell "run-as $pkg find . -name 'collection.sqlite*'"` and fix it in `Get-Db`.

### 2.4 Database check script

Save it outside the repository, e.g. `$out\check.py`:

```python
import sqlite3, sys, datetime as dt
db = sqlite3.connect(sys.argv[1])
q = lambda sql: db.execute(sql).fetchall()
ts = lambda ms: dt.datetime.fromtimestamp(ms / 1000).strftime('%Y-%m-%d %H:%M') if ms else '-'

print('schema:', q('PRAGMA user_version'))
print('valid / undone ratings:',
      q('SELECT COUNT(*) FROM review_logs WHERE invalidated_at IS NULL'),
      q('SELECT COUNT(*) FROM review_logs WHERE invalidated_at IS NOT NULL'))
print('FSRS version / hash:', q('SELECT DISTINCT fsrs_version, parameters_hash FROM review_logs'))
print('answer time > 60 s:', q('SELECT COUNT(*) FROM review_logs WHERE duration_ms > 60000'))
print('duplicate operation_id:', q('SELECT operation_id, COUNT(*) FROM review_logs GROUP BY 1 HAVING COUNT(*) > 1'))
print('\nsessions (id, status, current card, active ms, start, end):')
for r in q('SELECT id, status, current_card_id, active_ms, started_at, finished_at FROM study_sessions ORDER BY id'):
    print(' ', r[:4], ts(r[4]), ts(r[5]))
print('\nlast ratings (id, card, rating, when, ms, undone):')
for r in q('SELECT id, card_id, rating, reviewed_at, duration_ms, invalidated_at FROM review_logs ORDER BY id DESC LIMIT 30'):
    print(' ', r[0], r[1], 'AHGE'[r[2] - 1], ts(r[3]), r[4], 'YES' if r[5] else '')
print('\ncards after ratings (id, state, due, S, D, reps, lapses):')
for r in q('SELECT id, state, due_at, stability, difficulty, reps, lapses FROM cards WHERE last_review_at IS NOT NULL ORDER BY id'):
    print(' ', r[0], r[1], ts(r[2]), round(r[3] or 0, 3), round(r[4] or 0, 3), r[5], r[6])
```

Run: `python -I $out\check.py $out\<tag>\collection.sqlite`.

### 2.5 Rating log

For each session keep a **list of presses** on paper (or in a notepad): number, the word on the card, rating (A/H/G/E), and whether it was undone. This is the reference — after the run, the number of valid logs in the database must match this list exactly.

### 2.6 Rules for the run

- **Airplane mode for the whole run** (from part A step 1). `adb` over USB works in airplane mode. Wi-Fi and mobile data off; check the quick settings in case the phone turns Wi-Fi on by itself.
- Touch with **your fingers**, not `adb shell input tap`.
- With TalkBack — **listen** to the speech.
- Marks: ☑ pass · ◐ partly · ☒ fail. For every ◐/☒ take a screenshot (`adb shell screencap -p /sdcard/s.png; adb pull /sdcard/s.png docs\release\screenshots\android-phase10-<row>-<desc>.png`) and a note: row, screen, what you saw.
- Total time: about 3–4 h; the run can be split at the boundaries of a part.

## 3. Test order

### Part A — first install and content import (D2, "import measured separately")

| # | Step | Expected |
|---|---|---|
| A1 | Turn on airplane mode. Start recording: `adb shell screenrecord /sdcard/a-first.mp4`. Start the app from the launcher. | Onboarding screen (no demo banner). |
| A2 | Onboarding with your fingers: native language **Polski** → target language **Deutsch** → daily goal (default) → decks (select the DE decks) → "gotowe" (ready). Note the moment you press the last button. | Deck preparation screen with progress, no network error. |
| A3 | Wait for the "Talie" (decks) screen. Stop recording (Ctrl+C), `adb pull /sdcard/a-first.mp4 $out\`. | Decks screen with DE decks, "Na dziś" (due today) counts > 0, streak 0 days. |
| A4 | During preparation (repeat in a separate run if you miss it) measure memory: `adb shell dumpsys meminfo $pkg \| Select-String 'TOTAL'`, 2–3 times. | Record the highest `TOTAL PSS`. |
| A5 | Read the import time from the recording (frame by frame: from pressing the last onboarding button to the visible decks screen) and from the log: `Get-Log` (timestamps of `collection opened` and the preparation job entries). | **Import time on first install** — a value for the table; not part of the 2 s target. |
| A6 | `Stop-App`, `Start-App`. | Decks screen straight away, **without** the preparation screen again (phase 10 step 3). The log shows no second import on this start. |
| A7 | *(Optional, open from phase 8.)* Uninstall, install again, go through onboarding and **during preparation** do `Stop-App`, then `Start-App`. | Preparation resumes (log `bundled content preparation resumed`) and finishes on the decks screen; decks are not duplicated. |

Record: import time, peak PSS, result of A6 (and A7).

### Part B — 20 DE ratings → restart → due dates and history (D3, D1)

Without TalkBack, default settings (font 100%, light theme, portrait).

| # | Step | Expected |
|---|---|---|
| B1 | Record the counters on the decks screen (new/learning/review for the DE deck, "Na dziś", "Dziś przerobione" = studied today). | — |
| B2 | "Ucz się wszystkiego" (study everything) → "Pokaż odpowiedź" (show answer) → rate. Do **20 ratings**, mixed: at least 4× "Jeszcze raz" (Again), 3× "Trudne" (Hard), 9× "Dobrze" (Good), 4× "Łatwe" (Easy). Write each one in the rating log (2.5). | Each rating moves to the next card without delay; the buttons show intervals (e.g. "Jeszcze raz — 1 min", "Dobrze — 10 min" for a new card). A card rated "Jeszcze raz" comes back after about 1 minute in the same session. |
| B3 | At the 7th rating **tap a rating button twice quickly** (double tap). | One rating saved; the next card is not rated automatically. In the log: one rating. |
| B4 | After the 12th rating: "Cofnij ostatnią ocenę" (undo last rating). Then rate the same card again (with a different rating from before). | The previous card comes back; after the new rating the session moves on. In the log: 1 undo. |
| B5 | After the 20th rating (not counting the undone one): "Zakończ sesję" (end session). | Summary: number of ratings = 20, rating distribution matching the log, list of "Jeszcze raz" cards (button "Powtórz te karty"), real session time (minutes, not 0 or hours), indication of the next deck with cards. |
| B6 | "Przejdź do talii" (go to decks). | Decks screen: "Dziś przerobione" increased, "nowe" counters dropped by the number of new cards, streak = **1 day in a row**; streak hint: "Dzień liczy się po co najmniej jednej ocenie; nowy dzień zaczyna się o godzinie zmiany dnia. Bez dni ochronnych." (A day counts after at least one rating; a new day starts at the day-change hour. No protection days.) |
| B7 | `Get-Db b-before-restart`, `Start-App`, wait for the decks screen. Compare the counters with B6. | Counters and streak identical to before the restart. |
| B8 | `python -I $out\check.py $out\b-before-restart\collection.sqlite` | Valid ratings = **20**, undone = **1**; no duplicate `operation_id`; `fsrs_version` = `fsrs-6/py-fsrs-6.3.0`, one parameters hash; no `duration_ms > 60000`; session `finished`; every rated card has a due date matching its rating: "Jeszcze raz"/"Trudne"/"Dobrze" on a new card → minutes, "Łatwe" → days; S and D filled in. Order and ratings in the logs match the rating log. |
| B9 | After about 10 min, "Ucz się wszystkiego" again. | Cards with a 10-minute learning step come back into the queue (the "nauka" counter). |

Record: rows B1–B9, a screenshot of the summary (`android-phase10-B5-summary.png`) and the decks screen (`android-phase10-B6-dashboard.png`).

### Part C — durable session: interruption, background, resume (phase 10 step 2)

| # | Step | Expected |
|---|---|---|
| C1 | Start a session, do 3 ratings, remember the word on the current card. Back → "Opuścić sesję?" (leave session?) → leave. | Decks screen shows "Niedokończona sesja · … · Wznów sesję" (unfinished session · resume session). |
| C2 | "Wznów sesję" (resume session). | The same card you saw (not the next one, not the first in the queue). |
| C3 | "Wstrzymaj" (pause) → wait 1 min → `Stop-App` → `Start-App`. | Decks screen: session marked "Wstrzymana" (paused); after resuming, a screen with "Kontynuuj" (continue), then the same card. |
| C4 | Show the answer, go to the Home screen for **2 minutes**, come back, rate. Then `Get-Db c-background` and `check.py`. | The `duration_ms` of this rating does not include the 2 minutes in background (≤ 60 000 and realistically a few seconds). The session's `active_ms` did not grow by the background time. |
| C5 | Developer options → **Don't keep activities** → on. In a session, go to Home and come back. Turn the option off after the test. | The app restores on the same card or on the decks screen with "Wznów sesję"; no rating disappears or is duplicated. |
| C6 | End the session. | The summary counts only the ratings of this session. |

### Part D — real process kill during a rating (D4)

Goal: confirm that after a real process kill there is no double rating and no loss of a committed rating. In a terminal prepare `adb shell am force-stop com.mila.langualinker.beta` (ready to press Enter, not run yet).

| # | Step | Expected |
|---|---|---|
| D1 | **Before the request:** show the answer, `Stop-App` (no rating), `Start-App`, resume. | The same card, no new log. |
| D2 | **During / right after:** show the answer, press a rating and **immediately** press Enter in the terminal. Repeat 5 times (different cards); write each "rating + kill" in the log. | After each restart and resume: either the card was rated (the next card is on screen and there is a log in the database), or it was not rated (the same card, no log) — **never** two logs for one press, and never no log when the next card is shown. |
| D3 | `Get-Db d-kill`, `check.py`. Compare with the log. | No duplicate `operation_id`; number of valid logs matches the log (each "rating + kill" gives 0 or 1 log, according to what the screen showed after the restart). |
| D4 | **Undo + kill:** rate, "Cofnij ostatnią ocenę" (undo), kill immediately, restart, resume. | The undone rating stays undone (log with `invalidated_at`), the card returns to the queue; no "hanging" rating. |

Note: hitting the window "after COMMIT, before the response" by hand is random; this scenario is covered deterministically by the `replayed` test (DoD, point 2). Part D confirms that a real process kill on the device does not break the state.

### Part E — card actions, word sheet, content during study (phase 10 steps 8–10)

| # | Step | Expected |
|---|---|---|
| E1 | In a session: "Więcej akcji karty" (more card actions) → "Ukryj do jutra" (bury until tomorrow). | The card disappears from the session; after the session ends the decks counter does not include this card. |
| E2 | Next card → "Zawieś kartę" (suspend card). Remember the word. | The card disappears from the queue and does not appear in later sessions. |
| E3 | Next → "Oznacz flagą…" (flag) → choose a flag → "Ustaw flagę" (set flag). | Message "Zmieniono flagę." (flag changed); the card shows the flag mark; rating this card still works (fresh interval preview). |
| E4 | Next → "Edytuj notatkę" (edit note). | The editor opens and says the feature is **not available yet**; nothing is saved. Back to study on the same card. |
| E5 | A sentence card / a card with an underlined word: tap the word. | Word sheet with real deck content (translation, grammar tips, associations); empty sections are left out. Back closes the sheet, not the screen. |
| E6 | In the sheet: "Oznacz jako ulubione" (mark as favourite) on an association. | "Zapisano ulubione skojarzenie." (favourite association saved); after closing and reopening, exactly one favourite is marked. |
| E7 | AI association generation button (if visible). | Error or not-available message, without blocking rating (AI = phase 13, airplane mode). |
| E8 | Listen button. | On Android per ADR-12: "Mowa niedostępna na tym urządzeniu" (speech not available), disabled; does not block study. |
| E9 | "Usuń kartę…" (delete card) on one card → confirm. | The card disappears; the session moves on; after the session the card does not come back in the queue. |
| E10 | After E1–E9: `Get-Db e-actions`, `check.py` and by hand: `SELECT id, queue, flags, buried_until, deleted_at FROM cards WHERE id IN (…)`. | Suspended: in the suspended queue; buried: `buried_until` = end of the study day; flag set; deleted: `deleted_at` set. |

### Part F — TalkBack on real data (D1)

Turn on TalkBack (volume-key shortcut). Navigate **only with swipes and double taps**.

| # | Step | Expected (heard) |
|---|---|---|
| F1 | Decks screen: move through the tiles. | Read: "Na dziś" with the number, "Dziś przerobione", streak ("1 dzień z rzędu") **with its hint**, daily goal with progress, each deck with its name and counters, "Ucz się wszystkiego", possibly "Wznów sesję". Numbers match the screen. |
| F2 | "Ucz się wszystkiego" → double tap. | The card heading/question is read, with real DE content. |
| F3 | "Pokaż odpowiedź" (show answer). | Focus moves to the answer and it is read. |
| F4 | Rating group. | "Jak dobrze pamiętasz?" (how well did you remember?), each button with the **real** interval from the backend (e.g. "Dobrze — następna powtórka za 10 minut", days for a review card). |
| F5 | Rate. | Focus moves to the next question; focus is not lost at the top of the page. |
| F6 | "Cofnij ostatnią ocenę" (undo). | The undo message is announced; focus on the restored card. |
| F7 | Word sheet and "Więcej akcji karty" (more card actions). | Focus enters the sheet (Close button), all items reachable by swiping; Back closes the sheet. |
| F8 | "Opuścić sesję?" dialog. | First and last control reachable; focus does not escape behind the dialog; after closing, focus returns to the button that opened it. |
| F9 | "Zakończ sesję" → summary. | Read: number of ratings, time, rating distribution, "Jeszcze raz" list, next deck; buttons named. |
| F10 | Back to the decks screen. | Counters announced after the change (refreshed after `collection_changed`). |

Turn TalkBack off. Repeat F2–F5 quickly at **maximum system font** (`adb shell settings put system font_scale 2.0`, restart the app) and in **dark theme**: nothing clipped, no horizontal scroll, intervals do not break in the middle of a word. Restore `font_scale 1.0`.

### Part G — scheduler queue measurement on the device (D5)

The measurement corresponds to the ≈ 9 ms from the PC (ADR-15/ADR-16) on a 50 000-card / 500 000-log collection. We run the `due_query` benchmark as a native executable on the phone (as P7/P8 in phase 7, but `aarch64`).

1. Linker and C compiler from the NDK (in the same PowerShell session; adjust the API level and the `windows-x86_64` folder):

   ```powershell
   $tc = "$env:NDK_HOME\toolchains\llvm\prebuilt\windows-x86_64\bin"
   $env:CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER = "$tc\aarch64-linux-android26-clang.cmd"
   $env:CC_aarch64_linux_android  = "$tc\aarch64-linux-android26-clang.cmd"
   $env:AR_aarch64_linux_android  = "$tc\llvm-ar.exe"
   ```

2. Build without running:

   ```powershell
   cargo bench -p langualinker-infrastructure --bench due_query --target aarch64-linux-android --no-run
   $bin = Get-ChildItem target\aarch64-linux-android\release\deps\due_query-* | Where-Object { -not $_.Extension } | Sort-Object LastWriteTime | Select-Object -Last 1
   adb push $bin.FullName /data/local/tmp/due_query
   adb shell chmod 755 /data/local/tmp/due_query
   ```

3. Run (phone plugged into the charger, screen on, app closed):

   ```powershell
   adb shell "cd /data/local/tmp && ./due_query --bench scheduler"
   adb shell "cd /data/local/tmp && ./due_query --bench due_queue"
   ```

4. Record the median for `scheduler_counts_all_decks` and `scheduler_counts_one_deck` (and `all_decks_200`). Expected: the all-decks queue clearly below the 50 ms budget for a due-date query (test `due_query_budget_on_the_plan_collection`, plan step 25); a result above 50 ms is a defect for the backlog.
5. Clean up: `adb shell rm -r /data/local/tmp/due_query`.

### Part H — cold start → first card under 2 s (D2)

Measured on a **release-profile** build (the debug APK is not representative). The data from parts B–E stays (the app has a collection and cards due today), so the release build is installed **as an update** — with the same debug key, so the data is not erased.

1. Build and sign:

   ```powershell
   npm --prefix frontend run variant -- beta android build --apk --target aarch64
   $bt = Get-ChildItem "$env:ANDROID_HOME\build-tools" | Sort-Object Name | Select-Object -Last 1
   & "$($bt.FullName)\apksigner.bat" sign --ks "$env:USERPROFILE\.android\debug.keystore" --ks-pass pass:android --out $env:TEMP\ll-beta-release.apk src-tauri\gen\android\app\build\outputs\apk\universal\release\app-universal-release-unsigned.apk
   adb install -r $env:TEMP\ll-beta-release.apk
   ```

   If the install is refused (`INSTALL_FAILED_UPDATE_INCOMPATIBLE`), uninstall, go through onboarding again (part A, steps A1–A3), then do a few ratings so that cards are due today. Note this.

2. Precondition: "Na dziś" > 0 on the decks screen. No open session (end it), so that the "Ucz się wszystkiego" path is measured.
3. Measurement definition (write it into the results):
   - **T1** — from tapping the icon in the launcher to the visible decks screen with counters (cold start: `Stop-App` before each measurement);
   - **T2** — from tapping "Ucz się wszystkiego" to the visible content of the first card;
   - **result = T1 + T2** (without the tester's reaction time between the decks screen and the tap). Target: **< 2 s** (median of 5 tries).
4. Measurement, 5 times:

   ```powershell
   Stop-App
   adb shell screenrecord /sdcard/start.mp4      # in a separate window; Ctrl+C once the card appears
   # tap the icon → decks screen → "Ucz się wszystkiego" → card
   ```

   `adb pull /sdcard/start.mp4 $out\start-<n>.mp4`; read T1 and T2 frame by frame (e.g. VLC, key `E` = next frame). As a side check, `adb shell am start -W -n $pkg/com.mila.langualinker.MainActivity` gives `TotalTime` (first native frame only — it does not replace T1).
5. Second series (3 tries) with an **open session**: T2 measured from "Wznów sesję" (resume session) to the card.
6. Note whether a blank window shows between the launcher and the app.

Record: 5 × (T1, T2, sum), the median, 3 × resume, the build version.

After part H: if you need `run-as` again (part J or further database copies), reinstall the debug APK (`adb install -r …debug.apk`; same key, data stays).

### Part I — iOS / VoiceOver (simulator)

No physical iPhone is available (owner decision 2026-10-29), so iOS is tested in the **simulator**, the same way as the phase 6 gate ([emulator-testing.md, section 7](emulator-testing.md#7-ios-simulator-requires-a-mac)). The simulator requires a Mac with Xcode. The simulator result counts toward the DoD; the physical iPhone run is carried over to a later phase, as in phase 6.

The difference from phase 6: the **beta** variant, not dev.

Preparation on the Mac:

```bash
npm --prefix frontend ci
npm --prefix frontend run variant -- beta ios dev "iPhone 17"
```

Bundle-id suffixes for the iOS variants are not configured yet (README, "App variants"). They have to be set up before the run. Until then, beta builds with the base identifier `com.mila.langualinker`; note this in the results. `xcrun simctl get_app_container booted <bundle-id> data` and VoiceOver work the same way.

Scope: part B (20 ratings → restart, database copy from the simulator container with `xcrun simctl get_app_container booted <bundle-id> data`), C1–C3, E1–E10 and F1–F9 with VoiceOver (listened to). Parts D, G and H are run on Android, because they concern the phone and the measurement on the device. Screenshots: `xcrun simctl io booted screenshot docs/release/screenshots/ios-phase10-<row>-<desc>.png`.

### Part J — streak and day boundary (D6, optional, **at the end**)

Changing the system date mixes times in the logs — do it last, after copying the database, ideally on a copy you uninstall afterwards.

1. `Get-Db j-before`. Settings → System → Date & time → turn off automatic date → set **+1 day** (after the collection's day-change hour).
2. `Stop-App`, `Start-App`. Expected: streak still **1** (yesterday's rating counts, today has no rating yet), "Dziś przerobione" = 0, due counters grew by the cards due on that day; cards buried until tomorrow (E1) are back.
3. Do 1 rating. Expected: streak **2**.
4. Set **+3 days** from the original date, restart. Expected: streak **0** (no protection days).
5. *(Time zone)* Turn off automatic time zone, set e.g. Tokyo, restart. Expected: day boundaries are calculated in the **collection's** zone (saved when the collection was created), not the phone's — counters and streak do not shift.
6. Restore automatic date and time zone. Uninstall the app if the collection should be clean for further tests.

## 4. After the run — restore the phone

```powershell
adb shell settings put system font_scale 1.0
adb shell settings put secure enabled_accessibility_services '""'
adb shell settings put secure accessibility_enabled 0
```

Also: automatic date and time zone on, "Don't keep activities" off, airplane mode off, dark theme as you prefer.

## 5. DoD closing criteria

| DoD item | Closed when |
|---|---|
| "Offline flow on Android/iOS; TalkBack/VoiceOver" | A1–A6, B1–B9, C1–C6, E1–E10 and F1–F10 ☑ in airplane mode on the physical Android phone; iOS simulator ☑ per part I; no P0 defect. |
| "Cold start → first card target under 2 s; first-install content import measured separately" | Median T1+T2 from part H < 2 s on the release-profile build; import time from A5 recorded separately. Result ≥ 2 s → ☒ with the defect in the backlog (the item stays open). |
| ADR-15 and ADR-16 → Accepted | B8 and D3 without discrepancies (no double ratings, correct undo, due dates consistent), G measured, D1 run passed. |

## 6. Results template

Paste into [device-smoke.md](device-smoke.md) as a new section "Phase 10 physical Android run (YYYY-MM-DD)" and a new row in the **Results** table.

| Field | Value |
|---|---|
| Date | |
| Device / ABI | |
| Android / API | |
| WebView | |
| Phone time zone | |
| Commit | |
| Build | beta debug APK (A–G, J); beta release profile signed with debug key (H) |

| Row | Result | Note / screenshot |
|---|---|---|
| A1–A7 | | import time: … s; peak PSS: … MB |
| B1–B9 | | valid ratings: 20 / undone: 1 / duplicates: 0 |
| C1–C6 | | |
| D1–D4 | | kills: 5; logs match the rating log: yes/no |
| E1–E10 | | |
| F1–F10 | | |
| G | | scheduler all decks: … ms; one deck: … ms; due 200: … ms |
| H | | T1+T2 median: … s (tries: …); resume: … s; blank window: yes/no |
| I | | iOS simulator (`iPhone 17`, iOS …); bundle-id: … |
| J | | |

## 7. Defects

Every ◐/☒ becomes an entry with a proposed priority: **P0** — loss or duplicate of a rating, wrong due date, crash, no offline operation; **P1** — TalkBack does not announce a key element, start ≥ 2 s, clipped controls; **P2** — cosmetic. P0 blocks closing the DoD.

## 8. Documentation updates after passing

1. [Detailed-Implementation-Plan.md](../Detailed-Implementation-Plan.md), phase 10: tick the two DoD items with a note (device, date, median start, import time); in the notes of items 1 and 2 replace "device run pending" with the result of B and D; add a short *Status* with the date of the run.
2. [ADR-15](../adr/ADR-15-fsrs-scheduler.md) and [ADR-16](../adr/ADR-16-learning-flow.md): status → *Accepted (date)*, add the queue measurement from part G.
3. [integration-checklist.md](../contracts/integration-checklist.md): tick the Dashboard/Study/Summary screens ("ticked after the Android device run").
4. [device-smoke.md](device-smoke.md): results section (section 6), screenshots in `docs/release/screenshots/android-phase10-*.png`.
