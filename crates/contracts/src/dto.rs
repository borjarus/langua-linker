//! DTOs whose source of truth is Rust (plan step 28). Each derives `TS`; `typescript()` renders
//! them into `frontend/src/generated/contracts.ts`, which `frontend/src/api/types.ts` re-exports.
//! A DTO moves here when its command becomes real; the rest of `types.ts` follows phase by phase.
//!
//! Integers are `u32`/`f64`: an `i64`/`u64` would render as `bigint` in TypeScript and fail the
//! frontend type check — the intended guard against ids or counts beyond 2^53 crossing as numbers.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ErrorDto;

mod settings;
pub use settings::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum JobKind {
    PrepareContent,
    Import,
    Export,
    Backup,
    Restore,
    Reset,
    Llm,
    /// FSRS optimizer (contract 1.5).
    Optimize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum JobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct JobProgressDto {
    pub done: u32,
    pub total: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobDto {
    pub job_id: String,
    pub kind: JobKind,
    pub status: JobStatus,
    /// Monotonic per job; consumers drop events with a lower sequence.
    pub sequence: u32,
    pub progress: Option<JobProgressDto>,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub error: Option<ErrorDto>,
    /// Set once the job finished (also on some failures, e.g. the safety copy of a failed restore).
    pub result: Option<JobResultDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobEventDto {
    pub job_id: String,
    pub kind: JobKind,
    pub sequence: u32,
    pub status: JobStatus,
    pub progress: Option<JobProgressDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ImportRowErrorDto {
    /// 1-based line in the file.
    pub row: u32,
    pub error: ErrorDto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ImportCountsDto {
    pub create: u32,
    pub update: u32,
    pub skip: u32,
    pub duplicate: u32,
    pub error: u32,
}

/// Outcome of a finished job, read with `getJobStatus`; job events never carry it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum JobResultDto {
    Import {
        deck_id: String,
        row_errors: Vec<ImportRowErrorDto>,
        create: u32,
        update: u32,
        skip: u32,
        duplicate: u32,
        error: u32,
    },
    /// `format` and `losses` since contract 1.3; a 1.2 result has neither.
    Export {
        file_name: String,
        card_count: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        format: Option<ExportFormat>,
        #[serde(default)]
        losses: Vec<ExportLossDto>,
    },
    Backup {
        backup_id: String,
    },
    /// Present also on a failed restore: the safety copy made before anything changed.
    Restore {
        safety_backup_id: String,
    },
    Reset {
        safety_backup_id: Option<String>,
    },
    /// Counts and warnings since contract 1.1; a 1.0 frontend reads only `deckIds`.
    PrepareContent {
        deck_ids: Vec<String>,
        #[serde(default)]
        created: u32,
        #[serde(default)]
        updated: u32,
        #[serde(default)]
        unchanged: u32,
        /// Removed from the bundled content but kept: the learner has progress or edits.
        #[serde(default)]
        kept: u32,
        /// Removed from the bundled content without progress: cards archived, history intact.
        #[serde(default)]
        archived: u32,
        #[serde(default)]
        warnings: Vec<ContentWarningDto>,
    },
    /// A finished optimizer run (contract 1.5); read it with `getOptimizer`.
    Optimize {
        run_id: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ContentWarningCode {
    /// The learner's edit was kept over a changed bundled value.
    OverrideConflict,
    /// A deleted bundled deck is not reinstalled.
    DeckDeleted,
}

/// A notable outcome of preparing bundled content (contract 1.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContentWarningDto {
    pub code: ContentWarningCode,
    pub deck_guid: String,
    pub note_guid: Option<String>,
    /// The field kept for the learner (`front`, `back`, `tags`, `association:<guid>`, ...).
    pub field: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum NativeLanguage {
    #[serde(rename = "pl")]
    Pl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum TargetLanguage {
    #[serde(rename = "de")]
    De,
    #[serde(rename = "en")]
    En,
    #[serde(rename = "es")]
    Es,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum DeckType {
    Simple,
    TextWithAssociations,
    Linguistic,
}

/// Sections a deck shows; owned by Rust (`DeckType::capabilities`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeckCapabilitiesDto {
    pub associations: bool,
    pub grammar_tips: bool,
    pub word_links: bool,
    pub tts: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct DeckCountsDto {
    pub new: u32,
    pub learning: u32,
    pub review: u32,
    pub overdue: u32,
    pub total: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ContentState {
    Ready,
    Preparing,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeckSummaryDto {
    pub id: String,
    pub guid: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub target_language: TargetLanguage,
    pub native_language: NativeLanguage,
    pub deck_type: DeckType,
    pub capabilities: DeckCapabilitiesDto,
    pub counts: DeckCountsDto,
    /// Share of mature cards, 0..1.
    pub progress: f64,
    pub bundled: bool,
    pub content_state: ContentState,
    pub version: u32,
    /// Since 1.6: CEFR level of the content the deck came from (`A1`, or a range `A1-A2`);
    /// `null` for decks made in the app.
    #[serde(default)]
    pub level: Option<String>,
    /// Since 1.7: `false` for a reference deck kept out of study sessions and due counts (its
    /// words still serve as link targets); its `counts` are then zero.
    #[serde(default = "in_study_default")]
    pub in_study: bool,
}

const fn in_study_default() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ResumableStatus {
    Active,
    Paused,
}

/// An unfinished session the dashboard offers to resume (durable in Rust from Phase 10).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResumableSessionDto {
    pub session_id: String,
    /// `null` = a session over all decks.
    pub deck_id: Option<String>,
    pub deck_name: Option<String>,
    pub status: ResumableStatus,
    pub reviewed_count: u32,
    pub remaining: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DashboardDto {
    pub decks: Vec<DeckSummaryDto>,
    pub streak_days: u32,
    pub studied_today: u32,
    pub due_today: u32,
    pub daily_goal: u32,
    pub resumable: Option<ResumableSessionDto>,
    /// Earliest future due date over all decks; lets "nothing due today" say when the next review is.
    pub next_due_at: Option<String>,
    pub generated_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BundledDeckDto {
    pub guid: String,
    pub name: String,
    pub description: String,
    pub target_language: TargetLanguage,
    pub deck_type: DeckType,
    pub note_count: u32,
    pub card_count: u32,
    pub level: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum OnboardingStep {
    NativeLanguage,
    TargetLanguage,
    Goal,
    Decks,
    Ready,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OnboardingStateDto {
    pub completed: bool,
    pub step: OnboardingStep,
    pub available_decks: Vec<BundledDeckDto>,
    /// Bundled decks already in the collection; onboarding again only adds the missing ones.
    pub installed_deck_guids: Vec<String>,
    pub native_language: NativeLanguage,
    pub target_languages: Vec<TargetLanguage>,
    pub daily_goal: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompleteOnboardingInput {
    pub operation_id: String,
    pub native_language: NativeLanguage,
    pub target_languages: Vec<TargetLanguage>,
    pub daily_goal: u32,
    /// May be empty: the user can start without bundled decks.
    pub deck_guids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum AttributionScope {
    Dependency,
    Content,
    Font,
    Icon,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AttributionDto {
    pub name: String,
    pub license: String,
    pub url: Option<String>,
    pub scope: AttributionScope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum CollectionScope {
    Decks,
    Cards,
    Notes,
    Settings,
    Stats,
    Backups,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CollectionChangedEventDto {
    pub scopes: Vec<CollectionScope>,
    /// Read signal only: refetch through AppApi.
    pub changed_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AboutDto {
    pub app_version: String,
    pub api_version: String,
    /// `null` when the database could not be opened.
    pub schema_version: Option<u32>,
    pub fsrs_version: String,
    pub content_version: String,
    pub platform: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct LogLineDto {
    pub at: String,
    pub level: LogLevel,
    pub target: String,
    /// Already redacted by the backend: no card content, prompts or secrets.
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiagnosticsReportDto {
    pub generated_at: String,
    pub lines: Vec<LogLineDto>,
    /// Number of values the backend replaced with `[redacted]`.
    pub redactions: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum FilePurpose {
    Import,
    Restore,
    BackupFolder,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct PickFileInput {
    pub purpose: FilePurpose,
}

/// Opaque handle from the native picker; the GUI never sends arbitrary paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileHandleDto {
    pub token: String,
    pub name: String,
    pub size_bytes: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum NoteTypeKind {
    Basic,
    BasicAndReversed,
    Sentence,
    SentenceAndReversed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum DeckContentKind {
    Words,
    Sentences,
}

/// Where a deck package came from (`apkg-convert` records it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeckImportOriginDto {
    /// `apkg` for Anki.
    pub kind: String,
    pub file_name: String,
    /// Date of the conversion, `YYYY-MM-DD`.
    pub converted_on: String,
    pub tool: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct DeckImportWarningDto {
    /// Content-rule code, e.g. `front.homograph`.
    pub code: String,
    /// Line in the package file.
    pub line: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewDeckImportInput {
    pub file_token: String,
}

/// Dry run of a deck package import (contract 1.2): nothing is written until
/// `commitDeckImport` with the same `previewToken`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeckImportPreviewDto {
    pub file_name: String,
    pub deck_guid: String,
    pub name: String,
    pub description: String,
    pub target_language: TargetLanguage,
    pub deck_type: DeckType,
    pub note_type: NoteTypeKind,
    pub content_kind: DeckContentKind,
    pub level: String,
    pub origin: Option<DeckImportOriginDto>,
    pub notes: u32,
    pub cards: u32,
    /// Cards that arrive with progress (not new).
    pub scheduled_cards: u32,
    pub suspended_cards: u32,
    /// Past reviews imported as history.
    pub reviews: u32,
    pub new_notes: u32,
    /// Notes already in the collection: kept with their progress, content updated unless edited.
    pub existing_notes: u32,
    /// The deck this package updates, when it was imported before.
    pub existing_deck_id: Option<String>,
    pub warnings: Vec<DeckImportWarningDto>,
    pub preview_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommitDeckImportInput {
    pub operation_id: String,
    pub preview_token: String,
}

// ---------------------------------------------------------------- CSV and Anki (Phase 14)

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum CsvDelimiter {
    #[serde(rename = ",")]
    Comma,
    #[serde(rename = ";")]
    Semicolon,
    #[serde(rename = "	")]
    Tab,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum CsvEncoding {
    #[serde(rename = "UTF-8")]
    Utf8,
    #[serde(rename = "UTF-16LE")]
    Utf16Le,
    #[serde(rename = "UTF-16BE")]
    Utf16Be,
    #[serde(rename = "windows-1252")]
    Windows1252,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum DuplicatePolicy {
    Skip,
    Update,
    Duplicate,
}

/// What a CSV column becomes. Since 1.3 also the GUID, deck path, note type and the scheduling
/// columns of a LanguaLinker or Anki export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum CsvColumnTarget {
    Front,
    Back,
    Tags,
    Ignore,
    Guid,
    Deck,
    NoteType,
    Due,
    Interval,
    Reps,
    Lapses,
    Position,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportInput {
    pub file_token: String,
    /// `null` = detect.
    pub delimiter: Option<CsvDelimiter>,
    pub has_header: bool,
    /// `null` = suggested mapping.
    pub mapping: Option<Vec<CsvColumnTarget>>,
    pub duplicate_policy: DuplicatePolicy,
    /// `null` = a new deck named after the file.
    pub target_deck_id: Option<String>,
    /// Since 1.3; omitted or `null` = detect (BOM, UTF-8, else Windows-1252).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub encoding: Option<CsvEncoding>,
    /// Since 1.3: removes the `'` a guarded export put before `=`, `+`, `-`, `@`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub formula_guard: Option<bool>,
}

/// Dry run: nothing is written until `commitImport` with the same `previewToken`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportPreviewDto {
    pub file_name: String,
    pub detected_delimiter: Option<CsvDelimiter>,
    pub delimiter: CsvDelimiter,
    pub encoding: String,
    pub has_bom: bool,
    pub has_header: bool,
    pub columns: Vec<String>,
    pub sample_rows: Vec<Vec<String>>,
    pub mapping: Vec<CsvColumnTarget>,
    pub total_rows: u32,
    pub counts: ImportCountsDto,
    pub row_errors: Vec<ImportRowErrorDto>,
    pub target_deck_id: Option<String>,
    pub target_deck_name: String,
    /// Since 1.3: decks the import creates (full paths with `::`), the new target first.
    #[serde(default)]
    pub new_decks: Vec<String>,
    pub preview_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommitImportInput {
    pub operation_id: String,
    pub preview_token: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ExportFormat {
    Csv,
    /// Anki package (legacy schema 11), since 1.3.
    Apkg,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportDeckInput {
    pub operation_id: String,
    pub deck_id: String,
    pub format: ExportFormat,
    /// CSV: adds due date, interval, reps, lapses and position columns. Ignored for `apkg`,
    /// which always carries the scheduling and the review log.
    pub include_scheduling: bool,
    /// Since 1.3, CSV only: a `'` before fields a spreadsheet would evaluate. Omitted = raw.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub formula_guard: Option<bool>,
}

/// What the export format could not carry (`code`, e.g. `associations`, `reviewHistory`,
/// `fsrsMemoryUnverified`) and for how many items.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportLossDto {
    pub code: String,
    pub count: u32,
}

// ---------------------------------------------------------------- study (Phase 10)

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum Rating {
    Again,
    Hard,
    Good,
    Easy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum CardState {
    New,
    Learning,
    Review,
    Relearning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum QueueState {
    New,
    Learning,
    Review,
    DayLearn,
    Suspended,
    SchedBuried,
    UserBuried,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum CardFlag {
    None,
    Red,
    Orange,
    Green,
    Blue,
    Pink,
    Turquoise,
    Purple,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum CardDirection {
    Forward,
    Reverse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum AssociationProvenance {
    Bundled,
    Generated,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum AssociationStatus {
    Ready,
    Pending,
    OfflineQueued,
    Error,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssociationDto {
    pub id: String,
    pub note_id: String,
    pub text: String,
    pub provenance: AssociationProvenance,
    pub favorite: bool,
    pub status: AssociationStatus,
    pub model: Option<String>,
    pub error: Option<ErrorDto>,
    /// Since 1.4: long Markdown version shown collapsed under `text` (bundled content).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub extended: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GrammarTipDto {
    pub id: String,
    pub note_id: String,
    pub text: String,
    pub order: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum WordLinkResolution {
    Resolved,
    Unresolved,
    Ambiguous,
    UserOverridden,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WordCandidateDto {
    pub note_id: String,
    pub lemma: String,
    pub translation: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WordLinkDto {
    pub id: String,
    pub sentence_note_id: String,
    pub surface: String,
    /// UTF-16 offsets in NFC text, half-open [startIndex, endIndex) — plan §14.1.
    pub start_index: u32,
    pub end_index: u32,
    pub lemma: Option<String>,
    pub resolution: WordLinkResolution,
    pub word_note_id: Option<String>,
    pub translation: Option<String>,
    pub candidates: Vec<WordCandidateDto>,
}

/// Read-only content of a note, used by the word sheet in study.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NoteContentDto {
    pub note_id: String,
    pub deck_id: String,
    pub front: String,
    pub back: String,
    pub target_language: TargetLanguage,
    pub capabilities: DeckCapabilitiesDto,
    pub associations: Vec<AssociationDto>,
    pub grammar_tips: Vec<GrammarTipDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StudyCardDto {
    pub card_id: String,
    pub note_id: String,
    pub deck_id: String,
    pub version: u32,
    pub deck_type: DeckType,
    pub capabilities: DeckCapabilitiesDto,
    pub direction: CardDirection,
    pub state: CardState,
    pub flag: CardFlag,
    pub front: String,
    pub back: String,
    pub target_language: TargetLanguage,
    pub associations: Vec<AssociationDto>,
    pub grammar_tips: Vec<GrammarTipDto>,
    pub word_links: Vec<WordLinkDto>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum StudySessionStatus {
    Active,
    Paused,
    Finished,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StudySessionDto {
    pub session_id: String,
    pub deck_id: Option<String>,
    pub deck_name: Option<String>,
    pub status: StudySessionStatus,
    pub current: Option<StudyCardDto>,
    /// Cards still to study today in the session's scope; `total` is their sum.
    pub counts: DeckCountsDto,
    pub reviewed_count: u32,
    pub undo_available: bool,
    pub started_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartStudyInput {
    pub operation_id: String,
    /// `null` studies all decks.
    pub deck_id: Option<String>,
    /// Restricts the session to these cards (e.g. "study the Again cards again"); omitted = the
    /// normal queue.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub card_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum StudySessionAction {
    Pause,
    Resume,
    End,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateStudySessionInput {
    pub operation_id: String,
    pub session_id: String,
    pub action: StudySessionAction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewInput {
    pub session_id: String,
    pub card_id: String,
    pub card_version: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IntervalPreviewDto {
    pub due_at: String,
    pub interval_seconds: u32,
}

/// One entry per rating button (`Record<Rating, …>` in TypeScript).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct RatingIntervalsDto {
    pub again: IntervalPreviewDto,
    pub hard: IntervalPreviewDto,
    pub good: IntervalPreviewDto,
    pub easy: IntervalPreviewDto,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RatingPreviewDto {
    pub card_id: String,
    pub card_version: u32,
    /// Card + preset version token; stale tokens are rejected with PREVIEW_STALE.
    pub preview_token: String,
    pub intervals: RatingIntervalsDto,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RateCardInput {
    pub operation_id: String,
    pub session_id: String,
    pub card_id: String,
    pub card_version: u32,
    pub preview_token: String,
    pub rating: Rating,
    /// Monotonic active answer time, capped by the backend.
    pub answer_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewedCardDto {
    pub card_id: String,
    pub version: u32,
    pub state: CardState,
    pub due_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewResultDto {
    pub operation_id: String,
    pub session_id: String,
    pub reviewed: ReviewedCardDto,
    pub next: Option<StudyCardDto>,
    pub counts: DeckCountsDto,
    pub reviewed_count: u32,
    pub undo_available: bool,
    pub session_status: StudySessionStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UndoInput {
    pub operation_id: String,
    pub session_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum CardActionKind {
    Suspend,
    Unsuspend,
    Bury,
    Unbury,
    Flag,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CardActionInput {
    pub operation_id: String,
    pub card_id: String,
    pub expected_version: u32,
    pub action: CardActionKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub flag: Option<CardFlag>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CardActionResultDto {
    pub card_id: String,
    pub version: u32,
    pub queue: QueueState,
    pub flag: CardFlag,
    pub deleted: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionCardDto {
    pub card_id: String,
    pub front: String,
    pub back: String,
    pub target_language: TargetLanguage,
    pub direction: CardDirection,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct NextDeckDto {
    pub id: String,
    pub name: String,
    pub due: u32,
}

/// Valid ratings by button (`Record<Rating, number>` in TypeScript).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct RatingCountsDto {
    pub again: u32,
    pub hard: u32,
    pub good: u32,
    pub easy: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionSummaryDto {
    pub session_id: String,
    pub deck_id: Option<String>,
    pub deck_name: Option<String>,
    pub status: StudySessionStatus,
    pub ratings: RatingCountsDto,
    pub reviewed_count: u32,
    pub active_ms: u32,
    /// Cards rated Again at least once; the summary offers to study them again.
    pub again_cards: Vec<SessionCardDto>,
    pub next_due_at: Option<String>,
    /// Cards of the session's deck scope still due today.
    pub remaining_due: u32,
    /// Another deck with due cards, if any.
    pub next_deck: Option<NextDeckDto>,
    /// First finished session of this profile: the summary offers a daily reminder.
    pub first_completed: bool,
    pub started_at: String,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetFavoriteAssociationInput {
    pub operation_id: String,
    pub note_id: String,
    pub association_id: String,
}

/// Queues new AI associations of a note (Phase 13); the answer is a job of kind `llm`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GenerateAssociationInput {
    pub operation_id: String,
    pub note_id: String,
    /// 1–3 variants; each becomes its own placeholder and association.
    pub variants: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetAiApiKeyInput {
    pub operation_id: String,
    /// Written to the platform SecretStore; `null` removes the key. Never read back.
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolveWordLinkInput {
    pub operation_id: String,
    pub link_id: String,
    pub word_note_id: String,
}

// ---------------------------------------------------------------- Phase 11: decks and presets

/// Content kept but hidden by a deck type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HiddenSectionCountsDto {
    pub associations: u32,
    pub grammar_tips: u32,
    pub word_links: u32,
}

/// A deck type the deck could switch to, with what it would hide. Hidden content is kept, never
/// deleted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeckTypeOptionDto {
    pub deck_type: DeckType,
    pub capabilities: DeckCapabilitiesDto,
    pub hidden_counts: HiddenSectionCountsDto,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeckOverviewDto {
    pub deck: DeckSummaryDto,
    /// Every deck below this one (any depth), each parent listed before its children.
    pub subdecks: Vec<DeckSummaryDto>,
    pub description: String,
    pub preset_id: String,
    pub preset_name: String,
    pub preset_dependent_decks: u32,
    /// Content kept but hidden by the current deck type.
    pub hidden_section_counts: HiddenSectionCountsDto,
    pub type_options: Vec<DeckTypeOptionDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateDeckInput {
    pub operation_id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub target_language: TargetLanguage,
    pub deck_type: DeckType,
}

/// Reads a field that may be missing (`None`), `null` (`Some(None)`) or a value.
mod double_option {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn deserialize<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
    where
        D: Deserializer<'de>,
        T: Deserialize<'de>,
    {
        Option::<T>::deserialize(deserializer).map(Some)
    }

    pub fn serialize<S, T>(value: &Option<Option<T>>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        T: Serialize,
    {
        match value {
            Some(inner) => inner.serialize(serializer),
            None => serializer.serialize_none(),
        }
    }
}

/// Fields an `updateDeck` changes; a missing field keeps its value.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeckPatchDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
    /// `null` moves the deck to the top level.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "double_option"
    )]
    #[ts(optional, as = "Option<Option<String>>")]
    pub parent_id: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub deck_type: Option<DeckType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub preset_id: Option<String>,
    /// Since 1.7: takes the deck in or out of study sessions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub in_study: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateDeckInput {
    pub operation_id: String,
    pub deck_id: String,
    pub expected_version: u32,
    pub patch: DeckPatchDto,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteDeckInput {
    pub operation_id: String,
    pub deck_id: String,
    pub expected_version: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteDeckResultDto {
    pub deleted_decks: u32,
    pub deleted_cards: u32,
}

/// Scheduling options of a preset; ranges are checked by Rust (`DeckConfigOptions::validate`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PresetConfigDto {
    pub new_per_day: u32,
    pub reviews_per_day: u32,
    /// 0.70–0.99.
    pub desired_retention: f64,
    pub max_interval_days: u32,
    pub learning_steps_minutes: Vec<u32>,
    pub relearning_steps_minutes: Vec<u32>,
    /// FSRS-6 weights w0–w20.
    pub weights: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PresetDto {
    pub id: String,
    pub name: String,
    pub version: u32,
    /// Built-in presets can be cloned and edited like any other.
    pub built_in: bool,
    /// Decks using this preset; a change applies to all of them.
    pub dependent_decks: u32,
    pub config: PresetConfigDto,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SavePresetInput {
    pub operation_id: String,
    /// `null` creates a preset (also used to clone one).
    pub preset_id: Option<String>,
    pub expected_version: Option<u32>,
    pub name: String,
    pub config: PresetConfigDto,
}

// ---------------------------------------------------------------- Phase 18: FSRS optimizer (1.5)

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetOptimizerInput {
    pub preset_id: String,
}

/// Review history of the preset's cards as the optimizer reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OptimizerDatasetDto {
    /// Every logged answer of the preset's cards.
    pub reviews: u32,
    /// Cards with a usable history.
    pub cards: u32,
    /// Answers the optimizer learns from and validates on (on a later study day).
    pub usable: u32,
    pub excluded_undone: u32,
    pub excluded_manual: u32,
    /// Answers of cards whose history does not start with the first learning answer.
    pub excluded_incomplete: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum OptimizerRunStatus {
    /// Result ready; the preset is unchanged.
    Proposed,
    Applied,
    /// Applied, then rolled back to the weights before it.
    Reverted,
}

/// One weight vector measured on the held-out cards, with its workload preview.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OptimizerMetricsDto {
    /// Mean log loss per held-out answer (lower is better).
    pub log_loss: f64,
    /// Calibration error of the predicted recall, 0–1 (lower is better).
    pub rmse: f64,
    /// Steady-state reviews per day of the current cards.
    pub daily_reviews: f64,
    pub median_interval_days: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OptimizerRunDto {
    pub run_id: String,
    pub status: OptimizerRunStatus,
    pub created_at: String,
    pub applied_at: Option<String>,
    pub reverted_at: Option<String>,
    /// Method and model the run used (provenance).
    pub optimizer_version: String,
    pub fsrs_version: String,
    pub dataset: OptimizerDatasetDto,
    pub training_reviews: u32,
    pub validation_reviews: u32,
    /// The preset's weights when the run started; a revert restores them.
    pub weights_before: Vec<f64>,
    pub weights_proposed: Vec<f64>,
    pub current: OptimizerMetricsDto,
    pub proposed: OptimizerMetricsDto,
    /// The proposal predicts the held-out answers better than the weights before.
    pub recommended: bool,
    /// Fewer usable answers than `recommendedReviews`.
    pub few_reviews: bool,
    /// Proposed and the preset still has `weightsBefore`.
    pub can_apply: bool,
    /// Applied and the preset still has `weightsProposed`.
    pub can_revert: bool,
}

// ------------------------------------------------------------------- grammar (since 1.6)

/// `getGrammar`: `deckId` picks the deck whose chapters are returned; `null` takes the first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetGrammarInput {
    #[serde(default)]
    pub deck_id: Option<String>,
}

/// A deck with grammar tips.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GrammarDeckSummaryDto {
    pub deck_id: String,
    pub name: String,
    pub target_language: TargetLanguage,
    pub level: Option<String>,
    pub tip_count: u32,
    /// Chapters that hold at least one tip.
    pub section_count: u32,
}

/// A grammar tip with the sentence it explains.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GrammarEntryDto {
    pub tip_id: String,
    pub text: String,
    pub note_id: String,
    /// Opens the card (first card of the note).
    pub card_id: String,
    /// The note's front (Markdown-light, as on the card) and back.
    pub front: String,
    pub back: String,
}

/// A chapter of the deck; `tag` and `title` are `null` for tips outside every chapter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GrammarSectionDto {
    pub tag: Option<String>,
    pub title: Option<String>,
    pub entries: Vec<GrammarEntryDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GrammarDeckDto {
    pub deck_id: String,
    pub name: String,
    pub target_language: TargetLanguage,
    pub level: Option<String>,
    /// Reading order of the deck's content.
    pub sections: Vec<GrammarSectionDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GrammarDto {
    /// Decks with tips, by name.
    pub decks: Vec<GrammarDeckSummaryDto>,
    /// The requested deck, or the first one; `null` when no deck has tips.
    pub deck: Option<GrammarDeckDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OptimizerDto {
    pub preset_id: String,
    pub preset_name: String,
    /// `expectedVersion` of apply and revert.
    pub preset_version: u32,
    pub desired_retention: f64,
    pub current_weights: Vec<f64>,
    pub default_weights: Vec<f64>,
    pub dataset: OptimizerDatasetDto,
    /// `startOptimizer` refuses below this many usable answers.
    pub minimum_reviews: u32,
    /// Heuristic: below this a result is shown with a warning.
    pub recommended_reviews: u32,
    /// Job of a run in progress for this preset.
    pub running_job_id: Option<String>,
    /// Newest first.
    pub runs: Vec<OptimizerRunDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartOptimizerInput {
    pub operation_id: String,
    pub preset_id: String,
}

/// Apply or revert one run at the preset version shown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OptimizerRunActionInput {
    pub operation_id: String,
    pub run_id: String,
    pub expected_version: u32,
}

// ---------------------------------------------------------------- Phase 11: details and editor

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NoteDto {
    pub id: String,
    pub guid: String,
    pub note_type_kind: NoteTypeKind,
    pub front: String,
    pub back: String,
    pub tags: Vec<String>,
    pub version: u32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewLogDto {
    pub id: String,
    pub reviewed_at: String,
    pub rating: Rating,
    pub state_before: CardState,
    pub interval_seconds: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RetrievabilityDto {
    pub value: f64,
    pub computed_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CardDetailsDto {
    pub id: String,
    pub note_id: String,
    /// The scheduling deck belongs to the card: cards of one note may sit in different decks.
    pub deck_id: String,
    pub deck_name: String,
    pub ordinal: u32,
    pub direction: CardDirection,
    pub state: CardState,
    pub queue: QueueState,
    pub flag: CardFlag,
    pub due_at: Option<String>,
    pub stability: Option<f64>,
    pub difficulty: Option<f64>,
    pub retrievability: Option<RetrievabilityDto>,
    pub reps: u32,
    pub lapses: u32,
    pub version: u32,
    pub history: Vec<ReviewLogDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NoteDetailsDto {
    pub note: NoteDto,
    /// Deck of the note's first card; decides which sections the details and the editor show.
    pub deck_id: String,
    pub target_language: TargetLanguage,
    pub capabilities: DeckCapabilitiesDto,
    /// Live cards; each has its own schedule.
    pub cards: Vec<CardDetailsDto>,
    /// Template ordinals archived by a note-type change; switching back restores them with
    /// history.
    pub archived_ordinals: Vec<u32>,
    pub associations: Vec<AssociationDto>,
    pub grammar_tips: Vec<GrammarTipDto>,
    pub word_links: Vec<WordLinkDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteNoteInput {
    pub operation_id: String,
    pub note_id: String,
    pub expected_version: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteNoteResultDto {
    pub deleted_cards: u32,
}

/// One edit of a note's tips, associations or word links (each change is its own transaction).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum NoteContentChange {
    AddTip {
        text: String,
    },
    EditTip {
        tip_id: String,
        text: String,
    },
    DeleteTip {
        tip_id: String,
    },
    /// The complete new order; any other set of ids is rejected.
    ReorderTips {
        tip_ids: Vec<String>,
    },
    AddAssociation {
        text: String,
    },
    /// Only the user's own (manual) associations can be edited.
    EditAssociation {
        association_id: String,
        text: String,
    },
    DeleteAssociation {
        association_id: String,
    },
    /// Removes the link; the word note itself is untouched.
    UnlinkWord {
        link_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateNoteContentInput {
    pub operation_id: String,
    pub note_id: String,
    pub change: NoteContentChange,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditorDeckOptionDto {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub deck_type: DeckType,
    pub capabilities: DeckCapabilitiesDto,
    pub target_language: TargetLanguage,
    /// Live word notes (non-sentence note types) in the deck; a word added from a sentence
    /// defaults to the deck of its language with the most. Since 1.8; a 1.7 payload without it
    /// deserializes as 0.
    #[serde(default)]
    pub word_count: u32,
}

/// Card template of a note type: which side is asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NoteTemplateDto {
    pub ordinal: u32,
    pub direction: CardDirection,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NoteTypeDto {
    pub kind: NoteTypeKind,
    pub templates: Vec<NoteTemplateDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditorDataDto {
    pub decks: Vec<EditorDeckOptionDto>,
    pub note_types: Vec<NoteTypeDto>,
    pub tags: Vec<String>,
    pub max_field_length: u32,
}

/// A word link as the editor sends it; ranges are UTF-16 offsets in the NFC front text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WordLinkDraftDto {
    pub start_index: u32,
    pub end_index: u32,
    pub surface: String,
    /// `null` keeps the phrase marked but unresolved; the backend never picks a word on its own.
    pub word_note_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveNoteInput {
    pub operation_id: String,
    /// `null` creates a note.
    pub note_id: Option<String>,
    pub expected_version: Option<u32>,
    pub deck_id: String,
    pub note_type_kind: NoteTypeKind,
    pub front: String,
    pub back: String,
    pub tags: Vec<String>,
    pub manual_association: Option<String>,
    /// Tips appended to the note; existing tips are edited on the card details.
    pub grammar_tips: Vec<String>,
    /// Complete set of word links for a sentence; `null` keeps the stored links.
    pub word_links: Option<Vec<WordLinkDraftDto>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchWordsInput {
    pub target_language: TargetLanguage,
    pub text: String,
    pub limit: u32,
}

/// `exact`: same text; `normalized`: same after case/article/diacritic folding. Never a guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum WordMatchKind {
    Exact,
    Normalized,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WordMatchDto {
    pub note_id: String,
    pub lemma: String,
    pub translation: String,
    pub deck_name: String,
    #[serde(rename = "match")]
    #[ts(rename = "match")]
    pub match_kind: WordMatchKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DuplicateCheckInput {
    pub deck_id: String,
    pub front: String,
    pub exclude_note_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DuplicateNoteDto {
    pub note_id: String,
    pub front: String,
    pub deck_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DuplicateCheckDto {
    pub duplicates: Vec<DuplicateNoteDto>,
}

// ---------------------------------------------------------------- Phase 11: browser

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum SortKey {
    Due,
    Created,
    Front,
    Deck,
    Reps,
    Lapses,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum SortDirection {
    Asc,
    Desc,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchInput {
    pub query: String,
    pub deck_id: Option<String>,
    pub tag: Option<String>,
    pub states: Vec<CardState>,
    pub flags: Vec<CardFlag>,
    pub sort: SortKey,
    pub direction: SortDirection,
    /// Opaque keyset cursor from a previous page.
    pub cursor: Option<String>,
    pub page_size: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CardRowDto {
    pub card_id: String,
    pub note_id: String,
    pub deck_id: String,
    pub deck_name: String,
    pub front: String,
    pub back: String,
    pub state: CardState,
    pub queue: QueueState,
    pub flag: CardFlag,
    pub due_at: Option<String>,
    /// Order of a new card in the new queue; `null` for cards already studied.
    pub position: Option<u32>,
    pub direction: CardDirection,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CardPageDto {
    pub items: Vec<CardRowDto>,
    pub next_cursor: Option<String>,
    pub total: u32,
    /// Snapshot token used by bulk actions over "all matches".
    pub filter_version: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum BulkSelection {
    Ids {
        card_ids: Vec<String>,
    },
    Query {
        filter_version: String,
        excluded_card_ids: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum BulkAction {
    Suspend,
    Unsuspend,
    Flag {
        flag: CardFlag,
    },
    ChangeDeck {
        deck_id: String,
    },
    AddTag {
        tag: String,
    },
    RemoveTag {
        tag: String,
    },
    /// New cards only: positions start, start+step, …; studied cards are skipped.
    Reposition {
        start: u32,
        step: u32,
    },
    Delete,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BulkActionInput {
    pub operation_id: String,
    pub selection: BulkSelection,
    pub action: BulkAction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BulkPreviewInput {
    pub selection: BulkSelection,
    pub action: BulkAction,
}

/// What a bulk action would do, shown before the user confirms it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BulkPreviewDto {
    /// Cards in the selection.
    pub selected: u32,
    /// Cards the action would change.
    pub affected: u32,
    /// Cards left as they are (already in that state, or not new for reposition).
    pub unchanged: u32,
    /// Notes and decks the affected cards belong to.
    pub notes: u32,
    pub decks: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BulkResultDto {
    pub affected: u32,
}

/// Renders every Rust-owned DTO as one TypeScript module.
pub fn typescript() -> String {
    let cfg = ts_rs::Config::new();
    let declarations = [
        crate::ErrorCode::decl(&cfg),
        ErrorDto::decl(&cfg),
        crate::AppInfoDto::decl(&cfg),
        JobKind::decl(&cfg),
        JobStatus::decl(&cfg),
        JobProgressDto::decl(&cfg),
        JobDto::decl(&cfg),
        JobEventDto::decl(&cfg),
        ImportRowErrorDto::decl(&cfg),
        ImportCountsDto::decl(&cfg),
        JobResultDto::decl(&cfg),
        CollectionScope::decl(&cfg),
        CollectionChangedEventDto::decl(&cfg),
        AboutDto::decl(&cfg),
        LogLevel::decl(&cfg),
        LogLineDto::decl(&cfg),
        DiagnosticsReportDto::decl(&cfg),
        ContentWarningCode::decl(&cfg),
        ContentWarningDto::decl(&cfg),
        NativeLanguage::decl(&cfg),
        TargetLanguage::decl(&cfg),
        DeckType::decl(&cfg),
        DeckCapabilitiesDto::decl(&cfg),
        DeckCountsDto::decl(&cfg),
        ContentState::decl(&cfg),
        DeckSummaryDto::decl(&cfg),
        ResumableStatus::decl(&cfg),
        ResumableSessionDto::decl(&cfg),
        DashboardDto::decl(&cfg),
        BundledDeckDto::decl(&cfg),
        OnboardingStep::decl(&cfg),
        OnboardingStateDto::decl(&cfg),
        CompleteOnboardingInput::decl(&cfg),
        AttributionScope::decl(&cfg),
        AttributionDto::decl(&cfg),
        FilePurpose::decl(&cfg),
        PickFileInput::decl(&cfg),
        FileHandleDto::decl(&cfg),
        NoteTypeKind::decl(&cfg),
        DeckContentKind::decl(&cfg),
        DeckImportOriginDto::decl(&cfg),
        DeckImportWarningDto::decl(&cfg),
        PreviewDeckImportInput::decl(&cfg),
        DeckImportPreviewDto::decl(&cfg),
        CommitDeckImportInput::decl(&cfg),
        CsvDelimiter::decl(&cfg),
        CsvEncoding::decl(&cfg),
        DuplicatePolicy::decl(&cfg),
        CsvColumnTarget::decl(&cfg),
        ImportInput::decl(&cfg),
        ImportPreviewDto::decl(&cfg),
        CommitImportInput::decl(&cfg),
        ExportFormat::decl(&cfg),
        ExportDeckInput::decl(&cfg),
        ExportLossDto::decl(&cfg),
        Rating::decl(&cfg),
        CardState::decl(&cfg),
        QueueState::decl(&cfg),
        CardFlag::decl(&cfg),
        CardDirection::decl(&cfg),
        AssociationProvenance::decl(&cfg),
        AssociationStatus::decl(&cfg),
        AssociationDto::decl(&cfg),
        GrammarTipDto::decl(&cfg),
        WordLinkResolution::decl(&cfg),
        WordCandidateDto::decl(&cfg),
        WordLinkDto::decl(&cfg),
        NoteContentDto::decl(&cfg),
        StudyCardDto::decl(&cfg),
        StudySessionStatus::decl(&cfg),
        StudySessionDto::decl(&cfg),
        StartStudyInput::decl(&cfg),
        StudySessionAction::decl(&cfg),
        UpdateStudySessionInput::decl(&cfg),
        PreviewInput::decl(&cfg),
        IntervalPreviewDto::decl(&cfg),
        RatingIntervalsDto::decl(&cfg),
        RatingPreviewDto::decl(&cfg),
        RateCardInput::decl(&cfg),
        ReviewedCardDto::decl(&cfg),
        ReviewResultDto::decl(&cfg),
        UndoInput::decl(&cfg),
        CardActionKind::decl(&cfg),
        CardActionInput::decl(&cfg),
        CardActionResultDto::decl(&cfg),
        SessionCardDto::decl(&cfg),
        NextDeckDto::decl(&cfg),
        RatingCountsDto::decl(&cfg),
        SessionSummaryDto::decl(&cfg),
        SetFavoriteAssociationInput::decl(&cfg),
        GenerateAssociationInput::decl(&cfg),
        SetAiApiKeyInput::decl(&cfg),
        ResolveWordLinkInput::decl(&cfg),
        HiddenSectionCountsDto::decl(&cfg),
        DeckTypeOptionDto::decl(&cfg),
        DeckOverviewDto::decl(&cfg),
        CreateDeckInput::decl(&cfg),
        DeckPatchDto::decl(&cfg),
        UpdateDeckInput::decl(&cfg),
        DeleteDeckInput::decl(&cfg),
        DeleteDeckResultDto::decl(&cfg),
        PresetConfigDto::decl(&cfg),
        PresetDto::decl(&cfg),
        GetOptimizerInput::decl(&cfg),
        OptimizerDatasetDto::decl(&cfg),
        OptimizerRunStatus::decl(&cfg),
        OptimizerMetricsDto::decl(&cfg),
        OptimizerRunDto::decl(&cfg),
        OptimizerDto::decl(&cfg),
        GetGrammarInput::decl(&cfg),
        GrammarDeckSummaryDto::decl(&cfg),
        GrammarEntryDto::decl(&cfg),
        GrammarSectionDto::decl(&cfg),
        GrammarDeckDto::decl(&cfg),
        GrammarDto::decl(&cfg),
        StartOptimizerInput::decl(&cfg),
        OptimizerRunActionInput::decl(&cfg),
        SavePresetInput::decl(&cfg),
        NoteDto::decl(&cfg),
        ReviewLogDto::decl(&cfg),
        RetrievabilityDto::decl(&cfg),
        CardDetailsDto::decl(&cfg),
        NoteDetailsDto::decl(&cfg),
        DeleteNoteInput::decl(&cfg),
        DeleteNoteResultDto::decl(&cfg),
        NoteContentChange::decl(&cfg),
        UpdateNoteContentInput::decl(&cfg),
        EditorDeckOptionDto::decl(&cfg),
        NoteTemplateDto::decl(&cfg),
        NoteTypeDto::decl(&cfg),
        EditorDataDto::decl(&cfg),
        WordLinkDraftDto::decl(&cfg),
        SaveNoteInput::decl(&cfg),
        SearchWordsInput::decl(&cfg),
        WordMatchKind::decl(&cfg),
        WordMatchDto::decl(&cfg),
        DuplicateCheckInput::decl(&cfg),
        DuplicateNoteDto::decl(&cfg),
        DuplicateCheckDto::decl(&cfg),
        SortKey::decl(&cfg),
        SortDirection::decl(&cfg),
        SearchInput::decl(&cfg),
        CardRowDto::decl(&cfg),
        CardPageDto::decl(&cfg),
        BulkSelection::decl(&cfg),
        BulkAction::decl(&cfg),
        BulkActionInput::decl(&cfg),
        BulkPreviewInput::decl(&cfg),
        BulkPreviewDto::decl(&cfg),
        BulkResultDto::decl(&cfg),
        StatsRange::decl(&cfg),
        StatsInput::decl(&cfg),
        RatioDto::decl(&cfg),
        ReviewDayDto::decl(&cfg),
        ForecastDayDto::decl(&cfg),
        ForecastAssumptionsDto::decl(&cfg),
        CardStatesDto::decl(&cfg),
        MaturityDto::decl(&cfg),
        TrueRetentionDto::decl(&cfg),
        StabilityBucketDto::decl(&cfg),
        DifficultyStabilityPointDto::decl(&cfg),
        StatsDto::decl(&cfg),
        UiLanguage::decl(&cfg),
        ThemePreference::decl(&cfg),
        PermissionState::decl(&cfg),
        ConsentState::decl(&cfg),
        AiProviderOptionDto::decl(&cfg),
        AiQuotaDto::decl(&cfg),
        AiSettingsDto::decl(&cfg),
        BackupLocation::decl(&cfg),
        BackupSettingsDto::decl(&cfg),
        SettingsDto::decl(&cfg),
        SettingsPatch::decl(&cfg),
        UpdateSettingsInput::decl(&cfg),
        ResetScope::decl(&cfg),
        ResetPreviewInput::decl(&cfg),
        CollectionCountsDto::decl(&cfg),
        ResetPreviewDto::decl(&cfg),
        ResetInput::decl(&cfg),
        BackupKind::decl(&cfg),
        BackupDto::decl(&cfg),
        BackupInput::decl(&cfg),
        ShareTarget::decl(&cfg),
        ShareFileInput::decl(&cfg),
        ShareResultDto::decl(&cfg),
        RestoreSource::decl(&cfg),
        ValidateRestoreInput::decl(&cfg),
        RestorePreviewDto::decl(&cfg),
        RestoreInput::decl(&cfg),
    ];
    let mut out = String::from(
        "// Generated from crates/contracts (Rust is the DTO source, plan Phase 7 step 28).\n\
         // Do not edit: change the Rust type and run\n\
         //   UPDATE_CONTRACT=1 cargo test -p langualinker-contracts --test generated_typescript\n\n",
    );
    for declaration in declarations {
        out.push_str("export ");
        out.push_str(declaration.trim());
        out.push_str("\n\n");
    }
    out.truncate(out.trim_end().len());
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use serde::de::DeserializeOwned;
    use serde_json::{Value, json};

    use super::*;

    fn round_trip<T>(value: &T) -> Value
    where
        T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug,
    {
        let json = serde_json::to_value(value).unwrap();
        let back: T = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(&back, value);
        json
    }

    fn job_event() -> JobEventDto {
        JobEventDto {
            job_id: "12".into(),
            kind: JobKind::PrepareContent,
            sequence: 3,
            status: JobStatus::Running,
            progress: Some(JobProgressDto { done: 1, total: 4 }),
        }
    }

    #[test]
    fn enums_serialize_in_camel_case() {
        assert_eq!(json!(JobKind::PrepareContent), json!("prepareContent"));
        assert_eq!(json!(JobStatus::Cancelled), json!("cancelled"));
        assert_eq!(json!(CollectionScope::Backups), json!("backups"));
        assert_eq!(json!(LogLevel::Warn), json!("warn"));
    }

    #[test]
    fn job_event_uses_camel_case_fields() {
        assert_eq!(
            round_trip(&job_event()),
            json!({
                "jobId": "12",
                "kind": "prepareContent",
                "sequence": 3,
                "status": "running",
                "progress": { "done": 1, "total": 4 },
            })
        );
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let mut value = serde_json::to_value(job_event()).unwrap();
        value["extra"] = json!(1);
        assert!(serde_json::from_value::<JobEventDto>(value).is_err());
        assert!(
            serde_json::from_value::<JobProgressDto>(json!({"done": 1, "total": 2, "x": 0}))
                .is_err()
        );
    }

    #[test]
    fn integers_beyond_u32_are_rejected() {
        let too_big = json!({ "done": 4_294_967_296u64, "total": 1 });
        assert!(serde_json::from_value::<JobProgressDto>(too_big).is_err());
    }

    #[test]
    fn job_without_optional_parts_serializes_nulls() {
        let job = JobDto {
            job_id: "1".into(),
            kind: JobKind::Backup,
            status: JobStatus::Queued,
            sequence: 0,
            progress: None,
            started_at: "2026-03-02T08:00:00Z".into(),
            finished_at: None,
            error: None,
            result: None,
        };
        let json = round_trip(&job);
        assert_eq!(json["progress"], Value::Null);
        assert_eq!(json["finishedAt"], Value::Null);
        assert_eq!(json["result"], Value::Null);
        assert_eq!(json["startedAt"], "2026-03-02T08:00:00Z");
    }

    #[test]
    fn job_result_is_tagged_by_type() {
        let import = JobResultDto::Import {
            deck_id: "5".into(),
            row_errors: Vec::new(),
            create: 1,
            update: 2,
            skip: 3,
            duplicate: 4,
            error: 5,
        };
        assert_eq!(
            round_trip(&import),
            json!({
                "type": "import",
                "deckId": "5",
                "rowErrors": [],
                "create": 1,
                "update": 2,
                "skip": 3,
                "duplicate": 4,
                "error": 5,
            })
        );
        assert_eq!(
            round_trip(&JobResultDto::Export {
                file_name: "deck.csv".into(),
                card_count: 9,
                format: Some(ExportFormat::Csv),
                losses: vec![ExportLossDto {
                    code: "reviewHistory".into(),
                    count: 3,
                }],
            }),
            json!({
                "type": "export", "fileName": "deck.csv", "cardCount": 9, "format": "csv",
                "losses": [{ "code": "reviewHistory", "count": 3 }]
            })
        );
        // A 1.2 result still reads.
        let old: JobResultDto = serde_json::from_value(
            json!({ "type": "export", "fileName": "d.csv", "cardCount": 1 }),
        )
        .unwrap();
        assert_eq!(
            old,
            JobResultDto::Export {
                file_name: "d.csv".into(),
                card_count: 1,
                format: None,
                losses: Vec::new(),
            }
        );
        assert_eq!(
            round_trip(&JobResultDto::PrepareContent {
                deck_ids: vec!["1".into()],
                created: 40,
                updated: 2,
                unchanged: 1,
                kept: 1,
                archived: 0,
                warnings: vec![ContentWarningDto {
                    code: ContentWarningCode::OverrideConflict,
                    deck_guid: "bundled-de-words-a1".into(),
                    note_guid: Some("dew-1".into()),
                    field: Some("back".into()),
                }],
            }),
            json!({
                "type": "prepareContent", "deckIds": ["1"], "created": 40, "updated": 2,
                "unchanged": 1, "kept": 1, "archived": 0,
                "warnings": [{ "code": "overrideConflict", "deckGuid": "bundled-de-words-a1",
                    "noteGuid": "dew-1", "field": "back" }]
            })
        );
        // A 1.0 result (deck ids only) still reads.
        let old: JobResultDto =
            serde_json::from_value(json!({ "type": "prepareContent", "deckIds": [] })).unwrap();
        assert!(matches!(
            old,
            JobResultDto::PrepareContent { created: 0, .. }
        ));
    }

    #[test]
    fn phase8_enums_use_the_typescript_spelling() {
        assert_eq!(json!(TargetLanguage::De), json!("de"));
        assert_eq!(json!(NativeLanguage::Pl), json!("pl"));
        assert_eq!(
            json!(DeckType::TextWithAssociations),
            json!("TextWithAssociations")
        );
        assert_eq!(json!(ContentState::Preparing), json!("preparing"));
        assert_eq!(
            json!(OnboardingStep::NativeLanguage),
            json!("nativeLanguage")
        );
        assert_eq!(json!(AttributionScope::Content), json!("content"));
        assert_eq!(json!(ResumableStatus::Paused), json!("paused"));
    }

    #[test]
    fn every_result_variant_round_trips() {
        for result in [
            JobResultDto::Backup {
                backup_id: "b".into(),
            },
            JobResultDto::Restore {
                safety_backup_id: "s".into(),
            },
            JobResultDto::Reset {
                safety_backup_id: None,
            },
            JobResultDto::Reset {
                safety_backup_id: Some("s".into()),
            },
        ] {
            round_trip(&result);
        }
    }

    #[test]
    fn unknown_result_type_is_rejected() {
        assert!(serde_json::from_value::<JobResultDto>(json!({ "type": "nuke" })).is_err());
    }

    #[test]
    fn collection_changed_event_round_trips() {
        let event = CollectionChangedEventDto {
            scopes: vec![CollectionScope::Decks, CollectionScope::Stats],
            changed_at: "2026-03-02T08:00:00Z".into(),
        };
        assert_eq!(
            round_trip(&event),
            json!({ "scopes": ["decks", "stats"], "changedAt": "2026-03-02T08:00:00Z" })
        );
    }

    #[test]
    fn about_allows_a_missing_schema_version() {
        let about = AboutDto {
            app_version: "0.1.0".into(),
            api_version: "1".into(),
            schema_version: None,
            fsrs_version: "5".into(),
            content_version: "c1".into(),
            platform: "windows".into(),
        };
        let json = round_trip(&about);
        assert_eq!(json["schemaVersion"], Value::Null);
        assert_eq!(json["appVersion"], "0.1.0");
    }

    #[test]
    fn diagnostics_report_round_trips() {
        let report = DiagnosticsReportDto {
            generated_at: "2026-03-02T08:00:00Z".into(),
            lines: vec![LogLineDto {
                at: "2026-03-02T07:59:00Z".into(),
                level: LogLevel::Info,
                target: "jobs".into(),
                message: "started".into(),
            }],
            redactions: 2,
        };
        let json = round_trip(&report);
        assert_eq!(json["generatedAt"], "2026-03-02T08:00:00Z");
        assert_eq!(json["lines"][0]["level"], "info");
    }

    #[test]
    fn typescript_declares_every_dto_once() {
        let ts = typescript();
        for name in [
            "ErrorCode",
            "ErrorDto",
            "AppInfoDto",
            "JobKind",
            "JobStatus",
            "JobProgressDto",
            "JobDto",
            "JobEventDto",
            "ImportRowErrorDto",
            "ImportCountsDto",
            "JobResultDto",
            "CollectionScope",
            "CollectionChangedEventDto",
            "AboutDto",
            "LogLevel",
            "LogLineDto",
            "DiagnosticsReportDto",
        ] {
            let declaration = format!("export type {name} =");
            assert_eq!(ts.matches(&declaration).count(), 1, "{name}");
        }
        assert!(ts.ends_with("\n") && !ts.ends_with("\n\n"));
    }

    #[test]
    fn study_dtos_use_the_typescript_spelling() {
        assert_eq!(json!(Rating::Again), json!("Again"));
        assert_eq!(json!(QueueState::SchedBuried), json!("SchedBuried"));
        assert_eq!(json!(CardFlag::Turquoise), json!("turquoise"));
        assert_eq!(json!(CardDirection::Reverse), json!("reverse"));
        assert_eq!(
            json!(AssociationStatus::OfflineQueued),
            json!("offlineQueued")
        );
        assert_eq!(
            json!(WordLinkResolution::UserOverridden),
            json!("userOverridden")
        );
        let counts = RatingCountsDto {
            again: 1,
            hard: 2,
            good: 3,
            easy: 4,
        };
        assert_eq!(
            round_trip(&counts),
            json!({ "Again": 1, "Hard": 2, "Good": 3, "Easy": 4 })
        );
    }

    #[test]
    fn optional_inputs_may_be_omitted() {
        let start: StartStudyInput =
            serde_json::from_value(json!({ "operationId": "op", "deckId": null })).unwrap();
        assert_eq!(start.card_ids, None);
        assert!(
            !serde_json::to_value(&start)
                .unwrap()
                .as_object()
                .unwrap()
                .contains_key("cardIds")
        );
        let action: CardActionInput = serde_json::from_value(json!({
            "operationId": "op", "cardId": "1", "expectedVersion": 2, "action": "flag", "flag": "red"
        }))
        .unwrap();
        assert_eq!(action.flag, Some(CardFlag::Red));
        assert!(
            serde_json::from_value::<RateCardInput>(json!({
                "operationId": "op", "sessionId": "1", "cardId": "1", "cardVersion": 1,
                "previewToken": "t", "rating": "Good", "answerMs": -1
            }))
            .is_err(),
            "negative answer time is rejected by the shape"
        );
    }

    #[test]
    fn typescript_has_no_bigint() {
        assert!(!typescript().contains("bigint"));
    }
}
