//! Deck level, chapters and the Grammar screen's tips (schema v7).

use langualinker_application::AppResult;
use langualinker_application::repositories::{DeckSection, GrammarRepository, GrammarTipEntry};
use langualinker_domain::{CardId, DeckId, GrammarTipId, NoteId};
use rusqlite::params;

use super::SqliteRepos;
use crate::sqlite::{codec, map_sqlite};

/// Separates tag names in `group_concat` (tags never contain control characters).
const TAG_SEPARATOR: char = '\u{1f}';

struct EntryRow {
    deck_id: i64,
    tip_id: i64,
    text: String,
    note_id: i64,
    card_id: i64,
    fields: String,
    tags: Option<String>,
}

impl GrammarRepository for SqliteRepos<'_> {
    fn replace_deck_outline(
        &mut self,
        deck: DeckId,
        level: Option<&str>,
        sections: &[DeckSection],
    ) -> AppResult<()> {
        // Not a user edit: neither `updated_at` nor `version` change.
        self.tx
            .execute(
                "UPDATE decks SET level = ?2 WHERE id = ?1",
                params![deck.0, level],
            )
            .map_err(map_sqlite)?;
        self.tx
            .execute("DELETE FROM deck_sections WHERE deck_id = ?1", [deck.0])
            .map_err(map_sqlite)?;
        let mut insert = self
            .tx
            .prepare_cached(
                "INSERT INTO deck_sections (deck_id, tag, title, sort_order) VALUES (?1, ?2, ?3, ?4)",
            )
            .map_err(map_sqlite)?;
        for (order, section) in sections.iter().enumerate() {
            insert
                .execute(params![deck.0, section.tag, section.title, order as i64])
                .map_err(map_sqlite)?;
        }
        Ok(())
    }

    fn deck_sections(&mut self, deck: DeckId) -> AppResult<Vec<DeckSection>> {
        let mut statement = self
            .tx
            .prepare_cached(
                "SELECT tag, title FROM deck_sections WHERE deck_id = ?1 ORDER BY sort_order, tag",
            )
            .map_err(map_sqlite)?;
        statement
            .query_map([deck.0], |row| {
                Ok(DeckSection {
                    tag: row.get(0)?,
                    title: row.get(1)?,
                })
            })
            .and_then(Iterator::collect)
            .map_err(map_sqlite)
    }

    fn grammar_tip_entries(&mut self) -> AppResult<Vec<GrammarTipEntry>> {
        let sql = format!(
            "WITH first_card AS (
                 SELECT c.note_id, c.id, c.deck_id,
                        ROW_NUMBER() OVER (PARTITION BY c.note_id
                                           ORDER BY c.template_ordinal, c.id) AS rank
                 FROM cards c
                 WHERE c.deleted_at IS NULL
                   AND c.note_id IN (SELECT note_id FROM grammar_tips WHERE deleted_at IS NULL)
             )
             SELECT fc.deck_id, g.id, g.text, n.id, fc.id, n.fields,
                    (SELECT group_concat(t.name, char({sep}))
                     FROM note_tags nt JOIN tags t ON t.id = nt.tag_id
                     WHERE nt.note_id = n.id)
             FROM grammar_tips g
             JOIN notes n ON n.id = g.note_id AND n.deleted_at IS NULL
             JOIN first_card fc ON fc.note_id = n.id AND fc.rank = 1
             JOIN decks d ON d.id = fc.deck_id AND d.deleted_at IS NULL
             WHERE g.deleted_at IS NULL
             ORDER BY fc.deck_id, n.id, g.sort_order, g.id",
            sep = TAG_SEPARATOR as u32
        );
        let mut statement = self.tx.prepare_cached(&sql).map_err(map_sqlite)?;
        let rows: Vec<EntryRow> = statement
            .query_map([], |row| {
                Ok(EntryRow {
                    deck_id: row.get(0)?,
                    tip_id: row.get(1)?,
                    text: row.get(2)?,
                    note_id: row.get(3)?,
                    card_id: row.get(4)?,
                    fields: row.get(5)?,
                    tags: row.get(6)?,
                })
            })
            .and_then(Iterator::collect)
            .map_err(map_sqlite)?;
        rows.into_iter()
            .map(|row| {
                let fields = codec::note_fields_from_json(&row.fields)?;
                Ok(GrammarTipEntry {
                    deck_id: DeckId(row.deck_id),
                    tip_id: GrammarTipId(row.tip_id),
                    text: row.text,
                    note_id: NoteId(row.note_id),
                    card_id: CardId(row.card_id),
                    front: fields.front,
                    back: fields.back,
                    tags: row
                        .tags
                        .map(|tags| tags.split(TAG_SEPARATOR).map(str::to_owned).collect())
                        .unwrap_or_default(),
                })
            })
            .collect()
    }
}
