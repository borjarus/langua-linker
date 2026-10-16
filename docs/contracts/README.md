# AppApi contract v0.3

Source files (Phase 2: TypeScript is the source; Rust becomes the DTO source in Phase 7):

- `frontend/src/api/types.ts` — DTOs, enums, `ERROR_CODES`, `API_VERSION`.
- `frontend/src/api/appApi.ts` — the `AppApi` interface (operations and their input/result types).
- `frontend/src/api/operations.ts` — operation catalogue: kind, Tauri command, errors, emitted events, permission, screens, backend phase. Completeness is a compile-time check (`satisfies Record<AppApiOperation, OperationSpec>`).
- `crates/contracts/src/lib.rs` — Rust `AppInfoDto`, `ErrorDto`, `ErrorCode`; tests assert `API_VERSION` and error codes match the TypeScript side.

## Transport rules (enforced by `frontend/tests/contracts.test.ts`)

- Ids (`id`, `*Id`, `*Ids`) are strings; GUIDs are strings.
- Timestamps (`*At`) are UTC ISO-8601 with milliseconds; study days are `YYYY-MM-DD`.
- Payloads are JSON-safe: no `undefined`, `Date`, `bigint`, `NaN`.
- Every mutation and job takes an `operationId`; the backend returns the previous result for a repeated `operationId`.
- Versioned entities send `expectedVersion`; stale writes fail with `VERSION_CONFLICT`. Rating previews carry a `previewToken`; stale tokens fail with `PREVIEW_STALE`.
- Errors are `ErrorDto { code, messageKey, retryable, details }`; `messageKey` is `errors.<CODE>` and translated in the UI.
- Long-running work returns `JobDto`; `job_progress` events carry a monotonic `sequence`; `getJobStatus` is the durable truth.
- Tauri commands are snake_case and receive a single `input` argument.

## Changes in v0.3 (Phase 3)

- `updateStudySession({ operationId, sessionId, action: 'pause' | 'resume' | 'end' })` → `StudySessionDto`. A paused session rejects `rateCard` with `VALIDATION_FAILED`; ending early keeps every committed rating; one unfinished session at a time (`startStudy` closes the previous one).
- `getNoteContent(noteId)` → `NoteContentDto` (front/back, capabilities, associations, grammar tips) for the study word sheet.
- `StartStudyInput.cardIds?` restricts a session to given cards (summary: "study the Again cards again").
- `DashboardDto.resumableSessionId` → `resumable: ResumableSessionDto | null` (deck, status, progress) and `nextDueAt`.
- `StudySessionDto.deckName`; `SessionSummaryDto` gains `deckName`, `status`, `againCards` (replaces `againCardIds`), `remainingDue`, `nextDeck`, `firstCompleted`.
- Failed associations carry `error` for both `error` and `blocked` status, so the UI distinguishes provider error, invalid output and moderation.

Rating commits wait for the rating preview of the shown card; a commit never sends an empty or foreign `previewToken` (regression found by the Phase 3 E2E run).

## Demo scenarios (`MockAppApi`)

`happy`, `newUser`, `empty`, `noCards` (decks without cards), `caughtUp` (cards, nothing due today), `preparing` (one deck finishes preparing in the background and emits `collection_changed`), `loading`, `error`, `offline`, `quota`, `aiErrors` (provider error / invalid output / moderation, failing generation jobs), `ambiguous`, `large` (50 000 browser rows, generated on demand). Deterministic for a given seed and clock; `setScenario` resets all demo memory; `failNext(operation, code)` injects one failure for save-error flows. The mock never touches user files and is excluded from release bundles.

## UI → backend matrix (Phase 3 snapshot)

| Screen / route | Operations | Mock status | Rust phase |
|---|---|---|---|
| Dashboard `/home` | getDashboard, startStudy, createDeck | complete prototype (resume, new deck, refresh, no-cards / caught-up / preparing states) | 10–11 |
| Deck `/deck/:id` | getDeck, startStudy, create/update/deleteDeck | placeholder + data | 10–11 |
| Study `/study/:deckId?session=` | resumeStudy, updateStudySession, getRatingPreview, rateCard, undoReview, applyCardAction, getNoteContent, resolveWordLink, generateAssociation, setFavoriteAssociation, job events | complete prototype (pause/end, leave guard, recovery, card actions, word sheet, association states, TTS states) | 10–11/13/15 |
| Summary `/summary/:sessionId` | getSessionSummary, startStudy, getSettings, updateSettings | complete prototype (distribution, time, Again list, next steps, reminder offer) | 10/12/15 |
| Card `/card/:id` | getNoteDetails, setFavoriteAssociation | placeholder + data | 11/13 |
| Editor `/editor?deckId=&noteId=` | getEditorData, checkDuplicate, saveNote | placeholder + draft guard | 11/13 |
| Browser `/browser?q=&deck=` | searchCards, applyBulkAction | paging + VirtualList | 11 |
| Statistics `/stats` | getStats | placeholder + data | 12 |
| Settings `/settings` | getSettings, updateSettings, getAppInfo | language/theme/text scale | 12–13/15 |
| Transfer `/settings/transfer` | pickFile, previewImport, commitImport | dry-run preview | 14–15 |
| Backup `/settings/backup` | listBackups, createBackup, validateRestore, restoreBackup, job events | list + job progress | 12 |
| Attributions `/settings/attributions` | getAttributions | list | 8/12 |
| Onboarding `/onboarding` | getOnboardingState, completeOnboarding, job events | prepare job | 8/10/12 |
| Shell | getAppInfo, onCollectionChanged | demo banner, nav | 7 |
