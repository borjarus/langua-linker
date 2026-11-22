/**
 * Transport contract v1.3 between the Svelte frontend and the Rust backend.
 *
 * Rules (plan §4.1):
 * - SQLite i64 identifiers and GUIDs travel as strings (`Id`).
 * - Timestamps travel as UTC ISO-8601 strings (`IsoDateTime`); study days as `YYYY-MM-DD` (`StudyDay`).
 * - Every mutation carries an `operationId`; versioned entities carry `version`/`expectedVersion`.
 * - Long-running work returns a `JobDto`; events are signals, never durable truth.
 *
 * This file is framework-independent: no Svelte, no Tauri imports.
 * Rust is the DTO source for the types re-exported from `src/generated/contracts.ts` (Phases 7–14);
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
  CsvEncoding,
  ImportInput,
  ImportPreviewDto,
  CommitImportInput,
  ExportDeckInput,
  ExportFormat,
  ExportLossDto,
  CsvDelimiter as GeneratedCsvDelimiter,
  DuplicatePolicy as GeneratedDuplicatePolicy,
  CsvColumnTarget as GeneratedCsvColumnTarget,
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
  GenerateAssociationInput,
  SetAiApiKeyInput,
  ResolveWordLinkInput,
  AddWordLinkInput,
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
  GetOptimizerInput,
  OptimizerDatasetDto,
  OptimizerRunStatus,
  OptimizerMetricsDto,
  OptimizerRunDto,
  OptimizerDto,
  StartOptimizerInput,
  OptimizerRunActionInput,
  GetGrammarInput,
  GrammarDeckSummaryDto,
  GrammarEntryDto,
  GrammarSectionDto,
  GrammarDeckDto,
  GrammarDto,
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
  RatioDto,
  ReviewDayDto,
  ForecastDayDto,
  ForecastAssumptionsDto,
  StabilityBucketDto,
  DifficultyStabilityPointDto,
  StatsDto,
  StatsInput,
  PermissionState,
  ConsentState,
  AiProviderOptionDto,
  AiQuotaDto,
  AiSettingsDto,
  AiModelInput,
  LocalModelDto,
  LocalModelState,
  BackupLocation,
  BackupSettingsDto,
  SettingsDto,
  SettingsPatch,
  UpdateSettingsInput,
  ResetScope,
  ResetPreviewInput,
  ResetPreviewDto,
  ResetInput,
  BackupKind,
  BackupDto,
  BackupInput,
  DeleteBackupInput,
  ShareTarget,
  ShareFileInput,
  ShareResultDto,
  RestoreSource,
  ValidateRestoreInput,
  RestorePreviewDto,
  RestoreInput,
  UiLanguage,
  ThemePreference,
  CardStatesDto,
  MaturityDto,
  TrueRetentionDto,
  CollectionCountsDto,
  StatsRange as GeneratedStatsRange,
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
  GenerateAssociationInput,
  SetAiApiKeyInput,
  ResolveWordLinkInput,
  AddWordLinkInput,
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
  GetOptimizerInput,
  OptimizerDatasetDto,
  OptimizerRunStatus,
  OptimizerMetricsDto,
  OptimizerRunDto,
  OptimizerDto,
  StartOptimizerInput,
  OptimizerRunActionInput,
  GetGrammarInput,
  GrammarDeckSummaryDto,
  GrammarEntryDto,
  GrammarSectionDto,
  GrammarDeckDto,
  GrammarDto,
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
  RatioDto,
  ReviewDayDto,
  ForecastDayDto,
  ForecastAssumptionsDto,
  StabilityBucketDto,
  DifficultyStabilityPointDto,
  StatsDto,
  StatsInput,
  PermissionState,
  ConsentState,
  AiProviderOptionDto,
  AiQuotaDto,
  AiSettingsDto,
  AiModelInput,
  LocalModelDto,
  LocalModelState,
  BackupLocation,
  BackupSettingsDto,
  SettingsDto,
  SettingsPatch,
  UpdateSettingsInput,
  ResetScope,
  ResetPreviewInput,
  ResetPreviewDto,
  ResetInput,
  BackupKind,
  BackupDto,
  BackupInput,
  DeleteBackupInput,
  ShareTarget,
  ShareFileInput,
  ShareResultDto,
  RestoreSource,
  ValidateRestoreInput,
  RestorePreviewDto,
  RestoreInput,
  UiLanguage,
  ThemePreference,
  CardStatesDto,
  MaturityDto,
  TrueRetentionDto,
  CollectionCountsDto,
  CsvEncoding,
  ImportInput,
  ImportPreviewDto,
  CommitImportInput,
  ExportDeckInput,
  ExportFormat,
  ExportLossDto,
}

export const API_VERSION = '1.11'

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

// ---------------------------------------------------------------- browser

// Card search, paging and bulk-action DTOs come from Rust (Phase 11).

export const SORT_KEYS = ['due', 'created', 'front', 'deck', 'reps', 'lapses'] as const
export type SortKey = (typeof SORT_KEYS)[number]
/** Compile-time check: the runtime list equals the Rust `SortKey` enum. */
const sortKeysMatchRust: SameMembers<SortKey, GeneratedSortKey> = true
void sortKeysMatchRust

export type BulkActionType = BulkAction['type']

// ---------------------------------------------------------------- statistics

// Statistics, settings, reset, backup and restore DTOs come from Rust (Phase 12).

export const STATS_RANGES = ['1m', '3m', '1y', 'all'] as const
export type StatsRange = (typeof STATS_RANGES)[number]
/** Compile-time check: the runtime list equals the Rust `StatsRange` enum. */
const statsRangesMatchRust: SameMembers<StatsRange, GeneratedStatsRange> = true
void statsRangesMatchRust

// ---------------------------------------------------------------- settings / onboarding / privacy

export interface RequestPermissionInput {
  operationId: OperationId
}

export const ONBOARDING_STEPS = ['nativeLanguage', 'targetLanguage', 'goal', 'decks', 'ready'] as const
export type OnboardingStep = (typeof ONBOARDING_STEPS)[number]
const onboardingStepsMatchRust: SameMembers<OnboardingStep, GeneratedOnboardingStep> = true
void onboardingStepsMatchRust

// ---------------------------------------------------------------- transfer / backup

// CSV and export DTOs come from Rust (Phase 14).

export const CSV_DELIMITERS = [',', ';', '\t'] as const
export type CsvDelimiter = (typeof CSV_DELIMITERS)[number]
const csvDelimitersMatchRust: SameMembers<CsvDelimiter, GeneratedCsvDelimiter> = true
void csvDelimitersMatchRust
export const CSV_ENCODINGS = ['UTF-8', 'UTF-16LE', 'UTF-16BE', 'windows-1252'] as const satisfies readonly CsvEncoding[]
export const DUPLICATE_POLICIES = ['skip', 'update', 'duplicate'] as const
export type DuplicatePolicy = (typeof DUPLICATE_POLICIES)[number]
const duplicatePoliciesMatchRust: SameMembers<DuplicatePolicy, GeneratedDuplicatePolicy> = true
void duplicatePoliciesMatchRust
export const CSV_COLUMN_TARGETS = ['front', 'back', 'tags', 'guid', 'deck', 'noteType', 'due', 'interval', 'reps', 'lapses', 'position', 'ignore'] as const
export type CsvColumnTarget = (typeof CSV_COLUMN_TARGETS)[number]
const csvColumnTargetsMatchRust: SameMembers<CsvColumnTarget, GeneratedCsvColumnTarget> = true
void csvColumnTargetsMatchRust
export const EXPORT_FORMATS = ['csv', 'apkg'] as const satisfies readonly ExportFormat[]

