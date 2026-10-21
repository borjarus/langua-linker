# AppApi contract v0.4

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

## Changes in v0.4 (Phase 4)

- Decks: `DeckOverviewDto.children` → `subdecks` (every descendant, parents first) plus `typeOptions: DeckTypeOptionDto[]` (each deck type with what it would hide); `hiddenSectionCounts` counts only the deck's own notes. `UpdateDeckInput.patch.description`. A deck-type change hides sections and never deletes content.
- Presets: `listPresets()` → `PresetDto[]` (config, `dependentDecks`, `builtIn`, `version`) and `savePreset({ operationId, presetId | null, expectedVersion, name, config })` — `presetId: null` creates (clones). Validated ranges: new/day 0–9999, reviews/day 0–99 999, desired retention 0.70–0.99, max interval 1–36 500 days, up to 10 steps of 0–43 200 min, exactly `FSRS_WEIGHT_COUNT` (21) weights. A change applies to every dependent deck; the UI warns before saving a shared preset.
- Notes: `NoteDetailsDto` gains `deckId`, `targetLanguage`, `capabilities`, `archivedOrdinals`; `CardDetailsDto.deckName`. `EditorDataDto.noteTypes` is `NoteTypeDto[]` (kind + card templates with direction) so the editor previews cards without a second domain map; `EditorDeckOptionDto.parentId`.
- `SaveNoteInput` gains `grammarTips: string[]` (appended) and `wordLinks: WordLinkDraftDto[] | null` (complete replacement for a sentence; `null` keeps stored links and drops those whose words an edit removed). A note-type change archives cards (never destroys them); switching back restores the same card with its history.
- `deleteNote({ operationId, noteId, expectedVersion })` → `{ deletedCards }`; word links pointing at the deleted word become `unresolved`.
- `updateNoteContent({ operationId, noteId, change })` with `NoteContentChange` = add/edit/delete/reorder tip, add/edit/delete association (only `manual` ones are editable), unlink word. Each change is its own transaction and does not bump the note version, so an open editor draft stays valid.
- `searchWords({ targetLanguage, text, limit })` → `WordMatchDto[]` with `match: 'exact' | 'normalized'`; no match returns `[]` — the backend never picks a word.
- Browser: `SearchInput.tag`; `CardRowDto.position` (new-queue order) and `direction`; bulk actions `removeTag` and `reposition { start, step }` (new cards only); `previewBulkAction({ selection, action })` → `BulkPreviewDto { selected, affected, unchanged, notes, decks }` shown before the user confirms.
- The editor route takes `cardId` (`/editor?cardId=&deckId=`), because `getNoteDetails` reads a note through one of its cards.

## Changes in v0.3 (Phase 3)

- `updateStudySession({ operationId, sessionId, action: 'pause' | 'resume' | 'end' })` → `StudySessionDto`. A paused session rejects `rateCard` with `VALIDATION_FAILED`; ending early keeps every committed rating; one unfinished session at a time (`startStudy` closes the previous one).
- `getNoteContent(noteId)` → `NoteContentDto` (front/back, capabilities, associations, grammar tips) for the study word sheet.
- `StartStudyInput.cardIds?` restricts a session to given cards (summary: "study the Again cards again").
- `DashboardDto.resumableSessionId` → `resumable: ResumableSessionDto | null` (deck, status, progress) and `nextDueAt`.
- `StudySessionDto.deckName`; `SessionSummaryDto` gains `deckName`, `status`, `againCards` (replaces `againCardIds`), `remainingDue`, `nextDeck`, `firstCompleted`.
- Failed associations carry `error` for both `error` and `blocked` status, so the UI distinguishes provider error, invalid output and moderation.

Rating commits wait for the rating preview of the shown card; a commit never sends an empty or foreign `previewToken` (regression found by the Phase 3 E2E run).

## Demo scenarios (`MockAppApi`)

Phase 4 content (same seeds): deck 4 holds four reversed notes (eight cards), deck 1 has subdeck 5, decks 1/5/2/3 share the default preset (four dependent decks) and deck 4 uses the light one; `searchWords` matches `Bank` exactly to two homographs and `kuhlschrank` only after normalization.

`happy`, `newUser`, `empty`, `noCards` (decks without cards), `caughtUp` (cards, nothing due today), `preparing` (one deck finishes preparing in the background and emits `collection_changed`), `loading`, `error`, `offline`, `quota`, `aiErrors` (provider error / invalid output / moderation, failing generation jobs), `ambiguous`, `large` (50 000 browser rows, generated on demand). Deterministic for a given seed and clock; `setScenario` resets all demo memory; `failNext(operation, code)` injects one failure for save-error flows. The mock never touches user files and is excluded from release bundles.

## UI → backend matrix (Phase 4 snapshot)

Every operation a screen calls lists that screen in `operations.ts`; `frontend/tests/management.test.ts` records the calls of each Phase 4 flow and fails on an operation the catalogue does not assign to the screen (no action without a future backend contract).

| Screen / route | Operations | Mock status | Rust phase |
|---|---|---|---|
| Dashboard `/home` | getDashboard, startStudy, createDeck | complete prototype (resume, new deck, refresh, no-cards / caught-up / preparing states) | 10–11 |
| Deck `/deck/:id` | getDeck, getEditorData, startStudy, updateDeck, deleteDeck, listPresets, savePreset | complete prototype (counters, study/browse/add, subdeck tree with collapse, rename/move without cycles, type change that hides and restores, delete with scope, preset picker with dependents, preset editor with validation, shared-preset warning and clone) | 10–11 |
| Study `/study/:deckId?session=` | resumeStudy, updateStudySession, getRatingPreview, rateCard, undoReview, applyCardAction, getNoteContent, resolveWordLink, generateAssociation, setFavoriteAssociation, job events | complete prototype (pause/end, leave guard, recovery, card actions, word sheet, association states, TTS states) | 10–11/13/15 |
| Summary `/summary/:sessionId` | getSessionSummary, startStudy, getSettings, updateSettings | complete prototype (distribution, time, Again list, next steps, reminder offer) | 10/12/15 |
| Card `/card/:id` | getNoteDetails, applyCardAction, deleteNote, updateNoteContent, setFavoriteAssociation, resolveWordLink, searchWords, generateAssociation, getNoteContent, job events | complete prototype (note, every card with its own S/D/R/due/reps/lapses and history, R with calculation time, suspend/flag/delete per card, tips add/edit/delete/reorder, associations favorite/add/edit own/delete/generate, word links change/choose/unlink, delete note) | 10–11/13 |
| Editor `/editor?cardId=&deckId=` | getEditorData, getNoteDetails, checkDuplicate, saveNote, searchWords, generateAssociation | complete prototype (deck, note types with live card previews incl. kept/new/restored/archived, tags, capability-driven association/tip/link fields, phrase selection with word picker, duplicate warning, validation, save / save and add next, version conflict, draft guard, demo-draft notice) | 11/13 |
| Browser `/browser?q=&deck=` | searchCards, getEditorData, previewBulkAction, applyBulkAction | complete prototype (contract paging into VirtualList, search help and parser errors, deck/tag/state/flag filters, sort keys and direction, selected rows vs all matches minus exclusions, bulk change deck / add or remove tag / flag / suspend / unsuspend / reposition / delete with previewed effect and confirmation, arrow-key row navigation) | 11 |
| Statistics `/stats` | getStats | placeholder + data | 12 |
| Settings `/settings` | getSettings, updateSettings, getAppInfo | language/theme/text scale | 12–13/15 |
| Transfer `/settings/transfer` | pickFile, previewImport, commitImport | dry-run preview | 14–15 |
| Backup `/settings/backup` | listBackups, createBackup, validateRestore, restoreBackup, job events | list + job progress | 12 |
| Attributions `/settings/attributions` | getAttributions | list | 8/12 |
| Onboarding `/onboarding` | getOnboardingState, completeOnboarding, job events | prepare job | 8/10/12 |
| Shell | getAppInfo, onCollectionChanged | demo banner, nav | 7 |
