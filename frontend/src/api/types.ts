/**
 * Transport contract v0.4 between the Svelte frontend and the Rust backend.
 *
 * Rules (plan §4.1):
 * - SQLite i64 identifiers and GUIDs travel as strings (`Id`).
 * - Timestamps travel as UTC ISO-8601 strings (`IsoDateTime`); study days as `YYYY-MM-DD` (`StudyDay`).
 * - Every mutation carries an `operationId`; versioned entities carry `version`/`expectedVersion`.
 * - Long-running work returns a `JobDto`; events are signals, never durable truth.
 *
 * This file is framework-independent: no Svelte, no Tauri imports.
 * Rust becomes the DTO source in Phase 7 (generated into `src/generated/`).
 */

export const API_VERSION = '0.4'

export type Id = string
export type IsoDateTime = string
export type StudyDay = string
export type OperationId = string

export const RATINGS = ['Again', 'Hard', 'Good', 'Easy'] as const
export type Rating = (typeof RATINGS)[number]

export type UiLanguage = 'pl' | 'en'
export type NativeLanguage = 'pl'
export type TargetLanguage = 'de' | 'en'
export type ThemePreference = 'system' | 'light' | 'dark'

export const DECK_TYPES = ['Simple', 'TextWithAssociations', 'Linguistic'] as const
export type DeckType = (typeof DECK_TYPES)[number]

export interface DeckCapabilitiesDto {
  associations: boolean
  grammarTips: boolean
  wordLinks: boolean
  tts: boolean
}

export type CardState = 'New' | 'Learning' | 'Review' | 'Relearning'
export type QueueState = 'New' | 'Learning' | 'Review' | 'DayLearn' | 'Suspended' | 'SchedBuried' | 'UserBuried'
export const CARD_FLAGS = ['none', 'red', 'orange', 'green', 'blue', 'pink', 'turquoise', 'purple'] as const
export type CardFlag = (typeof CARD_FLAGS)[number]
export type NoteTypeKind = 'Basic' | 'BasicAndReversed' | 'Sentence'
export type CardDirection = 'forward' | 'reverse'

// ---------------------------------------------------------------- errors

export const ERROR_CODES = [
  'VALIDATION_FAILED',
  'VERSION_CONFLICT',
  'NOT_FOUND',
  'DELETED',
  'DB_BUSY',
  'STORAGE_FULL',
  'PERMISSION_DENIED',
  'NETWORK_UNAVAILABLE',
  'PROVIDER_ERROR',
  'PROVIDER_TIMEOUT',
  'INVALID_PROVIDER_OUTPUT',
  'MODERATION_BLOCKED',
  'QUOTA_EXCEEDED',
  'UNSUPPORTED_FORMAT',
  'BACKUP_CORRUPT',
  'SCHEMA_TOO_NEW',
  'PREVIEW_STALE',
  'CANCELLED',
  'NOT_IMPLEMENTED',
  'INTERNAL',
] as const
export type ErrorCode = (typeof ERROR_CODES)[number]

export interface ErrorDto {
  code: ErrorCode
  /** i18n key; the UI translates this, never raw SQL or stack traces. */
  messageKey: string
  retryable: boolean
  details: Record<string, string | number | boolean | null> | null
}

// ---------------------------------------------------------------- app / jobs / events

export interface AppInfoDto {
  apiVersion: string
  appName: string
  appVersion: string
  /** Native build variant (`dev` | `beta` | `release`); the in-memory mock reports `demo`. */
  mode: 'demo' | 'dev' | 'beta' | 'release'
}

export type JobKind = 'prepareContent' | 'import' | 'export' | 'backup' | 'restore' | 'llm'
export type JobStatus = 'queued' | 'running' | 'succeeded' | 'failed' | 'cancelled'

export interface JobProgressDto {
  done: number
  total: number
}

export interface JobDto {
  jobId: Id
  kind: JobKind
  status: JobStatus
  /** Monotonic per job; consumers drop events with a lower sequence. */
  sequence: number
  progress: JobProgressDto | null
  startedAt: IsoDateTime
  finishedAt: IsoDateTime | null
  error: ErrorDto | null
}

export interface JobEventDto {
  jobId: Id
  kind: JobKind
  sequence: number
  status: JobStatus
  progress: JobProgressDto | null
}

export type CollectionScope = 'decks' | 'cards' | 'notes' | 'settings' | 'stats' | 'backups'

export interface CollectionChangedEventDto {
  scopes: CollectionScope[]
  /** Read signal only: refetch through AppApi. */
  changedAt: IsoDateTime
}

export type Unsubscribe = () => void

// ---------------------------------------------------------------- decks / dashboard

export interface DeckCountsDto {
  new: number
  learning: number
  review: number
  overdue: number
  total: number
}

export type ContentState = 'ready' | 'preparing'

export interface DeckSummaryDto {
  id: Id
  guid: Id
  name: string
  parentId: Id | null
  targetLanguage: TargetLanguage
  nativeLanguage: NativeLanguage
  deckType: DeckType
  capabilities: DeckCapabilitiesDto
  counts: DeckCountsDto
  /** Share of mature cards, 0..1. */
  progress: number
  bundled: boolean
  contentState: ContentState
  version: number
}

/** An unfinished session the dashboard offers to resume (durable in Rust from Phase 10). */
export interface ResumableSessionDto {
  sessionId: Id
  /** `null` = a session over all decks. */
  deckId: Id | null
  deckName: string | null
  status: Exclude<StudySessionStatus, 'finished'>
  reviewedCount: number
  remaining: number
}

export interface DashboardDto {
  decks: DeckSummaryDto[]
  streakDays: number
  studiedToday: number
  dueToday: number
  dailyGoal: number
  resumable: ResumableSessionDto | null
  /** Earliest future due date over all decks; lets "nothing due today" say when the next review is. */
  nextDueAt: IsoDateTime | null
  generatedAt: IsoDateTime
}

export interface HiddenSectionCountsDto {
  associations: number
  grammarTips: number
  wordLinks: number
}

/** A deck type the deck could switch to, with what it would hide. Hidden content is kept, never deleted. */
export interface DeckTypeOptionDto {
  deckType: DeckType
  capabilities: DeckCapabilitiesDto
  hiddenCounts: HiddenSectionCountsDto
}

export interface DeckOverviewDto {
  deck: DeckSummaryDto
  /** Every deck below this one (any depth), each parent listed before its children. */
  subdecks: DeckSummaryDto[]
  description: string
  presetId: Id
  presetName: string
  presetDependentDecks: number
  /** Content kept but hidden by the current deck type. */
  hiddenSectionCounts: HiddenSectionCountsDto
  typeOptions: DeckTypeOptionDto[]
}

export interface CreateDeckInput {
  operationId: OperationId
  name: string
  parentId: Id | null
  targetLanguage: TargetLanguage
  deckType: DeckType
}

export interface UpdateDeckInput {
  operationId: OperationId
  deckId: Id
  expectedVersion: number
  patch: {
    name?: string
    description?: string
    parentId?: Id | null
    deckType?: DeckType
    presetId?: Id
  }
}

export interface DeleteDeckInput {
  operationId: OperationId
  deckId: Id
  expectedVersion: number
}

export interface DeleteDeckResultDto {
  deletedDecks: number
  deletedCards: number
}

// ---------------------------------------------------------------- presets (deck options)

/** Number of FSRS-6 weights w0–w20. */
export const FSRS_WEIGHT_COUNT = 21

export interface PresetConfigDto {
  newPerDay: number
  reviewsPerDay: number
  /** 0.70–0.99. */
  desiredRetention: number
  maxIntervalDays: number
  learningStepsMinutes: number[]
  relearningStepsMinutes: number[]
  /** FSRS-6 weights; exactly `FSRS_WEIGHT_COUNT` finite numbers. */
  weights: number[]
}

export interface PresetDto {
  id: Id
  name: string
  version: number
  /** Built-in presets can be cloned and edited like any other. */
  builtIn: boolean
  /** Decks using this preset; a change applies to all of them. */
  dependentDecks: number
  config: PresetConfigDto
}

export interface SavePresetInput {
  operationId: OperationId
  /** `null` creates a preset (also used to clone one). */
  presetId: Id | null
  expectedVersion: number | null
  name: string
  config: PresetConfigDto
}

// ---------------------------------------------------------------- notes / cards content

export type AssociationProvenance = 'bundled' | 'generated' | 'manual'
export type AssociationStatus = 'ready' | 'pending' | 'offlineQueued' | 'error' | 'blocked'

/** Read-only content of a note, used by the word sheet in study. */
export interface NoteContentDto {
  noteId: Id
  deckId: Id
  front: string
  back: string
  targetLanguage: TargetLanguage
  capabilities: DeckCapabilitiesDto
  associations: AssociationDto[]
  grammarTips: GrammarTipDto[]
}

export interface AssociationDto {
  id: Id
  noteId: Id
  text: string
  provenance: AssociationProvenance
  favorite: boolean
  status: AssociationStatus
  model: string | null
  error: ErrorDto | null
}

export interface GrammarTipDto {
  id: Id
  noteId: Id
  text: string
  order: number
}

export type WordLinkResolution = 'resolved' | 'unresolved' | 'ambiguous' | 'userOverridden'

export interface WordCandidateDto {
  noteId: Id
  lemma: string
  translation: string
}

export interface WordLinkDto {
  id: Id
  sentenceNoteId: Id
  surface: string
  /** UTF-16 offsets in NFC text, half-open [startIndex, endIndex) — plan §14.1. */
  startIndex: number
  endIndex: number
  lemma: string | null
  resolution: WordLinkResolution
  wordNoteId: Id | null
  translation: string | null
  candidates: WordCandidateDto[]
}

// ---------------------------------------------------------------- study

export interface StudyCardDto {
  cardId: Id
  noteId: Id
  deckId: Id
  version: number
  deckType: DeckType
  capabilities: DeckCapabilitiesDto
  direction: CardDirection
  state: CardState
  flag: CardFlag
  front: string
  back: string
  targetLanguage: TargetLanguage
  associations: AssociationDto[]
  grammarTips: GrammarTipDto[]
  wordLinks: WordLinkDto[]
}

export type StudySessionStatus = 'active' | 'paused' | 'finished'

export interface StudySessionDto {
  sessionId: Id
  deckId: Id | null
  deckName: string | null
  status: StudySessionStatus
  current: StudyCardDto | null
  counts: DeckCountsDto
  reviewedCount: number
  undoAvailable: boolean
  startedAt: IsoDateTime
}

export interface StartStudyInput {
  operationId: OperationId
  /** `null` studies all decks. */
  deckId: Id | null
  /** Restricts the session to these cards (e.g. "study the Again cards again"); omitted = the normal queue. */
  cardIds?: Id[]
}

export type StudySessionAction = 'pause' | 'resume' | 'end'

export interface UpdateStudySessionInput {
  operationId: OperationId
  sessionId: Id
  action: StudySessionAction
}

export interface PreviewInput {
  sessionId: Id
  cardId: Id
  cardVersion: number
}

export interface IntervalPreviewDto {
  dueAt: IsoDateTime
  intervalSeconds: number
}

export interface RatingPreviewDto {
  cardId: Id
  cardVersion: number
  /** Card + preset version token; stale tokens are rejected with PREVIEW_STALE. */
  previewToken: string
  intervals: Record<Rating, IntervalPreviewDto>
}

export interface RateCardInput {
  operationId: OperationId
  sessionId: Id
  cardId: Id
  cardVersion: number
  previewToken: string
  rating: Rating
  /** Monotonic active answer time, capped by the backend. */
  answerMs: number
}

export interface ReviewedCardDto {
  cardId: Id
  version: number
  state: CardState
  dueAt: IsoDateTime | null
}

export interface ReviewResultDto {
  operationId: OperationId
  sessionId: Id
  reviewed: ReviewedCardDto
  next: StudyCardDto | null
  counts: DeckCountsDto
  reviewedCount: number
  undoAvailable: boolean
  sessionStatus: StudySessionStatus
}

export interface UndoInput {
  operationId: OperationId
  sessionId: Id
}

export type CardActionKind = 'suspend' | 'unsuspend' | 'bury' | 'unbury' | 'flag' | 'delete'

export interface CardActionInput {
  operationId: OperationId
  cardId: Id
  expectedVersion: number
  action: CardActionKind
  flag?: CardFlag
}

export interface CardActionResultDto {
  cardId: Id
  version: number
  queue: QueueState
  flag: CardFlag
  deleted: boolean
}

export interface SessionCardDto {
  cardId: Id
  front: string
  back: string
  targetLanguage: TargetLanguage
  direction: CardDirection
}

export interface NextDeckDto {
  id: Id
  name: string
  due: number
}

export interface SessionSummaryDto {
  sessionId: Id
  deckId: Id | null
  deckName: string | null
  status: StudySessionStatus
  ratings: Record<Rating, number>
  reviewedCount: number
  activeMs: number
  /** Cards rated Again at least once; the summary offers to study them again. */
  againCards: SessionCardDto[]
  nextDueAt: IsoDateTime | null
  /** Cards of the session's deck scope still due today. */
  remainingDue: number
  /** Another deck with due cards, if any. */
  nextDeck: NextDeckDto | null
  /** First finished session of this profile: the summary offers a daily reminder. */
  firstCompleted: boolean
  startedAt: IsoDateTime
  finishedAt: IsoDateTime | null
}

// ---------------------------------------------------------------- details / editor

export interface NoteDto {
  id: Id
  guid: Id
  noteTypeKind: NoteTypeKind
  front: string
  back: string
  tags: string[]
  version: number
  createdAt: IsoDateTime
  updatedAt: IsoDateTime
}

export interface ReviewLogDto {
  id: Id
  reviewedAt: IsoDateTime
  rating: Rating
  stateBefore: CardState
  intervalSeconds: number
}

export interface RetrievabilityDto {
  value: number
  computedAt: IsoDateTime
}

export interface CardDetailsDto {
  id: Id
  noteId: Id
  /** The scheduling deck belongs to the card: cards of one note may sit in different decks. */
  deckId: Id
  deckName: string
  ordinal: number
  direction: CardDirection
  state: CardState
  queue: QueueState
  flag: CardFlag
  dueAt: IsoDateTime | null
  stability: number | null
  difficulty: number | null
  retrievability: RetrievabilityDto | null
  reps: number
  lapses: number
  version: number
  history: ReviewLogDto[]
}

export interface NoteDetailsDto {
  note: NoteDto
  /** Deck of the note's first card; decides which sections the details and the editor show. */
  deckId: Id
  targetLanguage: TargetLanguage
  capabilities: DeckCapabilitiesDto
  /** Live cards; each has its own schedule. */
  cards: CardDetailsDto[]
  /** Template ordinals archived by a note-type change; switching back restores them with history. */
  archivedOrdinals: number[]
  associations: AssociationDto[]
  grammarTips: GrammarTipDto[]
  wordLinks: WordLinkDto[]
}

export interface DeleteNoteInput {
  operationId: OperationId
  noteId: Id
  expectedVersion: number
}

export interface DeleteNoteResultDto {
  deletedCards: number
}

/** One edit of a note's tips, associations or word links (each change is its own transaction). */
export type NoteContentChange =
  | { type: 'addTip'; text: string }
  | { type: 'editTip'; tipId: Id; text: string }
  | { type: 'deleteTip'; tipId: Id }
  /** The complete new order; any other set of ids is rejected. */
  | { type: 'reorderTips'; tipIds: Id[] }
  | { type: 'addAssociation'; text: string }
  /** Only the user's own (manual) associations can be edited. */
  | { type: 'editAssociation'; associationId: Id; text: string }
  | { type: 'deleteAssociation'; associationId: Id }
  /** Removes the link; the word note itself is untouched. */
  | { type: 'unlinkWord'; linkId: Id }

export interface UpdateNoteContentInput {
  operationId: OperationId
  noteId: Id
  change: NoteContentChange
}

export interface EditorDeckOptionDto {
  id: Id
  name: string
  parentId: Id | null
  deckType: DeckType
  capabilities: DeckCapabilitiesDto
  targetLanguage: TargetLanguage
}

/** Card template of a note type: which side is asked. */
export interface NoteTemplateDto {
  ordinal: number
  direction: CardDirection
}

export interface NoteTypeDto {
  kind: NoteTypeKind
  templates: NoteTemplateDto[]
}

export interface EditorDataDto {
  decks: EditorDeckOptionDto[]
  noteTypes: NoteTypeDto[]
  tags: string[]
  maxFieldLength: number
}

/** A word link as the editor sends it; ranges are UTF-16 offsets in the NFC front text. */
export interface WordLinkDraftDto {
  startIndex: number
  endIndex: number
  surface: string
  /** `null` keeps the phrase marked but unresolved; the backend never picks a word on its own. */
  wordNoteId: Id | null
}

export interface SaveNoteInput {
  operationId: OperationId
  /** `null` creates a note. */
  noteId: Id | null
  expectedVersion: number | null
  deckId: Id
  noteTypeKind: NoteTypeKind
  front: string
  back: string
  tags: string[]
  manualAssociation: string | null
  /** Tips appended to the note; existing tips are edited on the card details. */
  grammarTips: string[]
  /** Complete set of word links for a sentence; `null` keeps the stored links. */
  wordLinks: WordLinkDraftDto[] | null
}

export interface SearchWordsInput {
  targetLanguage: TargetLanguage
  text: string
  limit: number
}

/** `exact`: same text; `normalized`: same after case/article/diacritic folding. Never a guess. */
export type WordMatchKind = 'exact' | 'normalized'

export interface WordMatchDto {
  noteId: Id
  lemma: string
  translation: string
  deckName: string
  match: WordMatchKind
}

export interface DuplicateCheckInput {
  deckId: Id
  front: string
  excludeNoteId: Id | null
}

export interface DuplicateCheckDto {
  duplicates: { noteId: Id; front: string; deckName: string }[]
}

export interface SetFavoriteAssociationInput {
  operationId: OperationId
  noteId: Id
  associationId: Id
}

export interface ResolveWordLinkInput {
  operationId: OperationId
  linkId: Id
  wordNoteId: Id
}

export interface GenerateAssociationInput {
  operationId: OperationId
  noteId: Id
  variants: number
}

// ---------------------------------------------------------------- browser

export const SORT_KEYS = ['due', 'created', 'front', 'deck', 'reps', 'lapses'] as const
export type SortKey = (typeof SORT_KEYS)[number]

export interface SearchInput {
  query: string
  deckId: Id | null
  tag: string | null
  states: CardState[]
  flags: CardFlag[]
  sort: SortKey
  direction: 'asc' | 'desc'
  /** Opaque keyset cursor from a previous page. */
  cursor: string | null
  pageSize: number
}

export interface CardRowDto {
  cardId: Id
  noteId: Id
  deckId: Id
  deckName: string
  front: string
  back: string
  state: CardState
  queue: QueueState
  flag: CardFlag
  dueAt: IsoDateTime | null
  /** Order of a new card in the new queue; `null` for cards already studied. */
  position: number | null
  direction: CardDirection
  tags: string[]
}

export interface CardPageDto {
  items: CardRowDto[]
  nextCursor: string | null
  total: number
  /** Snapshot token used by bulk actions over "all matches". */
  filterVersion: string
}

export type BulkSelection =
  | { kind: 'ids'; cardIds: Id[] }
  | { kind: 'query'; filterVersion: string; excludedCardIds: Id[] }

export type BulkAction =
  | { type: 'suspend' }
  | { type: 'unsuspend' }
  | { type: 'flag'; flag: CardFlag }
  | { type: 'changeDeck'; deckId: Id }
  | { type: 'addTag'; tag: string }
  | { type: 'removeTag'; tag: string }
  /** New cards only: positions start, start+step, …; studied cards are skipped. */
  | { type: 'reposition'; start: number; step: number }
  | { type: 'delete' }

export type BulkActionType = BulkAction['type']

export interface BulkActionInput {
  operationId: OperationId
  selection: BulkSelection
  action: BulkAction
}

export interface BulkPreviewInput {
  selection: BulkSelection
  action: BulkAction
}

/** What a bulk action would do, shown before the user confirms it. */
export interface BulkPreviewDto {
  /** Cards in the selection. */
  selected: number
  /** Cards the action would change. */
  affected: number
  /** Cards left as they are (already in that state, or not new for reposition). */
  unchanged: number
  /** Notes and decks the affected cards belong to. */
  notes: number
  decks: number
}

export interface BulkResultDto {
  affected: number
}

// ---------------------------------------------------------------- statistics

export type StatsRange = '1m' | '3m' | '1y' | 'all'

export interface StatsInput {
  deckId: Id | null
  range: StatsRange
}

export interface RatioDto {
  passed: number
  total: number
}

export interface StatsDto {
  range: StatsRange
  reviewsByDay: { day: StudyDay; count: number; activeMs: number }[]
  forecast: { day: StudyDay; due: number }[]
  cardStates: Record<CardState | 'Suspended', number>
  /** `null` = not enough data; never rendered as 0%. */
  trueRetention: { young: RatioDto | null; mature: RatioDto | null }
  streakDays: number
}

// ---------------------------------------------------------------- settings / onboarding / privacy

export type PermissionState = 'granted' | 'denied' | 'prompt' | 'unsupported'
export type ConsentState = 'unset' | 'granted' | 'declined'

export interface AiSettingsDto {
  consent: ConsentState
  provider: string | null
  model: string | null
  endpoint: string | null
  /** Secrets never cross into JavaScript; only their presence does. */
  hasApiKey: boolean
}

export interface SettingsDto {
  uiLanguage: UiLanguage
  nativeLanguage: NativeLanguage
  theme: ThemePreference
  dailyGoal: number
  timeZone: string
  dayCutoffHour: number
  notifications: PermissionState
  reminderTime: string | null
  ttsEnabled: boolean
  ai: AiSettingsDto
  telemetryOptIn: boolean
  version: number
}

export type SettingsPatch = Partial<
  Pick<
    SettingsDto,
    'uiLanguage' | 'theme' | 'dailyGoal' | 'timeZone' | 'dayCutoffHour' | 'reminderTime' | 'ttsEnabled' | 'telemetryOptIn'
  >
> & { aiConsent?: ConsentState }

export interface UpdateSettingsInput {
  operationId: OperationId
  expectedVersion: number
  patch: SettingsPatch
}

export interface BundledDeckDto {
  guid: Id
  name: string
  targetLanguage: TargetLanguage
  deckType: DeckType
  noteCount: number
  cardCount: number
  level: string
}

export type OnboardingStep = 'nativeLanguage' | 'targetLanguage' | 'goal' | 'decks' | 'ready'

export interface OnboardingStateDto {
  completed: boolean
  step: OnboardingStep
  availableDecks: BundledDeckDto[]
}

export interface CompleteOnboardingInput {
  operationId: OperationId
  targetLanguages: TargetLanguage[]
  dailyGoal: number
  deckGuids: Id[]
}

export interface AttributionDto {
  name: string
  license: string
  url: string | null
  scope: 'dependency' | 'content' | 'font' | 'icon'
}

// ---------------------------------------------------------------- transfer / backup

export type FilePurpose = 'import' | 'restore'

export interface PickFileInput {
  purpose: FilePurpose
}

/** Opaque handle from the native picker; the GUI never sends arbitrary paths. */
export interface FileHandleDto {
  token: string
  name: string
  sizeBytes: number
}

export type CsvDelimiter = ',' | ';' | '\t'
export type DuplicatePolicy = 'update' | 'skip' | 'duplicate'
export type CsvColumnTarget = 'front' | 'back' | 'deck' | 'tags' | 'due' | 'ignore'

export interface ImportInput {
  fileToken: string
  delimiter: CsvDelimiter | null
  hasHeader: boolean
  mapping: CsvColumnTarget[] | null
  duplicatePolicy: DuplicatePolicy
  targetDeckId: Id | null
}

export interface ImportPreviewDto {
  fileName: string
  detectedDelimiter: CsvDelimiter | null
  encoding: string
  hasBom: boolean
  columns: string[]
  sampleRows: string[][]
  mapping: CsvColumnTarget[]
  counts: { create: number; update: number; skip: number; duplicate: number; error: number }
  rowErrors: { row: number; error: ErrorDto }[]
  previewToken: string
}

export interface CommitImportInput {
  operationId: OperationId
  previewToken: string
}

export interface BackupDto {
  id: Id
  createdAt: IsoDateTime
  sizeBytes: number
  schemaVersion: number
  cardCount: number
  location: 'app' | 'external'
}

export interface BackupInput {
  operationId: OperationId
}

export interface ValidateRestoreInput {
  fileToken: string
}

export interface RestorePreviewDto {
  fileName: string
  createdAt: IsoDateTime
  schemaVersion: number
  counts: { decks: number; notes: number; cards: number; reviews: number }
  compatible: boolean
  issues: ErrorDto[]
  previewToken: string
}

export interface RestoreInput {
  operationId: OperationId
  previewToken: string
}
