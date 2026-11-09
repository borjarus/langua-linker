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
 * Rust is the DTO source for the types re-exported from `src/generated/contracts.ts` (Phases 7–11);
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
  SortKey as GeneratedSortKey,
  HiddenSectionCountsDto,
  DeckTypeOptionDto,
  DeckOverviewDto,
  CreateDeckInput,
  DeckPatchDto,
  UpdateDeckInput,
  DeleteDeckInput,
  DeleteDeckResultDto,
  PresetConfigDto,
  PresetDto,
  SavePresetInput,
  NoteDto,
  ReviewLogDto,
  RetrievabilityDto,
  CardDetailsDto,
  NoteDetailsDto,
  DeleteNoteInput,
  DeleteNoteResultDto,
  NoteContentChange,
  UpdateNoteContentInput,
  EditorDeckOptionDto,
  NoteTemplateDto,
  NoteTypeDto,
  EditorDataDto,
  WordLinkDraftDto,
  SaveNoteInput,
  SearchWordsInput,
  WordMatchKind,
  WordMatchDto,
  DuplicateCheckInput,
  DuplicateNoteDto,
  DuplicateCheckDto,
  SearchInput,
  CardRowDto,
  CardPageDto,
  BulkSelection,
  BulkAction,
  BulkActionInput,
  BulkPreviewInput,
  BulkPreviewDto,
  BulkResultDto,
  SortDirection,
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
  HiddenSectionCountsDto,
  DeckTypeOptionDto,
  DeckOverviewDto,
  CreateDeckInput,
  DeckPatchDto,
  UpdateDeckInput,
  DeleteDeckInput,
  DeleteDeckResultDto,
  PresetConfigDto,
  PresetDto,
  SavePresetInput,
  NoteDto,
  ReviewLogDto,
  RetrievabilityDto,
  CardDetailsDto,
  NoteDetailsDto,
  DeleteNoteInput,
  DeleteNoteResultDto,
  NoteContentChange,
  UpdateNoteContentInput,
  EditorDeckOptionDto,
  NoteTemplateDto,
  NoteTypeDto,
  EditorDataDto,
  WordLinkDraftDto,
  SaveNoteInput,
  SearchWordsInput,
  WordMatchKind,
  WordMatchDto,
  DuplicateCheckInput,
  DuplicateNoteDto,
  DuplicateCheckDto,
  SearchInput,
  CardRowDto,
  CardPageDto,
  BulkSelection,
  BulkAction,
  BulkActionInput,
  BulkPreviewInput,
  BulkPreviewDto,
  BulkResultDto,
  SortDirection,
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

export const TARGET_LANGUAGES = ['de', 'en', 'es'] as const
/** Compile-time check: the runtime list equals the Rust `TargetLanguage` enum. */
const targetLanguagesMatchRust: SameMembers<(typeof TARGET_LANGUAGES)[number], TargetLanguage> = true
void targetLanguagesMatchRust

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

// ---------------------------------------------------------------- decks / dashboard / presets

// Deck overview, deck CRUD and preset DTOs come from Rust (`src/generated/contracts.ts`, Phase 11).

/** Number of FSRS-6 weights w0–w20. */
export const FSRS_WEIGHT_COUNT = 21

// ---------------------------------------------------------------- study

// Study, word-sheet and card-action DTOs come from Rust (`src/generated/contracts.ts`, Phase 10).

// ---------------------------------------------------------------- details / editor

// Note details, editor, note content and word search DTOs come from Rust (Phase 11).

export interface GenerateAssociationInput {
  operationId: OperationId
  noteId: Id
  variants: number
}

// ---------------------------------------------------------------- browser

// Card search, paging and bulk-action DTOs come from Rust (Phase 11).

export const SORT_KEYS = ['due', 'created', 'front', 'deck', 'reps', 'lapses'] as const
export type SortKey = (typeof SORT_KEYS)[number]
/** Compile-time check: the runtime list equals the Rust `SortKey` enum. */
const sortKeysMatchRust: SameMembers<SortKey, GeneratedSortKey> = true
void sortKeysMatchRust

export type BulkActionType = BulkAction['type']

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

