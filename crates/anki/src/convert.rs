//! Anki collection → LanguaLinker import package (format v2, `source: "import"`).
//!
//! Scheduling mapping (Anki → LanguaLinker):
//! - `cards.type` 0/1/2/3 → state New/Learning/Review/Relearning; `cards.queue` keeps suspended
//!   and buried cards; cards in a filtered deck use their home deck and original due (`odue`).
//! - `due` of a new card is its position; otherwise a value above 10^9 is epoch seconds
//!   (intraday learning), anything else days since `col.crt` → UTC epoch milliseconds.
//! - `ivl` (days; negative = seconds while learning) → `scheduledDays`; `factor` → ease (‰);
//!   `reps`, `lapses`, flags 0–7; FSRS memory state from `cards.data` (`s`, `d`) on Anki 23.10+.
//! - `revlog` rows with a rating (ease 1–4) become review records; ease 0 rows (manual
//!   rescheduling) are skipped. Learning-step position is not carried: a learning card restarts
//!   its current step list.

use std::collections::{HashMap, HashSet};

use langualinker_content::check::Severity;
use langualinker_content::format::{
    CardRecord, CardStateName, ContentKind, DeckManifest, License, NoteRecord, Origin,
    PACKAGE_VERSION, PackageHeader, QueueName, Review as ReviewInfo, ReviewKindName, ReviewRecord,
    ReviewStatus, SCHEMA_VERSION, SourceKind,
};
use langualinker_content::reader::sha256_hex;
use langualinker_content::{Package, check};
use langualinker_domain::deck::{DeckType, TargetLanguage};
use langualinker_domain::note::NoteTypeKind;
use langualinker_domain::validation;
use sha2::{Digest, Sha256};

use crate::collection::{Card, Collection, Note, Review};
use crate::html::flatten;

/// Which Anki field becomes the front or the back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldRef {
    /// 0-based position in the note type's fields.
    Index(usize),
    /// Field name (case-insensitive).
    Name(String),
}

#[derive(Debug, Clone)]
pub struct Options {
    /// Deck name in LanguaLinker; defaults to the Anki deck (last level) or the file name.
    pub name: Option<String>,
    pub description: Option<String>,
    pub language: TargetLanguage,
    pub kind: ContentKind,
    /// Two cards per note (front→back and back→front).
    pub reverse: bool,
    pub deck_type: DeckType,
    pub level: String,
    /// Only this Anki deck and its subdecks.
    pub anki_deck: Option<String>,
    pub front_field: FieldRef,
    pub back_field: FieldRef,
    /// Carry due dates, intervals, ease, reps, lapses, queues, flags and FSRS state.
    pub scheduling: bool,
    /// Carry the review history (requires `scheduling`).
    pub reviews: bool,
    /// Stable deck GUID; the default derives from the deck name, so converting the same deck
    /// again updates it on import.
    pub deck_guid: Option<String>,
    /// Name of the `.apkg` file (no directories).
    pub file_name: String,
    /// Date of the conversion (`YYYY-MM-DD`), injected for reproducible output.
    pub today: chrono::NaiveDate,
    /// Fail instead of dropping notes the content rules reject.
    pub strict: bool,
}

impl Options {
    pub fn note_type(&self) -> NoteTypeKind {
        match (self.kind, self.reverse) {
            (ContentKind::Words, false) => NoteTypeKind::Basic,
            (ContentKind::Words, true) => NoteTypeKind::BasicAndReversed,
            (ContentKind::Sentences, false) => NoteTypeKind::Sentence,
            (ContentKind::Sentences, true) => NoteTypeKind::SentenceAndReversed,
        }
    }
}

/// What was converted and what was lost, for the user.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    pub notes: u32,
    pub cards: u32,
    pub cards_with_progress: u32,
    pub suspended_or_buried: u32,
    pub fsrs_cards: u32,
    pub learning_cards: u32,
    pub reviews: u32,
    pub skipped_cloze: u32,
    pub skipped_empty_front: u32,
    pub skipped_missing_field: u32,
    pub skipped_duplicates: u32,
    /// Notes the content rules rejected, with the rule code.
    pub skipped_invalid: Vec<(String, &'static str)>,
    /// Anki cards of templates the chosen note type does not have (e.g. reverse cards when
    /// `--reverse` is off).
    pub dropped_cards: u32,
    pub dropped_cards_with_progress: u32,
    pub dropped_reviews: u32,
    pub manual_reviews_skipped: u32,
    pub fields_with_markup: u32,
    pub media_refs_removed: u32,
    pub media_files_not_converted: u32,
    pub tags_dropped: u32,
}

#[derive(Debug, Clone)]
pub struct Conversion {
    pub manifest: DeckManifest,
    /// The import package file (NDJSON).
    pub package: Vec<u8>,
    pub report: Report,
}

const DAY_SECS: i64 = 86_400;
/// Anki's threshold between day numbers and epoch seconds in `cards.due`.
const EPOCH_SECONDS_THRESHOLD: i64 = 1_000_000_000;

fn hex(bytes: &[u8], chars: usize) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()[..chars]
        .to_owned()
}

fn derived_guid(prefix: &str, value: &str, chars: usize) -> String {
    format!("{prefix}{}", hex(&Sha256::digest(value.as_bytes()), chars))
}

fn deck_type_name(value: DeckType) -> &'static str {
    match value {
        DeckType::Simple => "Simple",
        DeckType::TextWithAssociations => "TextWithAssociations",
        DeckType::Linguistic => "Linguistic",
    }
}

fn note_type_name(value: NoteTypeKind) -> &'static str {
    match value {
        NoteTypeKind::Basic => "Basic",
        NoteTypeKind::BasicAndReversed => "BasicAndReversed",
        NoteTypeKind::Sentence => "Sentence",
        NoteTypeKind::SentenceAndReversed => "SentenceAndReversed",
    }
}

fn truncate(text: &str, max: usize) -> String {
    text.chars().take(max).collect::<String>().trim().to_owned()
}

fn is_cloze_text(fields: &[String]) -> bool {
    fields.iter().any(|field| {
        field.match_indices("{{c").any(|(index, _)| {
            let rest = &field[index + 3..];
            let digits = rest.chars().take_while(char::is_ascii_digit).count();
            digits > 0 && rest[digits..].starts_with("::")
        })
    })
}

fn field_index(fields: &[String], reference: &FieldRef) -> Option<usize> {
    match reference {
        FieldRef::Index(index) => (*index < fields.len()).then_some(*index),
        FieldRef::Name(name) => fields
            .iter()
            .position(|field| field.eq_ignore_ascii_case(name)),
    }
}

/// Converts the collection with the given options.
pub fn convert(collection: &Collection, options: &Options) -> Result<Conversion, String> {
    let selected: Option<HashSet<i64>> = match &options.anki_deck {
        Some(name) => {
            let ids: HashSet<i64> = collection.deck_tree(name).into_iter().collect();
            if ids.is_empty() {
                return Err(format!("no Anki deck named \"{name}\" (see --list)"));
            }
            Some(ids)
        }
        None => None,
    };
    let mut cards_by_note: HashMap<i64, Vec<&Card>> = HashMap::new();
    for card in &collection.cards {
        if selected
            .as_ref()
            .is_none_or(|ids| ids.contains(&card.deck_id))
        {
            cards_by_note.entry(card.note_id).or_default().push(card);
        }
    }
    let mut reviews_by_card: HashMap<i64, Vec<&Review>> = HashMap::new();
    for review in &collection.reviews {
        reviews_by_card
            .entry(review.card_id)
            .or_default()
            .push(review);
    }

    let note_type = options.note_type();
    let templates = note_type.templates().len() as u32;
    let mut report = Report {
        media_files_not_converted: collection.media_files as u32,
        ..Report::default()
    };
    let mut records = Vec::new();
    let mut seen_pairs = HashSet::new();
    for note in &collection.notes {
        let Some(cards) = cards_by_note.get(&note.id) else {
            continue;
        };
        if let Some(record) = convert_note(
            collection,
            options,
            note,
            cards,
            &reviews_by_card,
            templates,
            &mut report,
        ) {
            let key = (record.front.to_lowercase(), record.back.clone());
            if seen_pairs.insert(key) {
                records.push(record);
            } else {
                report.skipped_duplicates += 1;
            }
        }
    }

    let source_decks: Vec<String> = {
        let mut ids: Vec<i64> = cards_by_note
            .values()
            .flatten()
            .map(|card| card.deck_id)
            .collect();
        ids.sort_unstable();
        ids.dedup();
        let mut names: Vec<String> = ids
            .iter()
            .filter_map(|id| collection.decks.get(id))
            .map(|deck| deck.name.clone())
            .collect();
        names.sort();
        names
    };
    let default_name = options
        .anki_deck
        .as_deref()
        .and_then(|name| name.rsplit("::").next())
        .map(str::to_owned)
        .unwrap_or_else(|| options.file_name.trim_end_matches(".apkg").to_owned());
    let name = truncate(
        &langualinker_domain::text::nfc(options.name.as_deref().unwrap_or(&default_name)),
        validation::MAX_NAME_CHARS,
    );
    if name.is_empty() {
        return Err("the deck name is empty; pass --name".into());
    }
    let deck_guid = match &options.deck_guid {
        Some(guid) => guid.clone(),
        None => derived_guid("anki-deck-", &name.to_lowercase(), 16),
    };
    let description = truncate(
        options
            .description
            .as_deref()
            .unwrap_or(&format!("Import z Anki: {}", options.file_name)),
        validation::MAX_DESCRIPTION_CHARS,
    );
    let mut manifest = DeckManifest {
        schema_version: SCHEMA_VERSION,
        source: SourceKind::Import,
        origin: Some(Origin {
            kind: "apkg".into(),
            file_name: options.file_name.clone(),
            converted_at: options.today.format("%Y-%m-%d").to_string(),
            tool: format!("apkg-convert {}", env!("CARGO_PKG_VERSION")),
            source_decks,
        }),
        deck_guid,
        content_version: options.today.format("%Y.%m.%d").to_string(),
        name,
        description,
        level: options.level.clone(),
        native_language: "pl".into(),
        target_language: options.language.code().into(),
        deck_type: deck_type_name(options.deck_type).into(),
        note_type: note_type_name(note_type).into(),
        content_kind: options.kind,
        depends_on: Vec::new(),
        notes_file: "notes.ndjson".into(),
        sha256: String::new(),
        expected_notes: 0,
        expected_cards: 0,
        license: License {
            name: "Personal import from Anki — not for distribution".into(),
            attribution: None,
            url: None,
            rights_confirmed: false,
        },
        review: ReviewInfo {
            status: ReviewStatus::Pending,
            reviewer: None,
            reviewed_at: None,
        },
    };

    // Build, check with the content rules, drop what they reject (unless strict), rebuild.
    for attempt in 0..2 {
        let package = render(&mut manifest, &records, templates)?;
        let parsed = match Package::parse(&package) {
            Ok(Ok(parsed)) => parsed,
            Ok(Err(issues)) => {
                return Err(format!(
                    "internal: package header rejected: {:?}",
                    issues.first_error()
                ));
            }
            Err(kind) => return Err(format!("internal: not a package: {kind:?}")),
        };
        let checked = parsed.check();
        let errors: Vec<&check::Issue> = checked
            .issues
            .0
            .iter()
            .filter(|issue| issue.severity == Severity::Error)
            .collect();
        if errors.is_empty() {
            report.notes = records.len() as u32;
            report.cards = manifest.expected_cards;
            report.reviews = checked.reviews;
            report.suspended_or_buried = checked.suspended;
            return Ok(Conversion {
                manifest,
                package,
                report,
            });
        }
        let without_line: Vec<&&check::Issue> = errors
            .iter()
            .filter(|issue| issue.line.is_none_or(|line| line < 2))
            .collect();
        if options.strict || attempt == 1 || !without_line.is_empty() {
            let lines: Vec<String> = errors
                .iter()
                .take(20)
                .map(|issue| issue.to_string())
                .collect();
            return Err(format!(
                "the converted deck breaks the content rules:\n{}",
                lines.join("\n")
            ));
        }
        // Record index = file line − 2 (line 1 is the header).
        let mut rejected: HashMap<usize, &'static str> = HashMap::new();
        for issue in errors {
            if let Some(line) = issue.line {
                rejected.entry(line as usize - 2).or_insert(issue.code);
            }
        }
        let mut kept = Vec::with_capacity(records.len());
        for (index, record) in records.into_iter().enumerate() {
            match rejected.get(&index) {
                Some(code) => report.skipped_invalid.push((record.front.clone(), code)),
                None => kept.push(record),
            }
        }
        records = kept;
    }
    unreachable!("the loop returns on its second attempt")
}

fn render(
    manifest: &mut DeckManifest,
    records: &[NoteRecord],
    templates: u32,
) -> Result<Vec<u8>, String> {
    let mut body = Vec::new();
    for record in records {
        body.extend_from_slice(
            serde_json::to_string(record)
                .map_err(|error| error.to_string())?
                .as_bytes(),
        );
        body.push(b'\n');
    }
    manifest.sha256 = sha256_hex(&body);
    manifest.expected_notes = records.len() as u32;
    manifest.expected_cards = records.len() as u32 * templates;
    let header = PackageHeader {
        package: PACKAGE_VERSION,
        manifest: manifest.clone(),
    };
    let mut package = serde_json::to_vec(&header).map_err(|error| error.to_string())?;
    package.push(b'\n');
    package.extend_from_slice(&body);
    Ok(package)
}

#[allow(clippy::too_many_arguments)]
fn convert_note(
    collection: &Collection,
    options: &Options,
    note: &Note,
    cards: &[&Card],
    reviews_by_card: &HashMap<i64, Vec<&Review>>,
    templates: u32,
    report: &mut Report,
) -> Option<NoteRecord> {
    let note_type = collection.note_types.get(&note.notetype_id);
    if note_type.is_some_and(|kind| kind.cloze) || is_cloze_text(&note.fields) {
        report.skipped_cloze += 1;
        return None;
    }
    let field_names: Vec<String> = match note_type {
        Some(kind) if kind.fields.len() == note.fields.len() => kind.fields.clone(),
        _ => (0..note.fields.len())
            .map(|index| index.to_string())
            .collect(),
    };
    let (Some(front_index), Some(back_index)) = (
        field_index(&field_names, &options.front_field),
        field_index(&field_names, &options.back_field),
    ) else {
        report.skipped_missing_field += 1;
        return None;
    };
    let front = flatten(&note.fields[front_index], true);
    let back = flatten(&note.fields[back_index], false);
    report.fields_with_markup += u32::from(front.had_markup) + u32::from(back.had_markup);
    report.media_refs_removed += (front.media_refs + back.media_refs) as u32;
    if front.text.is_empty() {
        report.skipped_empty_front += 1;
        return None;
    }
    let mut tags = Vec::new();
    let mut lowered = HashSet::new();
    for tag in &note.tags {
        let tag = langualinker_domain::text::nfc(tag);
        if tag.chars().count() > validation::MAX_TAG_CHARS
            || tags.len() == validation::MAX_TAGS
            || !lowered.insert(tag.to_lowercase())
        {
            report.tags_dropped += 1;
            continue;
        }
        tags.push(tag);
    }

    let mut card_records = Vec::new();
    let mut review_records = Vec::new();
    for card in cards {
        let reviews = reviews_by_card
            .get(&card.id)
            .map(Vec::as_slice)
            .unwrap_or_default();
        if card.ord >= templates {
            report.dropped_cards += 1;
            if card.kind != 0 || card.reps > 0 {
                report.dropped_cards_with_progress += 1;
            }
            report.dropped_reviews += reviews.len() as u32;
            continue;
        }
        if !options.scheduling {
            continue;
        }
        let record = card_record(collection.created_secs, card, reviews);
        if record.state != CardStateName::New {
            report.cards_with_progress += 1;
        }
        if record.stability.is_some() {
            report.fsrs_cards += 1;
        }
        if matches!(
            record.state,
            CardStateName::Learning | CardStateName::Relearning
        ) {
            report.learning_cards += 1;
        }
        card_records.push(record);
        if options.reviews {
            for review in reviews {
                match review_record(card.ord, review) {
                    Some(record) => review_records.push(record),
                    None => report.manual_reviews_skipped += 1,
                }
            }
        }
    }
    card_records.sort_by_key(|card| card.ordinal);
    review_records.sort_by_key(|review| (review.reviewed_at_ms, review.ordinal));
    Some(NoteRecord {
        guid: derived_guid("anki-", &note.guid, 32),
        front: front.text,
        back: back.text,
        tags,
        associations: Vec::new(),
        grammar_tips: Vec::new(),
        word_links: Vec::new(),
        cards: card_records,
        reviews: review_records,
    })
}

fn state_of(card: &Card) -> CardStateName {
    match card.kind {
        0 => CardStateName::New,
        1 => CardStateName::Learning,
        3 => CardStateName::Relearning,
        _ => CardStateName::Review,
    }
}

/// The queue a card sits in when it is neither suspended nor buried.
fn natural_queue(state: CardStateName, due: i64) -> QueueName {
    match state {
        CardStateName::New => QueueName::New,
        CardStateName::Review => QueueName::Review,
        CardStateName::Learning | CardStateName::Relearning if due > EPOCH_SECONDS_THRESHOLD => {
            QueueName::Learning
        }
        CardStateName::Learning | CardStateName::Relearning => QueueName::DayLearn,
    }
}

fn card_record(created_secs: i64, card: &Card, reviews: &[&Review]) -> CardRecord {
    let state = state_of(card);
    let natural = natural_queue(state, card.due);
    let queue = match card.queue {
        -1 => QueueName::Suspended,
        -2 => QueueName::SchedBuried,
        -3 => QueueName::UserBuried,
        // 0–3 when consistent with the state; 4 (preview in a filtered deck) and anything
        // inconsistent fall back to the state's natural queue.
        0 if state == CardStateName::New => QueueName::New,
        2 if state == CardStateName::Review => QueueName::Review,
        1 | 3 if matches!(state, CardStateName::Learning | CardStateName::Relearning) => natural,
        _ => natural,
    };
    let is_new = state == CardStateName::New;
    let due_at_ms = (!is_new).then(|| {
        if card.due > EPOCH_SECONDS_THRESHOLD {
            card.due * 1_000
        } else {
            (created_secs + card.due * DAY_SECS) * 1_000
        }
    });
    let memory: Option<(f64, f64)> = serde_json::from_str::<serde_json::Value>(&card.data)
        .ok()
        .and_then(|data| Some((data.get("s")?.as_f64()?, data.get("d")?.as_f64()?)))
        .filter(|(s, d)| s.is_finite() && *s > 0.0 && (1.0..=10.0).contains(d));
    CardRecord {
        ordinal: card.ord,
        state,
        queue,
        due_at_ms,
        position: is_new.then_some(card.due.max(0)),
        scheduled_days: card.ivl.clamp(0, 36_500),
        stability: memory.map(|(s, _)| s),
        difficulty: memory.map(|(_, d)| d),
        ease: sm2_ease(card.factor),
        reps: card.reps.max(0),
        lapses: card.lapses.max(0),
        last_review_at_ms: reviews
            .iter()
            .filter(|review| review.ease > 0)
            .map(|review| review.id)
            .max(),
        flag: (card.flags & 0b111) as u8,
    }
}

/// SM-2 ease in permille. Anki's minimum ease is 1300 ‰; under FSRS the same columns hold the
/// difficulty (×100, so 100–1000), which is not an ease and is dropped.
fn sm2_ease(factor: i64) -> Option<i64> {
    (factor >= 1_300).then_some(factor)
}

fn review_record(ordinal: u32, review: &Review) -> Option<ReviewRecord> {
    if !(1..=4).contains(&review.ease) {
        return None;
    }
    Some(ReviewRecord {
        ordinal,
        reviewed_at_ms: review.id,
        rating: review.ease as u8,
        kind: match review.kind {
            0 => ReviewKindName::Learning,
            1 => ReviewKindName::Review,
            2 => ReviewKindName::Relearning,
            3 => ReviewKindName::Filtered,
            _ => ReviewKindName::Manual,
        },
        scheduled_days: review.ivl.clamp(0, 36_500),
        ease: sm2_ease(review.factor),
        duration_ms: review.time_ms.clamp(0, 86_400_000),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloze_markers_are_detected() {
        assert!(is_cloze_text(&["Das {{c1::Haus}} ist rot".into()]));
        assert!(!is_cloze_text(&["{{c::x}}".into(), "{{Front}}".into()]));
    }

    #[test]
    fn day_and_second_based_due_values_map_to_utc() {
        let created = 1_600_000_000; // col.crt
        let card = |kind, queue, due| Card {
            id: 1,
            note_id: 1,
            deck_id: 1,
            ord: 0,
            kind,
            queue,
            due,
            ivl: 5,
            factor: 2500,
            reps: 3,
            lapses: 0,
            flags: 0,
            data: String::new(),
        };
        let review = card_record(created, &card(2, 2, 10), &[]);
        assert_eq!(review.due_at_ms, Some((created + 10 * DAY_SECS) * 1_000));
        assert_eq!(review.queue, QueueName::Review);
        let learning = card_record(created, &card(1, 1, 1_700_000_000), &[]);
        assert_eq!(learning.due_at_ms, Some(1_700_000_000_000));
        assert_eq!(learning.queue, QueueName::Learning);
        let new = card_record(created, &card(0, 0, 42), &[]);
        assert_eq!((new.due_at_ms, new.position), (None, Some(42)));
        let suspended = card_record(created, &card(2, -1, 10), &[]);
        assert_eq!(suspended.queue, QueueName::Suspended);
        let preview = card_record(created, &card(2, 4, 10), &[]);
        assert_eq!(
            preview.queue,
            QueueName::Review,
            "filtered preview queue falls back"
        );
    }
}
