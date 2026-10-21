# AppApi contract v0.5

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

## Changes in v0.5 (Phase 5)

- Jobs: `JobDto.result: JobResultDto | null` — the outcome of a finished job (`import` counts and row errors, `export` file name, `backup` id, `restore` safety-copy id, `reset`, `prepareContent` deck ids), read with `getJobStatus`; events never carry it. A failed restore keeps its `result`, so the UI can offer the safety copy. New `JobKind` `reset`.
- Statistics: `getStats({ deckId, range })` returns every study day of the range (`reviewsByDay`, zeros included), a 30-day `forecast` with `stored` (due dates as stored; day 0 holds overdue cards) and `projected` (with the deck's reviews/day limit, backlog carried forward) plus `forecastAssumptions`, `overdue`, `maturity` (young/mature at `matureThresholdDays` = 21), `stability` buckets, `difficultyStability` points, `longestStreakDays`, `generatedAt`. `trueRetention` is `null` below 10 reviews — never 0%. Unknown deck → `NOT_FOUND`.
- Settings: `SettingsDto` gains `defaultTargetLanguage`, `defaultPresetId`, `ttsRate` (0.5–1.5), `ai.providers` (`AiProviderOptionDto`: models, key requirement, custom HTTPS endpoint) and `ai.quota`, and `backup` (`automatic`, `retention` 1–50, `target` app/external, `externalFolder` display name). `SettingsPatch` gains `aiProvider`/`aiModel`/`aiEndpoint` (HTTPS only) and `backupAutomatic`/`backupRetention`/`backupTarget`/`backupFolderToken` (from `pickFile({ purpose: 'backupFolder' })`). A provider change drops the stored key.
- `setAiApiKey({ operationId, apiKey | null })` → `AiSettingsDto`: the key goes to the SecretStore and is never returned (only `hasApiKey`). Generation with a key-requiring provider and no key fails `PERMISSION_DENIED { reason: 'apiKey' }`; declined consent fails `{ reason: 'aiConsent' }` — learning is never blocked.
- `requestNotificationPermission({ operationId })` → `SettingsDto`; a refusal is the state `denied`, not an error, and the prompt does not come back.
- Onboarding: `OnboardingStateDto` gains `installedDeckGuids`, `nativeLanguage`, `targetLanguages`, `dailyGoal`; `BundledDeckDto.description`; `CompleteOnboardingInput.nativeLanguage`. `deckGuids` may be empty and must match the chosen languages; onboarding again (from the settings) only adds missing bundled decks.
- Privacy: `previewReset({ scope: 'progress' | 'everything' })` → counts, `confirmationPhrase`, `latestBackupAt`, `previewToken`; `resetCollection({ operationId, previewToken, safetyBackupId, confirmationPhrase })` is a job and needs either a backup created after the preview or the exact phrase. `everything` keeps backups.
- About: `getAboutInfo()` (app, contract, schema — `null` without a database — FSRS, content, platform) and `getDiagnosticsReport()` (log lines already redacted by the backend, with the number of redactions). `AppInfoDto` is unchanged (Rust side).
- Transfer: `ImportInput.mapping` targets are `front | back | tags | ignore`; `targetDeckId: null` imports into a new deck named after the file. `ImportPreviewDto` gains `delimiter`, `hasHeader`, `totalRows`, `targetDeckId`, `targetDeckName`; `rowErrors` are 1-based file lines. A preview is a dry run; `commitImport` is all-or-nothing (cancel writes nothing) and rejects a preview older than the last collection change (`PREVIEW_STALE`). APKG → `UNSUPPORTED_FORMAT { format: 'apkg' }`. `exportDeck({ operationId, deckId, format: 'csv', includeScheduling })` (job) and `shareFile({ operationId, target: backup | export })` → `{ shared }`.
- Backup: `BackupDto.kind` (`manual | automatic | safety`); retention never prunes safety copies. `validateRestore({ source: { kind: 'file', fileToken } | { kind: 'backup', backupId } })` → preview with `current` counts next to the backup's. `restoreBackup` stores a safety copy before anything changes; on success the UI clears every cached read and reopens on the dashboard.
- `FilePurpose` gains `backupFolder`; the demo picker's answer per purpose is chosen in the demo banner (`DemoControls.setPickedFile`, `null` = picker closed).
- A scrolling dialog body becomes focusable (`Dialog.svelte`), so long content at 200% text can be scrolled from the keyboard.

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

Phase 5 content: three backups (automatic, manual, external) with snapshots of the collection, so restoring a listed backup really replaces the demo collection; demo files for the picker — `slowka-niemiecki.csv` (semicolon, BOM, header; 15 rows, 2 duplicates of deck 1, 1 row without a front), `english-phrases.csv` (comma, no header), `Deutsch_A1.apkg` (unsupported), and the backups `…-02-27` (valid), `…uszkodzona` (corrupt), `…-v9` (newer schema), `…przerwana` (validates, then fails while restoring); one backup folder. `newUser` starts with AI consent unset; `quota` reports the AI limit as used up.

`happy`, `newUser`, `empty`, `noCards` (decks without cards), `caughtUp` (cards, nothing due today), `preparing` (one deck finishes preparing in the background and emits `collection_changed`), `loading`, `error`, `offline`, `quota`, `aiErrors` (provider error / invalid output / moderation, failing generation jobs), `ambiguous`, `large` (50 000 browser rows, generated on demand). Deterministic for a given seed and clock; `setScenario` resets all demo memory; `failNext(operation, code)` injects one failure for save-error flows. The mock never touches user files and is excluded from release bundles.

## UI → backend matrix (Phase 5 snapshot)

Every operation a screen calls lists that screen in `operations.ts`; `frontend/tests/management.test.ts` (Phase 4) and `frontend/tests/phase5.test.ts` (Phase 5) record the calls of each flow and fail on an operation the catalogue does not assign to the screen (no action without a future backend contract).

| Screen / route | Operations | Mock status | Rust phase |
|---|---|---|---|
| Dashboard `/home` | getDashboard, startStudy, createDeck | complete prototype (resume, new deck, refresh, no-cards / caught-up / preparing states) | 10–11 |
| Deck `/deck/:id` | getDeck, getEditorData, startStudy, updateDeck, deleteDeck, listPresets, savePreset | complete prototype (counters, study/browse/add, subdeck tree with collapse, rename/move without cycles, type change that hides and restores, delete with scope, preset picker with dependents, preset editor with validation, shared-preset warning and clone) | 10–11 |
| Study `/study/:deckId?session=` | resumeStudy, updateStudySession, getRatingPreview, rateCard, undoReview, applyCardAction, getNoteContent, resolveWordLink, generateAssociation, setFavoriteAssociation, job events | complete prototype (pause/end, leave guard, recovery, card actions, word sheet, association states, TTS states) | 10–11/13/15 |
| Summary `/summary/:sessionId` | getSessionSummary, startStudy, getSettings, updateSettings | complete prototype (distribution, time, Again list, next steps, reminder offer) | 10/12/15 |
| Card `/card/:id` | getNoteDetails, applyCardAction, deleteNote, updateNoteContent, setFavoriteAssociation, resolveWordLink, searchWords, generateAssociation, getNoteContent, job events | complete prototype (note, every card with its own S/D/R/due/reps/lapses and history, R with calculation time, suspend/flag/delete per card, tips add/edit/delete/reorder, associations favorite/add/edit own/delete/generate, word links change/choose/unlink, delete note) | 10–11/13 |
| Editor `/editor?cardId=&deckId=` | getEditorData, getNoteDetails, checkDuplicate, saveNote, searchWords, generateAssociation | complete prototype (deck, note types with live card previews incl. kept/new/restored/archived, tags, capability-driven association/tip/link fields, phrase selection with word picker, duplicate warning, validation, save / save and add next, version conflict, draft guard, demo-draft notice) | 11/13 |
| Browser `/browser?q=&deck=` | searchCards, getEditorData, previewBulkAction, applyBulkAction | complete prototype (contract paging into VirtualList, search help and parser errors, deck/tag/state/flag filters, sort keys and direction, selected rows vs all matches minus exclusions, bulk change deck / add or remove tag / flag / suspend / unsuspend / reposition / delete with previewed effect and confirmation, arrow-key row navigation) | 11 |
| Statistics `/stats` | getStats, getEditorData | complete prototype (deck and period filters that keep the previous charts while reloading; streak, reviews, time and young/mature true retention with "not enough data" below 10 reviews and an explanation; activity heatmap, reviews and time per day/week/month, 30-day forecast with stored due dates vs expected with limits and its assumptions, card states with young/mature, stability histogram, advanced difficulty vs stability; every chart has a table view, keyboard reading and hover/focus tooltips; empty collection with a way out) | 12 |
| Settings `/settings` | getSettings, updateSettings, listPresets, requestNotificationPermission | complete prototype (UI language, native language, default target language, theme, text size, daily goal, time zone, day cutoff, default preset, notification permission and reminder, TTS on/off, rate and voice check; each change saved on its own with the current version, a conflict reloads) | 12/15 |
| AI `/settings/ai` | getSettings, updateSettings, setAiApiKey | complete prototype (consent that can be declined, provider/model/HTTPS endpoint, BYOK key that is never shown again, quota) | 12–13 |
| Privacy `/settings/privacy` | getSettings, updateSettings, previewReset, resetCollection, createBackup, listBackups, job events | complete prototype (telemetry opt-in, off by default; reset progress / delete everything naming scope and reversibility, protected by a fresh safety backup or the typed phrase) | 12 |
| About `/settings/about` | getAppInfo, getAboutInfo, getDiagnosticsReport | complete prototype (app/contract/schema/FSRS/content/platform, licences link, redacted diagnostic log with copy) | 7/12 |
| Transfer `/settings/transfer` | pickFile, getEditorData, previewImport, commitImport, exportDeck, shareFile, job events | complete prototype (file pick, dry run with delimiter, BOM, header, column mapping, target or new deck, skip/update/duplicate, counts and row errors, progress with a cancel that writes nothing, result report, APKG instructions; deck export with share, separate from backup) | 14–15 |
| Backup `/settings/backup` | listBackups, createBackup, getSettings, updateSettings, pickFile, validateRestore, restoreBackup, shareFile, job events | complete prototype (create with progress, automatic/retention/target folder, list with kind and location, share, restore from a listed backup or a file: validation, preview of now vs after, safety copy, confirmation, reopen; corrupt and newer files rejected before anything changes; a failed restore offers the safety copy) | 12/15 |
| Attributions `/settings/attributions` | getAttributions | grouped by content/software/fonts/icons | 8/12 |
| Onboarding `/onboarding?from=` | getOnboardingState, completeOnboarding, job events | complete prototype (native language → target languages → daily goal → bundled decks → ready, focus moves to each step, prepare job; from the settings it shows installed decks, adds only missing ones and returns there) | 8/10/12 |
| Shell | getAppInfo, onCollectionChanged | demo banner, nav | 7 |
