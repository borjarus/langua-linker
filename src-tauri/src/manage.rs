//! Management commands (plan Phase 11): decks, presets, note details, the editor, note content,
//! the word search and the card browser, mapped to contract v1 DTOs.
//!
//! Every mutation runs in one write transaction and is idempotent by `operationId`; a committed
//! change emits `collection_changed` as a read signal for the screens' queries.

use langualinker_application::browser::{
    self, BulkAction as AppBulkAction, BulkSelection as AppBulkSelection, CardSearch, SortName,
};
use langualinker_application::management::{
    self, CreateDeck, DeckOverview, DeckPatch, LinkDraft, NoteContentChange as AppChange,
    NoteDetails, PresetConfig, SaveNote, SavePreset,
};
use langualinker_application::ports::CollectionScope;
use langualinker_application::repositories::{ContentCounts, UnitOfWork};
use langualinker_contracts::ErrorDto;
use langualinker_contracts::dto::{
    BulkAction, BulkActionInput, BulkPreviewDto, BulkPreviewInput, BulkResultDto, BulkSelection,
    CardDetailsDto, CardPageDto, CardRowDto, ContentState, CreateDeckInput, DeckCountsDto,
    DeckOverviewDto, DeckSummaryDto, DeckType, DeckTypeOptionDto, DeleteDeckInput,
    DeleteDeckResultDto, DeleteNoteInput, DeleteNoteResultDto, DuplicateCheckDto,
    DuplicateCheckInput, DuplicateNoteDto, EditorDataDto, EditorDeckOptionDto,
    HiddenSectionCountsDto, NativeLanguage, NoteContentChange, NoteDetailsDto, NoteDto,
    NoteTemplateDto, NoteTypeDto, NoteTypeKind, PresetConfigDto, PresetDto, Rating,
    RetrievabilityDto, ReviewLogDto, SaveNoteInput, SavePresetInput, SearchInput, SearchWordsInput,
    SortDirection, SortKey, UpdateDeckInput, UpdateNoteContentInput, WordMatchDto, WordMatchKind,
};
use langualinker_domain::deck::{self, DeckConfig};
use langualinker_domain::{
    AssociationId, CardId, DeckConfigId, DeckId, GrammarTipId, NoteId, WordLinkId, note, review,
};
use serde_json::Value;
use tauri::State;

use crate::backend::{Backend, Collection, iso};
use crate::commands::{blocking, operation_id, parse_input};
use crate::content::{capabilities, clamp, deck_type, domain_target_language, target_language};
use crate::study::{
    app_error, association_dto, card_state, direction_dto, domain_flag, emit, flag_dto, id,
    link_dto, queue_state, text_id, tips_dto,
};

// ---------------------------------------------------------------- mapping

fn domain_deck_type(value: DeckType) -> deck::DeckType {
    match value {
        DeckType::Simple => deck::DeckType::Simple,
        DeckType::TextWithAssociations => deck::DeckType::TextWithAssociations,
        DeckType::Linguistic => deck::DeckType::Linguistic,
    }
}

fn note_type_dto(kind: note::NoteTypeKind) -> NoteTypeKind {
    match kind {
        note::NoteTypeKind::Basic => NoteTypeKind::Basic,
        note::NoteTypeKind::BasicAndReversed => NoteTypeKind::BasicAndReversed,
        note::NoteTypeKind::Sentence => NoteTypeKind::Sentence,
        note::NoteTypeKind::SentenceAndReversed => NoteTypeKind::SentenceAndReversed,
    }
}

fn domain_note_type(kind: NoteTypeKind) -> note::NoteTypeKind {
    match kind {
        NoteTypeKind::Basic => note::NoteTypeKind::Basic,
        NoteTypeKind::BasicAndReversed => note::NoteTypeKind::BasicAndReversed,
        NoteTypeKind::Sentence => note::NoteTypeKind::Sentence,
        NoteTypeKind::SentenceAndReversed => note::NoteTypeKind::SentenceAndReversed,
    }
}

fn rating_dto(rating: review::Rating) -> Rating {
    match rating {
        review::Rating::Again => Rating::Again,
        review::Rating::Hard => Rating::Hard,
        review::Rating::Good => Rating::Good,
        review::Rating::Easy => Rating::Easy,
    }
}

fn hidden_dto(counts: ContentCounts) -> HiddenSectionCountsDto {
    HiddenSectionCountsDto {
        associations: counts.associations,
        grammar_tips: counts.grammar_tips,
        word_links: counts.word_links,
    }
}

fn summary_dto(item: langualinker_application::dashboard::DashboardDeck) -> DeckSummaryDto {
    DeckSummaryDto {
        id: text_id(item.deck.id.0),
        guid: item.deck.guid.as_str().to_owned(),
        name: item.deck.name,
        parent_id: item.deck.parent_id.map(|id| text_id(id.0)),
        target_language: target_language(item.deck.target_language),
        native_language: NativeLanguage::Pl,
        deck_type: deck_type(item.deck.deck_type),
        capabilities: capabilities(item.deck.deck_type),
        counts: DeckCountsDto {
            new: item.counts.new,
            learning: item.counts.learning,
            review: item.counts.review,
            overdue: item.counts.overdue,
            total: item.counts.total,
        },
        progress: item.progress,
        bundled: item.deck.bundled,
        content_state: if item.preparing {
            ContentState::Preparing
        } else {
            ContentState::Ready
        },
        version: clamp(item.deck.version),
    }
}

fn overview_dto(overview: DeckOverview) -> DeckOverviewDto {
    let content = overview.content;
    let current = overview.deck.deck.deck_type;
    DeckOverviewDto {
        description: overview.deck.deck.description.clone(),
        preset_id: text_id(overview.preset.id.0),
        preset_name: overview.preset.name.clone(),
        preset_dependent_decks: overview.preset_dependent_decks,
        hidden_section_counts: hidden_dto(management::hidden_by(current, content)),
        type_options: deck::DeckType::ALL
            .iter()
            .map(|option| DeckTypeOptionDto {
                deck_type: deck_type(*option),
                capabilities: capabilities(*option),
                hidden_counts: hidden_dto(management::hidden_by(*option, content)),
            })
            .collect(),
        deck: summary_dto(overview.deck),
        subdecks: overview.subdecks.into_iter().map(summary_dto).collect(),
    }
}

fn preset_dto((config, dependents): (DeckConfig, u32)) -> PresetDto {
    let options = config.options;
    PresetDto {
        id: text_id(config.id.0),
        name: config.name,
        version: clamp(config.version),
        built_in: config.built_in,
        dependent_decks: dependents,
        config: PresetConfigDto {
            new_per_day: options.new_per_day,
            reviews_per_day: options.reviews_per_day,
            desired_retention: options.desired_retention,
            max_interval_days: options.maximum_interval_days,
            learning_steps_minutes: options.learning_steps_minutes,
            relearning_steps_minutes: options.relearning_steps_minutes,
            weights: options.weights,
        },
    }
}

fn details_dto(details: NoteDetails) -> NoteDetailsDto {
    let computed_at = iso(details.computed_at);
    NoteDetailsDto {
        note: NoteDto {
            id: text_id(details.note.id.0),
            guid: details.note.guid.as_str().to_owned(),
            note_type_kind: note_type_dto(details.kind),
            front: details.note.fields.front,
            back: details.note.fields.back,
            tags: details.note.tags,
            version: clamp(details.note.version),
            created_at: iso(details.note.created_at),
            updated_at: iso(details.note.updated_at),
        },
        deck_id: text_id(details.deck.id.0),
        target_language: target_language(details.deck.target_language),
        capabilities: capabilities(details.deck.deck_type),
        cards: details
            .cards
            .into_iter()
            .map(|item| CardDetailsDto {
                id: text_id(item.card.id.0),
                note_id: text_id(item.card.note_id.0),
                deck_id: text_id(item.deck.id.0),
                deck_name: item.deck.name,
                ordinal: item.card.template_ordinal,
                direction: direction_dto(item.direction),
                state: card_state(item.card.state),
                queue: queue_state(item.card.queue),
                flag: flag_dto(item.card.flag),
                due_at: item.card.due_at.map(iso),
                stability: item.card.fsrs.stability,
                difficulty: item.card.fsrs.difficulty,
                retrievability: item.retrievability.map(|value| RetrievabilityDto {
                    value,
                    computed_at: computed_at.clone(),
                }),
                reps: clamp(item.card.reps),
                lapses: clamp(item.card.lapses),
                version: clamp(item.card.version),
                history: item
                    .history
                    .into_iter()
                    .map(|log| ReviewLogDto {
                        id: text_id(log.id.0),
                        reviewed_at: iso(log.reviewed_at),
                        rating: rating_dto(log.rating),
                        state_before: card_state(log.card_before.state),
                        interval_seconds: log
                            .card_after
                            .due_at
                            .map_or(0, |due| clamp((due.0 - log.reviewed_at.0).max(0) / 1_000)),
                    })
                    .collect(),
            })
            .collect(),
        archived_ordinals: details.archived_ordinals,
        associations: details
            .associations
            .into_iter()
            .map(association_dto)
            .collect(),
        grammar_tips: tips_dto(details.grammar_tips),
        word_links: details.word_links.into_iter().map(link_dto).collect(),
    }
}

fn sort_name(sort: SortKey) -> SortName {
    match sort {
        SortKey::Due => SortName::Due,
        SortKey::Created => SortName::Created,
        SortKey::Front => SortName::Front,
        SortKey::Deck => SortName::Deck,
        SortKey::Reps => SortName::Reps,
        SortKey::Lapses => SortName::Lapses,
    }
}

fn card_ids(field: &'static str, values: &[String]) -> Result<Vec<CardId>, ErrorDto> {
    values
        .iter()
        .map(|value| id(field, value).map(CardId))
        .collect()
}

fn selection(selection: BulkSelection) -> Result<AppBulkSelection, ErrorDto> {
    Ok(match selection {
        BulkSelection::Ids { card_ids: ids } => AppBulkSelection::Ids(card_ids("cardIds", &ids)?),
        BulkSelection::Query {
            filter_version,
            excluded_card_ids,
        } => AppBulkSelection::Query {
            filter_version,
            excluded: card_ids("excludedCardIds", &excluded_card_ids)?,
        },
    })
}

fn bulk_action(action: BulkAction) -> Result<AppBulkAction, ErrorDto> {
    Ok(match action {
        BulkAction::Suspend => AppBulkAction::Suspend,
        BulkAction::Unsuspend => AppBulkAction::Unsuspend,
        BulkAction::Flag { flag } => AppBulkAction::Flag(domain_flag(flag)),
        BulkAction::ChangeDeck { deck_id } => {
            AppBulkAction::ChangeDeck(DeckId(id("deckId", &deck_id)?))
        }
        BulkAction::AddTag { tag } => AppBulkAction::AddTag(tag),
        BulkAction::RemoveTag { tag } => AppBulkAction::RemoveTag(tag),
        BulkAction::Reposition { start, step } => AppBulkAction::Reposition {
            start: i64::from(start),
            step: i64::from(step),
        },
        BulkAction::Delete => AppBulkAction::Delete,
    })
}

fn pending(collection: &Collection) -> Result<Vec<String>, ErrorDto> {
    collection.content.pending().map_err(app_error)
}

const DECK_SCOPES: &[CollectionScope] = &[CollectionScope::Decks];
const NOTE_SCOPES: &[CollectionScope] = &[
    CollectionScope::Notes,
    CollectionScope::Cards,
    CollectionScope::Decks,
];

// ---------------------------------------------------------------- decks and presets

#[tauri::command]
pub async fn get_deck(
    backend: State<'_, Backend>,
    input: Value,
) -> Result<DeckOverviewDto, ErrorDto> {
    let deck: String = parse_input(input)?;
    let deck = DeckId(id("deckId", &deck)?);
    let collection = backend.collection()?;
    let pending = pending(collection)?;
    let (db, now) = (collection.db.clone(), collection.clock.now());
    blocking(move || {
        db.write(move |tx| management::deck_overview(tx, deck, now, &pending))
            .map(overview_dto)
            .map_err(app_error)
    })
    .await
}

#[tauri::command]
pub async fn create_deck(
    backend: State<'_, Backend>,
    input: Value,
) -> Result<DeckOverviewDto, ErrorDto> {
    let input: CreateDeckInput = parse_input(input)?;
    operation_id(&input.operation_id)?;
    let create = CreateDeck {
        operation_id: input.operation_id,
        name: input.name,
        parent_id: input
            .parent_id
            .as_deref()
            .map(|value| id("parentId", value).map(DeckId))
            .transpose()?,
        target_language: domain_target_language(input.target_language),
        deck_type: domain_deck_type(input.deck_type),
    };
    let collection = backend.collection()?;
    let pending = pending(collection)?;
    let (db, rng, now) = (
        collection.db.clone(),
        collection.rng.clone(),
        collection.clock.now(),
    );
    let overview = blocking(move || {
        db.write(move |tx| {
            let deck = management::create_deck(tx, rng.as_ref(), &create, now)?;
            management::deck_overview(tx, deck, now, &pending)
        })
        .map_err(app_error)
    })
    .await?;
    emit(collection, DECK_SCOPES, now);
    Ok(overview_dto(overview))
}

#[tauri::command]
pub async fn update_deck(
    backend: State<'_, Backend>,
    input: Value,
) -> Result<DeckOverviewDto, ErrorDto> {
    let input: UpdateDeckInput = parse_input(input)?;
    operation_id(&input.operation_id)?;
    let deck = DeckId(id("deckId", &input.deck_id)?);
    let patch = DeckPatch {
        name: input.patch.name,
        description: input.patch.description,
        parent_id: match input.patch.parent_id {
            None => None,
            Some(None) => Some(None),
            Some(Some(parent)) => Some(Some(DeckId(id("parentId", &parent)?))),
        },
        deck_type: input.patch.deck_type.map(domain_deck_type),
        preset_id: input
            .patch
            .preset_id
            .as_deref()
            .map(|value| id("presetId", value).map(DeckConfigId))
            .transpose()?,
    };
    let collection = backend.collection()?;
    let pending = pending(collection)?;
    let (db, now) = (collection.db.clone(), collection.clock.now());
    let overview = blocking(move || {
        db.write(move |tx| {
            management::update_deck(
                tx,
                &input.operation_id,
                deck,
                i64::from(input.expected_version),
                &patch,
                now,
            )?;
            management::deck_overview(tx, deck, now, &pending)
        })
        .map_err(app_error)
    })
    .await?;
    emit(collection, DECK_SCOPES, now);
    Ok(overview_dto(overview))
}

#[tauri::command]
pub async fn delete_deck(
    backend: State<'_, Backend>,
    input: Value,
) -> Result<DeleteDeckResultDto, ErrorDto> {
    let input: DeleteDeckInput = parse_input(input)?;
    operation_id(&input.operation_id)?;
    let deck = DeckId(id("deckId", &input.deck_id)?);
    let collection = backend.collection()?;
    let (db, now) = (collection.db.clone(), collection.clock.now());
    let outcome = blocking(move || {
        db.write(move |tx| {
            management::delete_deck(
                tx,
                &input.operation_id,
                deck,
                i64::from(input.expected_version),
                now,
            )
        })
        .map_err(app_error)
    })
    .await?;
    emit(
        collection,
        &[
            CollectionScope::Decks,
            CollectionScope::Cards,
            CollectionScope::Notes,
            CollectionScope::Stats,
        ],
        now,
    );
    Ok(DeleteDeckResultDto {
        deleted_decks: outcome.deleted_decks,
        deleted_cards: outcome.deleted_cards,
    })
}

#[tauri::command]
pub async fn list_presets(backend: State<'_, Backend>) -> Result<Vec<PresetDto>, ErrorDto> {
    let db = backend.collection()?.db.clone();
    blocking(move || {
        db.read(management::presets)
            .map(|presets| presets.into_iter().map(preset_dto).collect())
            .map_err(app_error)
    })
    .await
}

#[tauri::command]
pub async fn save_preset(backend: State<'_, Backend>, input: Value) -> Result<PresetDto, ErrorDto> {
    let input: SavePresetInput = parse_input(input)?;
    operation_id(&input.operation_id)?;
    let save = SavePreset {
        operation_id: input.operation_id,
        preset_id: input
            .preset_id
            .as_deref()
            .map(|value| id("presetId", value).map(DeckConfigId))
            .transpose()?,
        expected_version: input.expected_version.map(i64::from),
        name: input.name,
        config: PresetConfig {
            new_per_day: input.config.new_per_day,
            reviews_per_day: input.config.reviews_per_day,
            desired_retention: input.config.desired_retention,
            max_interval_days: input.config.max_interval_days,
            learning_steps_minutes: input.config.learning_steps_minutes,
            relearning_steps_minutes: input.config.relearning_steps_minutes,
            weights: input.config.weights,
        },
    };
    let collection = backend.collection()?;
    let (db, rng, now) = (
        collection.db.clone(),
        collection.rng.clone(),
        collection.clock.now(),
    );
    let preset = blocking(move || {
        db.write(move |tx| management::save_preset(tx, rng.as_ref(), &save, now))
            .map_err(app_error)
    })
    .await?;
    emit(
        collection,
        &[CollectionScope::Decks, CollectionScope::Cards],
        now,
    );
    Ok(preset_dto(preset))
}

// ---------------------------------------------------------------- details and editor

#[tauri::command]
pub async fn get_note_details(
    backend: State<'_, Backend>,
    input: Value,
) -> Result<NoteDetailsDto, ErrorDto> {
    let card: String = parse_input(input)?;
    let card = CardId(id("cardId", &card)?);
    let collection = backend.collection()?;
    let (db, now) = (collection.db.clone(), collection.clock.now());
    blocking(move || {
        db.read(move |tx| management::card_note_details(tx, card, now))
            .map(details_dto)
            .map_err(app_error)
    })
    .await
}

#[tauri::command]
pub async fn get_editor_data(backend: State<'_, Backend>) -> Result<EditorDataDto, ErrorDto> {
    let db = backend.collection()?.db.clone();
    let (decks, tags) =
        blocking(move || db.read(management::editor_data).map_err(app_error)).await?;
    Ok(EditorDataDto {
        decks: decks
            .into_iter()
            .map(|deck| EditorDeckOptionDto {
                id: text_id(deck.id.0),
                name: deck.name,
                parent_id: deck.parent_id.map(|id| text_id(id.0)),
                deck_type: deck_type(deck.deck_type),
                capabilities: capabilities(deck.deck_type),
                target_language: target_language(deck.target_language),
            })
            .collect(),
        note_types: note::NoteTypeKind::ALL
            .iter()
            .map(|kind| NoteTypeDto {
                kind: note_type_dto(*kind),
                templates: kind
                    .templates()
                    .iter()
                    .enumerate()
                    .map(|(ordinal, direction)| NoteTemplateDto {
                        ordinal: ordinal as u32,
                        direction: direction_dto(*direction),
                    })
                    .collect(),
            })
            .collect(),
        tags,
        max_field_length: management::MAX_FIELD_CHARS as u32,
    })
}

#[tauri::command]
pub async fn check_duplicate(
    backend: State<'_, Backend>,
    input: Value,
) -> Result<DuplicateCheckDto, ErrorDto> {
    let input: DuplicateCheckInput = parse_input(input)?;
    let deck = DeckId(id("deckId", &input.deck_id)?);
    let exclude = input
        .exclude_note_id
        .as_deref()
        .map(|value| id("excludeNoteId", value).map(NoteId))
        .transpose()?;
    let db = backend.collection()?.db.clone();
    blocking(move || {
        db.read(move |tx| management::duplicates(tx, deck, &input.front, exclude))
            .map(|found| DuplicateCheckDto {
                duplicates: found
                    .into_iter()
                    .map(|(found, deck_name)| DuplicateNoteDto {
                        note_id: text_id(found.note_id.0),
                        front: found.front,
                        deck_name,
                    })
                    .collect(),
            })
            .map_err(app_error)
    })
    .await
}

#[tauri::command]
pub async fn save_note(
    backend: State<'_, Backend>,
    input: Value,
) -> Result<NoteDetailsDto, ErrorDto> {
    let input: SaveNoteInput = parse_input(input)?;
    operation_id(&input.operation_id)?;
    let word_links = match input.word_links {
        None => None,
        Some(drafts) => Some(
            drafts
                .into_iter()
                .map(|draft| {
                    Ok(LinkDraft {
                        start: draft.start_index,
                        end: draft.end_index,
                        surface: draft.surface,
                        word: draft
                            .word_note_id
                            .as_deref()
                            .map(|value| id("wordNoteId", value).map(NoteId))
                            .transpose()?,
                    })
                })
                .collect::<Result<Vec<_>, ErrorDto>>()?,
        ),
    };
    let save = SaveNote {
        operation_id: input.operation_id,
        note_id: input
            .note_id
            .as_deref()
            .map(|value| id("noteId", value).map(NoteId))
            .transpose()?,
        expected_version: input.expected_version.map(i64::from),
        deck_id: DeckId(id("deckId", &input.deck_id)?),
        kind: domain_note_type(input.note_type_kind),
        front: input.front,
        back: input.back,
        tags: input.tags,
        manual_association: input.manual_association,
        grammar_tips: input.grammar_tips,
        word_links,
    };
    let collection = backend.collection()?;
    let (db, rng, now) = (
        collection.db.clone(),
        collection.rng.clone(),
        collection.clock.now(),
    );
    let details = blocking(move || {
        db.write(move |tx| {
            let note = management::save_note(tx, rng.as_ref(), &save, now)?;
            management::note_details(tx, note, now)
        })
        .map_err(app_error)
    })
    .await?;
    emit(collection, NOTE_SCOPES, now);
    Ok(details_dto(details))
}

#[tauri::command]
pub async fn delete_note(
    backend: State<'_, Backend>,
    input: Value,
) -> Result<DeleteNoteResultDto, ErrorDto> {
    let input: DeleteNoteInput = parse_input(input)?;
    operation_id(&input.operation_id)?;
    let note = NoteId(id("noteId", &input.note_id)?);
    let collection = backend.collection()?;
    let (db, now) = (collection.db.clone(), collection.clock.now());
    let deleted_cards = blocking(move || {
        db.write(move |tx| {
            management::delete_note(
                tx,
                &input.operation_id,
                note,
                i64::from(input.expected_version),
                now,
            )
        })
        .map_err(app_error)
    })
    .await?;
    emit(
        collection,
        &[
            CollectionScope::Notes,
            CollectionScope::Cards,
            CollectionScope::Decks,
            CollectionScope::Stats,
        ],
        now,
    );
    Ok(DeleteNoteResultDto { deleted_cards })
}

#[tauri::command]
pub async fn update_note_content(
    backend: State<'_, Backend>,
    input: Value,
) -> Result<NoteDetailsDto, ErrorDto> {
    let input: UpdateNoteContentInput = parse_input(input)?;
    operation_id(&input.operation_id)?;
    let note = NoteId(id("noteId", &input.note_id)?);
    let change = match input.change {
        NoteContentChange::AddTip { text } => AppChange::AddTip(text),
        NoteContentChange::EditTip { tip_id, text } => {
            AppChange::EditTip(GrammarTipId(id("tipId", &tip_id)?), text)
        }
        NoteContentChange::DeleteTip { tip_id } => {
            AppChange::DeleteTip(GrammarTipId(id("tipId", &tip_id)?))
        }
        NoteContentChange::ReorderTips { tip_ids } => AppChange::ReorderTips(
            tip_ids
                .iter()
                .map(|value| id("tipIds", value).map(GrammarTipId))
                .collect::<Result<_, _>>()?,
        ),
        NoteContentChange::AddAssociation { text } => AppChange::AddAssociation(text),
        NoteContentChange::EditAssociation {
            association_id,
            text,
        } => AppChange::EditAssociation(AssociationId(id("associationId", &association_id)?), text),
        NoteContentChange::DeleteAssociation { association_id } => {
            AppChange::DeleteAssociation(AssociationId(id("associationId", &association_id)?))
        }
        NoteContentChange::UnlinkWord { link_id } => {
            AppChange::UnlinkWord(WordLinkId(id("linkId", &link_id)?))
        }
    };
    let collection = backend.collection()?;
    let (db, rng, now) = (
        collection.db.clone(),
        collection.rng.clone(),
        collection.clock.now(),
    );
    let details = blocking(move || {
        db.write(move |tx| {
            management::update_note_content(
                tx,
                rng.as_ref(),
                &input.operation_id,
                note,
                &change,
                now,
            )?;
            management::note_details(tx, note, now)
        })
        .map_err(app_error)
    })
    .await?;
    emit(collection, &[CollectionScope::Notes], now);
    Ok(details_dto(details))
}

#[tauri::command]
pub async fn search_words(
    backend: State<'_, Backend>,
    input: Value,
) -> Result<Vec<WordMatchDto>, ErrorDto> {
    let input: SearchWordsInput = parse_input(input)?;
    let language = domain_target_language(input.target_language);
    let db = backend.collection()?.db.clone();
    blocking(move || {
        db.read(move |tx| management::search_words(tx, language, &input.text, input.limit))
            .map(|matches| {
                matches
                    .into_iter()
                    .map(|found| WordMatchDto {
                        note_id: text_id(found.note_id.0),
                        lemma: found.lemma,
                        translation: found.translation,
                        deck_name: found.deck_name,
                        match_kind: if found.exact {
                            WordMatchKind::Exact
                        } else {
                            WordMatchKind::Normalized
                        },
                    })
                    .collect()
            })
            .map_err(app_error)
    })
    .await
}

// ---------------------------------------------------------------- browser

#[tauri::command]
pub async fn search_cards(
    backend: State<'_, Backend>,
    input: Value,
) -> Result<CardPageDto, ErrorDto> {
    let input: SearchInput = parse_input(input)?;
    let search = CardSearch {
        query: input.query,
        deck_id: input
            .deck_id
            .as_deref()
            .map(|value| id("deckId", value))
            .transpose()?,
        tag: input.tag,
        states: input
            .states
            .into_iter()
            .map(|state| {
                use langualinker_contracts::dto::CardState as Dto;
                use langualinker_domain::card::CardState as Domain;
                match state {
                    Dto::New => Domain::New,
                    Dto::Learning => Domain::Learning,
                    Dto::Review => Domain::Review,
                    Dto::Relearning => Domain::Relearning,
                }
                .db_code()
            })
            .collect(),
        flags: input
            .flags
            .into_iter()
            .map(|flag| domain_flag(flag).db_code())
            .collect(),
        sort: sort_name(input.sort),
        descending: input.direction == SortDirection::Desc,
    };
    let collection = backend.collection()?;
    let (db, now) = (collection.db.clone(), collection.clock.now());
    blocking(move || {
        db.read(move |tx| {
            browser::search_cards(tx, &search, input.cursor.as_deref(), input.page_size, now)
        })
        .map(|page| CardPageDto {
            items: page
                .rows
                .into_iter()
                .map(|row| {
                    let (front, back) = match row.direction {
                        note::CardDirection::Forward => {
                            (row.note.fields.front, row.note.fields.back)
                        }
                        note::CardDirection::Reverse => {
                            (row.note.fields.back, row.note.fields.front)
                        }
                    };
                    let is_new = row.card.state == langualinker_domain::card::CardState::New;
                    CardRowDto {
                        card_id: text_id(row.card.id.0),
                        note_id: text_id(row.note.id.0),
                        deck_id: text_id(row.card.deck_id.0),
                        deck_name: row.deck_name,
                        front,
                        back,
                        state: card_state(row.card.state),
                        queue: queue_state(row.card.queue),
                        flag: flag_dto(row.card.flag),
                        due_at: row.card.due_at.map(iso),
                        position: is_new.then(|| clamp(row.card.position)),
                        direction: direction_dto(row.direction),
                        tags: row.note.tags,
                    }
                })
                .collect(),
            next_cursor: page.next_cursor,
            total: page.total,
            filter_version: page.filter_version,
        })
        .map_err(app_error)
    })
    .await
}

#[tauri::command]
pub async fn preview_bulk_action(
    backend: State<'_, Backend>,
    input: Value,
) -> Result<BulkPreviewDto, ErrorDto> {
    let input: BulkPreviewInput = parse_input(input)?;
    let selection = selection(input.selection)?;
    let action = bulk_action(input.action)?;
    let collection = backend.collection()?;
    let (db, now) = (collection.db.clone(), collection.clock.now());
    blocking(move || {
        db.read(move |tx| browser::preview_bulk_action(tx, &selection, &action, now))
            .map(|preview| BulkPreviewDto {
                selected: preview.selected,
                affected: preview.affected,
                unchanged: preview.unchanged,
                notes: preview.notes,
                decks: preview.decks,
            })
            .map_err(app_error)
    })
    .await
}

#[tauri::command]
pub async fn apply_bulk_action(
    backend: State<'_, Backend>,
    input: Value,
) -> Result<BulkResultDto, ErrorDto> {
    let input: BulkActionInput = parse_input(input)?;
    operation_id(&input.operation_id)?;
    let selection = selection(input.selection)?;
    let action = bulk_action(input.action)?;
    let collection = backend.collection()?;
    let (db, now) = (collection.db.clone(), collection.clock.now());
    let affected = blocking(move || {
        db.write(move |tx| {
            browser::apply_bulk_action(tx, &input.operation_id, &selection, &action, now)
        })
        .map_err(app_error)
    })
    .await?;
    emit(
        collection,
        &[
            CollectionScope::Cards,
            CollectionScope::Notes,
            CollectionScope::Decks,
            CollectionScope::Stats,
        ],
        now,
    );
    Ok(BulkResultDto { affected })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_types_map_both_ways() {
        for kind in note::NoteTypeKind::ALL {
            assert_eq!(domain_note_type(note_type_dto(*kind)), *kind);
        }
    }

    #[test]
    fn every_deck_type_is_offered_with_what_it_hides() {
        let content = ContentCounts {
            associations: 2,
            grammar_tips: 1,
            word_links: 3,
        };
        let simple = management::hidden_by(deck::DeckType::Simple, content);
        assert_eq!(hidden_dto(simple).word_links, 3);
        assert_eq!(domain_deck_type(DeckType::Simple), deck::DeckType::Simple);
    }

    #[test]
    fn bulk_selection_ids_are_validated() {
        let bad = selection(BulkSelection::Ids {
            card_ids: vec!["0".into()],
        });
        assert!(bad.is_err());
        let good = selection(BulkSelection::Query {
            filter_version: "fv1.00".into(),
            excluded_card_ids: vec!["3".into()],
        })
        .unwrap();
        assert_eq!(
            good,
            AppBulkSelection::Query {
                filter_version: "fv1.00".into(),
                excluded: vec![CardId(3)],
            }
        );
    }

    #[test]
    fn invalid_reposition_values_reach_the_use_case_as_numbers() {
        let action = bulk_action(BulkAction::Reposition { start: 5, step: 2 }).unwrap();
        assert_eq!(action, AppBulkAction::Reposition { start: 5, step: 2 });
        assert!(
            bulk_action(BulkAction::ChangeDeck {
                deck_id: "x".into()
            })
            .is_err()
        );
    }
}
