# ADR-17: Statistics, settings, backup, restore and reset

- Status: Proposed (2026-11-10); implemented in Phase 12. Acceptance after the Phase 12 device run (Android: backup → uninstall → reinstall → restore from a shared copy).
- Plan references: Phase 12 steps 1–16 and DoD; D-14 (external backup/encryption); ADR-07 (secret store); ADR-09 (SQLite persistence); ADR-15 (scheduler); ADR-16 (learning flow)

## Context

Phase 5 built the statistics, settings, privacy and backup screens on the demo adapter. Phase 12 connects them to the Rust collection without changing the screens; contract v1 (1.2) stays frozen. Three things needed decisions: what each statistic means (the plan asks for explicit definitions and the differences from Anki), where settings live, and how a backup is taken and put back without risking the collection.

## Decision

### Statistics

1. **Reviews per day** — valid (not undone) ratings, manual entries excluded, grouped by study day of the collection calendar (IANA zone + cutoff). Time per day is the stored answer time (already pause-aware and capped at 60 s per rating, ADR-16). Ratings follow their card: a deck's statistics include the history of its cards, also of cards deleted later in the collection-wide view.
2. **True retention** — ratings whose card was in Review before the rating, made on or after the card's due day, first such rating per card per study day. **Again fails; Hard, Good and Easy pass.** Young/mature by the interval *before* the rating (mature ≥ 21 days). Fewer than 10 qualifying ratings → `null` ("not enough data"), never 0 %.
3. **Differences from Anki's True Retention table** (to be re-checked against the Anki version used for the import fixtures): we count only the first qualifying rating per card per study day, we exclude early reviews (made before the due day) because they do not test recall at the scheduled interval, and we have no filtered decks. A "Good/Easy proportion" would be a separate figure; contract v1 has no field for it, so it is not shown.
4. **Forecast** — stored due dates per study day for 30 days; day 0 also holds overdue cards (`overdue` says how many). The projection carries the backlog forward under the preset's review limit (the deck's preset, or the default preset for all decks). New cards and future answers are unknown, so they are never projected. Day 0 equals the scheduler's due count without limits (test `forecast_day_zero_matches_the_scheduler…`).
5. **Card states** come from the stored cards; suspended cards count only as suspended. Maturity uses the current interval. The stability histogram uses fixed day buckets; the D/S scatter is sampled evenly above 2 000 cards so the payload stays small.
6. **Streak** — the dashboard's definition (ADR-16) applied to the scope; the longest streak is the longest run within the range or the current streak, whichever is longer.

### Settings

7. **One versioned document** (`settings` key) for the settings screens. Values other code already reads stay where they are: the daily goal (`dailyGoal`) and the study-day calendar (`collection.time_zone`, `day_cutoff_minutes`). An update writes everything in one transaction and bumps the document version once; a stale form gets `VERSION_CONFLICT`. A retried `operationId` returns the current settings without applying the patch again.
8. **Calendar** — the first start adopts the device zone (the schema default was UTC). Changing zone or cutoff only regroups the UTC review log into study days; nothing is rewritten. Today's counts and the streak change at once (`collection_changed` with decks/cards/stats).
9. **Default preset** — new decks get the chosen preset while it exists, otherwise the built-in one.
10. **Consent** — AI and telemetry answers are stored with the consent-text version and the time; an answer to an older text counts as unset. Withdrawing AI consent blocks queued and in-flight LLM requests (`status = 'blocked'`) in the same transaction, before any can reach the network. Changing the provider removes the stored key, which belonged to the previous provider. The key itself is set in Phase 13.
11. **Not yet real**: notification permission reports `unsupported` until Phase 15 (no prompt that would answer `NOT_IMPLEMENTED`); AI quota is `null` until Phase 13.

### Backup

12. **Archive** (`.llbackup`): one manifest JSON line (format, format version, kind, app and schema versions, counts, size and SHA-256 of the database part, media list), then the SQLite bytes. Secrets never enter the database (ADR-07), so archives never hold the AI key (tested). Content is sensitive and **not encrypted**; passphrase encryption stays D-14.
13. **Snapshot** — `VACUUM INTO` on the database worker: one consistent read with the WAL content, never a raw copy of the live file. The copy is checked (`quick_check`, schema version, counts), jobs it caught mid-flight are closed as interrupted, then it is hashed, written to a hidden temporary file, synced and renamed. A kill leaves no archive or a complete one; a half-written file under a backup name is skipped by the list and refused on restore.
14. **Where** — `app` backups live in the app data folder and are **removed when the app is uninstalled**; the backup screen's target hint points to a folder outside the app. On desktop the user can pick an external folder (stored as a path in the settings, shown by name only). On every platform *Share* saves a copy wherever the user chooses through the system save dialog — the way out of the sandbox on mobile, where folder grants are not plain paths.
15. **Rotation** — the newest N regular backups per folder (default 3, setting 1–50), safety copies apart (newest 5). **Automatic** backups run when the app starts or returns to the foreground (checked at most hourly), only if enabled, studied since the newest regular backup and that backup is at least a day old. Nothing runs while the app is closed: there is no native scheduler.
16. **Errors** — a full disk is `STORAGE_FULL`; an unwritable folder `PERMISSION_DENIED` (`reason`: `backupFolder`, `backupStorage`, `externalFolder`, `fileRead`, `fileWrite`); a damaged archive `BACKUP_CORRUPT` (`reason`: `format`, `manifest`, `size`, `checksum`, `integrity`, `database`, `schema`); a newer schema `SCHEMA_TOO_NEW`; a raw SQLite or zip file, a newer archive format or an archive with media `UNSUPPORTED_FORMAT` (`format`: `sqlite`, `zip`, `newerBackup`, `media`). Upgrading the schema already writes a pre-migration copy (ADR-09).

### Restore

17. **Validate in staging** — the picked archive is copied into the staging folder right after the pick (mobile URIs do not stay readable); the database part is extracted with size and checksum checks, opened read-only and checked for integrity, schema and counts. Only then a preview token is issued; the collection is untouched.
18. **Restore job** — re-checks the staged copy, writes a **safety backup** of the current collection, migrates the staged copy to this build's schema, records the running job and its operation id in it (so the job stays readable and a retry returns it), and swaps it in with the SQLite backup API in **one step = one write transaction** on the database worker. A crash or kill during the copy leaves the previous collection (WAL rollback); a failed copy leaves it too (tested with a non-database source). A collection that changed after the preview, or another running job, refuses the restore (`PREVIEW_STALE`, `DB_BUSY`).

### Reset and delete

19. **Confirmation** — a reset needs a backup made after its preview whose archive verifies in full, or the phrase typed exactly (`RESETUJ POSTĘP` / `USUŃ WSZYSTKO`, `RESET PROGRESS` / `DELETE EVERYTHING` by UI language). A wrong phrase or an unverified backup keeps the preview usable; the token is spent when the job starts.
20. **Reset progress** — every live card returns to New: FSRS memory, reps, lapses, steps and due dates cleared, queue positions renumbered (new cards first, then studied ones in their old order). Suspended cards stay suspended. The review history, undo entries and study sessions are **deleted** (not invalidated): progress is gone unless restored from the safety copy. Notes, cards, decks, presets and FSRS parameters remain.
21. **Delete everything** — swaps in an empty collection (same zone and cutoff, UI language and theme kept, settings version moves on), deletes media and staged files, removes the AI key from the platform store. The database pages and the WAL are replaced and checkpointed; we do not promise a secure overwrite of flash storage. **Backup files stay** — they are the way back — and copies the user saved elsewhere are theirs to delete. The UI keeps no drafts in browser storage.

## Consequences

- Statistics, settings, backup, restore and reset are real in release builds; `generateAssociation`, `setAiApiKey` (13), CSV import/export (14) and notification permission (15) still answer `NOT_IMPLEMENTED`.
- Restore and delete replace the database under the open connection, so every in-memory preview (study, bulk, import) becomes stale and answers `PREVIEW_STALE`; the UI clears its query cache after a restore.
- `shareFile` is real for backups (save dialog); for exports it answers `NOT_FOUND` until export jobs exist (Phase 14). A native share sheet can replace the dialog in Phase 15 without a contract change.
- Archives are unencrypted; anyone with the file can read the collection. D-14 decides on passphrase encryption.
