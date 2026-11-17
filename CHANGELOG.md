# Changelog

All notable changes to LanguaLinker. Versions follow `X.Y.Z` / `X.Y.Z-(alpha|beta|rc).N`; a tag `vX…` publishes the entry below as the release notes (`.github/workflows/release.yml`), and `scripts/release-check.mjs --tag` refuses a tag without its entry. Entries describe what testers and users see; engineering detail stays in the plan and the ADRs.

## [Unreleased]

### Added
- Privacy screen explains what leaves the device (only the card text you send for an AI association, to the provider you chose) and how to keep your data elsewhere; a link to the privacy policy in store builds.
- Licences screen lists every bundled third-party component with its licence (generated report).

### Changed
- Android: targets Android 16 (API 36) as Google Play requires; content is laid out around the system bars and the keyboard (edge-to-edge).
- Android: no longer offered on Android TV.
- Links to web pages open in the system browser instead of inside the app.

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
