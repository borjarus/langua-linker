//! Grammar screen command (contract 1.6): the grammar tips of the collection grouped by deck
//! chapter.

use langualinker_application::grammar::{self, GrammarDeck, GrammarDeckSummary, GrammarOverview};
use langualinker_application::repositories::UnitOfWork;
use langualinker_contracts::ErrorDto;
use langualinker_contracts::dto::{
    GetGrammarInput, GrammarDeckDto, GrammarDeckSummaryDto, GrammarDto, GrammarEntryDto,
    GrammarSectionDto,
};
use langualinker_domain::DeckId;
use serde_json::Value;
use tauri::State;

use crate::backend::Backend;
use crate::commands::{blocking, parse_input};
use crate::content::target_language;
use crate::study::{app_error, id, text_id};

fn summary_dto(summary: GrammarDeckSummary) -> GrammarDeckSummaryDto {
    GrammarDeckSummaryDto {
        deck_id: text_id(summary.deck.id.0),
        name: summary.deck.name,
        target_language: target_language(summary.deck.target_language),
        level: summary.deck.level,
        tip_count: summary.tip_count,
        section_count: summary.section_count,
    }
}

fn deck_dto(deck: GrammarDeck) -> GrammarDeckDto {
    GrammarDeckDto {
        deck_id: text_id(deck.deck.id.0),
        name: deck.deck.name,
        target_language: target_language(deck.deck.target_language),
        level: deck.deck.level,
        sections: deck
            .sections
            .into_iter()
            .map(|section| {
                let (tag, title) = match section.section {
                    Some(section) => (Some(section.tag), Some(section.title)),
                    None => (None, None),
                };
                GrammarSectionDto {
                    tag,
                    title,
                    entries: section
                        .tips
                        .into_iter()
                        .map(|tip| GrammarEntryDto {
                            tip_id: text_id(tip.tip_id.0),
                            text: tip.text,
                            note_id: text_id(tip.note_id.0),
                            card_id: text_id(tip.card_id.0),
                            front: tip.front,
                            back: tip.back,
                        })
                        .collect(),
                }
            })
            .collect(),
    }
}

pub fn grammar_dto(overview: GrammarOverview) -> GrammarDto {
    GrammarDto {
        decks: overview.decks.into_iter().map(summary_dto).collect(),
        deck: overview.selected.map(deck_dto),
    }
}

#[tauri::command]
pub async fn get_grammar(
    backend: State<'_, Backend>,
    input: Value,
) -> Result<GrammarDto, ErrorDto> {
    let input: GetGrammarInput = parse_input(input)?;
    let deck = match input.deck_id.as_deref() {
        Some(value) => Some(DeckId(id("deckId", value)?)),
        None => None,
    };
    let db = backend.collection()?.db.clone();
    blocking(move || {
        db.read(move |tx| grammar::grammar(tx, deck))
            .map(grammar_dto)
            .map_err(app_error)
    })
    .await
}
