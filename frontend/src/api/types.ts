/**
 * Transport contract v1.2 between the Svelte frontend and the Rust backend.
 *
 * Rules (plan §4.1):
 * - SQLite i64 identifiers and GUIDs travel as strings (`Id`).
 * - Timestamps travel as UTC ISO-8601 strings (`IsoDateTime`); study days as `YYYY-MM-DD` (`StudyDay`).
 * - Every mutation carries an `operationId`; versioned entities carry `version`/`expectedVersion`.
 * - Long-running work returns a `JobDto`; events are signals, never durable truth.
 *
 * This file is framework-independent: no Svelte, no Tauri imports.
 * Rust is the DTO source for the types re-exported from `src/generated/contracts.ts` (Phases 7–10);
 * the remaining DTOs move there as their commands become real.
 */

import type {
  AboutDto,
  AppInfoDto,
  CollectionChangedEventDto,
  CollectionScope,
  DiagnosticsReportDto,
  ErrorDto,
  ImportCountsDto,
  ImportRowErrorDto,
  JobDto,
  JobEventDto,
  JobKind,
  JobProgressDto,
  JobResultDto,
  JobStatus,
  LogLevel,
  LogLineDto,
  AttributionDto,
  AttributionScope,
  BundledDeckDto,
  CompleteOnboardingInput,
  ContentState,
  ContentWarningCode,
  ContentWarningDto,
  DashboardDto,
  DeckCapabilitiesDto,
  DeckCountsDto,
  DeckSummaryDto,
  OnboardingStateDto,
  ResumableSessionDto,
  ResumableStatus,
  NativeLanguage,
  TargetLanguage,
  CommitDeckImportInput,
  DeckContentKind,
  DeckImportOriginDto,
  DeckImportPreviewDto,
  DeckImportWarningDto,
  FileHandleDto,
  FilePurpose,
  NoteTypeKind,
  PickFileInput,
  PreviewDeckImportInput,
  DeckType as GeneratedDeckType,
  OnboardingStep as GeneratedOnboardingStep,
  ErrorCode as GeneratedErrorCode,
  NoteContentDto,
  AssociationDto,
  GrammarTipDto,
  WordCandidateDto,
  WordLinkDto,
  StudyCardDto,
  StudySessionDto,
  StartStudyInput,
  UpdateStudySessionInput,
  PreviewInput,
  IntervalPreviewDto,
  RatingPreviewDto,
  RateCardInput,
  ReviewedCardDto,
  ReviewResultDto,
  UndoInput,
  CardActionInput,
  CardActionResultDto,
  SessionCardDto,
  NextDeckDto,
  SessionSummaryDto,
  SetFavoriteAssociationInput,
  ResolveWordLinkInput,
  AssociationProvenance,
  AssociationStatus,
  WordLinkResolution,
  StudySessionStatus,
  StudySessionAction,
  CardActionKind,
  CardState,
  QueueState,
  CardDirection,
  Rating as GeneratedRating,
  CardFlag as GeneratedCardFlag,
} from '../generated/contracts'

export type {
  AboutDto,
  AppInfoDto,
  CollectionChangedEventDto,
  CollectionScope,
  DiagnosticsReportDto,
  ErrorDto,
  ImportCountsDto,
  ImportRowErrorDto,
  JobDto,
  JobEventDto,
  JobKind,
  JobProgressDto,
  JobResultDto,
  JobStatus,
  LogLevel,
  LogLineDto,
  AttributionDto,
  AttributionScope,
  BundledDeckDto,
  CompleteOnboardingInput,
  ContentState,
  ContentWarningCode,
  ContentWarningDto,
  DashboardDto,
  DeckCapabilitiesDto,
  DeckCountsDto,
  DeckSummaryDto,
  OnboardingStateDto,
  ResumableSessionDto,
  ResumableStatus,
  NativeLanguage,
  TargetLanguage,
  CommitDeckImportInput,
  DeckContentKind,
  DeckImportOriginDto,
  DeckImportPreviewDto,
  DeckImportWarningDto,
  FileHandleDto,
  FilePurpose,
  NoteTypeKind,
  PickFileInput,
  PreviewDeckImportInput,
  NoteContentDto,
  AssociationDto,
  GrammarTipDto,
  WordCandidateDto,
  WordLinkDto,
  StudyCardDto,
  StudySessionDto,
  StartStudyInput,
  UpdateStudySessionInput,
  PreviewInput,
  IntervalPreviewDto,
  RatingPreviewDto,
  RateCardInput,
  ReviewedCardDto,
  ReviewResultDto,
  UndoInput,
  CardActionInput,
  CardActionResultDto,
  SessionCardDto,
  NextDeckDto,
  SessionSummaryDto,
  SetFavoriteAssociationInput,
  ResolveWordLinkInput,
  AssociationProvenance,
  AssociationStatus,
  WordLinkResolution,
  StudySessionStatus,
  StudySessionAction,
  CardActionKind,
  CardState,
  QueueState,
  CardDirection,
}

export const API_VERSION = '1.2'

/** Equal sets of members: guards runtime lists against the Rust enums. */
type SameMembers<A, B> = [A] extends [B] ? ([B] extends [A] ? true : false) : false

export type Id = string
export type IsoDateTime = string
export type StudyDay = string
export type OperationId = string

export const RATINGS = ['Again', 'Hard', 'Good', 'Easy'] as const
export type Rating = (typeof RATINGS)[number]
/** Compile-time check: the runtime list equals the Rust `Rating` enum. */
const ratingsMatchRust: SameMembers<Rating, GeneratedRating> = true
void ratingsMatchRust

export type UiLanguage = 'pl' | 'en'
export type ThemePreference = 'system' | 'light' | 'dark'

export const DECK_TYPES = ['Simple', 'TextWithAssociations', 'Linguistic'] as const
export type DeckType = (typeof DECK_TYPES)[number]
/** Compile-time check: the runtime list equals the Rust `DeckType` enum. */
const deckTypesMatchRust: SameMembers<DeckType, GeneratedDeckType> = true
void deckTypesMatchRust

export const CARD_FLAGS = ['none', 'red', 'orange', 'green', 'blue', 'pink', 'turquoise', 'purple'] as const
export type CardFlag = (typeof CARD_FLAGS)[number]
/** Compile-time check: the runtime list equals the Rust `CardFlag` enum. */
const cardFlagsMatchRust: SameMembers<CardFlag, GeneratedCardFlag> = true
void cardFlagsMatchRust
/** Note types whose front is a sentence: its words can link to word notes. */
export const isSentenceNoteType = (kind: NoteTypeKind): boolean => kind === 'Sentence' || kind === 'SentenceAndReversed'

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
/** Compile-time check: the runtime list equals the Rust `ErrorCode` enum. */
const errorCodesMatchRust: SameMembers<ErrorCode, GeneratedErrorCode> = true
void errorCodesMatchRust

// ---------------------------------------------------------------- app / jobs / events

export type Unsubscribe = () => void

// ---------------------------------------------------------------- decks / dashboard

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

// ---------------------------------------------------------------- study

// Study, word-sheet and card-action DTOs come from Rust (`src/generated/contracts.ts`, Phase 10).

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

export const STATS_RANGES = ['1m', '3m', '1y', 'all'] as const
export type StatsRange = (typeof STATS_RANGES)[number]

export interface StatsInput {
  /** `null` = the whole collection; a deck includes its subdecks. */
  deckId: Id | null
  range: StatsRange
}

export interface RatioDto {
  passed: number
  total: number
}

export interface ReviewDayDto {
  day: StudyDay
  count: number
  activeMs: number
}

/** One study day of the due forecast. */
export interface ForecastDayDto {
  day: StudyDay
  /** Cards whose stored due date falls on this day (day 0 also holds the overdue ones). */
  stored: number
  /** Reviews expected on this day once the deck limits carry the backlog forward. */
  projected: number
}

/** What the projected forecast assumes; shown next to the chart. */
export interface ForecastAssumptionsDto {
  reviewsPerDay: number
  newPerDay: number
  /** New cards are not part of the forecast: they are introduced by the user's daily limit. */
  includesNew: boolean
}

/** Stability histogram bucket in days, `[minDays, maxDays)`; `maxDays: null` is open-ended. */
export interface StabilityBucketDto {
  minDays: number
  maxDays: number | null
  count: number
}

export interface DifficultyStabilityPointDto {
  cardId: Id
  /** Days. */
  stability: number
  /** FSRS difficulty, 1–10. */
  difficulty: number
}

export interface StatsDto {
  range: StatsRange
  deckId: Id | null
  /** Every study day of the range, oldest first, including days without reviews. */
  reviewsByDay: ReviewDayDto[]
  forecast: ForecastDayDto[]
  forecastAssumptions: ForecastAssumptionsDto
  /** Cards already past their due day; part of `forecast[0].stored`. */
  overdue: number
  cardStates: Record<CardState | 'Suspended', number>
  /** Review cards split by interval: young below `matureThresholdDays`, mature from it. */
  maturity: { young: number; mature: number }
  matureThresholdDays: number
  /** `null` = not enough data; never rendered as 0%. */
  trueRetention: { young: RatioDto | null; mature: RatioDto | null }
  stability: StabilityBucketDto[]
  /** Studied cards only; the advanced D-vs-S view. */
  difficultyStability: DifficultyStabilityPointDto[]
  streakDays: number
  longestStreakDays: number
  generatedAt: IsoDateTime
}

// ---------------------------------------------------------------- settings / onboarding / privacy

export type PermissionState = 'granted' | 'denied' | 'prompt' | 'unsupported'
export type ConsentState = 'unset' | 'granted' | 'declined'

export interface AiProviderOptionDto {
  id: string
  name: string
  requiresKey: boolean
  /** Suggested models; empty = the user types the model name. */
  models: string[]
  /** The user supplies an HTTPS endpoint (OpenAI-compatible servers). */
  customEndpoint: boolean
}

export interface AiQuotaDto {
  used: number
  limit: number
  resetsAt: IsoDateTime
}

export interface AiSettingsDto {
  consent: ConsentState
  provider: string | null
  model: string | null
  endpoint: string | null
  /** Secrets never cross into JavaScript; only their presence does. */
  hasApiKey: boolean
  providers: AiProviderOptionDto[]
  /** `null` = the provider reports no limit. */
  quota: AiQuotaDto | null
}

export type BackupLocation = 'app' | 'external'

export interface BackupSettingsDto {
  /** A backup is made automatically once a day after studying. */
  automatic: boolean
  /** Newest backups kept; older ones are removed (safety copies are kept separately). */
  retention: number
  target: BackupLocation
  /** Display name of the chosen external folder; never a raw path. */
  externalFolder: string | null
}

export interface SettingsDto {
  uiLanguage: UiLanguage
  nativeLanguage: NativeLanguage
  /** Suggested target language for new decks; each deck keeps its own. */
  defaultTargetLanguage: TargetLanguage
  theme: ThemePreference
  dailyGoal: number
  /** IANA zone used to compute the study day. */
  timeZone: string
  /** Hour (0–23) at which a new study day starts. */
  dayCutoffHour: number
  /** Preset given to new decks. */
  defaultPresetId: Id
  notifications: PermissionState
  reminderTime: string | null
  ttsEnabled: boolean
  /** Speech rate, 0.5–1.5. */
  ttsRate: number
  ai: AiSettingsDto
  telemetryOptIn: boolean
  backup: BackupSettingsDto
  version: number
}

export type SettingsPatch = Partial<
  Pick<
    SettingsDto,
    | 'uiLanguage'
    | 'defaultTargetLanguage'
    | 'theme'
    | 'dailyGoal'
    | 'timeZone'
    | 'dayCutoffHour'
    | 'defaultPresetId'
    | 'reminderTime'
    | 'ttsEnabled'
    | 'ttsRate'
    | 'telemetryOptIn'
  >
> & {
  aiConsent?: ConsentState
  aiProvider?: string
  aiModel?: string | null
  /** HTTPS only; `null` clears it. */
  aiEndpoint?: string | null
  backupAutomatic?: boolean
  backupRetention?: number
  backupTarget?: BackupLocation
  /** File token of a folder chosen with `pickFile({ purpose: 'backupFolder' })`. */
  backupFolderToken?: string
}

export interface UpdateSettingsInput {
  operationId: OperationId
  expectedVersion: number
  patch: SettingsPatch
}

export interface SetAiApiKeyInput {
  operationId: OperationId
  /** Written to the platform SecretStore; `null` removes the key. Never read back. */
  apiKey: string | null
}

export interface RequestPermissionInput {
  operationId: OperationId
}

export const ONBOARDING_STEPS = ['nativeLanguage', 'targetLanguage', 'goal', 'decks', 'ready'] as const
export type OnboardingStep = (typeof ONBOARDING_STEPS)[number]
const onboardingStepsMatchRust: SameMembers<OnboardingStep, GeneratedOnboardingStep> = true
void onboardingStepsMatchRust

export type ResetScope = 'progress' | 'everything'

export interface ResetPreviewInput {
  scope: ResetScope
}

/** What a reset would remove; the user confirms it with a fresh backup or the phrase. */
export interface ResetPreviewDto {
  scope: ResetScope
  counts: { decks: number; notes: number; cards: number; reviews: number }
  /** Typed by the user when no safety backup is made. */
  confirmationPhrase: string
  latestBackupAt: IsoDateTime | null
  previewToken: string
}

export interface ResetInput {
  operationId: OperationId
  previewToken: string
  /** A backup created for this reset (job result); either it or the phrase is required. */
  safetyBackupId: Id | null
  confirmationPhrase: string | null
}

// ---------------------------------------------------------------- transfer / backup

export const CSV_DELIMITERS = [',', ';', '\t'] as const
export type CsvDelimiter = (typeof CSV_DELIMITERS)[number]
export const DUPLICATE_POLICIES = ['skip', 'update', 'duplicate'] as const
export type DuplicatePolicy = (typeof DUPLICATE_POLICIES)[number]
export const CSV_COLUMN_TARGETS = ['front', 'back', 'tags', 'ignore'] as const
export type CsvColumnTarget = (typeof CSV_COLUMN_TARGETS)[number]

export interface ImportInput {
  fileToken: string
  /** `null` = detect. */
  delimiter: CsvDelimiter | null
  hasHeader: boolean
  /** `null` = suggested mapping. */
  mapping: CsvColumnTarget[] | null
  duplicatePolicy: DuplicatePolicy
  /** `null` = a new deck named after the file. */
  targetDeckId: Id | null
}

/** Dry run: nothing is written until `commitImport` with the same `previewToken`. */
export interface ImportPreviewDto {
  fileName: string
  detectedDelimiter: CsvDelimiter | null
  delimiter: CsvDelimiter
  encoding: string
  hasBom: boolean
  hasHeader: boolean
  columns: string[]
  sampleRows: string[][]
  mapping: CsvColumnTarget[]
  totalRows: number
  counts: ImportCountsDto
  rowErrors: ImportRowErrorDto[]
  targetDeckId: Id | null
  targetDeckName: string
  previewToken: string
}

export interface CommitImportInput {
  operationId: OperationId
  previewToken: string
}

export interface ExportDeckInput {
  operationId: OperationId
  deckId: Id
  format: 'csv'
  /** Adds due date, interval and reps columns. */
  includeScheduling: boolean
}

export type BackupKind = 'manual' | 'automatic' | 'safety'

export interface BackupDto {
  id: Id
  createdAt: IsoDateTime
  sizeBytes: number
  schemaVersion: number
  cardCount: number
  location: BackupLocation
  kind: BackupKind
}

export interface BackupInput {
  operationId: OperationId
}

export type ShareTarget = { kind: 'backup'; backupId: Id } | { kind: 'export'; jobId: Id }

export interface ShareFileInput {
  operationId: OperationId
  target: ShareTarget
}

export interface ShareResultDto {
  /** `false` when the user closed the share sheet; not an error. */
  shared: boolean
}

export type RestoreSource = { kind: 'file'; fileToken: string } | { kind: 'backup'; backupId: Id }

export interface ValidateRestoreInput {
  source: RestoreSource
}

export interface RestorePreviewDto {
  fileName: string
  createdAt: IsoDateTime
  schemaVersion: number
  counts: { decks: number; notes: number; cards: number; reviews: number }
  /** What the restore replaces. */
  current: { decks: number; notes: number; cards: number; reviews: number }
  compatible: boolean
  issues: ErrorDto[]
  previewToken: string
}

export interface RestoreInput {
  operationId: OperationId
  previewToken: string
}

