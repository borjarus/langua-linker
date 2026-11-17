# Store listing — texts and assets (Phase 17 step 10)

Status: draft 2026-11-17. Rules: describe only what the submitted build does (no "coming soon", sync, desktop, optimizer or more languages than shipped); "Anki" only as a compatible file format, never in the title or as a keyword promise; AI is optional and BYOK — say so. Character limits: Play title 30, short description 80, full 4000; App Store name 30, subtitle 30, promotional text 170, keywords 100.

## Names

- Play title / App Store name: **LanguaLinker** (beta channel: "LanguaLinker Beta" is the app label of the beta variant).
- App Store subtitle (PL): *Fiszki z obrazowymi skojarzeniami* · (EN): *Flashcards with vivid mnemonics*

## Short description (Play, ≤ 80)

- PL: *Fiszki DE/EN dla Polaków: powtórki FSRS i obrazowe skojarzenia. Działa offline.*
- EN: *German & English flashcards for Polish speakers: FSRS reviews, vivid mnemonics.*

## Full description

### PL

LanguaLinker pomaga zapamiętać słówka i zdania po niemiecku i angielsku dzięki obrazowym skojarzeniom i powtórkom zaplanowanym algorytmem FSRS.

- Gotowe talie słówek i zdań (poziom A1–B1), zainstalowane od razu — bez konta i bez internetu.
- Skojarzenia, wskazówki gramatyczne i powiązania słówek ze zdaniami.
- Powtórki FSRS: aplikacja pokazuje kartę wtedy, gdy zaczynasz ją zapominać.
- Wymowa w języku docelowym (głos systemu).
- Własne karty, przeglądarka i wyszukiwarka, statystyki, przypomnienia.
- Kopia zapasowa, import CSV i eksport do pliku .apkg (format zgodny z Anki).
- Opcjonalnie: skojarzenia generowane przez AI u wybranego dostawcy (Anthropic, Gemini lub serwer zgodny z OpenAI) z Twoim własnym kluczem API. Oznaczone jako wygenerowane przez AI.

Twoje dane zostają na urządzeniu. Bez reklam i bez analityki.

### EN

LanguaLinker helps Polish speakers remember German and English words and sentences with vivid mnemonic associations and reviews scheduled by the FSRS algorithm.

- Ready-made word and sentence decks (A1–B1), installed at once — no account, no internet needed.
- Associations, grammar tips and links between words and sentences.
- FSRS reviews: a card comes back when you are about to forget it.
- Pronunciation in the target language (system voice).
- Your own cards, browser and search, statistics, reminders.
- Backup, CSV import and export to an .apkg file (Anki-compatible format).
- Optional: AI-generated associations from the provider you choose (Anthropic, Gemini or an OpenAI-compatible server) with your own API key. Marked as AI-generated.

Your data stays on your device. No ads, no analytics.

> Owner check before submitting: the deck sizes and levels named here must match the decks approved under D-04; Spanish is in the content but not in the D-06/D-01 scope statement — mention it only if the release ships it as supported.

## Screenshots (real build, PL and EN)

Taken from the release variant on a phone with the bundled decks (no demo data, no debug banner), light theme, default font size; one set per UI language. Play: phone 16:9 or 9:16, 2–8 images; App Store: 6.9" (1320×2868) set required, 6.5" optional.

1. Home / dashboard with due counts
2. Study card — question
3. Study card — answer with association and word sheet
4. Session summary
5. Statistics
6. Browser with search
7. Editor
8. Settings → Privacy ("Your data")

Existing images in [screenshots/](screenshots/) are test evidence from dev/beta builds and are **not** store assets.

## Other assets

- Icon: `src-tauri/app-icon.png` → Play 512×512 (no alpha requirement issues to check), App Store 1024×1024 without alpha.
- Play feature graphic 1024×500: to be made.
- Support URL / e-mail: [owner]. Privacy policy URL: same as `PRIVACY_POLICY_URL`.
- Category: Education. Contains ads: no. In-app purchases: no.
