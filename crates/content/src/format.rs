//! Bundled-content format, schema version 2 (plan Phase 8 steps 3–4, ADR-14).
//!
//! A content directory holds `catalog.json` (decks in import order), one JSON manifest per deck
//! and one NDJSON file of note records per deck. Every type rejects unknown fields, so a file
//! written for a newer schema fails loudly instead of losing data silently.
//!
//! An **import package** (`source: "import"`, e.g. a deck converted from Anki by `apkg-convert`)
//! is one NDJSON file: the first line is `{"package": 1, "manifest": {...}}`, the other lines are
//! note records. Only packages may carry scheduling state (`cards`) and review history
//! (`reviews`); bundled content always starts new.

use serde::{Deserialize, Serialize};

/// The only schema version this build reads.
pub const SCHEMA_VERSION: u32 = 2;

/// File name of the catalog inside a content directory.
pub const CATALOG_FILE: &str = "catalog.json";

/// Longest accepted NDJSON line; a longer one is rejected before it is parsed.
pub const MAX_LINE_BYTES: usize = 64 * 1024;

/// Longest GUID of a bundled note; leaves room for derived ids within the 64-character limit.
pub const MAX_BUNDLED_GUID_CHARS: usize = 48;

/// Longest association or grammar tip, equal to the editor's `maxlength` (GUI limit).
pub const MAX_SNIPPET_CHARS: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Catalog {
    pub schema_version: u32,
    /// Manifest file stems in import order: word decks before the sentence decks that link to
    /// them.
    pub decks: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContentKind {
    /// Vocabulary: every note needs an association (plan step 15).
    Words,
    /// Sentences: every note links at least one word; tips are optional.
    Sentences,
}

/// Where a deck comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SourceKind {
    /// Shipped with the app; new for every learner.
    #[default]
    Bundled,
    /// The learner's own deck (an import package), possibly with progress from another app.
    Import,
}

/// Provenance of an import package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Origin {
    /// `apkg` for Anki exports.
    pub kind: String,
    /// Name of the converted file (no directories).
    pub file_name: String,
    /// `YYYY-MM-DD`.
    pub converted_at: String,
    /// Converter and its version, e.g. `apkg-convert 0.1.0`.
    pub tool: String,
    /// Anki decks the notes came from.
    #[serde(default)]
    pub source_decks: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReviewStatus {
    /// Candidates; usable in development and tests, blocked for release builds.
    Pending,
    /// Every record was reviewed by a native speaker.
    Approved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct License {
    /// Shown on the Licences screen.
    pub name: String,
    pub attribution: Option<String>,
    pub url: Option<String>,
    /// The rights audit (D-04) confirmed the deck may ship.
    pub rights_confirmed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Review {
    pub status: ReviewStatus,
    pub reviewer: Option<String>,
    /// `YYYY-MM-DD`.
    pub reviewed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeckManifest {
    pub schema_version: u32,
    /// `bundled` when absent.
    #[serde(default, skip_serializing_if = "is_bundled")]
    pub source: SourceKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<Origin>,
    pub deck_guid: String,
    /// `YYYY.MM.DD`; compared as text, so a later release always sorts after an earlier one.
    pub content_version: String,
    pub name: String,
    pub description: String,
    /// CEFR level, `A1`–`C2`.
    pub level: String,
    /// L1, `pl` in v1.
    pub native_language: String,
    /// L2, `de` or `en`.
    pub target_language: String,
    /// `Simple` | `TextWithAssociations` | `Linguistic` (contract `DeckType`).
    pub deck_type: String,
    /// `Basic` | `BasicAndReversed` | `Sentence` | `SentenceAndReversed` (contract `NoteTypeKind`).
    pub note_type: String,
    pub content_kind: ContentKind,
    /// Deck GUIDs whose notes this deck links to; they come earlier in the catalog.
    #[serde(default)]
    pub depends_on: Vec<String>,
    pub notes_file: String,
    /// SHA-256 (lowercase hex) of the notes file.
    pub sha256: String,
    pub expected_notes: u32,
    /// Generated cards: notes × templates of the note type (plan step 16).
    pub expected_cards: u32,
    pub license: License,
    pub review: Review,
}

fn is_bundled(source: &SourceKind) -> bool {
    *source == SourceKind::Bundled
}

/// One line of a notes file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NoteRecord {
    /// Note GUID; cards get their own GUIDs when they are generated.
    pub guid: String,
    pub front: String,
    pub back: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub associations: Vec<AssociationRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grammar_tips: Vec<TipRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub word_links: Vec<WordLinkRecord>,
    /// Scheduling state per template (import packages only); a template without an entry starts
    /// new.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cards: Vec<CardRecord>,
    /// Review history, oldest first (import packages only).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reviews: Vec<ReviewRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CardStateName {
    New,
    Learning,
    Review,
    Relearning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum QueueName {
    New,
    Learning,
    Review,
    DayLearn,
    Suspended,
    SchedBuried,
    UserBuried,
}

/// Scheduling state of one card of the note.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CardRecord {
    /// Template ordinal (0 forward, 1 reverse).
    pub ordinal: u32,
    pub state: CardStateName,
    pub queue: QueueName,
    /// UTC epoch milliseconds; `null` exactly when the state is `new`.
    pub due_at_ms: Option<i64>,
    /// Introduction order among new cards (relative within the package).
    pub position: Option<i64>,
    /// Current interval in days (0 while learning).
    pub scheduled_days: i64,
    /// FSRS memory state, when the source scheduled with FSRS.
    pub stability: Option<f64>,
    pub difficulty: Option<f64>,
    /// SM-2 ease in permille (Anki `factor`), kept for interoperability.
    pub ease: Option<i64>,
    pub reps: i64,
    pub lapses: i64,
    pub last_review_at_ms: Option<i64>,
    /// 0 none, 1 red … 7 purple (Anki flag numbers).
    pub flag: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReviewKindName {
    Learning,
    Review,
    Relearning,
    Filtered,
    Manual,
}

/// One past review of a card of the note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewRecord {
    pub ordinal: u32,
    /// UTC epoch milliseconds.
    pub reviewed_at_ms: i64,
    /// 1 Again, 2 Hard, 3 Good, 4 Easy.
    pub rating: u8,
    pub kind: ReviewKindName,
    /// Interval given by this review in days (0 for an intraday step).
    pub scheduled_days: i64,
    /// SM-2 ease after the review, in permille.
    pub ease: Option<i64>,
    pub duration_ms: i64,
}

/// First line of an import package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PackageHeader {
    /// Package layout version, [`PACKAGE_VERSION`].
    pub package: u32,
    pub manifest: DeckManifest,
}

/// The only package layout this build reads.
pub const PACKAGE_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssociationRecord {
    pub guid: String,
    pub text: String,
    /// Id of the prompt in `content/tools/prompts` that produced the candidate; `None` when a
    /// person wrote it.
    pub prompt_version: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TipRecord {
    pub guid: String,
    pub text: String,
}

/// A phrase of the sentence front linked to a word note of a dependency deck.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WordLinkRecord {
    /// UTF-16 code units over the NFC front, half-open (plan §14.1).
    pub start: u32,
    pub end: u32,
    pub surface_form: String,
    /// Dictionary form of the linked word (multiword units allowed: `in das` for `ins`).
    pub lemma: String,
    /// GUID of the word note; homographs are told apart by GUID, never by text.
    pub word_guid: String,
}
