# Changelog

All notable changes to LanguaLinker. Versions follow `X.Y.Z` / `X.Y.Z-(alpha|beta|rc).N`; a tag `vX…` publishes the entry below as the release notes (`.github/workflows/release.yml`), and `scripts/release-check.mjs --tag` refuses a tag without its entry. Entries describe what testers and users see; engineering detail stays in the plan and the ADRs.

## [Unreleased]

## [0.5.1-beta.1] — 2026-11-19

GitHub pre-release for testers, like 0.5.0-beta.1: no link to the published privacy policy yet, not for store upload. Installed decks are updated to content version 2026.11.19 (chapters and levels) on the first start; progress stays.

### Added
- Optimize a preset's FSRS weights on your own answers, on the device: the screen compares the current and the proposed weights on answers held out from training, previews the daily workload, and applies the proposal only when you choose. Earlier weights can be restored; no card is rescheduled and your history never leaves the device.
- Grammar tab: every grammar tip of your sentence decks in one place, grouped into chapters that follow the course order, with a search over tips and sentences and a link from each tip to its card. Decks link to it too.
- Decks show their CEFR level (Spanish decks: A1–A2, the others: A1).
- Word associations have an extended version (sound split, story, quick hook, alternative route, chain, example sentences, grammar, a culture scene, related words). It sits under the short association and starts collapsed.
- A word of a sentence that is not in your decks yet can be added from the study card: the sheet offers matching words you already have, or a new word pre-filled with its base form, and links it to the sentence without leaving the session.
- Decks can be kept out of study sessions ("Reference only" in the deck settings): their cards are neither studied nor counted as due, but sentences still link to their words. Progress is kept.
- Study cards show how many associations and grammar tips a card has; after the answer, tapping the count opens that section.

### Changed
- Sentence decks study both directions: from the target language, and from Polish into the target language. An update adds the missing reverse cards; existing progress stays.
- New cards of a session come half from the front side and half from the reverse side, each from a different note.
- The studied word or sentence uses a smaller font, and the four rating buttons fit in one row on phones.
- "Rename or move…" on the deck screen is now "Edit deck…".

### Fixed
- Card details show the asked side with its formatting instead of raw `**` marks.
- The descriptions of the bundled sentence decks shown during setup.

## [0.5.0-beta.1] — 2026-11-17

GitHub pre-release for testers: this build has no link to the published privacy policy yet and is not for store upload.

### Added
- Report an AI-generated or bundled association (offensive, wrong, unclear): the app shows the message and opens your mail app addressed to the developer.
- Privacy screen explains what leaves the device (only the card text you send for an AI association, to the provider you chose) and how to keep your data elsewhere; a link to the privacy policy in store builds.
- Licences screen lists every bundled third-party component with its licence (generated report).

### Changed
- Android: targets Android 16 (API 36) as Google Play requires; content is laid out around the system bars and the keyboard (edge-to-edge).
- Android: no longer offered on Android TV.
- Links to web pages open in the system browser instead of inside the app.

### Fixed
- A build made on Windows could refuse to open a collection (or a backup) created on Android, macOS or Linux.
- Android: the status and navigation bars follow a light/dark system theme change while the app is open.

## [0.4.0-beta.1] — 2026-11-16

### Added
- Daily study reminders on Android (only when something is due, in the app language; tap opens the home screen).
- Native text to speech on Android with audio focus; Web Speech elsewhere.
- CSV import and Anki `.apkg` export.
- Gemini as an AI provider.

### Fixed
- Android build, a startup race and paging in Browse.

## [0.3.0-beta.2] — 2026-11-05

### Fixed
- Release packaging of the first beta.

## [0.3.0-beta.1] — 2026-11-05

### Added
- FSRS-6 scheduling and real study sessions connected to the screens.

## [0.2.0] — 2026-11-04

### Added
- Bundled decks, deck package import and the Anki converter.

## [0.1.0] — 2026-11-03

### Added
- First installable build: the complete GUI with demonstration data.
