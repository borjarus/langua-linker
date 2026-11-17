# Release checklist (Phase 17 step 19)

Copy this list per version (beta and production). "CI" items are enforced automatically; the rest are manual and need a name and date.

## Before tagging

- [ ] CI green on the commit: Rust (fmt, clippy, tests, `cargo audit`), frontend (check, lint, i18n, Vitest, `verify:beta`, `verify:release`), E2E, `release-check`, licence report current. *(CI)*
- [ ] Version bumped everywhere; `CHANGELOG.md` entry written for users. *(CI on tag)*
- [ ] `PRIVACY_POLICY_URL` variable set and the page live with the text matching [compliance.md](compliance.md). *(CI on tag: set)*
- [ ] New dependencies since the last release: licence report regenerated and reviewed. *(CI: allowlist)*
- [ ] Any migration in this release that rewrites data takes a safety backup first; migration fixture test from the previous release's schema passes.
- [ ] `content-lint --release` passes, **or** the owner's beta-only D-04 exception is recorded here.
- [ ] Network inventory unchanged since the forms were filled, or forms updated (Data safety, App Privacy, policy).

## Build and device check

- [ ] Release workflow: APK, AAB, `play-mapping`, desktop packages, `SHA256SUMS`, provenance attestation.
- [ ] AAB opened in Play Console app bundle explorer: target SDK 36, 16 KB pages OK, native debug symbols present, permissions = INTERNET, POST_NOTIFICATIONS, RECEIVE_BOOT_COMPLETED, WAKE_LOCK.
- [ ] Android device (Galaxy A56 / Android 17), release build installed from the Play internal track:
  - [ ] edge-to-edge: header not under the status bar, bottom navigation above the gesture bar, keyboard does not cover the editor fields, light and dark system theme
  - [ ] Licences screen and privacy-policy link open in the browser, app stays where it was
  - [ ] onboarding → study → rate → undo → summary; reminder delivered and tap opens home
  - [ ] backup → share → uninstall → install → restore, counts match
- [ ] iOS: archived with the release variant, privacy report reviewed, uploaded to TestFlight, smoke run on a device or simulator (same list).
- [ ] No demo data, debug gallery, devtools or source maps in the shipped bundle. *(CI: `verify:release`, `release_security_surface_is_minimal`)*

## Store submission

- [ ] Data safety / App Privacy / age rating / target audience / app access from [compliance.md](compliance.md).
- [ ] AI report action decision implemented (Play AI-generated content policy) — or submission postponed.
- [ ] Listing texts and real screenshots from [store-listing.md](store-listing.md), PL and EN.
- [ ] Beta exit criteria met ([beta-plan.md](beta-plan.md)) — production only.

## Rollout

- [ ] Staged rollout plan and gates from [runbook.md](runbook.md) §1; release owner named.
- [ ] Recovery text (runbook §5) ready in PL/EN.

## Record

| Field | Value |
|---|---|
| Version / versionCode | |
| Tag commit | |
| Tracks (Play / TestFlight / App Store) | |
| Content version | |
| Scope not shipped or disabled | |
| Rollout stages (date → %) | |
| Halts, hotfixes | |
| Known issues | |
| Approved by / date | |
