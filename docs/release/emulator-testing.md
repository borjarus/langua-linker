# Testing the app on an Android emulator (and iOS simulator)

Step-by-step instructions for a manual pass on the emulator. It closes the open DoD item of phase 5 ("Light/dark/200% and accessibility pass on devices") and can be repeated in later phases. Record results in [device-smoke.md](device-smoke.md) (rows 17–26).

The emulator does not replace a device: the phase 6 gate requires a physical Android phone and an iPhone ([plan, phase 6](../Detailed-Implementation-Plan.md#phase-6--complete-gui-acceptance-gate)).

## 1. Setup (one-time)

1. Tools installed per [mobile-setup.md](mobile-setup.md): JDK 17 (`JAVA_HOME`), Android SDK (`ANDROID_HOME`), NDK (`NDK_HOME`), Rust target `x86_64-linux-android`.
2. Frontend dependencies:

   ```powershell
   npm --prefix frontend ci
   ```

3. Emulator (AVD). Used so far: `Medium_Phone_API_36.1` (Android 16, x86_64, Play Store image, WebView 134). Check the list and start it:

   ```powershell
   & "$env:ANDROID_HOME\emulator\emulator.exe" -list-avds
   & "$env:ANDROID_HOME\emulator\emulator.exe" -avd Medium_Phone_API_36.1
   adb devices        # emulator-5554  device
   ```

   To check the on-screen keyboard, create a separate AVD with `hw.keyboard=no` (in an AVD with a hardware keyboard, Gboard only shows the bar).

4. If Gradle reports `SSLException: (bad_record_mac)`, run this in the same PowerShell session:

   ```powershell
   $env:JAVA_TOOL_OPTIONS = '-Djdk.tls.client.protocols=TLSv1.2 -Dhttps.protocols=TLSv1.2'
   ```

## 2. Build and install

The `dev` variant (`com.mila.langualinker.dev`, "LanguaLinker Dev") includes the demo frontend with a mock — this is what we need for phases 2–6.

```powershell
git rev-parse --short HEAD     # save the commit for the results table
npm --prefix frontend run android:apk -- --target x86_64
adb install -r src-tauri\gen\android\app\build\outputs\apk\universal\debug\app-universal-debug.apk
adb shell monkey -p com.mila.langualinker.dev 1      # launch the app
```

Alternative with live reload (the emulator must be able to reach the Vite server): `npm --prefix frontend run android:dev`.

## 3. Tools during the test

| Purpose | Command / location |
|---|---|
| Demo data scenario | Demo bar at the top → "Scenariusz demo" (e.g. "Nowy użytkownik", "Pusta kolekcja", "Błąd bazy danych"). Changing it resets the demo memory. |
| File that the system picker "returns" | On the Import and export and Backup screens, the demo bar shows "Wybór pliku w demo zwraca" — pick a file or "Nic (zamknięto okno wyboru)". The mock does not open a real picker and does not touch files. |
| 200% text (system) | `adb shell settings put system font_scale 2.0` (revert: `1.0`) |
| 200% text (in app) | Settings → Rozmiar tekstu → Bardzo duży (200%) |
| Dark theme | Settings → Motyw → Ciemny, or system-wide `adb shell cmd uimode night yes` (revert: `no`) with the "Systemowy" theme |
| TalkBack | `adb shell settings put secure enabled_accessibility_services com.google.android.marvin.talkback/com.google.android.marvin.talkback.TalkBackService` and `adb shell settings put secure accessibility_enabled 1`; to disable: `adb shell settings put secure enabled_accessibility_services ""` and `accessibility_enabled 0` |
| Screen rotation | Rotate buttons on the emulator bar (or `adb shell settings put system user_rotation 1` with auto-rotate off) |
| Screenshot | `adb exec-out screencap -p > docs\release\screenshots\android-phase5-<nr>-<opis>.png` |
| DOM/layout inspection | Chrome on PC → `chrome://inspect` → the app's WebView (debug build only) |
| Back | Hardware/gesture Back button of the emulator |

Note from ADR-12: Android WebView scales system text as "text zoom", so with `font_scale 2.0` the `rem` units do not grow along with the text — that is why you should check both 200% variants (system and in-app).

## 4. Phase 5 scenarios

Run each scenario in PL (default) and at least once in EN (Settings → Język aplikacji). On every screen, also check the general criteria from section 5.

### 4.1 Onboarding (scenario "Nowy użytkownik")

1. Demo bar → "Nowy użytkownik". The "Witaj" screen opens, step 1 of 5.
2. "Dalej" → step "Języki do nauki". Untick German: "Wybierz co najmniej jeden język." appears and "Dalej" is disabled. Tick German and English.
3. Step "Cel dzienny": choose "30 kart dziennie"; type 0 in the custom field → range message 1–500; go back to 30.
4. Step "Talie": the three built-in decks are visible (two German, one English) with description, level and card count.
5. Step "Gotowe": summary, "Zaczynamy" → preparation progress bar → deck screen with the decks.
6. Expected: after each "Dalej", TalkBack reads the title of the new step (focus on the heading).

### 4.2 Statistics

1. Scenario "Typowa kolekcja", "Statystyki" tab.
2. Tiles: study streak (with longest streak), reviews, study time, accuracy for young and mature cards. Expand "Czym jest rzeczywista skuteczność?".
3. Period: switch 1 miesiąc / 3 miesiące / 1 rok / Wszystko. The description under "Powtórki" changes (day/week/month); previous charts stay dimmed until reload, without skeleton flicker.
4. Deck: choose "— Złożenia (Komposita)" — the numbers decrease.
5. Chart "Prognoza powtórek": bars and a line, legend under the chart, description of assumptions above it. Tap a bar → tooltip with the date and two values, kept inside the chart edge, not off-screen.
6. "Pokaż jako tabelę" on each chart → table with the same values; "Pokaż jako wykres" switches back.
7. "Zaawansowane: trudność i stabilność" → scatter chart; tapping a point shows stability and difficulty.
8. TalkBack: the chart is announced as a group with a name; after entering the chart, arrows/gestures read the subsequent values (on an emulator without a keyboard, it is enough to check that the table is read).
9. Demo bar → "Pusta kolekcja": "Brak kart do analizy" with buttons "Przejdź do talii" and "Dodaj", no "0%".
10. Demo bar → "Błąd bazy danych": error message with "Spróbuj ponownie".

### 4.3 Settings

1. "Ustawienia" tab. Change "Cel dzienny" to 35 and confirm (Enter/Gotowe on the keyboard) → "Zapisano".
2. "Nowy dzień nauki zaczyna się o" → 05:00; the hint mentions 05:00.
3. Time zone, option set for new decks, language of new decks — each change shows "Zapisano".
4. "Zezwól na powiadomienia" — in the demo build the answer is given by the mock (consent); the Android system dialog will only appear with the real adapter (phase 15). Set the reminder time and "Wyłącz przypomnienie".
5. Pronunciation: untick "Pokazuj przycisk odsłuchu na kartach" → the speed slider becomes disabled; no listen button in study. On Android, with languages, "synteza mowy nie jest tu obsługiwana" appears (ADR-12) — this is expected.
6. App language → English: the whole app changes, including navigation and the demo bar.
7. The list at the bottom leads to: AI, Prywatność, Import i eksport, Kopia zapasowa, O aplikacji, Licencje, "Ponownie wybierz języki i talie".

### 4.4 AI and associations

1. "Nie używaj AI" → message that study works fully; provider and limit sections disappear.
2. Go to study, answer a card, rate it — works. Trying "Wygeneruj" for an association ends with a message about missing consent; rating still works.
3. Go back, "Zezwól…". Provider "OpenAI-compatible": type `http://x` in the address field → "Użyj adresu zaczynającego się od https://."; type `https://llm.example.com/v1` → "Zapisz".
4. Paste any key (min. 8 characters) → "Zapisz klucz": the field clears, "Klucz jest zapisany…" appears, the key is not shown anywhere. "Usuń klucz" → "Brak zapisanego klucza."
5. "Limit użycia" section: usage bar and renewal date. Scenario "Wyczerpany limit AI" → message that the limit is exhausted.

### 4.5 Privacy, reset and deletion

1. "Udostępniaj anonimowe statystyki użycia" is off by default.
2. "Resetuj postęp…": the dialog describes the scope (review counts, cards, decks) and irreversibility; "Resetuj postęp" is disabled.
3. Type the shown phrase ("RESETUJ POSTĘP") → button enabled → reset → message and deck screen. Statistics show no cards in reviews.
4. "Usuń wszystko…" → "Utwórz kopię bezpieczeństwa" (progress bar, then "Kopia bezpieczeństwa utworzona") → "Usuń wszystko" → the app starts from onboarding, a message points to the backup.
5. "Kopia zapasowa i przywracanie" screen: the backup from step 4 is in the list — restore it (4.8), the collection returns.
6. Back with the reset dialog open does not close it accidentally (a destructive dialog requires its button).

### 4.6 About and licenses

1. "O aplikacji": version, build "Demo", contract `0.5`, schema "brak (pamięć demo)", FSRS-6.
2. "Pokaż dziennik diagnostyczny" → rows with `[redacted]` and information about the number of hidden values. "Kopiuj dziennik" — note whether "skopiowany" or "Nie udało się skopiować" appears (clipboard in WebView depends on the platform; the second result is a known limitation, not a blocker).
3. "Licencje": groups Treści / Oprogramowanie.

### 4.7 Import and export

1. Demo bar → file `slowka-niemiecki.csv`. "Wybierz plik CSV" → "Próba na sucho": New notes 12, Skipped 2, Errors 1; info about BOM; expand "1 wiersza nie da się zaimportować" → "Wiersz 9: Pole „Przód” jest puste."
2. Separator → comma: error counts grow; go back to "Wykryty".
3. Duplicates → "Zaktualizuj je": Updated 2 and an overwrite warning.
4. Columns: set two columns as "Przód" → message and disabled import button; restore.
5. "Importuj 14 notatek" → progress → "Import zakończony…", "Otwórz talię" opens the deck with the new cards.
6. Cancellation: choose "Importuj do" → "Nowa talia …", start the import and immediately press "Anuluj" (on the demo emulator the steps take ~0.35 s, so it may fail — then skip) → "Import anulowany — nic nie zostało zaimportowane."
7. Demo bar → `Deutsch_A1.apkg` → 3-step Anki export instructions.
8. Demo bar → "Nic (zamknięto okno wyboru)" → nothing happens, no error.
9. Export: choose a deck → "Eksportuj talię" → "Plik … jest gotowy" → "Udostępnij lub zapisz plik" → "Plik udostępniony." (in demo, sharing is simulated; the Android WebView has no Web Share — ADR-12).

### 4.8 Backup and restore

1. List: three backups with type (automatic/manual) and location. "Utwórz kopię zapasową" → progress → "Kopia zapasowa utworzona.", the list has 4 items.
2. "Przechowuj" → 3 newest backups; "Wybierz folder…" → "Folderze: Dokumenty/LanguaLinker" is selected.
3. Demo bar → `langualinker-uszkodzona.llbackup`, "Przywróć z pliku…" → message about a damaged backup, the dialog does not open. The same for `langualinker-v9.llbackup` (newer app version).
4. Demo bar → `langualinker-2026-02-27.llbackup` → dialog: "Teraz / Po przywróceniu" table, warning about replacing the collection, info about the safety backup → "Zastąp i przywróć" → deck screen with a message to reopen.
5. Demo bar → `langualinker-przerwana.llbackup` → "Zastąp i przywróć" → "Przywracanie nie powiodło się. Obecna kolekcja nie została zmieniona." → "Przywróć zamiast tego kopię bezpieczeństwa" → the dialog shows the safety backup.
6. At 200%, the dialog content scrolls with a finger, while the title and buttons stay visible.

### 4.9 Declining AI, telemetry and notifications does not block study

1. AI: "Nie używaj AI"; telemetry off; notifications without consent.
2. Decks → "Ucz się wszystkiego" → "Pokaż odpowiedź" → rating "Dobrze" → counter "1 powtórka".

### 4.10 Re-onboarding from settings

1. Settings → "Ponownie wybierz języki i talie": a visible Back button in the header.
2. In the "Talie" step, installed decks are checked and described "Już jest w Twojej kolekcji".
3. "Zapisz i dodaj talie" → back to Settings.

## 5. Criteria for each screen

Check these screens: Statistics, Settings, AI, Privacy (and the reset dialog), About, Licenses, Onboarding, Import and export (with preview), Backup (and the restore dialog):

1. **Light and dark theme** — text readable, charts visible (two series colors), no white patches.
2. **200% system and 200% in-app** — no horizontal page scrolling, nothing cut off or overlapping, buttons have full labels.
3. **Landscape** — layout with the side menu, no clipping.
4. **TalkBack** — screen heading announced on entry; form fields have labels; switches and radios announce their state; "Zapisano" messages and errors are announced.
5. **Back** — closes an open dialog (except destructive dialogs in progress), otherwise returns to the previous screen.
6. **On-screen keyboard** (AVD with `hw.keyboard=no`) — the daily goal field, server address and API key are not covered.

## 6. After the test

1. Restore emulator settings:

   ```powershell
   adb shell settings put system font_scale 1.0
   adb shell cmd uimode night no
   adb shell settings put secure enabled_accessibility_services '""'
   adb shell settings put secure accessibility_enabled 0
   ```

2. Fill in rows 17–26 in [device-smoke.md](device-smoke.md) (☑ / ◐ / ☒) and add a row to the "Results" table (date, AVD, Android and WebView version, commit). Describe each defect in the notes with its row number and a `android-phase5-*.png` screenshot.
3. If everything passed on Android and iOS, tick the last DoD item of phase 5 in the [plan](../Detailed-Implementation-Plan.md).

## 7. iOS simulator (requires a Mac)

Setup: [mobile-setup.md, iOS section](mobile-setup.md#ios--own-mac-to-do-on-the-mac).

```bash
npm --prefix frontend run ios:dev -- "iPhone 17"
xcrun simctl ui booted appearance dark                                   # dark theme (light: revert)
xcrun simctl ui booted content_size accessibility-extra-extra-extra-large  # largest text (large: revert)
xcrun simctl io booted screenshot docs/release/screenshots/ios-phase5-<nr>.png
```

VoiceOver: Simulator Settings → Accessibility → VoiceOver (or Accessibility Inspector in Xcode). Same scenarios from section 4 and criteria from section 5; differences: on iOS pronunciation works (a sample is audible in Settings), and the WebView has Web Share. Results go in the iOS column of [device-smoke.md](device-smoke.md).
