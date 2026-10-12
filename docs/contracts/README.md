# AppApi contract v0.2

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

## Demo scenarios (`MockAppApi`)

`happy`, `newUser`, `empty`, `loading`, `error`, `offline`, `quota`, `ambiguous`, `large` (50 000 browser rows, generated on demand). Deterministic for a given seed and clock; `setScenario` resets all demo memory; `failNext(operation, code)` injects one failure for save-error flows. The mock never touches user files and is excluded from release bundles.

## UI → backend matrix (Phase 2 snapshot)

| Screen / route | Operations | Mock status | Rust phase |
|---|---|---|---|
| Dashboard `/home` | getDashboard, startStudy, resumeStudy | placeholder + data | 10–11 |
| Deck `/deck/:id` | getDeck, startStudy, create/update/deleteDeck | placeholder + data | 10–11 |
| Study `/study/:deckId?session=` | resumeStudy, getRatingPreview, rateCard, undoReview, applyCardAction, resolveWordLink, generateAssociation | reference controller | 10–11/13 |
| Summary `/summary/:sessionId` | getSessionSummary | placeholder + data | 10 |
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
