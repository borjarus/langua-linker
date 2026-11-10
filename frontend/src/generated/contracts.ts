// Generated from crates/contracts (Rust is the DTO source, plan Phase 7 step 28).
// Do not edit: change the Rust type and run
//   UPDATE_CONTRACT=1 cargo test -p langualinker-contracts --test generated_typescript

export type ErrorCode = "VALIDATION_FAILED" | "VERSION_CONFLICT" | "NOT_FOUND" | "DELETED" | "DB_BUSY" | "STORAGE_FULL" | "PERMISSION_DENIED" | "NETWORK_UNAVAILABLE" | "PROVIDER_ERROR" | "PROVIDER_TIMEOUT" | "INVALID_PROVIDER_OUTPUT" | "MODERATION_BLOCKED" | "QUOTA_EXCEEDED" | "UNSUPPORTED_FORMAT" | "BACKUP_CORRUPT" | "SCHEMA_TOO_NEW" | "PREVIEW_STALE" | "CANCELLED" | "NOT_IMPLEMENTED" | "INTERNAL";

export type ErrorDto = { code: ErrorCode, 
/**
 * i18n key; the UI translates this, never raw SQL or stack traces.
 */
messageKey: string, retryable: boolean, details: Record<string, string | number | boolean | null> | null, };

export type AppInfoDto = { apiVersion: string, appName: string, appVersion: string, 
/**
 * Native build variant (`dev` | `beta` | `release`); the in-memory mock reports `demo`.
 */
mode: 'demo' | 'dev' | 'beta' | 'release', };

export type JobKind = "prepareContent" | "import" | "export" | "backup" | "restore" | "reset" | "llm";

export type JobStatus = "queued" | "running" | "succeeded" | "failed" | "cancelled";

export type JobProgressDto = { done: number, total: number, };

export type JobDto = { jobId: string, kind: JobKind, status: JobStatus, 
/**
 * Monotonic per job; consumers drop events with a lower sequence.
 */
sequence: number, progress: JobProgressDto | null, startedAt: string, finishedAt: string | null, error: ErrorDto | null, 
/**
 * Set once the job finished (also on some failures, e.g. the safety copy of a failed restore).
 */
result: JobResultDto | null, };

export type JobEventDto = { jobId: string, kind: JobKind, sequence: number, status: JobStatus, progress: JobProgressDto | null, };

export type ImportRowErrorDto = { 
/**
 * 1-based line in the file.
 */
row: number, error: ErrorDto, };

export type ImportCountsDto = { create: number, update: number, skip: number, duplicate: number, error: number, };

export type JobResultDto = { "type": "import", deckId: string, rowErrors: Array<ImportRowErrorDto>, create: number, update: number, skip: number, duplicate: number, error: number, } | { "type": "export", fileName: string, cardCount: number, } | { "type": "backup", backupId: string, } | { "type": "restore", safetyBackupId: string, } | { "type": "reset", safetyBackupId: string | null, } | { "type": "prepareContent", deckIds: Array<string>, created: number, updated: number, unchanged: number, 
/**
 * Removed from the bundled content but kept: the learner has progress or edits.
 */
kept: number, 
/**
 * Removed from the bundled content without progress: cards archived, history intact.
 */
archived: number, warnings: Array<ContentWarningDto>, };

export type CollectionScope = "decks" | "cards" | "notes" | "settings" | "stats" | "backups";

export type CollectionChangedEventDto = { scopes: Array<CollectionScope>, 
/**
 * Read signal only: refetch through AppApi.
 */
changedAt: string, };

export type AboutDto = { appVersion: string, apiVersion: string, 
/**
 * `null` when the database could not be opened.
 */
schemaVersion: number | null, fsrsVersion: string, contentVersion: string, platform: string, };

export type LogLevel = "error" | "warn" | "info" | "debug";

export type LogLineDto = { at: string, level: LogLevel, target: string, 
/**
 * Already redacted by the backend: no card content, prompts or secrets.
 */
message: string, };

export type DiagnosticsReportDto = { generatedAt: string, lines: Array<LogLineDto>, 
/**
 * Number of values the backend replaced with `[redacted]`.
 */
redactions: number, };

export type ContentWarningCode = "overrideConflict" | "deckDeleted";

export type ContentWarningDto = { code: ContentWarningCode, deckGuid: string, noteGuid: string | null, 
/**
 * The field kept for the learner (`front`, `back`, `tags`, `association:<guid>`, ...).
 */
field: string | null, };

export type NativeLanguage = "pl";

export type TargetLanguage = "de" | "en" | "es";

export type DeckType = "Simple" | "TextWithAssociations" | "Linguistic";

export type DeckCapabilitiesDto = { associations: boolean, grammarTips: boolean, wordLinks: boolean, tts: boolean, };

export type DeckCountsDto = { new: number, learning: number, review: number, overdue: number, total: number, };

export type ContentState = "ready" | "preparing";

export type DeckSummaryDto = { id: string, guid: string, name: string, parentId: string | null, targetLanguage: TargetLanguage, nativeLanguage: NativeLanguage, deckType: DeckType, capabilities: DeckCapabilitiesDto, counts: DeckCountsDto, 
/**
 * Share of mature cards, 0..1.
 */
progress: number, bundled: boolean, contentState: ContentState, version: number, };

export type ResumableStatus = "active" | "paused";

export type ResumableSessionDto = { sessionId: string, 
/**
 * `null` = a session over all decks.
 */
deckId: string | null, deckName: string | null, status: ResumableStatus, reviewedCount: number, remaining: number, };

export type DashboardDto = { decks: Array<DeckSummaryDto>, streakDays: number, studiedToday: number, dueToday: number, dailyGoal: number, resumable: ResumableSessionDto | null, 
/**
 * Earliest future due date over all decks; lets "nothing due today" say when the next review is.
 */
nextDueAt: string | null, generatedAt: string, };

export type BundledDeckDto = { guid: string, name: string, description: string, targetLanguage: TargetLanguage, deckType: DeckType, noteCount: number, cardCount: number, level: string, };

export type OnboardingStep = "nativeLanguage" | "targetLanguage" | "goal" | "decks" | "ready";

export type OnboardingStateDto = { completed: boolean, step: OnboardingStep, availableDecks: Array<BundledDeckDto>, 
/**
 * Bundled decks already in the collection; onboarding again only adds the missing ones.
 */
installedDeckGuids: Array<string>, nativeLanguage: NativeLanguage, targetLanguages: Array<TargetLanguage>, dailyGoal: number, };

export type CompleteOnboardingInput = { operationId: string, nativeLanguage: NativeLanguage, targetLanguages: Array<TargetLanguage>, dailyGoal: number, 
/**
 * May be empty: the user can start without bundled decks.
 */
deckGuids: Array<string>, };

export type AttributionScope = "dependency" | "content" | "font" | "icon";

export type AttributionDto = { name: string, license: string, url: string | null, scope: AttributionScope, };

export type FilePurpose = "import" | "restore" | "backupFolder";

export type PickFileInput = { purpose: FilePurpose, };

export type FileHandleDto = { token: string, name: string, sizeBytes: number, };

export type NoteTypeKind = "Basic" | "BasicAndReversed" | "Sentence" | "SentenceAndReversed";

export type DeckContentKind = "words" | "sentences";

export type DeckImportOriginDto = { 
/**
 * `apkg` for Anki.
 */
kind: string, fileName: string, 
/**
 * Date of the conversion, `YYYY-MM-DD`.
 */
convertedOn: string, tool: string, };

export type DeckImportWarningDto = { 
/**
 * Content-rule code, e.g. `front.homograph`.
 */
code: string, 
/**
 * Line in the package file.
 */
line: number | null, };

export type PreviewDeckImportInput = { fileToken: string, };

export type DeckImportPreviewDto = { fileName: string, deckGuid: string, name: string, description: string, targetLanguage: TargetLanguage, deckType: DeckType, noteType: NoteTypeKind, contentKind: DeckContentKind, level: string, origin: DeckImportOriginDto | null, notes: number, cards: number, 
/**
 * Cards that arrive with progress (not new).
 */
scheduledCards: number, suspendedCards: number, 
/**
 * Past reviews imported as history.
 */
reviews: number, newNotes: number, 
/**
 * Notes already in the collection: kept with their progress, content updated unless edited.
 */
existingNotes: number, 
/**
 * The deck this package updates, when it was imported before.
 */
existingDeckId: string | null, warnings: Array<DeckImportWarningDto>, previewToken: string, };

export type CommitDeckImportInput = { operationId: string, previewToken: string, };

export type Rating = "Again" | "Hard" | "Good" | "Easy";

export type CardState = "New" | "Learning" | "Review" | "Relearning";

export type QueueState = "New" | "Learning" | "Review" | "DayLearn" | "Suspended" | "SchedBuried" | "UserBuried";

export type CardFlag = "none" | "red" | "orange" | "green" | "blue" | "pink" | "turquoise" | "purple";

export type CardDirection = "forward" | "reverse";

export type AssociationProvenance = "bundled" | "generated" | "manual";

export type AssociationStatus = "ready" | "pending" | "offlineQueued" | "error" | "blocked";

export type AssociationDto = { id: string, noteId: string, text: string, provenance: AssociationProvenance, favorite: boolean, status: AssociationStatus, model: string | null, error: ErrorDto | null, };

export type GrammarTipDto = { id: string, noteId: string, text: string, order: number, };

export type WordLinkResolution = "resolved" | "unresolved" | "ambiguous" | "userOverridden";

export type WordCandidateDto = { noteId: string, lemma: string, translation: string, };

export type WordLinkDto = { id: string, sentenceNoteId: string, surface: string, 
/**
 * UTF-16 offsets in NFC text, half-open [startIndex, endIndex) — plan §14.1.
 */
startIndex: number, endIndex: number, lemma: string | null, resolution: WordLinkResolution, wordNoteId: string | null, translation: string | null, candidates: Array<WordCandidateDto>, };

export type NoteContentDto = { noteId: string, deckId: string, front: string, back: string, targetLanguage: TargetLanguage, capabilities: DeckCapabilitiesDto, associations: Array<AssociationDto>, grammarTips: Array<GrammarTipDto>, };

export type StudyCardDto = { cardId: string, noteId: string, deckId: string, version: number, deckType: DeckType, capabilities: DeckCapabilitiesDto, direction: CardDirection, state: CardState, flag: CardFlag, front: string, back: string, targetLanguage: TargetLanguage, associations: Array<AssociationDto>, grammarTips: Array<GrammarTipDto>, wordLinks: Array<WordLinkDto>, };

export type StudySessionStatus = "active" | "paused" | "finished";

export type StudySessionDto = { sessionId: string, deckId: string | null, deckName: string | null, status: StudySessionStatus, current: StudyCardDto | null, 
/**
 * Cards still to study today in the session's scope; `total` is their sum.
 */
counts: DeckCountsDto, reviewedCount: number, undoAvailable: boolean, startedAt: string, };

export type StartStudyInput = { operationId: string, 
/**
 * `null` studies all decks.
 */
deckId: string | null, 
/**
 * Restricts the session to these cards (e.g. "study the Again cards again"); omitted = the
 * normal queue.
 */
cardIds?: Array<string>, };

export type StudySessionAction = "pause" | "resume" | "end";

export type UpdateStudySessionInput = { operationId: string, sessionId: string, action: StudySessionAction, };

export type PreviewInput = { sessionId: string, cardId: string, cardVersion: number, };

export type IntervalPreviewDto = { dueAt: string, intervalSeconds: number, };

export type RatingIntervalsDto = { Again: IntervalPreviewDto, Hard: IntervalPreviewDto, Good: IntervalPreviewDto, Easy: IntervalPreviewDto, };

export type RatingPreviewDto = { cardId: string, cardVersion: number, 
/**
 * Card + preset version token; stale tokens are rejected with PREVIEW_STALE.
 */
previewToken: string, intervals: RatingIntervalsDto, };

export type RateCardInput = { operationId: string, sessionId: string, cardId: string, cardVersion: number, previewToken: string, rating: Rating, 
/**
 * Monotonic active answer time, capped by the backend.
 */
answerMs: number, };

export type ReviewedCardDto = { cardId: string, version: number, state: CardState, dueAt: string | null, };

export type ReviewResultDto = { operationId: string, sessionId: string, reviewed: ReviewedCardDto, next: StudyCardDto | null, counts: DeckCountsDto, reviewedCount: number, undoAvailable: boolean, sessionStatus: StudySessionStatus, };

export type UndoInput = { operationId: string, sessionId: string, };

export type CardActionKind = "suspend" | "unsuspend" | "bury" | "unbury" | "flag" | "delete";

export type CardActionInput = { operationId: string, cardId: string, expectedVersion: number, action: CardActionKind, flag?: CardFlag, };

export type CardActionResultDto = { cardId: string, version: number, queue: QueueState, flag: CardFlag, deleted: boolean, };

export type SessionCardDto = { cardId: string, front: string, back: string, targetLanguage: TargetLanguage, direction: CardDirection, };

export type NextDeckDto = { id: string, name: string, due: number, };

export type RatingCountsDto = { Again: number, Hard: number, Good: number, Easy: number, };

export type SessionSummaryDto = { sessionId: string, deckId: string | null, deckName: string | null, status: StudySessionStatus, ratings: RatingCountsDto, reviewedCount: number, activeMs: number, 
/**
 * Cards rated Again at least once; the summary offers to study them again.
 */
againCards: Array<SessionCardDto>, nextDueAt: string | null, 
/**
 * Cards of the session's deck scope still due today.
 */
remainingDue: number, 
/**
 * Another deck with due cards, if any.
 */
nextDeck: NextDeckDto | null, 
/**
 * First finished session of this profile: the summary offers a daily reminder.
 */
firstCompleted: boolean, startedAt: string, finishedAt: string | null, };

export type SetFavoriteAssociationInput = { operationId: string, noteId: string, associationId: string, };

export type GenerateAssociationInput = { operationId: string, noteId: string, 
/**
 * 1–3 variants; each becomes its own placeholder and association.
 */
variants: number, };

export type SetAiApiKeyInput = { operationId: string, 
/**
 * Written to the platform SecretStore; `null` removes the key. Never read back.
 */
apiKey: string | null, };

export type ResolveWordLinkInput = { operationId: string, linkId: string, wordNoteId: string, };

export type HiddenSectionCountsDto = { associations: number, grammarTips: number, wordLinks: number, };

export type DeckTypeOptionDto = { deckType: DeckType, capabilities: DeckCapabilitiesDto, hiddenCounts: HiddenSectionCountsDto, };

export type DeckOverviewDto = { deck: DeckSummaryDto, 
/**
 * Every deck below this one (any depth), each parent listed before its children.
 */
subdecks: Array<DeckSummaryDto>, description: string, presetId: string, presetName: string, presetDependentDecks: number, 
/**
 * Content kept but hidden by the current deck type.
 */
hiddenSectionCounts: HiddenSectionCountsDto, typeOptions: Array<DeckTypeOptionDto>, };

export type CreateDeckInput = { operationId: string, name: string, parentId: string | null, targetLanguage: TargetLanguage, deckType: DeckType, };

export type DeckPatchDto = { name?: string, description?: string, 
/**
 * `null` moves the deck to the top level.
 */
parentId?: string | null, deckType?: DeckType, presetId?: string, };

export type UpdateDeckInput = { operationId: string, deckId: string, expectedVersion: number, patch: DeckPatchDto, };

export type DeleteDeckInput = { operationId: string, deckId: string, expectedVersion: number, };

export type DeleteDeckResultDto = { deletedDecks: number, deletedCards: number, };

export type PresetConfigDto = { newPerDay: number, reviewsPerDay: number, 
/**
 * 0.70–0.99.
 */
desiredRetention: number, maxIntervalDays: number, learningStepsMinutes: Array<number>, relearningStepsMinutes: Array<number>, 
/**
 * FSRS-6 weights w0–w20.
 */
weights: Array<number>, };

export type PresetDto = { id: string, name: string, version: number, 
/**
 * Built-in presets can be cloned and edited like any other.
 */
builtIn: boolean, 
/**
 * Decks using this preset; a change applies to all of them.
 */
dependentDecks: number, config: PresetConfigDto, };

export type SavePresetInput = { operationId: string, 
/**
 * `null` creates a preset (also used to clone one).
 */
presetId: string | null, expectedVersion: number | null, name: string, config: PresetConfigDto, };

export type NoteDto = { id: string, guid: string, noteTypeKind: NoteTypeKind, front: string, back: string, tags: Array<string>, version: number, createdAt: string, updatedAt: string, };

export type ReviewLogDto = { id: string, reviewedAt: string, rating: Rating, stateBefore: CardState, intervalSeconds: number, };

export type RetrievabilityDto = { value: number, computedAt: string, };

export type CardDetailsDto = { id: string, noteId: string, 
/**
 * The scheduling deck belongs to the card: cards of one note may sit in different decks.
 */
deckId: string, deckName: string, ordinal: number, direction: CardDirection, state: CardState, queue: QueueState, flag: CardFlag, dueAt: string | null, stability: number | null, difficulty: number | null, retrievability: RetrievabilityDto | null, reps: number, lapses: number, version: number, history: Array<ReviewLogDto>, };

export type NoteDetailsDto = { note: NoteDto, 
/**
 * Deck of the note's first card; decides which sections the details and the editor show.
 */
deckId: string, targetLanguage: TargetLanguage, capabilities: DeckCapabilitiesDto, 
/**
 * Live cards; each has its own schedule.
 */
cards: Array<CardDetailsDto>, 
/**
 * Template ordinals archived by a note-type change; switching back restores them with
 * history.
 */
archivedOrdinals: Array<number>, associations: Array<AssociationDto>, grammarTips: Array<GrammarTipDto>, wordLinks: Array<WordLinkDto>, };

export type DeleteNoteInput = { operationId: string, noteId: string, expectedVersion: number, };

export type DeleteNoteResultDto = { deletedCards: number, };

export type NoteContentChange = { "type": "addTip", text: string, } | { "type": "editTip", tipId: string, text: string, } | { "type": "deleteTip", tipId: string, } | { "type": "reorderTips", tipIds: Array<string>, } | { "type": "addAssociation", text: string, } | { "type": "editAssociation", associationId: string, text: string, } | { "type": "deleteAssociation", associationId: string, } | { "type": "unlinkWord", linkId: string, };

export type UpdateNoteContentInput = { operationId: string, noteId: string, change: NoteContentChange, };

export type EditorDeckOptionDto = { id: string, name: string, parentId: string | null, deckType: DeckType, capabilities: DeckCapabilitiesDto, targetLanguage: TargetLanguage, };

export type NoteTemplateDto = { ordinal: number, direction: CardDirection, };

export type NoteTypeDto = { kind: NoteTypeKind, templates: Array<NoteTemplateDto>, };

export type EditorDataDto = { decks: Array<EditorDeckOptionDto>, noteTypes: Array<NoteTypeDto>, tags: Array<string>, maxFieldLength: number, };

export type WordLinkDraftDto = { startIndex: number, endIndex: number, surface: string, 
/**
 * `null` keeps the phrase marked but unresolved; the backend never picks a word on its own.
 */
wordNoteId: string | null, };

export type SaveNoteInput = { operationId: string, 
/**
 * `null` creates a note.
 */
noteId: string | null, expectedVersion: number | null, deckId: string, noteTypeKind: NoteTypeKind, front: string, back: string, tags: Array<string>, manualAssociation: string | null, 
/**
 * Tips appended to the note; existing tips are edited on the card details.
 */
grammarTips: Array<string>, 
/**
 * Complete set of word links for a sentence; `null` keeps the stored links.
 */
wordLinks: Array<WordLinkDraftDto> | null, };

export type SearchWordsInput = { targetLanguage: TargetLanguage, text: string, limit: number, };

export type WordMatchKind = "exact" | "normalized";

export type WordMatchDto = { noteId: string, lemma: string, translation: string, deckName: string, match: WordMatchKind, };

export type DuplicateCheckInput = { deckId: string, front: string, excludeNoteId: string | null, };

export type DuplicateNoteDto = { noteId: string, front: string, deckName: string, };

export type DuplicateCheckDto = { duplicates: Array<DuplicateNoteDto>, };

export type SortKey = "due" | "created" | "front" | "deck" | "reps" | "lapses";

export type SortDirection = "asc" | "desc";

export type SearchInput = { query: string, deckId: string | null, tag: string | null, states: Array<CardState>, flags: Array<CardFlag>, sort: SortKey, direction: SortDirection, 
/**
 * Opaque keyset cursor from a previous page.
 */
cursor: string | null, pageSize: number, };

export type CardRowDto = { cardId: string, noteId: string, deckId: string, deckName: string, front: string, back: string, state: CardState, queue: QueueState, flag: CardFlag, dueAt: string | null, 
/**
 * Order of a new card in the new queue; `null` for cards already studied.
 */
position: number | null, direction: CardDirection, tags: Array<string>, };

export type CardPageDto = { items: Array<CardRowDto>, nextCursor: string | null, total: number, 
/**
 * Snapshot token used by bulk actions over "all matches".
 */
filterVersion: string, };

export type BulkSelection = { "kind": "ids", cardIds: Array<string>, } | { "kind": "query", filterVersion: string, excludedCardIds: Array<string>, };

export type BulkAction = { "type": "suspend" } | { "type": "unsuspend" } | { "type": "flag", flag: CardFlag, } | { "type": "changeDeck", deckId: string, } | { "type": "addTag", tag: string, } | { "type": "removeTag", tag: string, } | { "type": "reposition", start: number, step: number, } | { "type": "delete" };

export type BulkActionInput = { operationId: string, selection: BulkSelection, action: BulkAction, };

export type BulkPreviewInput = { selection: BulkSelection, action: BulkAction, };

export type BulkPreviewDto = { 
/**
 * Cards in the selection.
 */
selected: number, 
/**
 * Cards the action would change.
 */
affected: number, 
/**
 * Cards left as they are (already in that state, or not new for reposition).
 */
unchanged: number, 
/**
 * Notes and decks the affected cards belong to.
 */
notes: number, decks: number, };

export type BulkResultDto = { affected: number, };

export type StatsRange = "1m" | "3m" | "1y" | "all";

export type StatsInput = { 
/**
 * `null` = the whole collection; a deck includes its subdecks.
 */
deckId: string | null, range: StatsRange, };

export type RatioDto = { passed: number, total: number, };

export type ReviewDayDto = { day: string, count: number, activeMs: number, };

export type ForecastDayDto = { day: string, 
/**
 * Cards whose stored due date falls on this day (day 0 also holds the overdue ones).
 */
stored: number, 
/**
 * Reviews expected on this day once the deck limits carry the backlog forward.
 */
projected: number, };

export type ForecastAssumptionsDto = { reviewsPerDay: number, newPerDay: number, 
/**
 * New cards are not part of the forecast: they are introduced by the user's daily limit.
 */
includesNew: boolean, };

export type CardStatesDto = { New: number, Learning: number, Review: number, Relearning: number, Suspended: number, };

export type MaturityDto = { young: number, mature: number, };

export type TrueRetentionDto = { young: RatioDto | null, mature: RatioDto | null, };

export type StabilityBucketDto = { minDays: number, maxDays: number | null, count: number, };

export type DifficultyStabilityPointDto = { cardId: string, 
/**
 * Days.
 */
stability: number, 
/**
 * FSRS difficulty, 1–10.
 */
difficulty: number, };

export type StatsDto = { range: StatsRange, deckId: string | null, 
/**
 * Every study day of the range, oldest first, including days without reviews.
 */
reviewsByDay: Array<ReviewDayDto>, forecast: Array<ForecastDayDto>, forecastAssumptions: ForecastAssumptionsDto, 
/**
 * Cards already past their due day; part of `forecast[0].stored`.
 */
overdue: number, cardStates: CardStatesDto, 
/**
 * Review cards split by interval: young below `matureThresholdDays`, mature from it.
 */
maturity: MaturityDto, matureThresholdDays: number, trueRetention: TrueRetentionDto, stability: Array<StabilityBucketDto>, 
/**
 * Studied cards only; the advanced D-vs-S view.
 */
difficultyStability: Array<DifficultyStabilityPointDto>, streakDays: number, longestStreakDays: number, generatedAt: string, };

export type UiLanguage = "pl" | "en";

export type ThemePreference = "system" | "light" | "dark";

export type PermissionState = "granted" | "denied" | "prompt" | "unsupported";

export type ConsentState = "unset" | "granted" | "declined";

export type AiProviderOptionDto = { id: string, name: string, requiresKey: boolean, 
/**
 * Suggested models; empty = the user types the model name.
 */
models: Array<string>, 
/**
 * The user supplies an HTTPS endpoint (OpenAI-compatible servers).
 */
customEndpoint: boolean, };

export type AiQuotaDto = { used: number, limit: number, resetsAt: string, };

export type AiSettingsDto = { consent: ConsentState, provider: string | null, model: string | null, endpoint: string | null, 
/**
 * Secrets never cross into JavaScript; only their presence does.
 */
hasApiKey: boolean, providers: Array<AiProviderOptionDto>, 
/**
 * `null` = the provider reports no limit.
 */
quota: AiQuotaDto | null, };

export type BackupLocation = "app" | "external";

export type BackupSettingsDto = { 
/**
 * A backup is made automatically once a day after studying.
 */
automatic: boolean, 
/**
 * Newest backups kept; older ones are removed (safety copies are kept separately).
 */
retention: number, target: BackupLocation, 
/**
 * Display name of the chosen external folder; never a raw path.
 */
externalFolder: string | null, };

export type SettingsDto = { uiLanguage: UiLanguage, nativeLanguage: NativeLanguage, 
/**
 * Suggested target language for new decks; each deck keeps its own.
 */
defaultTargetLanguage: TargetLanguage, theme: ThemePreference, dailyGoal: number, 
/**
 * IANA zone used to compute the study day.
 */
timeZone: string, 
/**
 * Hour (0–23) at which a new study day starts.
 */
dayCutoffHour: number, 
/**
 * Preset given to new decks.
 */
defaultPresetId: string, notifications: PermissionState, reminderTime: string | null, ttsEnabled: boolean, 
/**
 * Speech rate, 0.5–1.5.
 */
ttsRate: number, ai: AiSettingsDto, telemetryOptIn: boolean, backup: BackupSettingsDto, version: number, };

export type SettingsPatch = { uiLanguage?: UiLanguage, defaultTargetLanguage?: TargetLanguage, theme?: ThemePreference, dailyGoal?: number, timeZone?: string, dayCutoffHour?: number, defaultPresetId?: string, 
/**
 * `null` turns the reminder off.
 */
reminderTime?: string | null, ttsEnabled?: boolean, ttsRate?: number, telemetryOptIn?: boolean, aiConsent?: ConsentState, aiProvider?: string, 
/**
 * `null` clears the model.
 */
aiModel?: string | null, 
/**
 * HTTPS only; `null` clears it.
 */
aiEndpoint?: string | null, backupAutomatic?: boolean, backupRetention?: number, backupTarget?: BackupLocation, 
/**
 * File token of a folder chosen with `pickFile({ purpose: 'backupFolder' })`.
 */
backupFolderToken?: string, };

export type UpdateSettingsInput = { operationId: string, expectedVersion: number, patch: SettingsPatch, };

export type ResetScope = "progress" | "everything";

export type ResetPreviewInput = { scope: ResetScope, };

export type CollectionCountsDto = { decks: number, notes: number, cards: number, reviews: number, };

export type ResetPreviewDto = { scope: ResetScope, counts: CollectionCountsDto, 
/**
 * Typed by the user when no safety backup is made.
 */
confirmationPhrase: string, latestBackupAt: string | null, previewToken: string, };

export type ResetInput = { operationId: string, previewToken: string, 
/**
 * A backup created for this reset (job result); either it or the phrase is required.
 */
safetyBackupId: string | null, confirmationPhrase: string | null, };

export type BackupKind = "manual" | "automatic" | "safety";

export type BackupDto = { id: string, createdAt: string, 
/**
 * Archive size; a JSON number (sizes above 4 GiB stay exact up to 2^53).
 */
sizeBytes: number, schemaVersion: number, cardCount: number, location: BackupLocation, kind: BackupKind, };

export type BackupInput = { operationId: string, };

export type ShareTarget = { "kind": "backup", backupId: string, } | { "kind": "export", jobId: string, };

export type ShareFileInput = { operationId: string, target: ShareTarget, };

export type ShareResultDto = { 
/**
 * `false` when the user closed the share sheet; not an error.
 */
shared: boolean, };

export type RestoreSource = { "kind": "file", fileToken: string, } | { "kind": "backup", backupId: string, };

export type ValidateRestoreInput = { source: RestoreSource, };

export type RestorePreviewDto = { fileName: string, createdAt: string, schemaVersion: number, counts: CollectionCountsDto, 
/**
 * What the restore replaces.
 */
current: CollectionCountsDto, compatible: boolean, issues: Array<ErrorDto>, previewToken: string, };

export type RestoreInput = { operationId: string, previewToken: string, };
