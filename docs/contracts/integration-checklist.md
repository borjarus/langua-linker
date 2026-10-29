# Per-screen integration checklist (contract v1.0)

Approved at the Phase 6 gate as the condition for replacing demo data with Rust, screen by screen, in Phases 7–15. The GUI is not rebuilt: the adapter is chosen once at the root (`frontend/src/app/bootstrap.ts`), and `frontend/tests/adapterSwap.test.ts` shows every screen already renders through `TauriAppApi` without changes.

## Rules for every command (no silent mock fallback)

1. **Registered before it works.** Phase 7 registers every v1 command from `crates/contracts/v1/catalogue.json`. A command whose phase has not arrived returns `ErrorDto { code: NOT_IMPLEMENTED }` — never a "command not found" string (that reaches the UI as `INTERNAL`) and never demo data.
2. **Rust DTO = frozen sample.** The Rust input type deserializes the frozen `input` of each sample for the command, and the output serializes to the frozen shape (keys, kinds, nullability). Add the check to `crates/contracts/tests/contract_v1.rs` when the DTO lands; a deliberate shape change follows the versioning rules in [README.md](README.md#versioning-v1-and-later).
3. **Declared errors only.** The command returns the error codes the catalogue declares for it (plus `INTERNAL`, `NOT_IMPLEMENTED`); `messageKey` is `errors.<CODE>`; no SQL, paths or card content in `details`.
4. **Idempotent mutations.** A repeated `operationId` returns the first result; versioned entities reject a stale `expectedVersion` with `VERSION_CONFLICT`; previews reject a stale token with `PREVIEW_STALE`.
5. **Events are signals.** `collection_changed` after each committed mutation with the catalogue's scopes; `job_progress` with a per-job monotonic `sequence`; `get_job_status` is the truth and carries the `result`.
6. **The adapter stays.** `TauriAppApi` is not changed per command. The production profile never imports `src/demo` (`npm run verify:release`, `verify:beta`), and an IPC error never switches adapters (`frontend/tests/contracts.test.ts`).
7. **Demo stays honest.** The demo profile keeps its banner and notices; the production profile shows neither (`Screen` renders notices only with `demo` services).

## Per screen

Tick a screen when all its operations are real in the production build and its checks pass on a device. "Phase" is the plan phase that connects the operation (`backendPhase` in `operations.ts`).

| Screen | Operations (phase) | Before switching the screen | Done |
|---|---|---|---|
| Shell | getAppInfo (1), onCollectionChanged (7), getSettings (12), getOnboardingState (10) | `collection_changed` emitted by every mutation; the first start routes to onboarding from the real `getOnboardingState` | ☐ |
| Onboarding | getOnboardingState, completeOnboarding (10); getJobStatus, cancelJob, onJobEvent (7); updateSettings (12) | bundled decks installed by the importer (Phase 8); the prepare job reports progress and survives leaving the screen | ☐ |
| Dashboard `/home` | getDashboard, startStudy, resumeStudy (10); createDeck (11) | counts from the scheduler, study day with zone and cutoff; resumable session read from SQLite | ☐ |
| Study | resumeStudy, updateStudySession, getRatingPreview, rateCard, undoReview, applyCardAction (10); getNoteContent, resolveWordLink, setFavoriteAssociation (11); generateAssociation (13); getJobStatus, onJobEvent (7); getSettings (12) | durable session (pause/resume after restart); the rating commits in one transaction before the next card; `PREVIEW_STALE` after a preset change; undo checks later mutations; TTS stays a platform adapter (15) | ☐ |
| Summary | getSessionSummary, startStudy (10); getSettings, updateSettings (12) | summary from review logs, not from the session object | ☐ |
| Deck `/deck/:id` | getDeck, createDeck, updateDeck, deleteDeck, listPresets, savePreset, getEditorData (11); startStudy (10) | no cycles on move; type change hides and restores; a preset change applies to every dependent deck and refreshes previews | ☐ |
| Card `/card/:id` | getNoteDetails, getNoteContent, deleteNote, updateNoteContent, setFavoriteAssociation, resolveWordLink, searchWords (11); applyCardAction (10); generateAssociation (13); onJobEvent (7) | per-card S/D/R from FSRS (Phase 9); delete removes history; word links become `unresolved` | ☐ |
| Editor | getEditorData, getNoteDetails, checkDuplicate, saveNote, searchWords (11); generateAssociation (13) | note-type change archives and restores cards with history; duplicate check on the normalized front; draft guard unchanged | ☐ |
| Browser | searchCards, previewBulkAction, applyBulkAction, getEditorData (11) | FTS5 search with keyset cursors; `filterVersion` snapshot for "all matches"; 50 000 cards within the ADR-03 frame budget on a device | ☐ |
| Statistics | getStats (12); getEditorData (11) | computed from review logs; `trueRetention` stays `null` below 10 reviews; forecast with deck limits | ☐ |
| Settings | getSettings, updateSettings (12); listPresets (11); requestNotificationPermission (15) | settings in SQLite with `version`; a refusal is the state `denied` | ☐ |
| AI | getSettings, updateSettings (12); setAiApiKey (13) | key only in the SecretStore (ADR-07), never returned or logged | ☐ |
| Privacy | getSettings, updateSettings, previewReset, resetCollection, createBackup, listBackups (12); onJobEvent, getJobStatus (7) | reset requires a backup newer than the preview or the phrase; `everything` keeps backups | ☐ |
| About | getAppInfo (1); getAboutInfo, getDiagnosticsReport (7) | schema version from the database; log lines redacted in Rust before IPC | ☐ |
| Licences | getAttributions (8) | generated from the bundled-content manifest and dependency licences | ☐ |
| Import & export | pickFile, previewImport, commitImport, exportDeck (14); shareFile (15); getEditorData (11); job events (7) | streaming CSV preview; commit all-or-nothing, cancel writes nothing; picker returns opaque tokens only (ADR-12) | ☐ |
| Backup & restore | listBackups, createBackup, validateRestore, restoreBackup, getSettings, updateSettings (12); pickFile (14); shareFile (15); job events (7) | safety copy before any change; corrupt and newer files rejected before anything changes; reopen clears every cached read | ☐ |

## Evidence per switched screen

- The screen's Vitest flow passes against the mock (unchanged) and its operations pass the Rust contract test.
- `frontend/tests/adapterSwap.test.ts` still passes (frozen payloads through the production adapter).
- The screen's rows in [device-smoke.md](../release/device-smoke.md) pass on a physical Android device and the iOS simulator with the dev or beta build (physical iPhone once one is available, owner decision 2026-10-29).
- The matrix row in [README.md](README.md#ui--backend-matrix-v10-phase-6-gate) moves from "complete prototype" to "integrated (Phase N)".
