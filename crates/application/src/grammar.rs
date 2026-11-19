//! Grammar read model: the grammar tips of the collection grouped by deck and chapter.
//!
//! A deck's chapters come from its content manifest (`sections`, stored by the importer). A tip
//! belongs to the first chapter whose tag its note carries; tips of notes without a chapter tag
//! (the learner's own notes, decks without chapters) close the deck in a section without a title.
//! Only decks whose type shows grammar tips and that have at least one tip are listed.

use std::collections::{HashMap, HashSet};

use langualinker_domain::DeckId;
use langualinker_domain::deck::Deck;

use crate::repositories::{DeckSection, GrammarTipEntry, Repositories};
use crate::{AppError, AppResult};

/// A deck offered on the Grammar screen.
#[derive(Debug, Clone, PartialEq)]
pub struct GrammarDeckSummary {
    pub deck: Deck,
    pub tip_count: u32,
    pub section_count: u32,
}

/// A chapter with its tips; `section` is `None` for tips outside every chapter.
#[derive(Debug, Clone, PartialEq)]
pub struct GrammarSection {
    pub section: Option<DeckSection>,
    pub tips: Vec<GrammarTipEntry>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GrammarDeck {
    pub deck: Deck,
    pub sections: Vec<GrammarSection>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GrammarOverview {
    /// Decks with tips, by name.
    pub decks: Vec<GrammarDeckSummary>,
    /// Chapters of the requested deck, or of the first listed one when none was requested.
    pub selected: Option<GrammarDeck>,
}

/// Groups the tips of one deck by its chapters (reading order); empty chapters are left out.
pub fn group_by_section(
    sections: &[DeckSection],
    tips: Vec<GrammarTipEntry>,
) -> Vec<GrammarSection> {
    let mut grouped: Vec<Vec<GrammarTipEntry>> = vec![Vec::new(); sections.len()];
    let mut other = Vec::new();
    let index: HashMap<&str, usize> = sections
        .iter()
        .enumerate()
        .map(|(position, section)| (section.tag.as_str(), position))
        .collect();
    for tip in tips {
        let position = tip
            .tags
            .iter()
            .filter_map(|tag| index.get(tag.as_str()).copied())
            .min();
        match position {
            Some(position) => grouped[position].push(tip),
            None => other.push(tip),
        }
    }
    let mut result: Vec<GrammarSection> = sections
        .iter()
        .zip(grouped)
        .filter(|(_, tips)| !tips.is_empty())
        .map(|(section, tips)| GrammarSection {
            section: Some(section.clone()),
            tips,
        })
        .collect();
    if !other.is_empty() {
        result.push(GrammarSection {
            section: None,
            tips: other,
        });
    }
    result
}

/// The Grammar screen: decks with tips and the chapters of `deck` (or of the first deck).
/// A requested deck that has no tips, is deleted or unknown answers `NOT_FOUND`.
pub fn grammar(tx: &mut dyn Repositories, deck: Option<DeckId>) -> AppResult<GrammarOverview> {
    let decks: Vec<Deck> = tx
        .active_decks()?
        .into_iter()
        .filter(|deck| deck.deck_type.capabilities().grammar_tips)
        .collect();
    let shown: HashSet<DeckId> = decks.iter().map(|deck| deck.id).collect();
    let mut tips: HashMap<DeckId, Vec<GrammarTipEntry>> = HashMap::new();
    for entry in tx.grammar_tip_entries()? {
        if shown.contains(&entry.deck_id) {
            tips.entry(entry.deck_id).or_default().push(entry);
        }
    }
    let mut summaries = Vec::new();
    for deck in decks {
        let Some(deck_tips) = tips.get(&deck.id) else {
            continue;
        };
        let sections = tx.deck_sections(deck.id)?;
        let used: HashSet<&str> = deck_tips
            .iter()
            .flat_map(|tip| tip.tags.iter().map(String::as_str))
            .collect();
        let section_count = sections
            .iter()
            .filter(|section| used.contains(section.tag.as_str()))
            .count();
        summaries.push(GrammarDeckSummary {
            tip_count: deck_tips.len() as u32,
            section_count: section_count as u32,
            deck,
        });
    }
    let target = match deck {
        Some(id) => Some(
            summaries
                .iter()
                .find(|summary| summary.deck.id == id)
                .ok_or(AppError::NotFound { entity: "deck" })?,
        ),
        None => summaries.first(),
    };
    let selected = match target {
        Some(summary) => {
            let sections = tx.deck_sections(summary.deck.id)?;
            let deck_tips = tips.remove(&summary.deck.id).unwrap_or_default();
            Some(GrammarDeck {
                deck: summary.deck.clone(),
                sections: group_by_section(&sections, deck_tips),
            })
        }
        None => None,
    };
    Ok(GrammarOverview {
        decks: summaries,
        selected,
    })
}

#[cfg(test)]
mod tests {
    use langualinker_domain::{CardId, GrammarTipId, NoteId};

    use super::*;

    fn section(tag: &str) -> DeckSection {
        DeckSection {
            tag: tag.into(),
            title: format!("Title {tag}"),
        }
    }

    fn tip(id: i64, tags: &[&str]) -> GrammarTipEntry {
        GrammarTipEntry {
            deck_id: DeckId(1),
            tip_id: GrammarTipId(id),
            text: format!("tip {id}"),
            note_id: NoteId(id),
            card_id: CardId(id),
            front: String::new(),
            back: String::new(),
            tags: tags.iter().map(|tag| (*tag).to_owned()).collect(),
        }
    }

    fn ids(section: &GrammarSection) -> Vec<i64> {
        section.tips.iter().map(|tip| tip.tip_id.0).collect()
    }

    #[test]
    fn tips_follow_the_chapter_order_and_keep_their_own_order() {
        let sections = [section("l-01"), section("l-02"), section("l-03")];
        let grouped = group_by_section(
            &sections,
            vec![
                tip(1, &["A1", "l-02"]),
                tip(2, &["l-01"]),
                tip(3, &["l-02"]),
            ],
        );
        let tags: Vec<_> = grouped
            .iter()
            .map(|group| group.section.as_ref().map(|section| section.tag.as_str()))
            .collect();
        assert_eq!(
            tags,
            [Some("l-01"), Some("l-02")],
            "empty chapter l-03 is left out"
        );
        assert_eq!(ids(&grouped[0]), [2]);
        assert_eq!(ids(&grouped[1]), [1, 3]);
    }

    #[test]
    fn tips_outside_every_chapter_close_the_deck() {
        let grouped = group_by_section(
            &[section("l-01")],
            vec![tip(1, &["own"]), tip(2, &["l-01"])],
        );
        assert_eq!(grouped.len(), 2);
        assert_eq!(grouped[1].section, None);
        assert_eq!(ids(&grouped[1]), [1]);
    }

    #[test]
    fn a_note_in_two_chapters_counts_in_the_earlier_one() {
        let sections = [section("l-01"), section("l-02")];
        let grouped = group_by_section(&sections, vec![tip(1, &["l-02", "l-01"])]);
        assert_eq!(grouped.len(), 1);
        assert_eq!(grouped[0].section.as_ref().unwrap().tag, "l-01");
    }

    #[test]
    fn a_deck_without_chapters_has_one_untitled_section() {
        let grouped = group_by_section(&[], vec![tip(1, &[]), tip(2, &["x"])]);
        assert_eq!(grouped.len(), 1);
        assert!(grouped[0].section.is_none());
        assert_eq!(ids(&grouped[0]), [1, 2]);
    }
}
