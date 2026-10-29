# Testing the app on a physical Android device

Step-by-step instructions for step 7 of the [Phase 6 gate](phase6-gate.md#step-7--physical-device-protocol): connecting a phone to the Windows PC, installing the dev build and running the checks in a fixed order. Results go in [device-smoke.md](device-smoke.md) (rows 1–26, Android column) and in the G1–G8 table of [phase6-gate.md](phase6-gate.md#step-7--physical-device-protocol).

Physical-device testing is Android only for now: no iPhone is available, so iOS runs on the simulator ([emulator-testing.md, section 7](emulator-testing.md#7-ios-simulator-requires-a-mac)) and the physical iPhone run is carried over (owner decision 2026-10-29).

Scenario steps that are the same as on the emulator are not repeated here; this guide points to [emulator-testing.md](emulator-testing.md) sections 4 and 5 and lists only what differs on a device.

## 1. What you need

- An Android phone with **Android 8.0 (API 26) or newer** and **Android System WebView 120 or newer** (section 2, step 5).
- A USB cable that carries data (some charging cables do not — if `adb devices` shows nothing, try another cable first).
- The PC set up per [mobile-setup.md](mobile-setup.md#android--windows-verified-2026-10-12): JDK 17, Android SDK with `platform-tools` (`adb`), NDK, Rust Android targets. Check:

  ```powershell
  adb version
  rustup target list --installed     # aarch64-linux-android must be on the list
  ```

- Headphones or the phone speaker on — TalkBack output has to be **listened to** this time (G1).
- About 3–4 hours for the full run; it can be split at the boundaries of section 6.

## 2. Prepare the phone (one-time)

1. **Developer options.** Settings → About phone → tap **Build number** 7 times (Samsung: Settings → About phone → Software information → Build number; Xiaomi: Settings → About phone → MIUI/HyperOS version). Enter the PIN if asked.
2. **USB debugging.** Settings → System → Developer options → turn on **USB debugging**.
   - Xiaomi/Redmi/POCO: also turn on **Install via USB** and **USB debugging (Security settings)** (requires a SIM and a Mi account).
   - Huawei/Honor: also allow **"ADB debugging in charge-only mode"** if available.
3. **Stay awake** (Developer options → *Stay awake*) — the screen does not turn off while charging, which saves re-unlocking during the run.
4. **Updates.** Google Play → update **Android System WebView** and **Chrome**; Google Play → install/update **Android Accessibility Suite** (contains TalkBack).
5. **WebView version.** Developer options → **WebView implementation** shows the provider and version. Write it down; it must be ≥ 120. You can also read it later over adb (section 3, step 6).
6. **TalkBack shortcut.** Settings → Accessibility → TalkBack → **TalkBack shortcut** → on (hold both volume keys for 3 seconds). It lets you switch TalkBack off quickly if it gets in the way. Run the TalkBack tutorial once if you have never used it.

## 3. Connect the phone to the PC

1. **USB driver (Windows).**
   - Pixel / Android One: Android Studio → SDK Manager → SDK Tools → **Google USB Driver**, or download it from developer.android.com; then Device Manager → the phone → Update driver → `%LOCALAPPDATA%\Android\Sdk\extras\google\usb_driver`.
   - Samsung: *Samsung Android USB Driver for Windows*. Other brands: the manufacturer's driver; many phones work with the generic Windows driver.
2. Connect the phone with the cable. On the phone, pick the USB mode **File transfer / Android Auto** (some phones do not expose ADB in "Charging only" mode).
3. On the phone, the **"Allow USB debugging?"** dialog shows the PC's RSA fingerprint → tick **Always allow from this computer** → **Allow**.
4. On the PC:

   ```powershell
   adb devices
   # List of devices attached
   # R58N12ABCDE     device
   ```

   | `adb devices` shows | Fix |
   |---|---|
   | nothing | other cable or USB port (avoid hubs), driver from step 1, USB mode "File transfer", `adb kill-server; adb start-server` |
   | `unauthorized` | unlock the phone and accept the dialog; if it does not appear: Developer options → **Revoke USB debugging authorizations**, reconnect the cable |
   | `offline` | reconnect the cable; `adb kill-server`; update `platform-tools` |

5. **If the emulator is running too**, adb sees two devices and every command needs a target. Simplest: close the emulator. Otherwise, for this PowerShell session:

   ```powershell
   $env:ANDROID_SERIAL = 'R58N12ABCDE'    # serial from adb devices
   ```

6. Record the device data for the results table:

   ```powershell
   adb shell getprop ro.product.model
   adb shell getprop ro.build.version.release          # Android version
   adb shell getprop ro.build.version.sdk              # API level
   adb shell getprop ro.product.cpu.abi                # arm64-v8a on almost all current phones
   adb shell dumpsys webviewupdate | Select-String 'Current WebView package'
   git rev-parse --short HEAD
   ```

7. **Optional — Wi-Fi instead of a cable** (Android 11+, phone and PC on the same network): Developer options → **Wireless debugging** → on → **Pair device with pairing code**, then

   ```powershell
   adb pair 192.168.1.23:37123        # address and code from the pairing dialog
   adb connect 192.168.1.23:41234     # address shown on the Wireless debugging screen
   ```

   Use the cable for the start-time measurement (G4) and for TalkBack, so the connection does not drop mid-run.

## 4. Build and install the dev build

The dev variant (`com.mila.langualinker.dev`, launcher label "LanguaLinker Dev") contains the demo frontend with `MockAppApi`, the demo banner, and the dev-only screens *Galeria komponentów* and *Diagnostyka natywna*.

```powershell
npm --prefix frontend ci
# if Gradle reports SSLException (bad_record_mac):
# $env:JAVA_TOOL_OPTIONS = '-Djdk.tls.client.protocols=TLSv1.2 -Dhttps.protocols=TLSv1.2'

npm --prefix frontend run android:apk -- --target aarch64    # armeabi-v7a phone: --target armv7
adb install -r src-tauri\gen\android\app\build\outputs\apk\universal\debug\app-universal-debug.apk
adb shell monkey -p com.mila.langualinker.dev 1               # launch
```

- `INSTALL_FAILED_UPDATE_INCOMPATIBLE` means an older build signed with another key is installed: `adb uninstall com.mila.langualinker.dev` and install again.
- Live reload instead of an APK: `npm --prefix frontend run android:dev` with only the phone connected. The phone loads the Vite server from the PC over the network, so both must be on the same Wi-Fi/LAN and Windows Firewall must allow Node.js on private networks. Use it for fixing defects found during the run; take the results from the APK.

## 5. Tools during the test

| Purpose | On a device |
|---|---|
| Demo data scenario | Demo bar at the top → "Scenariusz demo". Changing it resets the demo memory. |
| File returned by the "picker" | Demo bar on Import and export / Backup → "Wybór pliku w demo zwraca". |
| 200% text (system) | Settings → Display → Font size → largest (and Display size if you test G5), or `adb shell settings put system font_scale 2.0` (revert: `1.0`). Phones may cap the slider below 2.0 — then use adb. |
| 200% text (in app) | Ustawienia → Rozmiar tekstu → Bardzo duży (200%) |
| Dark theme | Ustawienia → Motyw → Ciemny, or the system dark theme in quick settings with the app theme "Systemowy" |
| TalkBack | Volume-key shortcut (section 2, step 6) or Settings → Accessibility → TalkBack. Gestures: swipe right/left = next/previous element, double tap = activate, two-finger swipe = scroll, swipe down then left (or the system Back button) = Back. |
| Rotation | Turn on auto-rotate in quick settings and rotate the phone. |
| Screenshot | Power + volume down on the phone, or from the PC: `adb shell screencap -p /sdcard/s.png; adb pull /sdcard/s.png docs\release\screenshots\android-device-<row>-<desc>.png` |
| Screen recording | `adb shell screenrecord /sdcard/run.mp4` (stop with Ctrl+C, max 3 min), then `adb pull /sdcard/run.mp4` |
| DOM/layout inspection | Chrome on the PC → `chrome://inspect/#devices` → the device → "LanguaLinker Dev" WebView → *inspect* (debug APK only) |
| App log | `adb shell "run-as com.mila.langualinker.dev ls files/logs"`; if the path differs, `adb shell "run-as com.mila.langualinker.dev find . -name '*.log'"` |

Rules for this run (they are why the emulator run did not count):

- **Touch input with your fingers**, not `adb shell input tap`. Injected taps bypass TalkBack's explore-by-touch.
- With TalkBack on, **listen to the spoken output**; the accessibility tree alone is not enough.
- The **on-screen keyboard is the phone's real keyboard** (Gboard, Samsung Keyboard) — no hardware keyboard is attached.

## 6. Test order

Do the parts in this order. Settings changes (font, theme, TalkBack) are grouped, so you switch them as rarely as possible. Before each part make sure the phone is back to default: system font 100%, light theme, TalkBack off, portrait, in-app text 100%.

Mark each row ☑ pass · ◐ partly · ☒ fail. For every ◐/☒ take a screenshot and write down the row number, the screen and what you saw.

### Part A — basic smoke (rows 1–4, 6, 10)

| Row | What to do | Expected |
|---|---|---|
| 1 | Force-stop the app (`adb shell am force-stop com.mila.langualinker.dev`), start it from the launcher, count/stopwatch. | Demo banner and the "Talie" screen within 3 s (the debug build is slower than release; the binding measurement is G4 in part H — note the debug time here). |
| 2 | Ustawienia → O aplikacji. | App name, version, contract version `1.0`, build "Demo". |
| 3 | Tap all 5 tabs (Talie, Przeglądaj, Dodaj, Statystyki, Ustawienia); then system Back several times. | Each tab opens; Back walks back through the previous screens and only then leaves the app. |
| 4 | Ustawienia → Galeria komponentów: open the example dialog, close it; show a toast. Then open the dialog and press Back. | Dialog opens and closes; toast above the bottom navigation; Back closes the dialog, not the screen. |
| 6 | Look at the top and bottom of several screens (gesture navigation and 3-button navigation if the phone allows switching). | Status bar, camera cut-out and gesture bar do not cover the banner, headings or the bottom navigation. |
| 10 | `adb shell "run-as com.mila.langualinker.dev find . -name '*.log'"` and `adb shell "run-as com.mila.langualinker.dev cat <path>"` | A log for today with `app started … platform="android"`; no card or deck text. |

### Part B — on-screen keyboard (row 5, G2)

1. Przeglądaj → tap the search field, type `halt`. Keyboard opens; the field and the results stay visible above it; the list filters as you type and the keyboard does not close after each letter.
2. Dodaj (editor): tap each text field in turn, the lowest one last. Each focused field scrolls above the keyboard.
3. Ustawienia → Cel dzienny: type a value; Ustawienia → AI → address and API key fields: not covered.
4. Ustawienia → Prywatność → "Resetuj postęp…" → the phrase field in the dialog stays visible with the keyboard open, and so does the confirm button (scroll if needed).
5. With the keyboard open, press Back once: **only the keyboard closes**; the second Back closes the dialog/screen.
6. Rotate to landscape with the keyboard open: the field remains reachable.

Record: row 5 and G2.

### Part C — Phase 3 learning flow (rows 12–16, then row 11)

1. **Row 16 — touch only.** Demo bar → "Nowy użytkownik" → complete the onboarding → Talie → "Ucz się wszystkiego" → "Pokaż odpowiedź" → rate 5 cards → "Zakończ sesję" → summary → back to the decks.
2. **Row 14 — leave guard.** Start a session, press Back → "Opuścić sesję?"; "Ucz się dalej" (keep studying) stays; Back again → leave → decks show "Wznów sesję".
3. **Row 13 — sheets.** In study, tap a linked word (word sheet) and the "more actions" button (card actions). Both dock at the bottom above the gesture bar; Back closes the sheet and stays in the study screen.
4. **Row 15 — TTS button.** On Android the listen button says "Speech is not available on this device" / its Polish equivalent and is disabled (ADR-12: Android WebView has no `speechSynthesis`). If the phone's WebView *does* speak, note it — it is a positive deviation.
5. **Row 12 — maximum font.** Set system font to maximum (section 5) — then **restart the app** (force-stop and launch; row 13 found that changing the font while the app runs can recreate the activity). Study, reveal the answer, rate, summary: no clipped text or controls, no horizontal page scroll; rating labels do not break mid-word. Also try Back on the word sheet at this font size (repeat of the row 13 observation). Revert the font.
6. **Row 11 + G1 — TalkBack by touch gestures.** Turn TalkBack on. Navigate **only with swipes and double taps**:
   - Bottom navigation: each item read with its name and "selected" state; the screen heading is read after a tab change.
   - Talie → "Ucz się wszystkiego" → double tap. After "Pokaż odpowiedź", focus jumps to the answer and it is read aloud.
   - Rating group: read as a group ("How well did you remember?" in PL), each button with its interval ("Dobrze — następna powtórka za 10 minut"). After a rating, focus goes to the next question.
   - Word sheet: focus moves into the sheet (Close button); swipe through all items; Back closes it.
   - Dialogs: open "Opuścić sesję?" and later (part D) the reset, restore and import dialogs: swipe to the **first and last control**; focus does not leave the dialog behind it; after closing, focus returns to the opening button.
   - Turn TalkBack off.

Record: rows 11–16 and G1 (G1 may stay ◐ until the dialogs in part D are covered).

### Part D — Phase 5 screens (rows 17–25)

Go through [emulator-testing.md, section 4](emulator-testing.md#4-phase-5-scenarios) in order, at default settings, with your fingers. Differences on the device:

| Section | Row | Device-specific note |
|---|---|---|
| 4.1 Onboarding | 17 | Do it once with TalkBack on: each "Dalej" makes TalkBack read the title of the next step. |
| 4.2 Statistics | 18 | Tooltips by finger tap on bars/points; at maximum font no horizontal scroll. With TalkBack, the chart is a named group and the table view is read row by row. |
| 4.3 Settings | 19 | Number fields: the keyboard's "Gotowe/Done" confirms and shows "Zapisano". |
| 4.4 AI | 20 | Paste a key from the clipboard (long press → Wklej); after "Zapisz klucz" the field is empty and the key is nowhere on screen. |
| 4.5 Privacy | 21 | System Back with the reset/delete dialog open must **not** close it mid-way. Do these dialogs with TalkBack too (G1, first and last control). |
| 4.6 About | 22 | Write down the exact result of "Kopiuj dziennik" ("skopiowany" or "Nie udało się skopiować"); paste into another app to check. |
| 4.7 Import and export | 23 | The cancel test (step 6) has a better chance on a device than on the emulator; "Udostępnij lub zapisz plik" is simulated in demo. |
| 4.8 Backup | 24 | Step 6: at 200% the restore dialog scrolls with a finger, title and buttons stay visible. |
| 4.9 Declined consents | 25 | As described. |
| 4.10 Re-onboarding | 17 | As described. |

Also run in EN once (Ustawienia → Język aplikacji → English) — at least onboarding and Settings.

### Part E — criteria matrix on every screen (row 26, rows 7–9, G3, G5)

For each screen listed in [emulator-testing.md, section 5](emulator-testing.md#5-criteria-for-each-screen) plus Talie, Przeglądaj, Dodaj, study and summary, go through these passes. One pass = all screens with one setting, then change the setting:

1. **Dark theme** (in app "Ciemny", then system dark with "Systemowy"): readable text, both chart colours visible, no white patches.
2. **Landscape** (row 7, G3): rotate on every screen; then open a dialog (reset, restore) and a sheet (word sheet) and rotate **with it open** and back — it stays open, nothing is clipped, Back still closes it.
3. **Maximum system font + in-app 200%** (row 8, G5): system font at maximum (and Display size larger if the phone has it), restart the app, in-app text "Bardzo duży (200%)". No clipped control, no horizontal page scroll, bottom navigation labels readable (note mid-word breaks such as "Statys-tyki" — known P2).
4. **TalkBack** (row 9, row 26): on each screen the heading is read on entry, fields have labels, switches and radios read their state, "Zapisano" and error messages are announced.
5. **Back** on each screen and dialog (except destructive dialogs in progress).

Revert: font 100%, in-app text 100%, light theme, portrait, TalkBack off.

Record: rows 7, 8, 9, 26, G3, G5.

### Part F — 50 000-card list (G6)

1. Demo bar → "Duża kolekcja (50 tys.)" → Przeglądaj.
2. Fling the list fast several times down and up, then slowly. Tick a few checkboxes while scrolling.
3. Repeat at in-app text 200%.
4. Optional measurement: Chrome → `chrome://inspect` → inspect → Performance → record during a fling; frame time p50 ≤ 20 ms, p95 ≤ 34 ms, max ≤ 150 ms (ADR-03 budget). Without the measurement, describe what you see (smooth / stutters when new pages load / blank rows).
5. TalkBack on: the list is announced with row positions ("12 z 50 000"). Off again.

Record: G6.

### Part G — native probes (G7, G8)

Ustawienia → **Diagnostyka natywna** (dev build only). These probes call the real plugins, not the mock.

1. **SecretStore (G7, ADR-07).** "Magazyn sekretów" → "Uruchom test" → "Wszystkie kroki zaliczone", Magazyn = Android Keystore backend. Force-stop the app (`adb shell am force-stop com.mila.langualinker.dev`), launch again, run the test again → "Przetrwało restart: Tak".
2. **Files (G7, ADR-12).** Put test files on the phone first:

   ```powershell
   "front,back`nHaus,dom`nHund,pies" | Set-Content -Encoding utf8 $env:TEMP\deck.csv
   adb push $env:TEMP\deck.csv /sdcard/Download/deck.csv
   ```

   "Otwórz CSV/APKG" → the system picker opens → Download → `deck.csv` → location, size and content shown. "Otwórz dowolny plik" → pick any file. "Zapisz testową kopię" → choose a folder → it is saved and read back. Closing the picker without a choice shows "Selektor zamknięto bez wyboru pliku."
3. **Notifications (G8).** "Sprawdź uprawnienie" → "Poproś o uprawnienie" → on Android 13+ the system dialog appears → Allow. "Wyślij teraz" → the notification appears in the shade. "Zaplanuj za … s" → send the app to the background (Home) → the notification appears on time → tap it → the app opens and the screen shows "Stuknięte". Repeat once with the app closed (swipe it away from recents) before the scheduled one fires. Also test deny: Settings → Apps → LanguaLinker Dev → Notifications → off → "Sprawdź uprawnienie" reports it.
4. **TTS (G8).** "Synteza mowy": ADR-12 expects "Ten WebView nie ma syntezy mowy…" on Android (a native plugin comes later). Record exactly what the device reports and whether any voice is listed; this is the known Android limitation, not a new defect.

Record: G7, G8.

### Part H — start time on a release-profile build (G4, row 1)

The debug APK is not representative. Build the dev variant in the release profile and sign it with the debug key:

```powershell
npm --prefix frontend run variant -- dev android build --apk --target aarch64
$bt = Get-ChildItem "$env:ANDROID_HOME\build-tools" | Sort-Object Name | Select-Object -Last 1
& "$($bt.FullName)\apksigner.bat" sign --ks "$env:USERPROFILE\.android\debug.keystore" --ks-pass pass:android --out $env:TEMP\ll-dev-release.apk src-tauri\gen\android\app\build\outputs\apk\universal\release\app-universal-release-unsigned.apk
adb install -r $env:TEMP\ll-dev-release.apk
```

If the install fails with a signature error, uninstall the debug build first (`adb uninstall com.mila.langualinker.dev`).

Measure 5 cold starts:

```powershell
adb shell am force-stop com.mila.langualinker.dev
adb shell am start -W -n com.mila.langualinker.dev/com.mila.langualinker.MainActivity
```

`TotalTime` is the first native frame only. The gate counts **until the "Talie" screen is visible**: record the screen (`adb shell screenrecord /sdcard/start.mp4`), launch from the launcher, stop the recording and read the time from the tap to the visible decks frame by frame (or with a stopwatch on 5 runs and take the median). Note whether a blank window shows between the launcher and the app.

If the activity name is rejected, read it with `adb shell cmd package resolve-activity --brief com.mila.langualinker.dev`.

Expected: ≤ 3 s. Record G4 and update row 1. Reinstall the debug APK afterwards if you need DevTools again.

## 7. After the test

1. Restore the phone:

   ```powershell
   adb shell settings put system font_scale 1.0
   adb shell settings put secure enabled_accessibility_services '""'
   adb shell settings put secure accessibility_enabled 0
   ```

   Also: Display size back, dark theme off, auto-rotate as you like it, notification permission back on if you turned it off.

2. Copy screenshots to `docs/release/screenshots/android-device-<row>-<desc>.png` (`adb pull /sdcard/Pictures/Screenshots/ …` for the ones taken on the phone).
3. In [device-smoke.md](device-smoke.md): set the Android column of rows 1–26, add a row to the **Results** table (date, model, Android version and API, WebView version, commit, build: dev debug APK / release-profile for G4) and a "Notes for the … physical Android run" section with one numbered note per ◐/☒ row.
4. In [phase6-gate.md](phase6-gate.md#step-7--physical-device-protocol): set the Android column of G1–G8; add new defects to the blocker backlog with a proposed priority (P0/P1/P2) and update the existing items (start time, keyboard resize, spoken TalkBack, nav labels, Back after a font change).
5. When the iOS simulator run is also done and no P0 remains, record step 7 in [Owner acceptance](phase6-gate.md#owner-acceptance).
