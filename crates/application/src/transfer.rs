//! CSV import and deck export (plan Phase 14 steps 1–3, ADR-19).
//!
//! **CSV import** runs in two steps. [`read_csv`] decodes and parses the file; [`plan_csv`] maps
//! every row, validates it and decides what happens to it (create, update, skip, duplicate,
//! error) without writing — the preview shows exactly these numbers. [`apply_csv`] runs the same
//! plan inside one write transaction: a cancel or an error rolls the whole import back, so there
//! is no half-imported deck and no staging table to clean up.
//!
//! - Matching: a GUID column matches the note with that GUID anywhere in the collection; without
//!   one a row matches a note of its deck (with subdecks) whose front is equal after trimming, NFC
//!   and case folding — the editor's duplicate rule. Rows of the same file match each other too.
//! - Policies: `skip` leaves the match alone; `update` writes the row's back (and front for a
//!   GUID match) and, when mapped, its tags — scheduling and note type stay; `duplicate` creates a
//!   new note anyway (with a new GUID when the GUID matched).
//! - Scheduling columns apply only to notes the import creates, and only to the forward card. A
//!   `due` (ISO 8601; a date means the start of that study day) makes the card a review card; an
//!   empty `due` keeps it new, and `position` orders it after the existing new cards. Basic CSV
//!   carries no FSRS memory and no history.
//! - GUIDs that are not ours (Anki's base91 GUIDs) are mapped to a stable derived GUID, so a
//!   second import of the same file updates instead of duplicating.
//!
//! **Export** reads one consistent snapshot of a deck and its subdecks ([`export_snapshot`]); the
//! CSV writer is here ([`csv_export`]), the `.apkg` writer in the infrastructure. Both report
//! what the format cannot carry ([`Losses`]).

use std::collections::{BTreeSet, HashMap, HashSet};

use chrono::{DateTime, NaiveDate, NaiveDateTime};
use langualinker_domain::calendar::{StudyCalendar, StudyDay};
use langualinker_domain::card::{Card, CardFlag, CardState, NewCard, QueueState};
use langualinker_domain::deck::{Deck, DeckType, NativeLanguage, NewDeck, TargetLanguage};
use langualinker_domain::note::{NewNote, Note, NoteFields, NoteTypeKind};
use langualinker_domain::review::ReviewLog;
use langualinker_domain::text::nfc;
use langualinker_domain::{
    DeckId, Guid, NoteId, NoteTypeId, TimestampMs, ValidationError, validation,
};
use langualinker_interop::anki::{Loss, Losses};
use langualinker_interop::csv::{self, CsvError, Delimiter, Encoding, Limits, Record};
use sha2::{Digest, Sha256};

use crate::jobs::CancelToken;
use crate::management;
use crate::ports::Rng;
use crate::repositories::Repositories;
use crate::{AppError, AppResult};

/// Largest file the importer reads.
pub const MAX_CSV_BYTES: usize = 32 * 1024 * 1024;
/// Rows kept in a preview or result; the rest are only counted.
pub const MAX_ROW_ERRORS: usize = 200;
/// Rows shown as samples in the preview.
pub const SAMPLE_ROWS: usize = 5;
/// Rows between two cancel checks inside the import transaction.
const CANCEL_EVERY: usize = 100;
const DAY_MS: i64 = 86_400_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CsvTarget {
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

impl CsvTarget {
    pub const ALL: [Self; 12] = [
        Self::Front,
        Self::Back,
        Self::Tags,
        Self::Ignore,
        Self::Guid,
        Self::Deck,
        Self::NoteType,
        Self::Due,
        Self::Interval,
        Self::Reps,
        Self::Lapses,
        Self::Position,
    ];

    /// Header names (lower case) suggesting this target.
    fn header_names(self) -> &'static [&'static str] {
        match self {
            Self::Front => &[
                "front",
                "przód",
                "przod",
                "vorderseite",
                "question",
                "pytanie",
                "word",
                "słowo",
            ],
            Self::Back => &[
                "back",
                "tył",
                "tyl",
                "rückseite",
                "answer",
                "odpowiedź",
                "translation",
                "tłumaczenie",
            ],
            Self::Tags => &["tags", "tagi"],
            Self::Ignore => &[],
            Self::Guid => &["guid"],
            Self::Deck => &["deck", "talia"],
            Self::NoteType => &["notetype", "note type", "typ notatki"],
            Self::Due => &["due", "termin"],
            Self::Interval => &["interval", "ivl"],
            Self::Reps => &["reps"],
            Self::Lapses => &["lapses"],
            Self::Position => &["position", "pozycja"],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DuplicatePolicy {
    Skip,
    Update,
    Duplicate,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CsvOptions {
    /// `None` = detect.
    pub delimiter: Option<Delimiter>,
    /// `None` = detect.
    pub encoding: Option<Encoding>,
    pub has_header: bool,
    /// `None` = suggested mapping.
    pub mapping: Option<Vec<CsvTarget>>,
    pub duplicate_policy: DuplicatePolicy,
    /// `None` = a new deck named after the file.
    pub target_deck: Option<DeckId>,
    /// Removes the `'` a guarded export put before `=`, `+`, `-`, `@`.
    pub formula_guard: bool,
}

/// A decoded and parsed file.
#[derive(Debug, Clone, PartialEq)]
pub struct CsvFile {
    pub file_name: String,
    pub encoding: Encoding,
    pub has_bom: bool,
    pub detected: Option<Delimiter>,
    pub delimiter: Delimiter,
    /// Header names (from the first row or an Anki `#columns:` line).
    pub header: Option<Vec<String>>,
    /// Data rows.
    pub records: Vec<Record>,
    pub columns: usize,
    /// Fields are HTML (Anki `#html:true`).
    pub html: bool,
}

fn file_error(error: CsvError) -> AppError {
    ValidationError::new("file", error.code()).into()
}

/// Decodes and parses an import file (no collection access).
pub fn read_csv(file_name: &str, bytes: &[u8], options: &CsvOptions) -> AppResult<CsvFile> {
    if bytes.len() > MAX_CSV_BYTES {
        return Err(ValidationError::new("file", "tooLarge").into());
    }
    if bytes.starts_with(b"PK\x03\x04") {
        return Err(AppError::UnsupportedFormat { format: "apkg" });
    }
    if bytes.starts_with(b"SQLite format 3\0") || bytes.starts_with(b"LLBK") {
        return Err(AppError::UnsupportedFormat { format: "unknown" });
    }
    let decoded = csv::decode(bytes, options.encoding).map_err(file_error)?;
    if decoded.text.contains('\0') {
        return Err(AppError::UnsupportedFormat { format: "unknown" });
    }
    let directives = csv::directives(&decoded.text);
    let detected = directives
        .separator
        .or_else(|| csv::detect_delimiter(&decoded.text, directives.lines));
    let delimiter = options.delimiter.or(detected).unwrap_or(Delimiter::Comma);
    let mut records = csv::parse(
        &decoded.text,
        directives.lines,
        delimiter,
        Limits::default(),
    )
    .map_err(file_error)?;
    let header = match directives.columns {
        Some(columns) => Some(columns),
        None if options.has_header => Some(records.remove(0).fields),
        None => None,
    };
    if records.is_empty() {
        return Err(file_error(CsvError::Empty));
    }
    let columns = records
        .iter()
        .map(|record| record.fields.len())
        .chain(header.iter().map(Vec::len))
        .max()
        .unwrap_or(0);
    Ok(CsvFile {
        file_name: file_name.to_owned(),
        encoding: decoded.encoding,
        has_bom: decoded.has_bom,
        detected,
        delimiter,
        header,
        records,
        columns,
        html: directives.html == Some(true),
    })
}

/// Mapping from header names, else front, back, tags by position.
pub fn suggest_mapping(header: Option<&[String]>, columns: usize) -> Vec<CsvTarget> {
    let mut mapping = vec![CsvTarget::Ignore; columns];
    let mut used: HashSet<CsvTarget> = HashSet::new();
    if let Some(header) = header {
        for (index, name) in header.iter().enumerate().take(columns) {
            let name = name.trim().to_lowercase();
            if let Some(target) = CsvTarget::ALL.into_iter().find(|target| {
                !used.contains(target) && target.header_names().contains(&name.as_str())
            }) {
                mapping[index] = target;
                used.insert(target);
            }
        }
    }
    for target in [CsvTarget::Front, CsvTarget::Back] {
        if !used.contains(&target)
            && let Some(free) = mapping
                .iter()
                .position(|current| *current == CsvTarget::Ignore)
        {
            mapping[free] = target;
            used.insert(target);
        }
    }
    if header.is_none() && columns >= 3 && mapping[2] == CsvTarget::Ignore {
        mapping[2] = CsvTarget::Tags;
    }
    mapping
}

/// Checks a mapping chosen in the UI.
pub fn check_mapping(mapping: &[CsvTarget], columns: usize) -> AppResult<()> {
    if mapping.len() != columns {
        return Err(ValidationError::new("mapping", "length").into());
    }
    for target in CsvTarget::ALL {
        if target == CsvTarget::Ignore {
            continue;
        }
        let count = mapping.iter().filter(|current| **current == target).count();
        let required = matches!(target, CsvTarget::Front | CsvTarget::Back);
        if (required && count != 1) || count > 1 {
            return Err(ValidationError::new(
                "mapping",
                if required {
                    "frontBack"
                } else {
                    "duplicateTarget"
                },
            )
            .into());
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- plan

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImportCounts {
    pub create: u32,
    pub update: u32,
    pub skip: u32,
    pub duplicate: u32,
    pub error: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowError {
    pub line: u32,
    pub error: ValidationError,
}

/// Where a row's note goes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum DeckKey {
    Existing(DeckId),
    /// A deck the import creates: its path (levels joined with `::`), case-folded for matching.
    New(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NoteRef {
    Existing(NoteId),
    /// A note created by an earlier row of the file.
    Planned(usize),
}

#[derive(Debug, Clone, PartialEq)]
enum Action {
    Create { duplicate: bool },
    Update { note: NoteRef, by_guid: bool },
    Skip,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct RowSchedule {
    due: Option<TimestampMs>,
    interval: Option<i64>,
    reps: Option<i64>,
    lapses: Option<i64>,
    position: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
struct PlannedRow {
    action: Action,
    deck: DeckKey,
    kind: NoteTypeKind,
    guid: Option<Guid>,
    front: String,
    back: String,
    /// `None` when no column is mapped to tags.
    tags: Option<Vec<String>>,
    schedule: RowSchedule,
}

/// A deck the import creates; `path` keeps the file's spelling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewDeckPath {
    pub path: Vec<String>,
}

/// What an import will do; computed without writing (preview) and again inside the commit.
#[derive(Debug, Clone, PartialEq)]
pub struct CsvPlan {
    pub mapping: Vec<CsvTarget>,
    pub counts: ImportCounts,
    /// The first [`MAX_ROW_ERRORS`] errors; `counts.error` has the total.
    pub row_errors: Vec<RowError>,
    pub target_deck_id: Option<DeckId>,
    pub target_deck_name: String,
    /// Decks the import creates (full paths, `::` between levels), the target first.
    pub new_decks: Vec<String>,
    rows: Vec<PlannedRow>,
    target: DeckKey,
    new_paths: Vec<NewDeckPath>,
}

/// Key of the new deck named after the file (never equal to a lower-cased path).
const NEW_TARGET: &str = "\u{0}target";

/// The duplicate key of a front: trimmed, NFC, lower case (`management::duplicates`).
fn front_key(front: &str) -> String {
    nfc(front).trim().to_lowercase()
}

/// Our GUID for a GUID column value: kept when valid, else derived (stable across imports).
fn row_guid(value: &str) -> Guid {
    Guid::parse(value).unwrap_or_else(|_| {
        let digest = Sha256::digest(format!("csv-guid:{value}").as_bytes());
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&digest[..16]);
        Guid::from_bytes(bytes)
    })
}

pub fn note_type_code(kind: NoteTypeKind) -> &'static str {
    match kind {
        NoteTypeKind::Basic => "basic",
        NoteTypeKind::BasicAndReversed => "basicAndReversed",
        NoteTypeKind::Sentence => "sentence",
        NoteTypeKind::SentenceAndReversed => "sentenceAndReversed",
    }
}

fn parse_note_type(value: &str) -> Option<NoteTypeKind> {
    let folded: String = value
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect::<String>()
        .to_lowercase();
    match folded.as_str() {
        "basic" => Some(NoteTypeKind::Basic),
        "basicandreversed" | "basicandreversedcard" | "basicreversed" | "reversed" => {
            Some(NoteTypeKind::BasicAndReversed)
        }
        "sentence" => Some(NoteTypeKind::Sentence),
        "sentenceandreversed" | "sentencereversed" => Some(NoteTypeKind::SentenceAndReversed),
        _ => None,
    }
}

/// ISO 8601: a date (start of that study day) or a date-time with `Z`/offset (a date-time
/// without one is read as UTC).
pub fn parse_due(value: &str, calendar: &StudyCalendar) -> Option<TimestampMs> {
    let value = value.trim();
    let epoch = NaiveDate::from_ymd_opt(1970, 1, 1)?;
    let at = if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        calendar.day_start(StudyDay((date - epoch).num_days()))
    } else if let Ok(at) = DateTime::parse_from_rfc3339(value) {
        TimestampMs(at.timestamp_millis())
    } else {
        let naive = NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f")
            .or_else(|_| NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f"))
            .or_else(|_| NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M"))
            .ok()?;
        TimestampMs(naive.and_utc().timestamp_millis())
    };
    // Years 1970–9999.
    (0..=253_402_300_799_999).contains(&at.0).then_some(at)
}

fn parse_count(field: &'static str, value: &str, max: i64) -> Result<Option<i64>, ValidationError> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    let number: i64 = value
        .parse()
        .map_err(|_| ValidationError::new(field, "format"))?;
    validation::in_range(field, number, 0, max).map(Some)
}

/// Splits a deck column into levels.
fn deck_path(value: &str) -> Result<Vec<String>, ValidationError> {
    let levels: Vec<String> = value.split("::").map(|level| nfc(level.trim())).collect();
    if levels.iter().any(String::is_empty) {
        return Err(ValidationError::new("deck", "format"));
    }
    if levels.len() > validation::MAX_DECK_DEPTH {
        return Err(ValidationError::new("deck", "tooDeep"));
    }
    for level in &levels {
        validation::required_text("deck", level, validation::MAX_NAME_CHARS)?;
    }
    Ok(levels)
}

/// The import's deck tree: existing decks and the ones it creates.
struct DeckResolver {
    decks: Vec<Deck>,
    new_paths: Vec<NewDeckPath>,
    /// Name (lower case) of the new deck named after the file: a path starting with it goes
    /// into that deck, not into an existing deck of the same name.
    new_target: Option<String>,
}

impl DeckResolver {
    fn find_child(&self, parent: Option<DeckId>, name: &str) -> Option<DeckId> {
        let wanted = name.to_lowercase();
        self.decks
            .iter()
            .find(|deck| deck.parent_id == parent && deck.name.to_lowercase() == wanted)
            .map(|deck| deck.id)
    }

    fn resolve(&mut self, levels: Vec<String>) -> DeckKey {
        if self.new_target.as_deref() == Some(levels[0].to_lowercase().as_str()) {
            if levels.len() == 1 {
                return DeckKey::New(NEW_TARGET.to_owned());
            }
            return self.add_path(levels);
        }
        let mut parent: Option<DeckId> = None;
        for level in &levels {
            match self.find_child(parent, level) {
                Some(id) => parent = Some(id),
                None => return self.add_path(levels),
            }
        }
        DeckKey::Existing(parent.expect("at least one level"))
    }

    fn add_path(&mut self, levels: Vec<String>) -> DeckKey {
        let key = levels.join("::").to_lowercase();
        if !self
            .new_paths
            .iter()
            .any(|known| known.path.join("::").to_lowercase() == key)
        {
            self.new_paths.push(NewDeckPath { path: levels });
        }
        DeckKey::New(key)
    }
}

fn file_stem(file_name: &str) -> String {
    let stem = match file_name.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem,
        _ => file_name,
    };
    let name: String = nfc(stem.trim())
        .chars()
        .filter(|c| !c.is_control())
        .take(validation::MAX_NAME_CHARS)
        .collect::<String>()
        .replace("::", ":");
    if name.trim().is_empty() {
        "Import".to_owned()
    } else {
        name.trim().to_owned()
    }
}

/// Plans the import (read-only); see the module notes.
pub fn plan_csv(
    tx: &mut dyn Repositories,
    file: &CsvFile,
    options: &CsvOptions,
) -> AppResult<CsvPlan> {
    let mapping = match &options.mapping {
        Some(mapping) => mapping.clone(),
        None => suggest_mapping(file.header.as_deref(), file.columns),
    };
    check_mapping(&mapping, file.columns)?;
    let column = |target: CsvTarget| mapping.iter().position(|current| *current == target);

    let (zone, cutoff) = tx.collection_calendar()?;
    let calendar = StudyCalendar::new(&zone, cutoff).unwrap_or(StudyCalendar::utc(cutoff));
    let decks = tx.active_decks()?;
    let mut resolver = DeckResolver {
        decks: decks.clone(),
        new_paths: Vec::new(),
        new_target: None,
    };
    let (target, target_deck_id, target_deck_name) = match options.target_deck {
        Some(id) => {
            let deck = decks
                .iter()
                .find(|deck| deck.id == id)
                .ok_or(ValidationError::new("targetDeckId", "notFound"))?;
            (DeckKey::Existing(deck.id), Some(deck.id), deck.name.clone())
        }
        None => {
            let name = file_stem(&file.file_name);
            resolver.new_target = Some(name.to_lowercase());
            (DeckKey::New(NEW_TARGET.to_owned()), None, name)
        }
    };

    let mut plan = CsvPlan {
        mapping: mapping.clone(),
        counts: ImportCounts::default(),
        row_errors: Vec::new(),
        target_deck_id,
        target_deck_name,
        new_decks: Vec::new(),
        rows: Vec::new(),
        target: target.clone(),
        new_paths: Vec::new(),
    };
    // Existing fronts per deck (with subdecks), loaded on first use.
    let mut fronts: HashMap<DeckId, HashMap<String, NoteId>> = HashMap::new();
    // Rows of this file: by deck and front, and by GUID.
    let mut planned_fronts: HashMap<(DeckKey, String), usize> = HashMap::new();
    let mut planned_guids: HashMap<Guid, usize> = HashMap::new();

    for record in &file.records {
        let cell = |target: CsvTarget| -> String {
            let raw = column(target)
                .and_then(|index| record.fields.get(index))
                .map_or("", String::as_str);
            let raw = if options.formula_guard {
                csv::unguard(raw)
            } else {
                raw
            };
            if file.html {
                csv::html_to_text(raw)
            } else {
                raw.to_owned()
            }
        };
        let row = (|| -> Result<PlannedRow, ValidationError> {
            let front = nfc(cell(CsvTarget::Front).trim());
            let back = nfc(cell(CsvTarget::Back).trim());
            let fields = NoteFields::validated(&front, &back)?;
            if fields.back.is_empty() {
                return Err(ValidationError::new("back", "required"));
            }
            let tags = match column(CsvTarget::Tags) {
                Some(_) => {
                    let tags: Vec<String> =
                        cell(CsvTarget::Tags).split_whitespace().map(nfc).collect();
                    Some(validation::tags(&tags)?)
                }
                None => None,
            };
            let kind = match column(CsvTarget::NoteType) {
                Some(_) => {
                    let value = cell(CsvTarget::NoteType);
                    if value.trim().is_empty() {
                        NoteTypeKind::Basic
                    } else {
                        parse_note_type(&value)
                            .ok_or(ValidationError::new("noteType", "unknown"))?
                    }
                }
                None => NoteTypeKind::Basic,
            };
            let guid = match cell(CsvTarget::Guid).trim() {
                "" => None,
                value => Some(row_guid(value)),
            };
            let deck_value = cell(CsvTarget::Deck);
            let deck = if deck_value.trim().is_empty() {
                target.clone()
            } else {
                resolver.resolve(deck_path(&deck_value)?)
            };
            let due = match cell(CsvTarget::Due).trim() {
                "" => None,
                value => {
                    Some(parse_due(value, &calendar).ok_or(ValidationError::new("due", "format"))?)
                }
            };
            let schedule = RowSchedule {
                due,
                interval: parse_count("interval", &cell(CsvTarget::Interval), 36_500)?,
                reps: parse_count("reps", &cell(CsvTarget::Reps), 1_000_000)?,
                lapses: parse_count("lapses", &cell(CsvTarget::Lapses), 1_000_000)?,
                position: parse_count("position", &cell(CsvTarget::Position), 100_000_000)?,
            };
            Ok(PlannedRow {
                action: Action::Skip,
                deck,
                kind,
                guid,
                front: fields.front,
                back: fields.back,
                tags,
                schedule,
            })
        })();
        let mut row = match row {
            Ok(row) => row,
            Err(error) => {
                plan.counts.error += 1;
                if plan.row_errors.len() < MAX_ROW_ERRORS {
                    plan.row_errors.push(RowError {
                        line: record.line,
                        error,
                    });
                }
                continue;
            }
        };

        // Match: GUID first, then the front in the row's deck.
        let mut matched: Option<(NoteRef, bool)> = None;
        if let Some(guid) = &row.guid {
            if let Some(index) = planned_guids.get(guid) {
                matched = Some((NoteRef::Planned(*index), true));
            } else if let Some(note) = tx.note_by_guid(guid)? {
                if note.deleted_at.is_some() {
                    plan.counts.error += 1;
                    if plan.row_errors.len() < MAX_ROW_ERRORS {
                        plan.row_errors.push(RowError {
                            line: record.line,
                            error: ValidationError::new("guid", "deleted"),
                        });
                    }
                    continue;
                }
                matched = Some((NoteRef::Existing(note.id), true));
            }
        }
        let key = front_key(&row.front);
        if matched.is_none() {
            if let Some(index) = planned_fronts.get(&(row.deck.clone(), key.clone())) {
                matched = Some((NoteRef::Planned(*index), false));
            } else if let DeckKey::Existing(deck) = row.deck {
                let index = match fronts.entry(deck) {
                    std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        entry.insert(existing_fronts(tx, &decks, deck)?)
                    }
                };
                if let Some(note) = index.get(&key) {
                    matched = Some((NoteRef::Existing(*note), false));
                }
            }
        }
        row.action = match (matched, options.duplicate_policy) {
            (None, _) => {
                plan.counts.create += 1;
                Action::Create { duplicate: false }
            }
            (Some(_), DuplicatePolicy::Skip) => {
                plan.counts.skip += 1;
                Action::Skip
            }
            (Some((note, by_guid)), DuplicatePolicy::Update) => {
                plan.counts.update += 1;
                Action::Update { note, by_guid }
            }
            (Some((_, by_guid)), DuplicatePolicy::Duplicate) => {
                plan.counts.duplicate += 1;
                if by_guid {
                    row.guid = None;
                }
                Action::Create { duplicate: true }
            }
        };
        let index = plan.rows.len();
        if matches!(row.action, Action::Create { .. }) {
            planned_fronts
                .entry((row.deck.clone(), key))
                .or_insert(index);
            if let Some(guid) = &row.guid {
                planned_guids.entry(guid.clone()).or_insert(index);
            }
        }
        plan.rows.push(row);
    }

    // Only the decks some row (or the target) needs.
    let used: HashSet<DeckKey> = plan
        .rows
        .iter()
        .filter(|row| !matches!(row.action, Action::Skip))
        .map(|row| row.deck.clone())
        .chain(std::iter::once(target))
        .collect();
    plan.new_paths = resolver
        .new_paths
        .into_iter()
        .filter(|path| used.contains(&DeckKey::New(path.path.join("::").to_lowercase())))
        .collect();
    plan.new_decks = plan
        .target_deck_id
        .is_none()
        .then(|| plan.target_deck_name.clone())
        .into_iter()
        .chain(plan.new_paths.iter().map(|path| path.path.join("::")))
        .collect();
    Ok(plan)
}

/// Fronts of the notes in a deck and its subdecks.
fn existing_fronts(
    tx: &mut dyn Repositories,
    decks: &[Deck],
    deck: DeckId,
) -> AppResult<HashMap<String, NoteId>> {
    let ids = management::subtree(decks, deck);
    let notes: BTreeSet<NoteId> = tx
        .cards_of_decks(&ids)?
        .into_iter()
        .map(|card| card.note_id)
        .collect();
    let mut index = HashMap::with_capacity(notes.len());
    for id in notes {
        if let Some(note) = tx.note(id)?
            && note.deleted_at.is_none()
        {
            index
                .entry(front_key(&note.fields.front))
                .or_insert(note.id);
        }
    }
    Ok(index)
}

// ---------------------------------------------------------------- apply

/// Result of a committed import (job `import`).
#[derive(Debug, Clone, PartialEq)]
pub struct CsvOutcome {
    pub deck_id: DeckId,
    pub counts: ImportCounts,
    pub row_errors: Vec<RowError>,
    pub decks_created: u32,
}

/// Languages and type of decks the import creates: the target deck's, else the onboarding
/// choice (a simple deck).
fn new_deck_template(
    tx: &mut dyn Repositories,
    plan: &CsvPlan,
) -> AppResult<(TargetLanguage, DeckType)> {
    if let Some(id) = plan.target_deck_id
        && let Some(deck) = tx.deck(id)?
    {
        return Ok((deck.target_language, deck.deck_type));
    }
    let language = crate::content::onboarding_target_languages(tx)?
        .first()
        .and_then(|code| {
            TargetLanguage::ALL
                .iter()
                .copied()
                .find(|language| language.code() == code)
        })
        .unwrap_or(TargetLanguage::De);
    Ok((language, DeckType::Simple))
}

/// Writes the plan inside the caller's write transaction. Checks `cancel` every
/// [`CANCEL_EVERY`] rows and enters the commit phase as its last step.
pub fn apply_csv(
    tx: &mut dyn Repositories,
    rng: &dyn Rng,
    plan: &CsvPlan,
    now: TimestampMs,
    cancel: &CancelToken,
) -> AppResult<CsvOutcome> {
    let (language, deck_type) = new_deck_template(tx, plan)?;
    let config_id = crate::settings::default_preset(tx)?;
    let (zone, cutoff) = tx.collection_calendar()?;
    let calendar = StudyCalendar::new(&zone, cutoff).unwrap_or(StudyCalendar::utc(cutoff));

    // Decks first: the new target deck, then every missing level of every new path.
    let mut created: HashMap<String, DeckId> = HashMap::new();
    let mut decks_created = 0u32;
    let new_deck = |name: &str, parent: Option<DeckId>| NewDeck {
        guid: rng.guid(),
        name: name.to_owned(),
        parent_id: parent,
        description: String::new(),
        native_language: NativeLanguage::Pl,
        target_language: language,
        deck_type,
        config_id,
        content_version: None,
        bundled: false,
    };
    if plan.target_deck_id.is_none() {
        let id = tx
            .insert_deck(&new_deck(&plan.target_deck_name, None), now)?
            .id;
        created.insert(NEW_TARGET.to_owned(), id);
        decks_created += 1;
    }
    for path in &plan.new_paths {
        let mut parent: Option<DeckId> = None;
        for depth in 0..path.path.len() {
            let key = path.path[..=depth].join("::").to_lowercase();
            if let Some(id) = created.get(&key) {
                parent = Some(*id);
                continue;
            }
            let wanted = path.path[depth].to_lowercase();
            if depth == 0
                && plan.target_deck_id.is_none()
                && wanted == plan.target_deck_name.to_lowercase()
                && let Some(id) = created.get(NEW_TARGET)
            {
                parent = Some(*id);
                continue;
            }
            let decks = tx.active_decks()?;
            let existing = decks
                .iter()
                .find(|deck| deck.parent_id == parent && deck.name.to_lowercase() == wanted)
                .map(|deck| deck.id);
            let id = match existing {
                Some(id) => id,
                None => {
                    decks_created += 1;
                    tx.insert_deck(&new_deck(&path.path[depth], parent), now)?
                        .id
                }
            };
            created.insert(key, id);
            parent = Some(id);
        }
    }
    let deck_of = |key: &DeckKey| -> AppResult<DeckId> {
        match key {
            DeckKey::Existing(id) => Ok(*id),
            DeckKey::New(path) => created
                .get(path)
                .copied()
                .ok_or_else(|| AppError::Internal("planned deck missing".into())),
        }
    };
    let target_deck = deck_of(&plan.target)?;

    let mut note_types: HashMap<NoteTypeKind, NoteTypeId> = HashMap::new();
    let mut created_notes: HashMap<usize, NoteId> = HashMap::new();
    // One position query for the whole import (MAX over every card per insert would be
    // quadratic); positions only grow inside this transaction.
    let base_position = tx.next_position()?;
    let mut next_position = base_position;
    for (index, row) in plan.rows.iter().enumerate() {
        if index % CANCEL_EVERY == 0 {
            cancel.check()?;
        }
        match &row.action {
            Action::Skip => {}
            Action::Create { .. } => {
                let deck = deck_of(&row.deck)?;
                let note_type = match note_types.get(&row.kind) {
                    Some(id) => *id,
                    None => {
                        let id = tx.note_type(row.kind)?.id;
                        note_types.insert(row.kind, id);
                        id
                    }
                };
                let guid = match &row.guid {
                    Some(guid) if tx.note_by_guid(guid)?.is_none() => guid.clone(),
                    _ => rng.guid(),
                };
                let note = tx.insert_note(
                    &NewNote {
                        guid,
                        note_type_id: note_type,
                        fields: NoteFields {
                            front: row.front.clone(),
                            back: row.back.clone(),
                        },
                        tags: row.tags.clone().unwrap_or_default(),
                    },
                    now,
                )?;
                created_notes.insert(index, note.id);
                for ordinal in 0..row.kind.templates().len() as u32 {
                    let position = match row.schedule.position {
                        Some(relative) if ordinal == 0 && row.schedule.due.is_none() => {
                            base_position + relative
                        }
                        _ => next_position,
                    };
                    next_position = next_position.max(position) + 1;
                    let card = tx.insert_card(
                        &NewCard {
                            guid: rng.guid(),
                            note_id: note.id,
                            deck_id: deck,
                            template_ordinal: ordinal,
                            position,
                        },
                        now,
                    )?;
                    if ordinal == 0
                        && let Some(due) = row.schedule.due
                    {
                        let mut card = card;
                        schedule_card(&mut card, &row.schedule, due, &calendar, now);
                        tx.update_card(&card, now)?;
                    }
                }
            }
            Action::Update { note, by_guid } => {
                let id = match note {
                    NoteRef::Existing(id) => *id,
                    NoteRef::Planned(planned) => *created_notes
                        .get(planned)
                        .ok_or_else(|| AppError::Internal("planned note missing".into()))?,
                };
                let Some(mut note) = tx.note(id)? else {
                    return Err(AppError::PreviewStale);
                };
                if note.deleted_at.is_some() {
                    return Err(AppError::PreviewStale);
                }
                let mut changed: Vec<&str> = Vec::new();
                if *by_guid && note.fields.front != row.front {
                    note.fields.front = row.front.clone();
                    changed.push("front");
                }
                if note.fields.back != row.back {
                    note.fields.back = row.back.clone();
                    changed.push("back");
                }
                if let Some(tags) = &row.tags
                    && &note.tags != tags
                {
                    note.tags = tags.clone();
                    changed.push("tags");
                }
                if !changed.is_empty() {
                    for field in changed {
                        management::mark_override(tx, note.id, field, now)?;
                    }
                    tx.update_note(&note, now)?;
                }
            }
        }
    }
    cancel.begin_commit()?;
    Ok(CsvOutcome {
        deck_id: target_deck,
        counts: plan.counts,
        row_errors: plan.row_errors.clone(),
        decks_created,
    })
}

/// A forward card with a `due` from the file becomes a review card. Without FSRS memory the
/// scheduler treats it as a legacy card (as imported Anki cards).
fn schedule_card(
    card: &mut Card,
    schedule: &RowSchedule,
    due: TimestampMs,
    calendar: &StudyCalendar,
    now: TimestampMs,
) {
    let interval = schedule
        .interval
        .unwrap_or_else(|| calendar.days_between(now, due).max(1))
        .max(1);
    card.state = CardState::Review;
    card.queue = QueueState::Review;
    card.due_at = Some(due);
    card.fsrs.scheduled_days = interval;
    card.fsrs.last_review_at = Some(TimestampMs((due.0 - interval * DAY_MS).min(now.0)));
    card.reps = schedule.reps.unwrap_or(1).max(1);
    card.lapses = schedule.lapses.unwrap_or(0);
}

// ---------------------------------------------------------------- export

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Csv,
    Apkg,
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Apkg => "apkg",
        }
    }
}

/// One consistent read of a deck and its subdecks.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportSnapshot {
    /// The exported deck first, then its subdecks (parents before children).
    pub decks: Vec<Deck>,
    pub notes: Vec<(Note, NoteTypeKind)>,
    /// Live cards that are not archived, by note and template.
    pub cards: Vec<Card>,
    /// Reviews of the cards (only when asked for).
    pub reviews: Vec<ReviewLog>,
    pub calendar: StudyCalendar,
    /// What no export format carries: associations, tips, links, archived cards.
    pub losses: Losses,
}

/// Reads the snapshot inside the caller's (read) transaction.
pub fn export_snapshot(
    tx: &mut dyn Repositories,
    deck_id: DeckId,
    with_reviews: bool,
) -> AppResult<ExportSnapshot> {
    let all = tx.active_decks()?;
    if !all.iter().any(|deck| deck.id == deck_id) {
        return Err(match tx.deck(deck_id)? {
            Some(_) => AppError::Deleted { entity: "deck" },
            None => AppError::NotFound { entity: "deck" },
        });
    }
    let ids = management::subtree(&all, deck_id);
    let decks: Vec<Deck> = ids
        .iter()
        .filter_map(|id| all.iter().find(|deck| deck.id == *id).cloned())
        .collect();
    let mut losses = Losses::default();
    for deck in &decks {
        let counts = tx.deck_content_counts(deck.id)?;
        losses.add(Loss::Associations, counts.associations);
        losses.add(Loss::GrammarTips, counts.grammar_tips);
        losses.add(Loss::WordLinks, counts.word_links);
    }
    let (mut cards, archived): (Vec<Card>, Vec<Card>) = tx
        .cards_of_decks(&ids)?
        .into_iter()
        .partition(|card| card.archived_at.is_none());
    losses.add(
        Loss::ArchivedCards,
        u32::try_from(archived.len()).unwrap_or(u32::MAX),
    );
    let note_ids: BTreeSet<NoteId> = cards.iter().map(|card| card.note_id).collect();
    let mut kinds: HashMap<NoteTypeId, NoteTypeKind> = HashMap::new();
    let mut notes = Vec::with_capacity(note_ids.len());
    for id in note_ids {
        let Some(note) = tx.note(id)? else { continue };
        if note.deleted_at.is_some() {
            continue;
        }
        let kind = match kinds.get(&note.note_type_id) {
            Some(kind) => *kind,
            None => {
                let kind = tx
                    .note_type_by_id(note.note_type_id)?
                    .map_or(NoteTypeKind::Basic, |note_type| note_type.kind);
                kinds.insert(note.note_type_id, kind);
                kind
            }
        };
        notes.push((note, kind));
    }
    let live: HashSet<NoteId> = notes.iter().map(|(note, _)| note.id).collect();
    cards.retain(|card| live.contains(&card.note_id));
    let mut reviews = Vec::new();
    if with_reviews {
        for card in &cards {
            let mut logs = tx.review_logs(card.id)?;
            logs.reverse();
            reviews.extend(logs);
        }
    }
    let (zone, cutoff) = tx.collection_calendar()?;
    let calendar = StudyCalendar::new(&zone, cutoff).unwrap_or(StudyCalendar::utc(cutoff));
    Ok(ExportSnapshot {
        decks,
        notes,
        cards,
        reviews,
        calendar,
        losses,
    })
}

/// Full deck names (`Parent::Child`) from the exported deck down.
pub fn deck_paths(decks: &[Deck]) -> HashMap<DeckId, String> {
    let mut paths = HashMap::new();
    if let Some(root) = decks.first() {
        paths.insert(root.id, root.name.clone());
    }
    for _ in 0..decks.len() {
        for deck in decks.iter().skip(1) {
            if paths.contains_key(&deck.id) {
                continue;
            }
            if let Some(parent) = deck
                .parent_id
                .and_then(|parent| paths.get(&parent).cloned())
            {
                paths.insert(deck.id, format!("{parent}::{}", deck.name));
            }
        }
    }
    paths
}

/// ISO 8601 UTC with milliseconds.
pub fn iso(at: TimestampMs) -> String {
    DateTime::from_timestamp_millis(at.0)
        .unwrap_or_default()
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// Writes the CSV: one row per note, scheduling of its forward card when asked for.
pub fn csv_export(
    snapshot: &ExportSnapshot,
    include_scheduling: bool,
    formula_guard: bool,
) -> (Vec<u8>, Losses, u32) {
    let mut losses = snapshot.losses.clone();
    let paths = deck_paths(&snapshot.decks);
    let mut writer = csv::Writer::new(Delimiter::Comma, formula_guard);
    // Read back by `suggest_mapping` (names are matched case-insensitively).
    let mut header = vec!["guid", "front", "back", "tags", "deck", "noteType"];
    if include_scheduling {
        header.extend(["due", "interval", "reps", "lapses", "position"]);
    }
    writer.record(&header);
    let mut by_note: HashMap<NoteId, Vec<&Card>> = HashMap::new();
    for card in &snapshot.cards {
        by_note.entry(card.note_id).or_default().push(card);
    }
    let mut history = 0u32;
    let mut memory = 0u32;
    let mut flags = 0u32;
    let mut reverse = 0u32;
    let mut rows = 0u32;
    for (note, kind) in &snapshot.notes {
        let cards = by_note.get(&note.id).cloned().unwrap_or_default();
        let Some(first) = cards
            .iter()
            .find(|card| card.template_ordinal == 0)
            .or(cards.first())
        else {
            continue;
        };
        for card in &cards {
            history += u32::from(card.reps > 0);
            memory += u32::from(card.fsrs.stability.is_some());
            flags += u32::from(card.flag != CardFlag::None);
            if include_scheduling && card.template_ordinal > 0 && card.state != CardState::New {
                reverse += 1;
            }
        }
        let mut row = vec![
            note.guid.to_string(),
            note.fields.front.clone(),
            note.fields.back.clone(),
            note.tags.join(" "),
            paths.get(&first.deck_id).cloned().unwrap_or_default(),
            note_type_code(*kind).to_owned(),
        ];
        if include_scheduling {
            let new = first.state == CardState::New;
            row.extend([
                if new {
                    String::new()
                } else {
                    first.due_at.map(iso).unwrap_or_default()
                },
                if new {
                    String::new()
                } else {
                    first.fsrs.scheduled_days.to_string()
                },
                first.reps.to_string(),
                first.lapses.to_string(),
                if new {
                    first.position.to_string()
                } else {
                    String::new()
                },
            ]);
        }
        writer.record(&row);
        rows += 1;
    }
    losses.add(Loss::ReviewHistory, history);
    losses.add(Loss::FsrsMemory, memory);
    losses.add(Loss::Flags, flags);
    losses.add(Loss::ReverseScheduling, reverse);
    (writer.finish(), losses, rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> CsvOptions {
        CsvOptions {
            delimiter: None,
            encoding: None,
            has_header: true,
            mapping: None,
            duplicate_policy: DuplicatePolicy::Skip,
            target_deck: None,
            formula_guard: false,
        }
    }

    #[test]
    fn header_names_suggest_the_mapping() {
        let header: Vec<String> = [
            "GUID", "Front", "Back", "Tags", "Deck", "noteType", "due", "extra",
        ]
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
        assert_eq!(
            suggest_mapping(Some(&header), 8),
            [
                CsvTarget::Guid,
                CsvTarget::Front,
                CsvTarget::Back,
                CsvTarget::Tags,
                CsvTarget::Deck,
                CsvTarget::NoteType,
                CsvTarget::Due,
                CsvTarget::Ignore
            ]
        );
        let unknown: Vec<String> = vec!["Wort".into(), "Bedeutung".into()];
        assert_eq!(
            suggest_mapping(Some(&unknown), 2),
            [CsvTarget::Front, CsvTarget::Back]
        );
        assert_eq!(
            suggest_mapping(None, 4),
            [
                CsvTarget::Front,
                CsvTarget::Back,
                CsvTarget::Tags,
                CsvTarget::Ignore
            ]
        );
    }

    #[test]
    fn mappings_need_one_front_one_back_and_no_repeats() {
        use CsvTarget::*;
        assert!(check_mapping(&[Front, Back, Ignore, Ignore], 4).is_ok());
        let reason = |mapping: &[CsvTarget], columns| match check_mapping(mapping, columns) {
            Err(AppError::Validation(error)) => error.reason,
            other => panic!("{other:?}"),
        };
        assert_eq!(reason(&[Front, Front], 2), "frontBack");
        assert_eq!(reason(&[Front, Back, Tags, Tags], 4), "duplicateTarget");
        assert_eq!(reason(&[Front, Back], 3), "length");
    }

    #[test]
    fn files_are_read_with_header_bom_and_detected_delimiter() {
        let bytes = "\u{feff}front;back\nHund;dog\nKatze;cat\n".as_bytes();
        let file = read_csv("words.csv", bytes, &options()).unwrap();
        assert_eq!(file.delimiter, Delimiter::Semicolon);
        assert_eq!(file.detected, Some(Delimiter::Semicolon));
        assert!(file.has_bom);
        assert_eq!(
            file.header.as_deref(),
            Some(&["front".to_owned(), "back".to_owned()][..])
        );
        assert_eq!(file.records.len(), 2);
        assert_eq!(file.records[0].line, 2);
        assert_eq!(file.columns, 2);
    }

    #[test]
    fn packages_and_binaries_are_refused_by_format() {
        assert_eq!(
            read_csv("deck.apkg", b"PK\x03\x04rest", &options()),
            Err(AppError::UnsupportedFormat { format: "apkg" })
        );
        assert_eq!(
            read_csv("x.bin", b"a\0b,c\n", &options()),
            Err(AppError::UnsupportedFormat { format: "unknown" })
        );
        assert_eq!(
            read_csv("only-header.csv", b"front,back\n", &options()),
            Err(ValidationError::new("file", "empty").into())
        );
    }

    #[test]
    fn due_values_are_dates_or_utc_instants() {
        let warsaw = StudyCalendar::new("Europe/Warsaw", 240).unwrap();
        // A date: the start of that study day (04:00 local, CET = 03:00 UTC).
        assert_eq!(
            parse_due("2026-03-01", &warsaw),
            Some(TimestampMs(1_772_334_000_000))
        );
        assert_eq!(
            parse_due("2026-03-01T12:00:00.000Z", &warsaw),
            Some(TimestampMs(1_772_366_400_000))
        );
        assert_eq!(
            parse_due("2026-03-01T13:00:00+01:00", &warsaw),
            Some(TimestampMs(1_772_366_400_000))
        );
        assert_eq!(
            parse_due("2026-03-01 12:00:00", &warsaw),
            Some(TimestampMs(1_772_366_400_000))
        );
        assert_eq!(parse_due("tomorrow", &warsaw), None);
        assert_eq!(parse_due("1969-12-31", &StudyCalendar::utc(0)), None);
    }

    #[test]
    fn foreign_guids_map_to_a_stable_guid() {
        assert_eq!(row_guid("abc123").as_str(), "abc123");
        let anki = row_guid("f&w!3Xz/k:");
        assert_eq!(anki, row_guid("f&w!3Xz/k:"));
        assert_eq!(anki.as_str().len(), 32);
    }

    #[test]
    fn note_types_and_deck_paths_parse() {
        assert_eq!(
            parse_note_type("basicAndReversed"),
            Some(NoteTypeKind::BasicAndReversed)
        );
        assert_eq!(
            parse_note_type("Basic (and reversed card)"),
            Some(NoteTypeKind::BasicAndReversed)
        );
        assert_eq!(parse_note_type("cloze"), None);
        assert_eq!(deck_path(" A :: B ").unwrap(), ["A", "B"]);
        assert_eq!(deck_path("A::::B").unwrap_err().reason, "format");
        assert_eq!(
            deck_path(&["x"; 9].join("::")).unwrap_err().reason,
            "tooDeep"
        );
        assert_eq!(file_stem("German A1.csv"), "German A1");
        assert_eq!(file_stem(".csv"), ".csv");
        assert_eq!(file_stem("a::b.txt"), "a:b");
    }
}
