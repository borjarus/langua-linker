# Release and incident runbook (Phase 17 step 4)

Rules behind it: [ADR-21](../adr/ADR-21-release-engineering.md). One person is release owner per version and records every decision in §6.

## 1. Staged rollout (step 16)

Android (Play production track, staged rollout):

| Stage | Users | Hold at least | Go to next only if |
|---|---|---|---|
| 1 | 5 % | 48 h | no S1; Play vitals crash rate and ANR rate below Play's bad-behaviour thresholds and not above the previous version; no data-loss report |
| 2 | 20 % | 48 h | same, plus no new S2 cluster (3+ reports of the same issue) |
| 3 | 50 % | 72 h | same |
| 4 | 100 % | — | — |

iOS: App Store **phased release** for automatic updates (7-day ramp, 1→2→5→10→20→50→100 %); pause it (up to 30 days) under the same conditions. New downloads always get the newest version — phased release limits only automatic updates.

Cost gate: none on our side (BYOK, no server). If a provider changes pricing or behaviour, it affects only users with AI on; communicate, do not block the rollout.

## 2. Stop a rollout

- **Play**: Release → Production → *Halt rollout*. Users who already updated keep the version; fix forward (§4). Resuming is possible after the fix is understood.
- **App Store**: *Pause phased release*; for a severe issue also *Remove from sale* only as a last resort (existing installs stay).
- **Beta**: Play closed test → *Halt*/new release; TestFlight → *Expire build*.

## 3. Disable a feature without a server (step 17)

There is no remote switch (ADR-21 §15). In order of preference:

1. **AI associations misbehave** (provider outage, bad answers): nothing to do for outages — requests back off up to 15 min, are capped at 3 calls and 100/day, and study continues offline. For harmful output: tell users to turn AI off (Settings → AI), extend the local term list, ship a hotfix. Users can revoke their own key at the provider.
2. **Reminders misfire**: users turn them off in Settings; hotfix replaces the schedule at the next start.
3. **Anything else**: hotfix; if the defect can damage data, halt the rollout first.

## 4. Hotfix

1. Branch `hotfix/X.Y.Z+1` from the release tag; fix with a test reproducing the issue.
2. Bump the patch version everywhere (`node scripts/release-check.mjs` lists the files; the versionCode follows), CHANGELOG entry.
3. CI green → tag → release workflow → upload AAB to the **same** track, IPA via Xcode (request expedited App Review if S1).
4. Roll out the hotfix starting at the stage where the bad build was halted; do not resume the bad build.

## 5. Database and data recovery (step 18)

- **Never roll back to an older build** once a release migrated the database: older builds refuse a newer schema and touch nothing (by design). Stores do not offer downgrades either.
- **Fix forward**: a new build with a corrective migration. A migration that rewrites data must take a safety backup first (release checklist).
- **User recovery path** (send this text, PL/EN): Settings → Backup lists automatic backups (taken on start/foreground after study, newest 3 kept) and safety copies (before every restore/reset). "Restore" validates the archive, takes another safety copy, then swaps it in. A backup made by a newer version cannot be restored into an older one.
- **Collection fails to open at start** (startup error screen): ask for the redacted diagnostic events (About → Diagnostic events), never the database file; check on a copy which recovery the build offers before advising the user.

## 6. Communication

| Situation | Channel | Within |
|---|---|---|
| S1 confirmed | store "What's new" of the hotfix, support page, testers' channel | 24 h |
| Rollout halted | internal record (§7); users only if they are affected | same day |
| Data-loss risk | direct message to affected testers with the recovery text from §5 | at once |

Message rules: say what happened, who is affected, what to do now (usually: update; do not reinstall without a backup), and when the fix comes. Never ask users to send their database or API key.

## 7. Release record (step 19)

Copy per version into the [release checklist](release-checklist.md) "Record" section: version, versionCode, tag commit, platforms and tracks, scope disabled or not shipped (e.g. Spanish, desktop, AI report action), content version, rollout stages with dates, halts and why, hotfixes, open known issues.
