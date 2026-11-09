//! Card browser (plan Phase 11 step 3): the search language, keyset paging and bulk actions.
//!
//! Search language: free words (full-text, prefix match per word, diacritics folded by FTS5),
//! `deck:<prefix>` (deck name, subdecks included), `tag:<name>`, `is:new|due|suspended` and
//! `added:<days>` (notes created during the last N study days, today included). Values may be
//! quoted (`deck:"Niemiecki — zdania"`). Unknown fields, values and unbalanced quotes are
//! validation errors naming the token; nothing typed by the user is ever interpolated into SQL.
//!
//! Paging is keyset over (sort value, card id) with a separate total. A page carries a
//! `filterVersion`: the search itself plus the collection fingerprint it ran against. A bulk
//! action over "all matches" re-runs that search and refuses (`PREVIEW_STALE`) when the
//! collection changed in between, so it never acts on rows the user did not see.

use std::collections::{HashMap, HashSet};

use langualinker_domain::card::{CardFlag, CardState, QueueState};
use langualinker_domain::scheduling;
use langualinker_domain::text::nfc;
use langualinker_domain::validation;
use langualinker_domain::{CardId, DeckId, NoteId, TimestampMs, ValidationError};
use serde::{Deserialize, Serialize};

use crate::learning;
use crate::management::{active_deck, subtree};
use crate::repositories::{CardCursor, CardFilter, CardRow, CardSort, Repositories};
use crate::study;
use crate::{AppError, AppResult};

/// Largest page of the browser.
pub const MAX_PAGE_SIZE: u32 = 500;
/// Largest explicit selection of a bulk action.
pub const MAX_SELECTED_IDS: usize = 100_000;

const OP_BULK: &str = "applyBulkAction";

// ---------------------------------------------------------------- query language

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IsFilter {
    New,
    Due,
    Suspended,
}

/// A parsed query; free words keep their order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedQuery {
    pub words: Vec<String>,
    pub deck: Option<String>,
    pub tags: Vec<String>,
    pub is: Option<IsFilter>,
    pub added_days: Option<u32>,
}

fn query_error(reason: &'static str) -> AppError {
    ValidationError::new("query", reason).into()
}

/// Splits the query into `(field, value)` tokens; a quoted value may contain spaces.
fn tokens(query: &str) -> AppResult<Vec<(Option<String>, String)>> {
    if query.matches('"').count() % 2 == 1 {
        return Err(query_error("unbalancedQuote"));
    }
    let mut result = Vec::new();
    let mut rest = query.trim_start();
    while !rest.is_empty() {
        // An optional `field:` prefix: word characters starting with a letter.
        let field_end = rest
            .char_indices()
            .find(|(_, ch)| !(ch.is_alphanumeric() || *ch == '_'))
            .map_or(rest.len(), |(index, _)| index);
        let starts_with_letter = rest.chars().next().is_some_and(char::is_alphabetic);
        let (field, after) =
            if starts_with_letter && field_end > 0 && rest[field_end..].starts_with(':') {
                (
                    Some(rest[..field_end].to_lowercase()),
                    &rest[field_end + 1..],
                )
            } else {
                (None, rest)
            };
        let (value, remaining) = if let Some(quoted) = after.strip_prefix('"') {
            let close = quoted
                .find('"')
                .ok_or_else(|| query_error("unbalancedQuote"))?;
            (&quoted[..close], &quoted[close + 1..])
        } else {
            let end = after.find(char::is_whitespace).unwrap_or(after.len());
            (&after[..end], &after[end..])
        };
        result.push((field, value.to_owned()));
        rest = remaining.trim_start();
    }
    Ok(result)
}

/// Parses the browser's search language.
pub fn parse_query(query: &str) -> AppResult<ParsedQuery> {
    let mut parsed = ParsedQuery::default();
    for (field, value) in tokens(&nfc(query))? {
        let value = value.trim().to_owned();
        match field.as_deref() {
            None => {
                if !value.is_empty() {
                    parsed.words.push(value);
                }
            }
            Some("deck") => parsed.deck = Some(value.to_lowercase()),
            Some("tag") => {
                if value.is_empty() || value.chars().any(char::is_whitespace) {
                    return Err(query_error("invalidValue"));
                }
                parsed.tags.push(value);
            }
            Some("is") => {
                parsed.is = Some(match value.to_lowercase().as_str() {
                    "new" => IsFilter::New,
                    "due" => IsFilter::Due,
                    "suspended" => IsFilter::Suspended,
                    _ => return Err(query_error("unknownValue")),
                });
            }
            Some("added") => {
                let days = value
                    .parse::<u32>()
                    .ok()
                    .filter(|days| (1..=36_500).contains(days))
                    .ok_or_else(|| query_error("invalidValue"))?;
                parsed.added_days = Some(days);
            }
            Some(_) => return Err(query_error("unknownField")),
        }
    }
    Ok(parsed)
}

/// FTS5 query for the free words: each word a quoted prefix phrase (quotes doubled), all
/// required. Words without a letter or digit cannot match a token and are skipped.
pub fn fts_query(words: &[String]) -> Option<String> {
    let phrases: Vec<String> = words
        .iter()
        .filter(|word| word.chars().any(char::is_alphanumeric))
        .map(|word| format!("\"{}\"*", word.replace('"', "\"\"")))
        .collect();
    (!phrases.is_empty()).then(|| phrases.join(" "))
}

// ---------------------------------------------------------------- search

/// What the browser asks for, without paging. Serialized into the filter version.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CardSearch {
    pub query: String,
    pub deck_id: Option<i64>,
    pub tag: Option<String>,
    /// Card state codes (`CardState::db_code`).
    pub states: Vec<i64>,
    /// Flag codes (`CardFlag::db_code`).
    pub flags: Vec<i64>,
    pub sort: SortName,
    pub descending: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SortName {
    Due,
    Created,
    Front,
    Deck,
    Reps,
    Lapses,
}

impl SortName {
    fn sort(self) -> CardSort {
        match self {
            Self::Due => CardSort::Due,
            Self::Created => CardSort::Created,
            Self::Front => CardSort::Front,
            Self::Deck => CardSort::Deck,
            Self::Reps => CardSort::Reps,
            Self::Lapses => CardSort::Lapses,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CardPage {
    pub rows: Vec<CardRow>,
    pub next_cursor: Option<String>,
    pub total: u32,
    pub filter_version: String,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) || text.len() > 16_384 {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(text.get(index..index + 2)?, 16).ok())
        .collect()
}

fn encode_cursor(cursor: &CardCursor) -> String {
    hex(serde_json::to_string(cursor)
        .expect("cursor serializes")
        .as_bytes())
}

fn decode_cursor(text: &str) -> AppResult<CardCursor> {
    unhex(text)
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .ok_or_else(|| ValidationError::new("cursor", "format").into())
}

#[derive(Serialize, Deserialize)]
struct FilterVersion {
    search: CardSearch,
    fingerprint: String,
}

const FILTER_VERSION_PREFIX: &str = "fv1.";

fn encode_filter_version(search: &CardSearch, fingerprint: String) -> String {
    let version = FilterVersion {
        search: search.clone(),
        fingerprint,
    };
    format!(
        "{FILTER_VERSION_PREFIX}{}",
        hex(serde_json::to_string(&version)
            .expect("filter version serializes")
            .as_bytes())
    )
}

fn decode_filter_version(text: &str) -> AppResult<FilterVersion> {
    text.strip_prefix(FILTER_VERSION_PREFIX)
        .and_then(unhex)
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .ok_or_else(|| ValidationError::new("filterVersion", "format").into())
}

/// Resolves a search into the repository filter (deck names and dates become ids and times).
fn card_filter(
    tx: &mut dyn Repositories,
    search: &CardSearch,
    now: TimestampMs,
) -> AppResult<CardFilter> {
    let parsed = parse_query(&search.query)?;
    let mut filter = CardFilter {
        fts: fts_query(&parsed.words),
        ..CardFilter::default()
    };
    let decks = tx.active_decks()?;
    let mut scope: Option<HashSet<DeckId>> = None;
    if let Some(deck_id) = search.deck_id {
        let deck = active_deck(tx, DeckId(deck_id))
            .map_err(|_| ValidationError::new("deckId", "notFound"))?;
        scope = Some(subtree(&decks, deck.id).into_iter().collect());
    }
    if let Some(prefix) = &parsed.deck {
        let mut named = HashSet::new();
        for deck in decks
            .iter()
            .filter(|deck| deck.name.to_lowercase().starts_with(prefix.as_str()))
        {
            named.extend(subtree(&decks, deck.id));
        }
        scope = Some(match scope {
            Some(current) => current.intersection(&named).copied().collect(),
            None => named,
        });
    }
    if let Some(scope) = scope {
        let mut ids: Vec<DeckId> = scope.into_iter().collect();
        ids.sort_by_key(|id| id.0);
        filter.decks = Some(ids);
    }
    filter.tags = parsed.tags;
    if let Some(tag) = &search.tag {
        let tag = nfc(tag.trim());
        if !tag.is_empty() {
            filter.tags.push(tag);
        }
    }
    let moment = if parsed.is == Some(IsFilter::Due) || parsed.added_days.is_some() {
        Some(study::study_moment(tx, now)?)
    } else {
        None
    };
    match parsed.is {
        Some(IsFilter::New) => filter.new_only = true,
        Some(IsFilter::Suspended) => filter.suspended_only = true,
        // Due today: scheduled cards due before the end of the current study day.
        Some(IsFilter::Due) => filter.due_before = moment.as_ref().map(|moment| moment.day_end),
        None => {}
    }
    if let (Some(days), Some(moment)) = (parsed.added_days, &moment) {
        let first = moment.today.plus(-(i64::from(days) - 1));
        filter.added_since = Some(moment.calendar.day_start(first));
    }
    filter.states = search
        .states
        .iter()
        .map(|code| CardState::from_db_code(*code))
        .collect::<Result<_, _>>()
        .map_err(|_| ValidationError::new("states", "unknown"))?;
    filter.flags = search
        .flags
        .iter()
        .map(|code| CardFlag::from_db_code(*code))
        .collect::<Result<_, _>>()
        .map_err(|_| ValidationError::new("flags", "unknown"))?;
    Ok(filter)
}

/// SearchCards: one page after `cursor`, the total of matches and the filter version.
pub fn search_cards(
    tx: &mut dyn Repositories,
    search: &CardSearch,
    cursor: Option<&str>,
    page_size: u32,
    now: TimestampMs,
) -> AppResult<CardPage> {
    validation::in_range(
        "pageSize",
        i64::from(page_size),
        1,
        i64::from(MAX_PAGE_SIZE),
    )?;
    let after = cursor.map(decode_cursor).transpose()?;
    let filter = card_filter(tx, search, now)?;
    let sort = search.sort.sort();
    let mut rows = tx.search_cards(
        &filter,
        sort,
        search.descending,
        after.as_ref(),
        page_size + 1,
    )?;
    let more = rows.len() > page_size as usize;
    rows.truncate(page_size as usize);
    let next_cursor = if more {
        rows.last().map(|row| {
            encode_cursor(&CardCursor {
                value: row.sort_value.clone(),
                card_id: row.card.id.0,
            })
        })
    } else {
        None
    };
    let total = tx.count_cards(&filter)?;
    let fingerprint = tx.collection_fingerprint()?;
    Ok(CardPage {
        rows,
        next_cursor,
        total,
        filter_version: encode_filter_version(search, fingerprint),
    })
}

// ---------------------------------------------------------------- bulk actions

#[derive(Debug, Clone, PartialEq)]
pub enum BulkSelection {
    Ids(Vec<CardId>),
    /// Every match of the search behind `filter_version`, minus the excluded cards.
    Query {
        filter_version: String,
        excluded: Vec<CardId>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum BulkAction {
    Suspend,
    Unsuspend,
    Flag(CardFlag),
    ChangeDeck(DeckId),
    AddTag(String),
    RemoveTag(String),
    /// New cards only: positions start, start+step, …; studied cards are left alone.
    Reposition {
        start: i64,
        step: i64,
    },
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BulkPreview {
    pub selected: u32,
    pub affected: u32,
    pub unchanged: u32,
    pub notes: u32,
    pub decks: u32,
}

fn check_tag(tag: &str) -> AppResult<String> {
    let tag = nfc(tag.trim());
    if tag.is_empty() || tag.chars().any(char::is_whitespace) {
        return Err(ValidationError::new("tag", "invalid").into());
    }
    Ok(validation::bounded_text(
        "tag",
        &tag,
        validation::MAX_TAG_CHARS,
    )?)
}

fn check_action(tx: &mut dyn Repositories, action: &BulkAction) -> AppResult<BulkAction> {
    Ok(match action {
        BulkAction::AddTag(tag) => BulkAction::AddTag(check_tag(tag)?),
        BulkAction::RemoveTag(tag) => BulkAction::RemoveTag(check_tag(tag)?),
        BulkAction::ChangeDeck(deck) => {
            active_deck(tx, *deck).map_err(|_| ValidationError::new("deckId", "notFound"))?;
            action.clone()
        }
        BulkAction::Reposition { start, step } => {
            validation::in_range("start", *start, 1, 1_000_000_000)?;
            validation::in_range("step", *step, 1, 1_000_000)?;
            action.clone()
        }
        _ => action.clone(),
    })
}

/// The live cards of a selection, in browser order for a query selection.
fn selected_cards(
    tx: &mut dyn Repositories,
    selection: &BulkSelection,
    now: TimestampMs,
) -> AppResult<Vec<langualinker_domain::card::Card>> {
    let ids: Vec<CardId> = match selection {
        BulkSelection::Ids(ids) => {
            if ids.len() > MAX_SELECTED_IDS {
                return Err(ValidationError::new("cardIds", "count").into());
            }
            let mut seen = HashSet::new();
            ids.iter().copied().filter(|id| seen.insert(*id)).collect()
        }
        BulkSelection::Query {
            filter_version,
            excluded,
        } => {
            let version = decode_filter_version(filter_version)?;
            if tx.collection_fingerprint()? != version.fingerprint {
                return Err(AppError::PreviewStale);
            }
            let filter = card_filter(tx, &version.search, now)?;
            let excluded: HashSet<CardId> = excluded.iter().copied().collect();
            tx.matching_card_ids(
                &filter,
                version.search.sort.sort(),
                version.search.descending,
            )?
            .into_iter()
            .filter(|id| !excluded.contains(id))
            .collect()
        }
    };
    let mut cards = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(card) = tx.card(id)?
            && card.deleted_at.is_none()
        {
            cards.push(card);
        }
    }
    Ok(cards)
}

/// Whether the action would change the card (unchanged cards are counted, never touched).
fn changes(
    action: &BulkAction,
    card: &langualinker_domain::card::Card,
    tags: &HashMap<NoteId, Vec<String>>,
) -> bool {
    let has = |tag: &str| {
        tags.get(&card.note_id).is_some_and(|tags| {
            tags.iter()
                .any(|known| known.to_lowercase() == tag.to_lowercase())
        })
    };
    match action {
        BulkAction::Suspend => card.queue != QueueState::Suspended,
        BulkAction::Unsuspend => card.queue == QueueState::Suspended,
        BulkAction::Flag(flag) => card.flag != *flag,
        BulkAction::ChangeDeck(deck) => card.deck_id != *deck,
        BulkAction::AddTag(tag) => !has(tag),
        BulkAction::RemoveTag(tag) => has(tag),
        BulkAction::Reposition { .. } => card.state == CardState::New,
        BulkAction::Delete => true,
    }
}

fn note_tags(
    tx: &mut dyn Repositories,
    cards: &[langualinker_domain::card::Card],
    action: &BulkAction,
) -> AppResult<HashMap<NoteId, Vec<String>>> {
    let mut tags = HashMap::new();
    if matches!(action, BulkAction::AddTag(_) | BulkAction::RemoveTag(_)) {
        for card in cards {
            if let std::collections::hash_map::Entry::Vacant(entry) = tags.entry(card.note_id)
                && let Some(note) = tx.note(card.note_id)?
            {
                entry.insert(note.tags);
            }
        }
    }
    Ok(tags)
}

/// PreviewBulkAction: what the action would do. Read-only.
pub fn preview_bulk_action(
    tx: &mut dyn Repositories,
    selection: &BulkSelection,
    action: &BulkAction,
    now: TimestampMs,
) -> AppResult<BulkPreview> {
    let action = check_action(tx, action)?;
    let cards = selected_cards(tx, selection, now)?;
    let tags = note_tags(tx, &cards, &action)?;
    let changed: Vec<_> = cards
        .iter()
        .filter(|card| changes(&action, card, &tags))
        .collect();
    let notes: HashSet<NoteId> = changed.iter().map(|card| card.note_id).collect();
    let decks: HashSet<DeckId> = changed.iter().map(|card| card.deck_id).collect();
    let count = |value: usize| u32::try_from(value).unwrap_or(u32::MAX);
    Ok(BulkPreview {
        selected: count(cards.len()),
        affected: count(changed.len()),
        unchanged: count(cards.len() - changed.len()),
        notes: count(notes.len()),
        decks: count(decks.len()),
    })
}

/// ApplyBulkAction in one transaction: every affected card changes, or none does.
pub fn apply_bulk_action(
    tx: &mut dyn Repositories,
    operation_id: &str,
    selection: &BulkSelection,
    action: &BulkAction,
    now: TimestampMs,
) -> AppResult<u32> {
    if learning::replayed(tx, operation_id, OP_BULK)? {
        return tx
            .recorded_operation(operation_id)?
            .and_then(|recorded| recorded.result_json.parse::<u32>().ok())
            .ok_or_else(|| AppError::Internal("recorded bulk action unreadable".into()));
    }
    let action = check_action(tx, action)?;
    let cards = selected_cards(tx, selection, now)?;
    let tags = note_tags(tx, &cards, &action)?;
    let mut changed: Vec<_> = cards
        .into_iter()
        .filter(|card| changes(&action, card, &tags))
        .collect();
    let affected = u32::try_from(changed.len()).unwrap_or(u32::MAX);
    match &action {
        BulkAction::AddTag(tag) | BulkAction::RemoveTag(tag) => {
            let mut done = HashSet::new();
            for card in &changed {
                if !done.insert(card.note_id) {
                    continue;
                }
                let mut note = learning::live_note(tx, card.note_id)?;
                if matches!(action, BulkAction::AddTag(_)) {
                    note.tags.push(tag.clone());
                } else {
                    note.tags
                        .retain(|known| known.to_lowercase() != tag.to_lowercase());
                }
                tx.update_note(&note, now)?;
                if tx.bundled_note(note.id)?.is_some() {
                    let bundled = tx.bundled_note(note.id)?.expect("checked above");
                    if !tx
                        .content_overrides(note.id)?
                        .iter()
                        .any(|field| field == "tags")
                    {
                        tx.put_content_override(note.id, "tags", &bundled.content_version, now)?;
                    }
                }
            }
        }
        BulkAction::Delete => {
            for card in &changed {
                tx.soft_delete_card(card.id, card.version, now)?;
            }
            for note in tx.soft_delete_orphan_notes(now)? {
                tx.unresolve_links_to(note, now)?;
            }
        }
        BulkAction::Reposition { start, step } => {
            // Keeps the cards' current relative order.
            changed.sort_by_key(|card| (card.position, card.id.0));
            for (index, card) in changed.iter_mut().enumerate() {
                card.position = start + index as i64 * step;
                tx.update_card(card, now)?;
            }
        }
        other => {
            let moment = study::study_moment(tx, now)?;
            for card in &mut changed {
                match other {
                    BulkAction::Suspend => {
                        card.queue = QueueState::Suspended;
                        card.buried_until = None;
                    }
                    BulkAction::Unsuspend => card.queue = scheduling::natural_queue(card, &moment),
                    BulkAction::Flag(flag) => card.flag = *flag,
                    BulkAction::ChangeDeck(deck) => card.deck_id = *deck,
                    _ => {}
                }
                tx.update_card(card, now)?;
            }
        }
    }
    learning::record(tx, operation_id, OP_BULK, affected.to_string(), now)?;
    Ok(affected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_query_language_reads_fields_quotes_and_free_words() {
        let parsed =
            parse_query(r#"Hund deck:"Niemiecki — zdania" tag:zwierzęta is:due added:7 der"#)
                .unwrap();
        assert_eq!(parsed.words, ["Hund", "der"]);
        assert_eq!(parsed.deck.as_deref(), Some("niemiecki — zdania"));
        assert_eq!(parsed.tags, ["zwierzęta"]);
        assert_eq!(parsed.is, Some(IsFilter::Due));
        assert_eq!(parsed.added_days, Some(7));
        assert_eq!(parse_query("   ").unwrap(), ParsedQuery::default());
        // A colon inside a free word that is not a field prefix stays text.
        assert_eq!(parse_query("12:30").unwrap().words, ["12:30"]);
    }

    #[test]
    fn query_errors_name_the_reason() {
        let reason = |query: &str| match parse_query(query).unwrap_err() {
            AppError::Validation(error) => error.reason,
            other => panic!("{other:?}"),
        };
        assert_eq!(reason(r#"deck:"open"#), "unbalancedQuote");
        assert_eq!(reason("is:late"), "unknownValue");
        assert_eq!(reason("color:red"), "unknownField");
        assert_eq!(reason("added:0"), "invalidValue");
        assert_eq!(reason("added:x"), "invalidValue");
        assert_eq!(reason("tag:"), "invalidValue");
    }

    #[test]
    fn free_words_become_escaped_prefix_phrases() {
        let words = vec!["Hund".to_owned(), "say\"hi".to_owned(), "—".to_owned()];
        assert_eq!(fts_query(&words).as_deref(), Some(r#""Hund"* "say""hi"*"#));
        assert_eq!(fts_query(&["…".to_owned()]), None);
        // SQL metacharacters are plain text inside an FTS phrase.
        assert_eq!(
            fts_query(&["x' OR 1=1 --".to_owned()]).as_deref(),
            Some(r#""x' OR 1=1 --"*"#)
        );
    }

    #[test]
    fn cursors_and_filter_versions_round_trip_and_reject_garbage() {
        let cursor = CardCursor {
            value: crate::repositories::SortValue::Text("der Hund".into()),
            card_id: 7,
        };
        assert_eq!(decode_cursor(&encode_cursor(&cursor)).unwrap(), cursor);
        assert!(decode_cursor("zz").is_err());
        assert!(decode_cursor("7b").is_err());
        let search = CardSearch {
            query: "Hund".into(),
            deck_id: Some(3),
            tag: None,
            states: vec![2],
            flags: vec![],
            sort: SortName::Front,
            descending: true,
        };
        let version = encode_filter_version(&search, "1:2/3:4/5:6".into());
        let decoded = decode_filter_version(&version).unwrap();
        assert_eq!(decoded.search, search);
        assert_eq!(decoded.fingerprint, "1:2/3:4/5:6");
        assert!(decode_filter_version("fv1.zz").is_err());
        assert!(decode_filter_version("other").is_err());
    }
}
