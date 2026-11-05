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

export type TargetLanguage = "de" | "en";

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

export type ResolveWordLinkInput = { operationId: string, linkId: string, wordNoteId: string, };
