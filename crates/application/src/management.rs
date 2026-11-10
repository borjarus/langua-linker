//! Content management on the existing screens (plan Phase 11): decks and their hierarchy,
//! presets, note details, the editor (create/update/delete notes, card generation, word links),
//! note content edits and the word search.
//!
//! Every function runs inside the caller's transaction. Mutations need a write transaction and
//! are idempotent by `operationId`: a retry returns the current state of what the first call
//! changed instead of acting twice. Rules kept here:
//!
//! - Editing text never touches a card's schedule; a note-type change archives the cards of
//!   templates it drops (history kept) and restores them when switched back.
//! - A deck type hides sections; associations, tips and links are never deleted by it.
//! - The word matcher offers exact, then normalized matches; it never picks a word on its own.
//! - Edits of bundled notes are recorded as content overrides, so a content update keeps them.

use std::collections::{HashMap, HashSet};

use langualinker_domain::card::{Card, NewCard};
use langualinker_domain::content::{
    Association, AssociationProvenance, GrammarTip, ModerationStatus, NewAssociation,
    SentenceWordLink, TipProvenance, WordLinkResolution,
};
use langualinker_domain::deck::{
    Deck, DeckConfig, DeckConfigOptions, DeckType, NativeLanguage, NewDeck, TargetLanguage,
};
use langualinker_domain::fsrs::FsrsParameters;
use langualinker_domain::note::{CardDirection, NewNote, Note, NoteFields, NoteTypeKind};
use langualinker_domain::review::ReviewLog;
use langualinker_domain::text::{fold, nfc, utf16_len, utf16_offset, utf16_slice};
use langualinker_domain::validation;
use langualinker_domain::{
    AssociationId, CardId, DeckConfigId, DeckId, GrammarTipId, NoteId, TimestampMs,
    ValidationError, WordLinkId,
};

use crate::dashboard::{self, DashboardDeck};
use crate::learning::{self, LinkView};
use crate::ports::Rng;
use crate::repositories::{ContentCounts, NoteMatch, Repositories};
use crate::study;
use crate::{AppError, AppResult};

/// Longest grammar tip or association typed by the user (the bundled-content limit).
pub const MAX_SNIPPET_CHARS: usize = 500;
/// Longest note field.
pub const MAX_FIELD_CHARS: usize = validation::MAX_FIELD_CHARS;
/// Results of one word search.
pub const MAX_WORD_MATCHES: u32 = 50;

const OP_CREATE_DECK: &str = "createDeck";
const OP_UPDATE_DECK: &str = "updateDeck";
const OP_DELETE_DECK: &str = "deleteDeck";
const OP_SAVE_PRESET: &str = "savePreset";
const OP_SAVE_NOTE: &str = "saveNote";
const OP_DELETE_NOTE: &str = "deleteNote";
const OP_NOTE_CONTENT: &str = "updateNoteContent";

const DAY_MS: i64 = 86_400_000;

// ---------------------------------------------------------------- shared helpers

/// A deck that exists and is not deleted.
pub(crate) fn active_deck(tx: &mut dyn Repositories, id: DeckId) -> AppResult<Deck> {
    let deck = tx.deck(id)?.ok_or(AppError::NotFound { entity: "deck" })?;
    if deck.deleted_at.is_some() {
        return Err(AppError::Deleted { entity: "deck" });
    }
    Ok(deck)
}

/// `root` and every deck below it, parents before children, siblings in name order.
pub fn subtree(decks: &[Deck], root: DeckId) -> Vec<DeckId> {
    fn walk(decks: &[Deck], id: DeckId, out: &mut Vec<DeckId>, depth: usize) {
        out.push(id);
        if depth > validation::MAX_DECK_DEPTH * 4 {
            return;
        }
        // `decks` come sorted by name (case-insensitive), so children keep that order.
        for child in decks.iter().filter(|deck| deck.parent_id == Some(id)) {
            if !out.contains(&child.id) {
                walk(decks, child.id, out, depth + 1);
            }
        }
    }
    let mut out = Vec::new();
    walk(decks, root, &mut out, 0);
    out
}

/// The deck a note is shown in: the deck of its first live card that is not archived (any live
/// card when every card is archived).
fn primary_card(cards: &[Card]) -> Option<&Card> {
    cards
        .iter()
        .find(|card| card.archived_at.is_none())
        .or_else(|| cards.first())
}

fn recorded_id(tx: &mut dyn Repositories, operation_id: &str) -> AppResult<i64> {
    tx.recorded_operation(operation_id)?
        .and_then(|recorded| recorded.result_json.parse::<i64>().ok())
        .ok_or_else(|| AppError::Internal("recorded operation without an id".into()))
}

fn check_version(entity: &'static str, current: i64, expected: i64) -> AppResult<()> {
    if current != expected {
        return Err(AppError::VersionConflict { entity, current });
    }
    Ok(())
}

/// Trimmed NFC snippet (tip or association text) within the snippet limit.
fn snippet(field: &'static str, text: &str) -> AppResult<String> {
    Ok(validation::required_text(
        field,
        &nfc(text),
        MAX_SNIPPET_CHARS,
    )?)
}

/// Records that the user changed `field` of a bundled note, so content updates keep the edit.
fn mark_override(
    tx: &mut dyn Repositories,
    note: NoteId,
    field: &str,
    now: TimestampMs,
) -> AppResult<()> {
    if let Some(bundled) = tx.bundled_note(note)?
        && !tx
            .content_overrides(note)?
            .iter()
            .any(|known| known == field)
    {
        tx.put_content_override(note, field, &bundled.content_version, now)?;
    }
    Ok(())
}

// ---------------------------------------------------------------- decks

/// Deck screen read model.
#[derive(Debug, Clone, PartialEq)]
pub struct DeckOverview {
    pub deck: DashboardDeck,
    /// Every deck below it, parents before children.
    pub subdecks: Vec<DashboardDeck>,
    pub preset: DeckConfig,
    pub preset_dependent_decks: u32,
    /// Content of the deck's own notes (subdecks have their own type).
    pub content: ContentCounts,
}

/// What a deck type hides of `content`: the sections it does not show.
pub fn hidden_by(deck_type: DeckType, content: ContentCounts) -> ContentCounts {
    let caps = deck_type.capabilities();
    ContentCounts {
        associations: if caps.associations {
            0
        } else {
            content.associations
        },
        grammar_tips: if caps.grammar_tips {
            0
        } else {
            content.grammar_tips
        },
        word_links: if caps.word_links {
            0
        } else {
            content.word_links
        },
    }
}

/// GetDeck. A write transaction: expired burials return to their queues before counting, as on
/// the dashboard.
pub fn deck_overview(
    tx: &mut dyn Repositories,
    deck_id: DeckId,
    now: TimestampMs,
    pending: &[String],
) -> AppResult<DeckOverview> {
    let deck = active_deck(tx, deck_id)?;
    let moment = study::study_moment(tx, now)?;
    study::unbury_expired(tx, &moment)?;
    let decks = tx.active_decks()?;
    let ids = subtree(&decks, deck.id);
    let mut rows = dashboard::summaries(tx, &moment, &decks, &ids, pending)?;
    if rows.is_empty() {
        return Err(AppError::NotFound { entity: "deck" });
    }
    let own = rows.remove(0);
    let preset = tx
        .config(deck.config_id)?
        .ok_or(AppError::NotFound { entity: "preset" })?;
    let dependents = tx
        .decks_per_config()?
        .into_iter()
        .find(|(config, _)| *config == preset.id)
        .map_or(0, |(_, count)| count);
    let content = tx.deck_content_counts(deck.id)?;
    Ok(DeckOverview {
        deck: own,
        subdecks: rows,
        preset,
        preset_dependent_decks: dependents,
        content,
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreateDeck {
    pub operation_id: String,
    pub name: String,
    pub parent_id: Option<DeckId>,
    pub target_language: TargetLanguage,
    pub deck_type: DeckType,
}

/// CreateDeck with the default preset. Returns the new deck's id.
pub fn create_deck(
    tx: &mut dyn Repositories,
    rng: &dyn Rng,
    input: &CreateDeck,
    now: TimestampMs,
) -> AppResult<DeckId> {
    if learning::replayed(tx, &input.operation_id, OP_CREATE_DECK)? {
        return recorded_id(tx, &input.operation_id).map(DeckId);
    }
    let name = validation::required_text("name", &nfc(&input.name), validation::MAX_NAME_CHARS)?;
    if let Some(parent) = input.parent_id {
        match tx.deck(parent)? {
            Some(deck) if deck.deleted_at.is_none() => {}
            _ => return Err(ValidationError::new("parentId", "notFound").into()),
        }
        let decks = tx.active_decks()?;
        let parent_of = |id: DeckId| {
            decks
                .iter()
                .find(|deck| deck.id == id)
                .and_then(|deck| deck.parent_id)
        };
        // A new deck cannot close a cycle; only the depth below the parent matters.
        langualinker_domain::deck::check_reparent(DeckId(0), Some(parent), parent_of)?;
    }
    // The preset chosen in the settings (Phase 12), or the built-in one.
    let config_id = crate::settings::default_preset(tx)?;
    let deck = tx.insert_deck(
        &NewDeck {
            guid: rng.guid(),
            name,
            parent_id: input.parent_id,
            description: String::new(),
            native_language: NativeLanguage::Pl,
            target_language: input.target_language,
            deck_type: input.deck_type,
            config_id,
            content_version: None,
            bundled: false,
        },
        now,
    )?;
    learning::record(
        tx,
        &input.operation_id,
        OP_CREATE_DECK,
        deck.id.0.to_string(),
        now,
    )?;
    Ok(deck.id)
}

/// Fields an UpdateDeck changes; `None` keeps the stored value.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DeckPatch {
    pub name: Option<String>,
    pub description: Option<String>,
    /// `Some(None)` moves the deck to the top level.
    pub parent_id: Option<Option<DeckId>>,
    pub deck_type: Option<DeckType>,
    pub preset_id: Option<DeckConfigId>,
}

/// UpdateDeck at the version the screen showed: rename, describe, move (no cycles, bounded
/// depth), change the type (hides sections, deletes nothing) or the preset.
pub fn update_deck(
    tx: &mut dyn Repositories,
    operation_id: &str,
    deck_id: DeckId,
    expected_version: i64,
    patch: &DeckPatch,
    now: TimestampMs,
) -> AppResult<DeckId> {
    if learning::replayed(tx, operation_id, OP_UPDATE_DECK)? {
        return Ok(deck_id);
    }
    let mut deck = active_deck(tx, deck_id)?;
    check_version("deck", deck.version, expected_version)?;
    if let Some(name) = &patch.name {
        deck.name = validation::required_text("name", &nfc(name), validation::MAX_NAME_CHARS)?;
    }
    if let Some(description) = &patch.description {
        deck.description = validation::bounded_text(
            "description",
            nfc(description).trim(),
            validation::MAX_DESCRIPTION_CHARS,
        )?;
    }
    if let Some(parent) = patch.parent_id {
        if let Some(parent) = parent {
            match tx.deck(parent)? {
                Some(found) if found.deleted_at.is_none() => {}
                _ => return Err(ValidationError::new("parentId", "notFound").into()),
            }
        }
        // The repository checks cycles and depth against the stored tree.
        deck.parent_id = parent;
    }
    if let Some(deck_type) = patch.deck_type {
        deck.deck_type = deck_type;
    }
    if let Some(preset) = patch.preset_id {
        match tx.config(preset)? {
            Some(config) if config.deleted_at.is_none() => deck.config_id = config.id,
            _ => return Err(ValidationError::new("presetId", "notFound").into()),
        }
    }
    tx.update_deck(&deck, now)?;
    learning::record(tx, operation_id, OP_UPDATE_DECK, deck.id.0.to_string(), now)?;
    Ok(deck.id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeleteDeckOutcome {
    pub deleted_decks: u32,
    pub deleted_cards: u32,
}

/// DeleteDeck: the deck, its subdecks and their cards. Notes left without a card are deleted
/// too, and links pointing at them become unresolved. History stays with the soft-deleted cards.
pub fn delete_deck(
    tx: &mut dyn Repositories,
    operation_id: &str,
    deck_id: DeckId,
    expected_version: i64,
    now: TimestampMs,
) -> AppResult<DeleteDeckOutcome> {
    if learning::replayed(tx, operation_id, OP_DELETE_DECK)? {
        let recorded = tx
            .recorded_operation(operation_id)?
            .map(|recorded| recorded.result_json)
            .unwrap_or_default();
        let mut parts = recorded.split(':').map(|part| part.parse::<u32>().ok());
        return match (parts.next().flatten(), parts.next().flatten()) {
            (Some(deleted_decks), Some(deleted_cards)) => Ok(DeleteDeckOutcome {
                deleted_decks,
                deleted_cards,
            }),
            _ => Err(AppError::Internal(
                "recorded deck deletion unreadable".into(),
            )),
        };
    }
    let deck = active_deck(tx, deck_id)?;
    check_version("deck", deck.version, expected_version)?;
    let decks = tx.active_decks()?;
    let ids = subtree(&decks, deck.id);
    let deleted_cards = tx.soft_delete_cards_of_decks(&ids, now)?;
    for id in ids.iter().rev() {
        let version = decks
            .iter()
            .find(|candidate| candidate.id == *id)
            .map_or(expected_version, |candidate| candidate.version);
        tx.soft_delete_deck(*id, version, now)?;
    }
    for note in tx.soft_delete_orphan_notes(now)? {
        tx.unresolve_links_to(note, now)?;
    }
    let outcome = DeleteDeckOutcome {
        deleted_decks: u32::try_from(ids.len()).unwrap_or(u32::MAX),
        deleted_cards,
    };
    learning::record(
        tx,
        operation_id,
        OP_DELETE_DECK,
        format!("{}:{}", outcome.deleted_decks, outcome.deleted_cards),
        now,
    )?;
    Ok(outcome)
}

// ---------------------------------------------------------------- presets

/// The preset fields the deck options screen edits.
#[derive(Debug, Clone, PartialEq)]
pub struct PresetConfig {
    pub new_per_day: u32,
    pub reviews_per_day: u32,
    pub desired_retention: f64,
    pub max_interval_days: u32,
    pub learning_steps_minutes: Vec<u32>,
    pub relearning_steps_minutes: Vec<u32>,
    pub weights: Vec<f64>,
}

impl PresetConfig {
    /// Applies the edited fields; options the screen does not show keep their value.
    fn apply(&self, options: &mut DeckConfigOptions) {
        options.new_per_day = self.new_per_day;
        options.reviews_per_day = self.reviews_per_day;
        options.desired_retention = self.desired_retention;
        options.maximum_interval_days = self.max_interval_days;
        options.learning_steps_minutes = self.learning_steps_minutes.clone();
        options.relearning_steps_minutes = self.relearning_steps_minutes.clone();
        options.weights = self.weights.clone();
    }
}

/// ListPresets: built-in first, then by name, each with the number of decks using it.
pub fn presets(tx: &mut dyn Repositories) -> AppResult<Vec<(DeckConfig, u32)>> {
    let counts: HashMap<DeckConfigId, u32> = tx.decks_per_config()?.into_iter().collect();
    let mut presets: Vec<(DeckConfig, u32)> = tx
        .active_configs()?
        .into_iter()
        .map(|config| {
            let count = counts.get(&config.id).copied().unwrap_or(0);
            (config, count)
        })
        .collect();
    presets.sort_by(|(a, _), (b, _)| {
        b.built_in
            .cmp(&a.built_in)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then(a.id.0.cmp(&b.id.0))
    });
    Ok(presets)
}

#[derive(Debug, Clone, PartialEq)]
pub struct SavePreset {
    pub operation_id: String,
    /// `None` creates a preset (also how a preset is cloned).
    pub preset_id: Option<DeckConfigId>,
    pub expected_version: Option<i64>,
    pub name: String,
    pub config: PresetConfig,
}

/// SavePreset: creates or updates (at the version shown) a preset. A change applies to every
/// deck using it; the screen warns with the dependent count first.
pub fn save_preset(
    tx: &mut dyn Repositories,
    rng: &dyn Rng,
    input: &SavePreset,
    now: TimestampMs,
) -> AppResult<(DeckConfig, u32)> {
    let find = |tx: &mut dyn Repositories, id: DeckConfigId| -> AppResult<(DeckConfig, u32)> {
        presets(tx)?
            .into_iter()
            .find(|(config, _)| config.id == id)
            .ok_or(AppError::NotFound { entity: "preset" })
    };
    if learning::replayed(tx, &input.operation_id, OP_SAVE_PRESET)? {
        let id = recorded_id(tx, &input.operation_id)?;
        return find(tx, DeckConfigId(id));
    }
    let name = validation::required_text("name", &nfc(&input.name), validation::MAX_NAME_CHARS)?;
    let saved = match input.preset_id {
        None => {
            let mut options = DeckConfigOptions::default();
            input.config.apply(&mut options);
            options.validate()?;
            tx.insert_config(&rng.guid(), &name, &options, now)?
        }
        Some(id) => {
            let mut config = tx
                .config(id)?
                .filter(|config| config.deleted_at.is_none())
                .ok_or(AppError::NotFound { entity: "preset" })?;
            let expected = input
                .expected_version
                .ok_or(ValidationError::new("expectedVersion", "required"))?;
            check_version("preset", config.version, expected)?;
            input.config.apply(&mut config.options);
            config.options.validate()?;
            config.name = name;
            tx.update_config(&config, now)?
        }
    };
    learning::record(
        tx,
        &input.operation_id,
        OP_SAVE_PRESET,
        saved.id.0.to_string(),
        now,
    )?;
    find(tx, saved.id)
}

// ---------------------------------------------------------------- note details

#[derive(Debug, Clone, PartialEq)]
pub struct CardDetails {
    pub card: Card,
    pub deck: Deck,
    pub direction: CardDirection,
    /// Valid reviews, newest first.
    pub history: Vec<ReviewLog>,
    /// Current probability of recall; `None` for a card without a memory state.
    pub retrievability: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NoteDetails {
    pub note: Note,
    pub kind: NoteTypeKind,
    /// Deck of the note's first card; its type decides the sections shown.
    pub deck: Deck,
    pub cards: Vec<CardDetails>,
    /// Ordinals of cards archived by a note-type change.
    pub archived_ordinals: Vec<u32>,
    pub associations: Vec<Association>,
    pub grammar_tips: Vec<GrammarTip>,
    pub word_links: Vec<LinkView>,
    pub computed_at: TimestampMs,
}

fn retrievability(
    tx: &mut dyn Repositories,
    card: &Card,
    deck: &Deck,
    now: TimestampMs,
) -> AppResult<Option<f64>> {
    let (Some(stability), Some(last)) = (card.fsrs.stability, card.fsrs.last_review_at) else {
        return Ok(None);
    };
    let Some(config) = tx.config(deck.config_id)? else {
        return Ok(None);
    };
    let Ok(parameters) = FsrsParameters::new(
        &config.options.weights,
        config.options.desired_retention,
        config.options.maximum_interval_days,
    ) else {
        return Ok(None);
    };
    let elapsed = (now.0 - last.0).max(0) as f64 / DAY_MS as f64;
    let value = parameters.retrievability(elapsed, stability);
    Ok(Some((value * 1000.0).round() / 1000.0))
}

/// GetNoteDetails of a note: live cards with history, archived ordinals and the note content
/// (every section; the screen hides what the deck type does not show).
pub fn note_details(
    tx: &mut dyn Repositories,
    note_id: NoteId,
    now: TimestampMs,
) -> AppResult<NoteDetails> {
    let note = learning::live_note(tx, note_id)?;
    let kind = tx
        .note_type_by_id(note.note_type_id)?
        .map(|note_type| note_type.kind)
        .ok_or(AppError::NotFound { entity: "noteType" })?;
    let all = tx.cards_of_note(note.id)?;
    let primary = primary_card(&all)
        .cloned()
        .ok_or(AppError::NotFound { entity: "card" })?;
    let deck = learning::live_deck(tx, primary.deck_id)?;
    let mut cards = Vec::new();
    let mut archived_ordinals = Vec::new();
    for card in all {
        if card.archived_at.is_some() {
            archived_ordinals.push(card.template_ordinal);
            continue;
        }
        let card_deck = learning::live_deck(tx, card.deck_id)?;
        let direction = learning::direction_of(tx, &note, &card)?;
        let history = tx
            .review_logs(card.id)?
            .into_iter()
            .filter(|log| log.invalidated_at.is_none())
            .collect();
        let retrievability = retrievability(tx, &card, &card_deck, now)?;
        cards.push(CardDetails {
            card,
            deck: card_deck,
            direction,
            history,
            retrievability,
        });
    }
    let associations = tx.associations(note.id)?;
    let grammar_tips = tx.grammar_tips(note.id)?;
    let mut word_links = Vec::new();
    for link in tx.word_links(note.id)? {
        word_links.push(learning::link_view(tx, link)?);
    }
    Ok(NoteDetails {
        note,
        kind,
        deck,
        cards,
        archived_ordinals,
        associations,
        grammar_tips,
        word_links,
        computed_at: now,
    })
}

/// GetNoteDetails by one of the note's cards (the card screen's route parameter).
pub fn card_note_details(
    tx: &mut dyn Repositories,
    card_id: CardId,
    now: TimestampMs,
) -> AppResult<NoteDetails> {
    let card = tx
        .card(card_id)?
        .ok_or(AppError::NotFound { entity: "card" })?;
    if card.deleted_at.is_some() {
        return Err(AppError::Deleted { entity: "card" });
    }
    note_details(tx, card.note_id, now)
}

// ---------------------------------------------------------------- editor

/// A word link as the editor sends it; ranges are UTF-16 offsets in the NFC front.
#[derive(Debug, Clone, PartialEq)]
pub struct LinkDraft {
    pub start: u32,
    pub end: u32,
    pub surface: String,
    /// `None` keeps the phrase marked but unresolved.
    pub word: Option<NoteId>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SaveNote {
    pub operation_id: String,
    /// `None` creates a note.
    pub note_id: Option<NoteId>,
    pub expected_version: Option<i64>,
    pub deck_id: DeckId,
    pub kind: NoteTypeKind,
    pub front: String,
    pub back: String,
    pub tags: Vec<String>,
    pub manual_association: Option<String>,
    /// Tips appended to the note.
    pub grammar_tips: Vec<String>,
    /// The complete set of links of a sentence; `None` keeps (and remaps) the stored ones.
    pub word_links: Option<Vec<LinkDraft>>,
}

/// Checks drafts against the front: ranges inside it, not overlapping, covering exactly the
/// surface; every word note live and not the note itself.
fn check_links(
    tx: &mut dyn Repositories,
    front: &str,
    drafts: &[LinkDraft],
    note_id: Option<NoteId>,
) -> AppResult<Vec<LinkDraft>> {
    let mut sorted: Vec<LinkDraft> = drafts.to_vec();
    sorted.sort_by_key(|draft| (draft.start, draft.end));
    let mut previous_end = 0;
    for draft in &mut sorted {
        draft.surface = nfc(&draft.surface);
        let covered = utf16_slice(front, draft.start, draft.end);
        if draft.start < previous_end || covered != Some(draft.surface.as_str()) {
            return Err(ValidationError::new("wordLinks", "range").into());
        }
        previous_end = draft.end;
        if let Some(word) = draft.word {
            if Some(word) == note_id {
                return Err(ValidationError::new("wordLinks", "sameNote").into());
            }
            learning::live_note(tx, word)
                .map_err(|_| ValidationError::new("wordLinks", "wordNotFound"))?;
        }
    }
    Ok(sorted)
}

/// Links of a sentence after its front changed without new drafts: a link whose text is still
/// at its range stays; one whose text moved is remapped when the new front contains it exactly
/// once; any other link is dropped (never left under another word).
fn remap_links(links: &[SentenceWordLink], front: &str) -> Vec<SentenceWordLink> {
    let mut kept = Vec::new();
    for link in links {
        if utf16_slice(front, link.start_index, link.end_index) == Some(link.surface_form.as_str())
        {
            kept.push(link.clone());
            continue;
        }
        let found: Vec<usize> = front
            .match_indices(link.surface_form.as_str())
            .map(|(byte, _)| byte)
            .collect();
        if let [byte] = found.as_slice() {
            let start = utf16_offset(front, *byte);
            let mut moved = link.clone();
            moved.start_index = start;
            moved.end_index = start + utf16_len(&link.surface_form);
            kept.push(moved);
        }
    }
    kept.sort_by_key(|link| link.start_index);
    // Remapping may make two links overlap; the first one wins.
    let mut result: Vec<SentenceWordLink> = Vec::new();
    for link in kept {
        if result
            .last()
            .is_none_or(|last| last.end_index <= link.start_index)
        {
            result.push(link);
        }
    }
    result
}

fn link_key(link: &SentenceWordLink) -> (u32, u32, Option<NoteId>, String) {
    (
        link.start_index,
        link.end_index,
        link.word_note_id,
        link.surface_form.clone(),
    )
}

/// SaveNote: creates or updates a note with its cards. Returns the note id.
pub fn save_note(
    tx: &mut dyn Repositories,
    rng: &dyn Rng,
    input: &SaveNote,
    now: TimestampMs,
) -> AppResult<NoteId> {
    if learning::replayed(tx, &input.operation_id, OP_SAVE_NOTE)? {
        return recorded_id(tx, &input.operation_id).map(NoteId);
    }
    let front = nfc(input.front.trim());
    let back = nfc(input.back.trim());
    if back.is_empty() {
        return Err(ValidationError::new("back", "required").into());
    }
    let fields = NoteFields::validated(&front, &back)?;
    let tags: Vec<String> = validation::tags(
        &input
            .tags
            .iter()
            .map(|tag| nfc(tag.trim()))
            .collect::<Vec<_>>(),
    )?;
    let tips = input
        .grammar_tips
        .iter()
        .filter(|tip| !tip.trim().is_empty())
        .map(|tip| snippet("grammarTips", tip))
        .collect::<AppResult<Vec<_>>>()?;
    let association = match &input.manual_association {
        Some(text) if !text.trim().is_empty() => Some(snippet("manualAssociation", text)?),
        _ => None,
    };
    let deck = active_deck(tx, input.deck_id).map_err(|error| match error {
        AppError::NotFound { .. } | AppError::Deleted { .. } => {
            ValidationError::new("deckId", "notFound").into()
        }
        other => other,
    })?;
    let drafts = match &input.word_links {
        Some(drafts) => {
            if !drafts.is_empty() && !deck.deck_type.capabilities().word_links {
                return Err(ValidationError::new("wordLinks", "notSupported").into());
            }
            Some(check_links(tx, &fields.front, drafts, input.note_id)?)
        }
        None => None,
    };
    let note_type = tx.note_type(input.kind)?;

    let (note, previous_front) = match input.note_id {
        None => {
            let note = tx.insert_note(
                &NewNote {
                    guid: rng.guid(),
                    note_type_id: note_type.id,
                    fields,
                    tags,
                },
                now,
            )?;
            (note, None)
        }
        Some(id) => {
            let mut note = learning::live_note(tx, id)?;
            let expected = input
                .expected_version
                .ok_or(ValidationError::new("expectedVersion", "required"))?;
            check_version("note", note.version, expected)?;
            let previous_front = note.fields.front.clone();
            for (field, changed) in [
                ("front", note.fields.front != fields.front),
                ("back", note.fields.back != fields.back),
                ("tags", note.tags != tags),
                ("noteType", note.note_type_id != note_type.id),
            ] {
                if changed {
                    mark_override(tx, note.id, field, now)?;
                }
            }
            note.fields = fields;
            note.tags = tags;
            note.note_type_id = note_type.id;
            (tx.update_note(&note, now)?, Some(previous_front))
        }
    };

    // Cards: one live card per template of the type. Text edits keep every schedule; a dropped
    // template archives its card, a template that comes back restores the archived card.
    let cards = tx.cards_of_note(note.id)?;
    let current_deck = primary_card(&cards).map(|card| card.deck_id);
    let move_cards = current_deck.is_some_and(|current| current != deck.id);
    let templates = input.kind.templates();
    for card in &cards {
        let wanted = (card.template_ordinal as usize) < templates.len();
        let mut updated = card.clone();
        let mut changed = false;
        if wanted && card.archived_at.is_some() {
            updated.archived_at = None;
            changed = true;
        }
        if !wanted && card.archived_at.is_none() {
            updated.archived_at = Some(now);
            changed = true;
        }
        if wanted && move_cards && card.deck_id != deck.id {
            updated.deck_id = deck.id;
            changed = true;
        }
        if changed {
            tx.update_card(&updated, now)?;
        }
    }
    for ordinal in 0..templates.len() as u32 {
        if !cards.iter().any(|card| card.template_ordinal == ordinal) {
            let position = tx.next_position()?;
            tx.insert_card(
                &NewCard {
                    guid: rng.guid(),
                    note_id: note.id,
                    deck_id: deck.id,
                    template_ordinal: ordinal,
                    position,
                },
                now,
            )?;
        }
    }

    if let Some(text) = association {
        tx.insert_association(
            &NewAssociation {
                guid: rng.guid(),
                note_id: note.id,
                text,
                provenance: AssociationProvenance::Manual,
                native_language: deck.native_language.code().to_owned(),
                target_language: deck.target_language.code().to_owned(),
                prompt_version: None,
                model: None,
                moderation: ModerationStatus::NotRequired,
                cache_key: None,
            },
            now,
        )?;
    }
    for tip in tips {
        tx.insert_grammar_tip(note.id, &rng.guid(), &tip, TipProvenance::Manual, now)?;
    }

    let stored = tx.word_links(note.id)?;
    let desired: Option<Vec<SentenceWordLink>> = match drafts {
        Some(drafts) => {
            let mut desired = Vec::with_capacity(drafts.len());
            for draft in drafts {
                if let Some(kept) = stored.iter().find(|link| {
                    link.start_index == draft.start
                        && link.end_index == draft.end
                        && link.word_note_id == draft.word
                }) {
                    desired.push(kept.clone());
                    continue;
                }
                let lemma = match draft.word {
                    Some(word) => tx.note(word)?.map(|word| word.fields.front),
                    None => None,
                };
                desired.push(SentenceWordLink {
                    id: WordLinkId(0),
                    sentence_note_id: note.id,
                    word_note_id: draft.word,
                    preferred_word_card_id: None,
                    surface_form: draft.surface,
                    lemma,
                    start_index: draft.start,
                    end_index: draft.end,
                    resolution: if draft.word.is_some() {
                        WordLinkResolution::UserOverridden
                    } else {
                        WordLinkResolution::Unresolved
                    },
                    created_at: now,
                    updated_at: now,
                });
            }
            Some(desired)
        }
        None if previous_front.as_deref() != Some(note.fields.front.as_str()) => {
            Some(remap_links(&stored, &note.fields.front))
        }
        None => None,
    };
    if let Some(desired) = desired {
        let same = stored.len() == desired.len()
            && stored
                .iter()
                .zip(&desired)
                .all(|(a, b)| link_key(a) == link_key(b) && a.resolution == b.resolution);
        if !same {
            tx.replace_word_links(note.id, &desired, now)?;
            if input.note_id.is_some() {
                mark_override(tx, note.id, "wordLinks", now)?;
            }
        }
    }

    learning::record(
        tx,
        &input.operation_id,
        OP_SAVE_NOTE,
        note.id.0.to_string(),
        now,
    )?;
    Ok(note.id)
}

/// DeleteNote at the version shown: the note with every card (history stays with the
/// soft-deleted cards); links of other sentences to it become unresolved. Returns the number of
/// cards deleted.
pub fn delete_note(
    tx: &mut dyn Repositories,
    operation_id: &str,
    note_id: NoteId,
    expected_version: i64,
    now: TimestampMs,
) -> AppResult<u32> {
    if learning::replayed(tx, operation_id, OP_DELETE_NOTE)? {
        return recorded_id(tx, operation_id).map(|count| u32::try_from(count).unwrap_or(0));
    }
    let note = learning::live_note(tx, note_id)?;
    check_version("note", note.version, expected_version)?;
    let cards = u32::try_from(tx.cards_of_note(note.id)?.len()).unwrap_or(u32::MAX);
    tx.soft_delete_note(note.id, expected_version, now)?;
    tx.unresolve_links_to(note.id, now)?;
    learning::record(tx, operation_id, OP_DELETE_NOTE, cards.to_string(), now)?;
    Ok(cards)
}

/// One edit of a note's tips, associations or word links.
#[derive(Debug, Clone, PartialEq)]
pub enum NoteContentChange {
    AddTip(String),
    EditTip(GrammarTipId, String),
    DeleteTip(GrammarTipId),
    /// The complete new order; any other set of ids is rejected.
    ReorderTips(Vec<GrammarTipId>),
    AddAssociation(String),
    /// Only the user's own (manual) associations can be edited.
    EditAssociation(AssociationId, String),
    DeleteAssociation(AssociationId),
    /// Removes the link; the word note is untouched.
    UnlinkWord(WordLinkId),
}

/// UpdateNoteContent: one transactional change. The note's version is not bumped, so an open
/// editor draft of the note stays valid.
pub fn update_note_content(
    tx: &mut dyn Repositories,
    rng: &dyn Rng,
    operation_id: &str,
    note_id: NoteId,
    change: &NoteContentChange,
    now: TimestampMs,
) -> AppResult<NoteId> {
    if learning::replayed(tx, operation_id, OP_NOTE_CONTENT)? {
        return Ok(note_id);
    }
    let note = learning::live_note(tx, note_id)?;
    let tip_of = |tx: &mut dyn Repositories, id: GrammarTipId| -> AppResult<GrammarTip> {
        tx.grammar_tips(note.id)?
            .into_iter()
            .find(|tip| tip.id == id)
            .ok_or(AppError::NotFound {
                entity: "grammarTip",
            })
    };
    let association_of = |tx: &mut dyn Repositories, id: AssociationId| -> AppResult<Association> {
        tx.associations(note.id)?
            .into_iter()
            .find(|association| association.id == id)
            .ok_or(AppError::NotFound {
                entity: "association",
            })
    };
    match change {
        NoteContentChange::AddTip(text) => {
            let text = snippet("text", text)?;
            tx.insert_grammar_tip(note.id, &rng.guid(), &text, TipProvenance::Manual, now)?;
        }
        NoteContentChange::EditTip(id, text) => {
            let text = snippet("text", text)?;
            let tip = tip_of(tx, *id)?;
            if tip.text != text {
                tx.update_grammar_tip_text(tip.id, &text, now)?;
                if tip.provenance == TipProvenance::Bundled {
                    mark_override(tx, note.id, &format!("tip:{}", tip.guid.as_str()), now)?;
                }
            }
        }
        NoteContentChange::DeleteTip(id) => {
            let tip = tip_of(tx, *id)?;
            tx.soft_delete_grammar_tip(tip.id, now)?;
            let remaining: Vec<GrammarTipId> =
                tx.grammar_tips(note.id)?.iter().map(|tip| tip.id).collect();
            for (order, id) in remaining.into_iter().enumerate() {
                tx.set_grammar_tip_order(id, order as i64, now)?;
            }
        }
        NoteContentChange::ReorderTips(ids) => {
            let current: Vec<GrammarTipId> =
                tx.grammar_tips(note.id)?.iter().map(|tip| tip.id).collect();
            let unique: HashSet<&GrammarTipId> = ids.iter().collect();
            if ids.len() != current.len()
                || unique.len() != current.len()
                || !ids.iter().all(|id| current.contains(id))
            {
                return Err(ValidationError::new("tipIds", "mismatch").into());
            }
            for (order, id) in ids.iter().enumerate() {
                tx.set_grammar_tip_order(*id, order as i64, now)?;
            }
        }
        NoteContentChange::AddAssociation(text) => {
            let text = snippet("text", text)?;
            let cards = tx.cards_of_note(note.id)?;
            let deck_id = primary_card(&cards)
                .map(|card| card.deck_id)
                .ok_or(AppError::NotFound { entity: "card" })?;
            let deck = learning::live_deck(tx, deck_id)?;
            tx.insert_association(
                &NewAssociation {
                    guid: rng.guid(),
                    note_id: note.id,
                    text,
                    provenance: AssociationProvenance::Manual,
                    native_language: deck.native_language.code().to_owned(),
                    target_language: deck.target_language.code().to_owned(),
                    prompt_version: None,
                    model: None,
                    moderation: ModerationStatus::NotRequired,
                    cache_key: None,
                },
                now,
            )?;
        }
        NoteContentChange::EditAssociation(id, text) => {
            let text = snippet("text", text)?;
            let association = association_of(tx, *id)?;
            if association.provenance != AssociationProvenance::Manual {
                return Err(ValidationError::new("associationId", "notEditable").into());
            }
            tx.update_manual_association_text(association.id, &text, now)?;
        }
        NoteContentChange::DeleteAssociation(id) => {
            let association = association_of(tx, *id)?;
            tx.soft_delete_association(association.id, now)?;
        }
        NoteContentChange::UnlinkWord(id) => {
            let link = tx
                .word_link(*id)?
                .filter(|link| link.sentence_note_id == note.id)
                .ok_or(AppError::NotFound { entity: "wordLink" })?;
            tx.delete_word_link(link.id)?;
            mark_override(tx, note.id, "wordLinks", now)?;
        }
    }
    learning::record(
        tx,
        operation_id,
        OP_NOTE_CONTENT,
        note.id.0.to_string(),
        now,
    )?;
    Ok(note.id)
}

/// Editor lookups: active decks (name order) and the tags in use.
pub fn editor_data(tx: &mut dyn Repositories) -> AppResult<(Vec<Deck>, Vec<String>)> {
    Ok((tx.active_decks()?, tx.all_tags()?))
}

/// CheckDuplicate: notes in the deck (with subdecks) whose front matches after trimming, NFC and
/// case folding. Only a warning: homographs stay valid notes.
pub fn duplicates(
    tx: &mut dyn Repositories,
    deck_id: DeckId,
    front: &str,
    exclude: Option<NoteId>,
) -> AppResult<Vec<(NoteMatch, String)>> {
    let key = nfc(front).trim().to_lowercase();
    if key.is_empty() {
        return Ok(Vec::new());
    }
    let deck = active_deck(tx, deck_id).map_err(|_| ValidationError::new("deckId", "notFound"))?;
    let decks = tx.active_decks()?;
    let ids = subtree(&decks, deck.id);
    let matches = tx.notes_by_sort_field(&ids, &key, exclude)?;
    Ok(matches
        .into_iter()
        .map(|found| {
            let name = decks
                .iter()
                .find(|deck| deck.id == found.deck_id)
                .map(|deck| deck.name.clone())
                .unwrap_or_default();
            (found, name)
        })
        .collect())
}

// ---------------------------------------------------------------- word search

/// Leading articles and particles the word matcher ignores (`der Tee` matches `Tee`).
const ARTICLES: &[&str] = &[
    "der", "die", "das", "den", "dem", "des", "ein", "eine", "the", "a", "an", "to", "el", "la",
    "los", "las", "un", "una",
];

/// The lemma without a trailing sense label (`die Bank (Sitz)` → `die Bank`).
fn without_label(text: &str) -> &str {
    let trimmed = text.trim_end();
    if trimmed.ends_with(')')
        && let Some(open) = trimmed.rfind('(')
    {
        return trimmed[..open].trim_end();
    }
    trimmed
}

/// The lemma without its leading article.
fn bare(text: &str) -> &str {
    let text = without_label(text).trim();
    if let Some((first, rest)) = text.split_once(char::is_whitespace)
        && ARTICLES
            .iter()
            .any(|article| article.eq_ignore_ascii_case(first))
    {
        return rest.trim_start();
    }
    text
}

/// Folded matching key: no sense label, no article, case and diacritics folded.
pub fn word_key(text: &str) -> String {
    fold(bare(text))
}

#[derive(Debug, Clone, PartialEq)]
pub struct WordMatch {
    pub note_id: NoteId,
    pub lemma: String,
    pub translation: String,
    pub deck_name: String,
    /// Same text (or same text without the article); otherwise equal after folding.
    pub exact: bool,
}

/// SearchWords: word notes of decks in `language` matching `text`, exact matches first, then
/// normalized ones; empty when nothing matches (never a guess).
pub fn search_words(
    tx: &mut dyn Repositories,
    language: TargetLanguage,
    text: &str,
    limit: u32,
) -> AppResult<Vec<WordMatch>> {
    validation::in_range("limit", i64::from(limit), 1, i64::from(MAX_WORD_MATCHES))?;
    let text = nfc(text.trim());
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let key = word_key(&text);
    let decks: Vec<Deck> = tx
        .active_decks()?
        .into_iter()
        .filter(|deck| deck.target_language == language)
        .collect();
    let ids: Vec<DeckId> = decks.iter().map(|deck| deck.id).collect();
    let mut matches: Vec<WordMatch> = tx
        .word_notes(&ids)?
        .into_iter()
        .filter_map(|word| {
            let exact = word.front == text || bare(&word.front) == text;
            if !exact && word_key(&word.front) != key {
                return None;
            }
            let deck_name = decks
                .iter()
                .find(|deck| deck.id == word.deck_id)
                .map(|deck| deck.name.clone())
                .unwrap_or_default();
            Some(WordMatch {
                note_id: word.note_id,
                lemma: word.front,
                translation: word.back,
                deck_name,
                exact,
            })
        })
        .collect();
    matches.sort_by(|a, b| {
        b.exact
            .cmp(&a.exact)
            .then_with(|| a.lemma.to_lowercase().cmp(&b.lemma.to_lowercase()))
            .then(a.note_id.0.cmp(&b.note_id.0))
    });
    matches.truncate(limit as usize);
    Ok(matches)
}

#[cfg(test)]
mod tests {
    //! Use cases that need a repository are covered on SQLite by
    //! `crates/infrastructure/tests/management.rs`.

    use langualinker_domain::Guid;

    use super::*;

    fn deck(id: i64, parent: Option<i64>, name: &str) -> Deck {
        Deck {
            id: DeckId(id),
            guid: Guid::from_bytes([id as u8; 16]),
            name: name.into(),
            parent_id: parent.map(DeckId),
            description: String::new(),
            native_language: NativeLanguage::Pl,
            target_language: TargetLanguage::De,
            deck_type: DeckType::Linguistic,
            config_id: DeckConfigId(1),
            content_version: None,
            bundled: false,
            created_at: TimestampMs(0),
            updated_at: TimestampMs(0),
            deleted_at: None,
            usn: 0,
            version: 1,
        }
    }

    #[test]
    fn subtree_lists_parents_before_children() {
        let decks = vec![
            deck(1, None, "A"),
            deck(2, Some(1), "B"),
            deck(3, Some(2), "C"),
            deck(4, Some(1), "D"),
            deck(5, None, "E"),
        ];
        assert_eq!(
            subtree(&decks, DeckId(1)),
            vec![DeckId(1), DeckId(2), DeckId(3), DeckId(4)]
        );
        assert_eq!(subtree(&decks, DeckId(5)), vec![DeckId(5)]);
    }

    #[test]
    fn a_deck_type_hides_only_the_sections_it_does_not_show() {
        let content = ContentCounts {
            associations: 3,
            grammar_tips: 2,
            word_links: 1,
        };
        assert_eq!(
            hidden_by(DeckType::Linguistic, content),
            ContentCounts::default()
        );
        assert_eq!(
            hidden_by(DeckType::TextWithAssociations, content),
            ContentCounts {
                associations: 0,
                grammar_tips: 2,
                word_links: 1
            }
        );
        assert_eq!(hidden_by(DeckType::Simple, content), content);
    }

    #[test]
    fn word_keys_ignore_articles_labels_case_and_diacritics() {
        assert_eq!(word_key("der Tee"), "tee");
        assert_eq!(word_key("die Bank (Sitz)"), "bank");
        assert_eq!(word_key("die Brücke"), word_key("brucke"));
        assert_eq!(word_key("die Straße"), "strasse");
        assert_eq!(word_key("el niño"), "nino");
        assert_eq!(word_key("to go"), "go");
        // A single word that happens to be an article stays as it is.
        assert_eq!(word_key("die"), "die");
        assert_eq!(bare("der Tee"), "Tee");
    }

    fn link(start: u32, end: u32, surface: &str) -> SentenceWordLink {
        SentenceWordLink {
            id: WordLinkId(1),
            sentence_note_id: NoteId(1),
            word_note_id: Some(NoteId(2)),
            preferred_word_card_id: None,
            surface_form: surface.into(),
            lemma: None,
            start_index: start,
            end_index: end,
            resolution: WordLinkResolution::Resolved,
            created_at: TimestampMs(0),
            updated_at: TimestampMs(0),
        }
    }

    #[test]
    fn an_edited_sentence_keeps_remaps_or_drops_its_links() {
        let links = vec![link(0, 3, "Ich"), link(4, 8, "gehe"), link(9, 13, "Haus")];
        // "gehe" moved by two units, "Haus" disappeared, "Ich" stayed.
        let remapped = remap_links(&links, "Ich, gehe heim");
        assert_eq!(remapped.len(), 2);
        assert_eq!((remapped[0].start_index, remapped[0].end_index), (0, 3));
        assert_eq!((remapped[1].start_index, remapped[1].end_index), (5, 9));
        // Two occurrences are ambiguous: the link is dropped rather than guessed.
        let twice = remap_links(&[link(30, 34, "gehe")], "gehe und gehe");
        assert!(twice.is_empty());
    }
}
