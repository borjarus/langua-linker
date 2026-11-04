//! Content rules shared by the linter and the importer (plan steps 14–16). The importer runs the
//! same checks before it opens a transaction, so content the linter rejects never reaches SQLite.

use std::collections::{HashMap, HashSet};
use std::fmt;

use langualinker_domain::deck::{DeckType, NativeLanguage, TargetLanguage};
use langualinker_domain::note::NoteTypeKind;
use langualinker_domain::text::{is_nfc, utf16_slice};
use langualinker_domain::{Guid, validation};

use crate::format::{
    CardStateName, Catalog, ContentKind, DeckManifest, MAX_BUNDLED_GUID_CHARS, MAX_SNIPPET_CHARS,
    NoteRecord, QueueName, ReviewStatus, SCHEMA_VERSION, SourceKind,
};
use crate::source::is_plain_file_name;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Warning,
    Error,
}

/// One finding. `code` is stable and safe for logs and `ErrorDto.details`; `message` may quote
/// content and is meant for the linter's output only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub severity: Severity,
    pub code: &'static str,
    pub file: String,
    pub line: Option<u32>,
    pub message: String,
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let level = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        match self.line {
            Some(line) => write!(
                f,
                "{level}[{}] {}:{line}: {}",
                self.code, self.file, self.message
            ),
            None => write!(f, "{level}[{}] {}: {}", self.code, self.file, self.message),
        }
    }
}

/// Which gate the content must pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Development, tests and demo builds: structure and links must be valid.
    Dev,
    /// Beta and release builds: additionally reviewed and rights-confirmed (ADR-14).
    Release,
}

/// Collects issues for one file.
#[derive(Debug, Default)]
pub struct Issues(pub Vec<Issue>);

impl Issues {
    pub fn error(
        &mut self,
        code: &'static str,
        file: &str,
        line: Option<u32>,
        message: impl Into<String>,
    ) {
        self.push(Severity::Error, code, file, line, message);
    }

    pub fn warning(
        &mut self,
        code: &'static str,
        file: &str,
        line: Option<u32>,
        message: impl Into<String>,
    ) {
        self.push(Severity::Warning, code, file, line, message);
    }

    fn push(
        &mut self,
        severity: Severity,
        code: &'static str,
        file: &str,
        line: Option<u32>,
        message: impl Into<String>,
    ) {
        self.0.push(Issue {
            severity,
            code,
            file: file.to_owned(),
            line,
            message: message.into(),
        });
    }

    pub fn has_errors(&self) -> bool {
        self.0.iter().any(|issue| issue.severity == Severity::Error)
    }

    pub fn first_error(&self) -> Option<&Issue> {
        self.0
            .iter()
            .find(|issue| issue.severity == Severity::Error)
    }
}

pub fn parse_deck_type(value: &str) -> Option<DeckType> {
    match value {
        "Simple" => Some(DeckType::Simple),
        "TextWithAssociations" => Some(DeckType::TextWithAssociations),
        "Linguistic" => Some(DeckType::Linguistic),
        _ => None,
    }
}

pub fn parse_note_type(value: &str) -> Option<NoteTypeKind> {
    match value {
        "Basic" => Some(NoteTypeKind::Basic),
        "BasicAndReversed" => Some(NoteTypeKind::BasicAndReversed),
        "Sentence" => Some(NoteTypeKind::Sentence),
        "SentenceAndReversed" => Some(NoteTypeKind::SentenceAndReversed),
        _ => None,
    }
}

pub fn parse_native_language(value: &str) -> Option<NativeLanguage> {
    NativeLanguage::ALL
        .iter()
        .copied()
        .find(|language| language.code() == value)
}

pub fn parse_target_language(value: &str) -> Option<TargetLanguage> {
    TargetLanguage::ALL
        .iter()
        .copied()
        .find(|language| language.code() == value)
}

/// `YYYY.MM.DD`, so versions compare correctly as text.
pub fn is_content_version(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'.'
        && bytes[7] == b'.'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
}

/// 2000-01-01 .. 2200-01-01 in epoch milliseconds.
fn is_plausible_time(ms: i64) -> bool {
    (946_684_800_000..7_258_118_400_000).contains(&ms)
}

fn is_bundled_guid(value: &str) -> bool {
    Guid::parse(value).is_ok() && value.len() <= MAX_BUNDLED_GUID_CHARS
}

fn is_prompt_id(value: &str) -> bool {
    let Some((name, version)) = value.rsplit_once("-v") else {
        return false;
    };
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !version.is_empty()
        && version.bytes().all(|byte| byte.is_ascii_digit())
}

/// Catalog-level checks; returns the deck stems worth loading.
pub fn check_catalog(catalog: &Catalog, file: &str, issues: &mut Issues) {
    if catalog.schema_version != SCHEMA_VERSION {
        issues.error(
            "schema.unsupported",
            file,
            None,
            format!(
                "schemaVersion {} (this build reads {SCHEMA_VERSION})",
                catalog.schema_version
            ),
        );
    }
    let mut seen = HashSet::new();
    for stem in &catalog.decks {
        if !seen.insert(stem) {
            issues.error(
                "catalog.duplicate",
                file,
                None,
                format!("deck {stem} listed twice"),
            );
        }
        if !is_plain_file_name(&format!("{stem}.json")) {
            issues.error(
                "catalog.name",
                file,
                None,
                format!("deck {stem} is not a plain name"),
            );
        }
    }
    if catalog.decks.is_empty() {
        issues.error("catalog.empty", file, None, "no decks");
    }
}

/// Manifest checks. `earlier` holds the manifests listed before this one in the catalog.
pub fn check_manifest(
    manifest: &DeckManifest,
    file: &str,
    earlier: &[&DeckManifest],
    mode: Mode,
    issues: &mut Issues,
) {
    if manifest.schema_version != SCHEMA_VERSION {
        issues.error(
            "schema.unsupported",
            file,
            None,
            format!(
                "schemaVersion {} (this build reads {SCHEMA_VERSION})",
                manifest.schema_version
            ),
        );
    }
    if !is_bundled_guid(&manifest.deck_guid) {
        issues.error(
            "deck.guid",
            file,
            None,
            "deckGuid must be 1–48 characters of [A-Za-z0-9_-]",
        );
    }
    if earlier
        .iter()
        .any(|other| other.deck_guid == manifest.deck_guid)
    {
        issues.error(
            "guid.duplicate",
            file,
            None,
            format!("deckGuid {} is used twice", manifest.deck_guid),
        );
    }
    if !is_content_version(&manifest.content_version) {
        issues.error(
            "deck.contentVersion",
            file,
            None,
            "contentVersion must be YYYY.MM.DD",
        );
    }
    for (field, value, max) in [
        ("name", &manifest.name, validation::MAX_NAME_CHARS),
        (
            "description",
            &manifest.description,
            validation::MAX_DESCRIPTION_CHARS,
        ),
    ] {
        if value.trim().is_empty() || value.chars().count() > max || !is_nfc(value) {
            issues.error(
                "deck.text",
                file,
                None,
                format!("{field} is empty, too long or not NFC"),
            );
        }
    }
    if !matches!(
        manifest.level.as_str(),
        "A1" | "A2" | "B1" | "B2" | "C1" | "C2"
    ) {
        issues.error(
            "deck.level",
            file,
            None,
            format!("unknown CEFR level {}", manifest.level),
        );
    }
    if parse_native_language(&manifest.native_language).is_none() {
        issues.error(
            "language.native",
            file,
            None,
            format!("unsupported L1 {}", manifest.native_language),
        );
    }
    if parse_target_language(&manifest.target_language).is_none() {
        issues.error(
            "language.target",
            file,
            None,
            format!("unsupported L2 {}", manifest.target_language),
        );
    }
    if parse_deck_type(&manifest.deck_type).is_none() {
        issues.error(
            "deck.type",
            file,
            None,
            format!("unknown deckType {}", manifest.deck_type),
        );
    }
    match parse_note_type(&manifest.note_type) {
        None => issues.error(
            "deck.noteType",
            file,
            None,
            format!("unknown noteType {}", manifest.note_type),
        ),
        Some(kind) if kind.is_sentence() != (manifest.content_kind == ContentKind::Sentences) => {
            issues.error(
                "deck.noteType",
                file,
                None,
                "sentence decks use a Sentence note type, word decks a Basic one",
            )
        }
        Some(_) => {}
    }
    if !is_plain_file_name(&manifest.notes_file) || !manifest.notes_file.ends_with(".ndjson") {
        issues.error(
            "deck.notesFile",
            file,
            None,
            "notesFile must be a plain *.ndjson name",
        );
    }
    if manifest.sha256.len() != 64
        || !manifest
            .sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        issues.error(
            "checksum.format",
            file,
            None,
            "sha256 must be 64 lowercase hex characters",
        );
    }
    if manifest.license.name.trim().is_empty() {
        issues.error("license.missing", file, None, "license.name is required");
    }
    for dependency in &manifest.depends_on {
        match earlier.iter().find(|other| &other.deck_guid == dependency) {
            None => issues.error(
                "deck.dependency",
                file,
                None,
                format!("dependsOn {dependency} is not a deck listed earlier in the catalog"),
            ),
            Some(other)
                if other.content_kind != ContentKind::Words
                    || other.target_language != manifest.target_language =>
            {
                issues.error(
                    "deck.dependency",
                    file,
                    None,
                    format!("dependsOn {dependency} is not a word deck of the same language"),
                )
            }
            Some(_) => {}
        }
    }
    if manifest.content_kind == ContentKind::Sentences
        && manifest.source == SourceKind::Bundled
        && manifest.depends_on.is_empty()
    {
        issues.error(
            "deck.dependency",
            file,
            None,
            "a sentence deck depends on at least one word deck",
        );
    }
    match manifest.source {
        SourceKind::Import if !manifest.depends_on.is_empty() => issues.error(
            "package.dependency",
            file,
            None,
            "an import package stands alone and depends on no deck",
        ),
        SourceKind::Bundled if manifest.origin.is_some() => issues.error(
            "deck.origin",
            file,
            None,
            "origin describes import packages only",
        ),
        _ => {}
    }
    if mode == Mode::Release {
        let review = &manifest.review;
        if review.status != ReviewStatus::Approved
            || review.reviewer.is_none()
            || review.reviewed_at.is_none()
        {
            issues.error(
                "release.notReviewed",
                file,
                None,
                "native review is not recorded as approved",
            );
        }
        if !manifest.license.rights_confirmed {
            issues.error(
                "release.rightsUnconfirmed",
                file,
                None,
                "rights audit (D-04) is not confirmed",
            );
        }
    }
}

/// Text that ends up on a card: NFC, trimmed, no control characters or markup.
fn check_text(
    field: &str,
    value: &str,
    max: usize,
    required: bool,
    file: &str,
    line: u32,
    issues: &mut Issues,
) {
    if value.trim().is_empty() {
        if required {
            issues.error(
                "text.required",
                file,
                Some(line),
                format!("{field} is empty"),
            );
        }
        return;
    }
    if value.chars().count() > max {
        issues.error(
            "text.tooLong",
            file,
            Some(line),
            format!("{field} is longer than {max} characters"),
        );
    }
    if value.trim() != value {
        issues.error(
            "text.whitespace",
            file,
            Some(line),
            format!("{field} has leading or trailing whitespace"),
        );
    }
    if !is_nfc(value) {
        issues.error(
            "text.notNfc",
            file,
            Some(line),
            format!("{field} is not NFC"),
        );
    }
    if value.chars().any(|c| c.is_control() && c != '\n') {
        issues.error(
            "text.control",
            file,
            Some(line),
            format!("{field} contains control characters"),
        );
    }
    let bytes = value.as_bytes();
    let markup = bytes.windows(2).any(|pair| {
        pair[0] == b'<' && (pair[1].is_ascii_alphabetic() || pair[1] == b'/' || pair[1] == b'!')
    });
    if markup {
        issues.error(
            "text.markup",
            file,
            Some(line),
            format!("{field} looks like HTML; bundled text is plain"),
        );
    }
}

/// Per-deck record checks, fed one record at a time while streaming.
pub struct DeckChecker<'a> {
    manifest: &'a DeckManifest,
    file: String,
    kind: Option<NoteTypeKind>,
    deck_type: Option<DeckType>,
    /// GUIDs of notes, associations and tips across every deck checked so far.
    seen_guids: &'a mut HashSet<String>,
    /// Note GUIDs of the dependency decks (word links must resolve into them).
    dependency_guids: &'a HashSet<String>,
    fronts: HashMap<String, (String, u32)>,
    pub note_guids: HashSet<String>,
    pub notes: u32,
    /// Cards with progress (not new) in an import package.
    pub scheduled: u32,
    /// Suspended or buried cards in an import package.
    pub suspended: u32,
    /// Review records in an import package.
    pub reviews: u32,
}

impl<'a> DeckChecker<'a> {
    pub fn new(
        manifest: &'a DeckManifest,
        seen_guids: &'a mut HashSet<String>,
        dependency_guids: &'a HashSet<String>,
    ) -> Self {
        Self {
            manifest,
            file: manifest.notes_file.clone(),
            kind: parse_note_type(&manifest.note_type),
            deck_type: parse_deck_type(&manifest.deck_type),
            seen_guids,
            dependency_guids,
            fronts: HashMap::new(),
            note_guids: HashSet::new(),
            notes: 0,
            scheduled: 0,
            suspended: 0,
            reviews: 0,
        }
    }

    fn guid(
        &mut self,
        guid: &str,
        what: &str,
        bundled_limit: bool,
        line: u32,
        issues: &mut Issues,
    ) {
        let valid = if bundled_limit {
            is_bundled_guid(guid)
        } else {
            Guid::parse(guid).is_ok()
        };
        if !valid {
            issues.error(
                "guid.invalid",
                &self.file,
                Some(line),
                format!("{what} GUID {guid:?} is not valid"),
            );
        }
        if !self.seen_guids.insert(guid.to_owned()) {
            issues.error(
                "guid.duplicate",
                &self.file,
                Some(line),
                format!("{what} GUID {guid} is used twice"),
            );
        }
    }

    pub fn record(&mut self, line: u32, record: &NoteRecord, issues: &mut Issues) {
        self.notes += 1;
        let file = self.file.clone();
        self.guid(&record.guid, "note", true, line, issues);
        self.note_guids.insert(record.guid.clone());
        check_text(
            "front",
            &record.front,
            validation::MAX_FIELD_CHARS,
            true,
            &file,
            line,
            issues,
        );
        let import = self.manifest.source == SourceKind::Import;
        check_text(
            "back",
            &record.back,
            validation::MAX_FIELD_CHARS,
            !import,
            &file,
            line,
            issues,
        );
        if record.front.contains('\n') {
            issues.error(
                "front.multiline",
                &file,
                Some(line),
                "front is a single line",
            );
        }
        if validation::tags(&record.tags).is_err() {
            issues.error(
                "tags.invalid",
                &file,
                Some(line),
                "tags must be non-empty, without whitespace, at most 50",
            );
        }

        // Front review: an exact duplicate is an error, a homograph (same front, other meaning)
        // is allowed but reported so a reviewer looks at both.
        let key = record.front.trim().to_lowercase();
        if let Some((back, first_line)) = self.fronts.get(&key) {
            if back == &record.back {
                issues.error(
                    "front.duplicate",
                    &file,
                    Some(line),
                    format!("same front and back as line {first_line}"),
                );
            } else {
                issues.warning(
                    "front.homograph",
                    &file,
                    Some(line),
                    format!("same front as line {first_line} with another meaning"),
                );
            }
        } else {
            self.fronts.insert(key, (record.back.clone(), line));
        }

        let capabilities = self.deck_type.map(DeckType::capabilities);
        let words = self.manifest.content_kind == ContentKind::Words;

        if capabilities.is_some_and(|caps| !caps.associations) && !record.associations.is_empty() {
            issues.error(
                "associations.unsupported",
                &file,
                Some(line),
                "the deck type shows no associations",
            );
        }
        if words && !import && record.associations.is_empty() {
            issues.error(
                "associations.required",
                &file,
                Some(line),
                "every word note needs an association",
            );
        }
        for association in &record.associations {
            self.guid(&association.guid, "association", false, line, issues);
            check_text(
                "association",
                &association.text,
                MAX_SNIPPET_CHARS,
                true,
                &file,
                line,
                issues,
            );
            match (&association.prompt_version, &association.model) {
                (Some(prompt), _) if !is_prompt_id(prompt) => issues.error(
                    "association.promptVersion",
                    &file,
                    Some(line),
                    format!("promptVersion {prompt:?} is not <name>-v<N>"),
                ),
                (None, Some(_)) => issues.error(
                    "association.promptVersion",
                    &file,
                    Some(line),
                    "a model-generated association names its prompt",
                ),
                _ => {}
            }
        }

        if capabilities.is_some_and(|caps| !caps.grammar_tips) && !record.grammar_tips.is_empty() {
            issues.error(
                "grammarTips.unsupported",
                &file,
                Some(line),
                "the deck type shows no grammar tips",
            );
        }
        for tip in &record.grammar_tips {
            self.guid(&tip.guid, "grammar tip", false, line, issues);
            check_text(
                "grammar tip",
                &tip.text,
                MAX_SNIPPET_CHARS,
                true,
                &file,
                line,
                issues,
            );
        }

        self.word_links(
            line,
            record,
            capabilities.map(|caps| caps.word_links),
            issues,
        );
        self.scheduling(line, record, issues);
    }

    /// Scheduling state and history: import packages only, consistent per card.
    fn scheduling(&mut self, line: u32, record: &NoteRecord, issues: &mut Issues) {
        let file = self.file.clone();
        if self.manifest.source != SourceKind::Import {
            if !record.cards.is_empty() || !record.reviews.is_empty() {
                issues.error(
                    "scheduling.bundled",
                    &file,
                    Some(line),
                    "bundled content carries no progress; only import packages do",
                );
            }
            return;
        }
        let templates = self.kind.map_or(1, |kind| kind.templates().len() as u32);
        let mut seen = HashSet::new();
        for card in &record.cards {
            let mut bad = |reason: &str| {
                issues.error(
                    "card.invalid",
                    &file,
                    Some(line),
                    format!("card {}: {reason}", card.ordinal),
                )
            };
            if card.ordinal >= templates {
                bad("ordinal has no template in this note type");
            }
            if !seen.insert(card.ordinal) {
                bad("ordinal listed twice");
            }
            let is_new = card.state == CardStateName::New;
            if is_new != card.due_at_ms.is_none() {
                bad("dueAtMs is null exactly for new cards");
            }
            let queue_fits = match card.queue {
                QueueName::New => is_new,
                QueueName::Learning | QueueName::DayLearn => {
                    matches!(
                        card.state,
                        CardStateName::Learning | CardStateName::Relearning
                    )
                }
                QueueName::Review => card.state == CardStateName::Review,
                QueueName::Suspended | QueueName::SchedBuried | QueueName::UserBuried => true,
            };
            if !queue_fits {
                bad("queue does not fit the state");
            }
            if card.due_at_ms.is_some_and(|due| !is_plausible_time(due)) {
                bad("dueAtMs is outside 2000–2200");
            }
            if card
                .last_review_at_ms
                .is_some_and(|at| !is_plausible_time(at))
            {
                bad("lastReviewAtMs is outside 2000–2200");
            }
            if card.stability.is_some() != card.difficulty.is_some() {
                bad("stability and difficulty come together");
            }
            if card.stability.is_some_and(|s| !(s.is_finite() && s > 0.0)) {
                bad("stability must be positive");
            }
            if card.difficulty.is_some_and(|d| !(1.0..=10.0).contains(&d)) {
                bad("difficulty must be 1–10");
            }
            if card.reps < 0
                || card.lapses < 0
                || card.scheduled_days < 0
                || card.scheduled_days > 36_500
            {
                bad("negative counters or an interval beyond 100 years");
            }
            if card.flag > 7 {
                bad("flag must be 0–7");
            }
            if !is_new {
                self.scheduled += 1;
            }
            if matches!(
                card.queue,
                QueueName::Suspended | QueueName::SchedBuried | QueueName::UserBuried
            ) {
                self.suspended += 1;
            }
        }
        let mut previous = i64::MIN;
        for review in &record.reviews {
            self.reviews += 1;
            let mut bad = |reason: &str| {
                issues.error(
                    "review.invalid",
                    &file,
                    Some(line),
                    format!("review at {}: {reason}", review.reviewed_at_ms),
                )
            };
            if review.ordinal >= templates {
                bad("ordinal has no template in this note type");
            }
            if !(1..=4).contains(&review.rating) {
                bad("rating must be 1–4");
            }
            if !is_plausible_time(review.reviewed_at_ms) {
                bad("reviewedAtMs is outside 2000–2200");
            }
            if review.reviewed_at_ms < previous {
                bad("reviews are listed oldest first");
            }
            previous = review.reviewed_at_ms;
            if !(0..=86_400_000).contains(&review.duration_ms)
                || !(0..=36_500).contains(&review.scheduled_days)
            {
                bad("duration or interval out of range");
            }
        }
    }

    fn word_links(
        &mut self,
        line: u32,
        record: &NoteRecord,
        supported: Option<bool>,
        issues: &mut Issues,
    ) {
        let file = &self.file;
        let sentences = self.manifest.content_kind == ContentKind::Sentences;
        if !sentences && !record.word_links.is_empty() {
            issues.error(
                "wordLinks.unexpected",
                file,
                Some(line),
                "only sentence notes link words",
            );
            return;
        }
        if supported == Some(false) && !record.word_links.is_empty() {
            issues.error(
                "wordLinks.unsupported",
                file,
                Some(line),
                "the deck type shows no word links",
            );
        }
        if sentences && self.manifest.source == SourceKind::Bundled && record.word_links.is_empty()
        {
            issues.error(
                "wordLinks.required",
                file,
                Some(line),
                "a sentence note links at least one word",
            );
        }
        let mut previous_end = 0;
        for link in &record.word_links {
            match utf16_slice(&record.front, link.start, link.end) {
                None => issues.error(
                    "range.invalid",
                    file,
                    Some(line),
                    format!(
                        "range [{}, {}) is empty, out of bounds or splits a character",
                        link.start, link.end
                    ),
                ),
                Some(slice) if slice != link.surface_form => issues.error(
                    "range.mismatch",
                    file,
                    Some(line),
                    format!(
                        "range [{}, {}) is {slice:?}, not {:?}",
                        link.start, link.end, link.surface_form
                    ),
                ),
                Some(_) => {}
            }
            if link.start < previous_end {
                issues.error(
                    "range.overlap",
                    file,
                    Some(line),
                    "links must be sorted and must not overlap",
                );
            }
            previous_end = previous_end.max(link.end);
            if link.lemma.trim().is_empty() || !is_nfc(&link.lemma) {
                issues.error("link.lemma", file, Some(line), "lemma is empty or not NFC");
            }
            if !self.dependency_guids.contains(&link.word_guid) {
                issues.error(
                    "link.unresolved",
                    file,
                    Some(line),
                    format!(
                        "wordGuid {} is not a note of a dependency deck",
                        link.word_guid
                    ),
                );
            }
        }
    }

    /// Checks done once the whole file was read.
    pub fn finish(self, sha256: &str, issues: &mut Issues) {
        let manifest = self.manifest;
        if sha256 != manifest.sha256 {
            issues.error(
                "checksum.mismatch",
                &self.file,
                None,
                "notes file does not match the manifest sha256",
            );
        }
        if self.notes != manifest.expected_notes {
            issues.error(
                "counts.notes",
                &self.file,
                None,
                format!(
                    "{} notes, manifest expects {}",
                    self.notes, manifest.expected_notes
                ),
            );
        }
        if let Some(kind) = self.kind {
            let cards = self.notes * kind.templates().len() as u32;
            if cards != manifest.expected_cards {
                issues.error(
                    "counts.cards",
                    &self.file,
                    None,
                    format!(
                        "{cards} cards generated, manifest expects {}",
                        manifest.expected_cards
                    ),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_versions_are_dates_with_dots() {
        assert!(is_content_version("2026.11.04"));
        for bad in [
            "2026.11.4",
            "2026-11-04",
            "v1",
            "2026.11.04.1",
            "abcd.ef.gh",
        ] {
            assert!(!is_content_version(bad), "{bad}");
        }
    }

    #[test]
    fn prompt_ids_carry_a_version() {
        assert!(is_prompt_id("association-candidates-v1"));
        assert!(is_prompt_id("x-v12"));
        for bad in ["association", "-v1", "Assoc-v1", "a-v", "a-vx"] {
            assert!(!is_prompt_id(bad), "{bad}");
        }
    }

    #[test]
    fn names_of_types_and_languages_map_to_the_domain() {
        assert_eq!(parse_deck_type("Linguistic"), Some(DeckType::Linguistic));
        assert_eq!(
            parse_note_type("BasicAndReversed"),
            Some(NoteTypeKind::BasicAndReversed)
        );
        assert_eq!(parse_target_language("en"), Some(TargetLanguage::En));
        assert_eq!(parse_native_language("pl"), Some(NativeLanguage::Pl));
        assert_eq!(parse_deck_type("linguistic"), None);
        assert_eq!(parse_target_language("fr"), None);
    }

    #[test]
    fn text_rules_flag_each_problem() {
        let mut issues = Issues::default();
        check_text("front", " x", 10, true, "f", 1, &mut issues);
        check_text("front", "z\u{0307}", 10, true, "f", 2, &mut issues);
        check_text("front", "<b>x</b>", 10, true, "f", 3, &mut issues);
        check_text("front", "a\u{0007}", 10, true, "f", 4, &mut issues);
        check_text("front", "abcdefghijk", 10, true, "f", 5, &mut issues);
        check_text("front", "", 10, true, "f", 6, &mut issues);
        check_text("front", "x < y", 10, true, "f", 7, &mut issues);
        let codes: Vec<_> = issues
            .0
            .iter()
            .map(|issue| (issue.line.unwrap(), issue.code))
            .collect();
        assert_eq!(
            codes,
            [
                (1, "text.whitespace"),
                (2, "text.notNfc"),
                (3, "text.markup"),
                (4, "text.control"),
                (5, "text.tooLong"),
                (6, "text.required"),
            ]
        );
    }
}
