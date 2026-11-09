//! Management read models and writes of Phase 11: deck and preset counts, the editor's lookups,
//! tip order, and the card browser. The browser's SQL is assembled from fixed fragments; every
//! user value (FTS text, tags, ids, cursor) is a bound parameter and the sort expression comes
//! from an allowlist (`CardSort`).

use langualinker_application::repositories::{
    CardCursor, CardFilter, CardRow, CardSort, ContentCounts, ManagementRepository, NoteMatch,
    SortValue, WordNote,
};
use langualinker_application::{AppError, AppResult};
use langualinker_domain::note::CardDirection;
use langualinker_domain::{
    AssociationId, CardId, DeckConfigId, DeckId, GrammarTipId, NoteId, TimestampMs, WordLinkId,
};
use rusqlite::types::Value;
use rusqlite::{Row, params, params_from_iter};

use super::{CARD_COLUMNS, LOCAL_USN, NOTE_COLUMNS, SqliteRepos, card_row, code, placeholders};
use crate::sqlite::{codec, map_sqlite};

fn prefixed(columns: &str, alias: &str) -> String {
    columns
        .split(',')
        .map(|column| format!("{alias}.{}", column.trim()))
        .collect::<Vec<_>>()
        .join(", ")
}

fn sort_expression(sort: CardSort) -> &'static str {
    match sort {
        // A new card sorts after every scheduled card, in queue order.
        CardSort::Due => {
            "CASE WHEN c.due_at IS NULL THEN 9000000000000000 + c.position ELSE c.due_at END"
        }
        CardSort::Created => "n.id",
        CardSort::Front => "n.sort_field",
        CardSort::Deck => "d.name COLLATE NOCASE",
        CardSort::Reps => "c.reps",
        CardSort::Lapses => "c.lapses",
    }
}

fn is_text_sort(sort: CardSort) -> bool {
    matches!(sort, CardSort::Front | CardSort::Deck)
}

/// `FROM … WHERE …` of a browser search and its parameters.
fn filter_sql(filter: &CardFilter) -> (String, Vec<Value>) {
    let mut sql = String::from(
        " FROM cards c
          JOIN notes n ON n.id = c.note_id
          JOIN decks d ON d.id = c.deck_id
          LEFT JOIN card_templates t ON t.note_type_id = n.note_type_id AND t.ordinal = c.template_ordinal
          WHERE c.deleted_at IS NULL AND c.archived_at IS NULL AND n.deleted_at IS NULL
            AND d.deleted_at IS NULL",
    );
    let mut args: Vec<Value> = Vec::new();
    if let Some(fts) = &filter.fts {
        sql.push_str(" AND n.id IN (SELECT rowid FROM notes_fts WHERE notes_fts MATCH ?)");
        args.push(Value::Text(fts.clone()));
    }
    if let Some(decks) = &filter.decks {
        if decks.is_empty() {
            sql.push_str(" AND 0");
        } else {
            sql.push_str(&format!(
                " AND c.deck_id IN ({})",
                placeholders(decks.len())
            ));
            args.extend(decks.iter().map(|deck| Value::Integer(deck.0)));
        }
    }
    for tag in &filter.tags {
        sql.push_str(
            " AND EXISTS (SELECT 1 FROM note_tags nt JOIN tags tg ON tg.id = nt.tag_id
                          WHERE nt.note_id = n.id AND tg.name = ?)",
        );
        args.push(Value::Text(tag.clone()));
    }
    if filter.new_only {
        sql.push_str(" AND c.state = 0");
    }
    if let Some(until) = filter.due_before {
        sql.push_str(" AND c.queue IN (1, 2, 3) AND c.due_at < ?");
        args.push(Value::Integer(until.0));
    }
    if filter.suspended_only {
        sql.push_str(" AND c.queue = -1");
    }
    if let Some(since) = filter.added_since {
        sql.push_str(" AND n.created_at >= ?");
        args.push(Value::Integer(since.0));
    }
    if !filter.states.is_empty() {
        sql.push_str(&format!(
            " AND c.state IN ({})",
            placeholders(filter.states.len())
        ));
        args.extend(
            filter
                .states
                .iter()
                .map(|state| Value::Integer(state.db_code())),
        );
    }
    if !filter.flags.is_empty() {
        sql.push_str(&format!(
            " AND c.flags IN ({})",
            placeholders(filter.flags.len())
        ));
        args.extend(
            filter
                .flags
                .iter()
                .map(|flag| Value::Integer(flag.db_code())),
        );
    }
    (sql, args)
}

fn order_sql(sort: CardSort, descending: bool) -> String {
    let direction = if descending { "DESC" } else { "ASC" };
    format!(
        " ORDER BY {} {direction}, c.id {direction}",
        sort_expression(sort)
    )
}

fn sort_value(row: &Row<'_>, index: usize, sort: CardSort) -> rusqlite::Result<SortValue> {
    if is_text_sort(sort) {
        row.get::<_, String>(index).map(SortValue::Text)
    } else {
        row.get::<_, i64>(index).map(SortValue::Number)
    }
}

impl ManagementRepository for SqliteRepos<'_> {
    fn deck_content_counts(&mut self, deck: DeckId) -> AppResult<ContentCounts> {
        self.tx
            .query_row(
                "WITH own AS (SELECT DISTINCT c.note_id AS id FROM cards c
                              JOIN notes n ON n.id = c.note_id
                              WHERE c.deck_id = ?1 AND c.deleted_at IS NULL AND n.deleted_at IS NULL)
                 SELECT
                     (SELECT COUNT(*) FROM associations a WHERE a.note_id IN (SELECT id FROM own)
                          AND a.deleted_at IS NULL),
                     (SELECT COUNT(*) FROM grammar_tips g WHERE g.note_id IN (SELECT id FROM own)
                          AND g.deleted_at IS NULL),
                     (SELECT COUNT(*) FROM sentence_word_links l
                          WHERE l.sentence_note_id IN (SELECT id FROM own))",
                [deck.0],
                |row| {
                    Ok(ContentCounts {
                        associations: row.get(0)?,
                        grammar_tips: row.get(1)?,
                        word_links: row.get(2)?,
                    })
                },
            )
            .map_err(map_sqlite)
    }

    fn decks_per_config(&mut self) -> AppResult<Vec<(DeckConfigId, u32)>> {
        let mut statement = self
            .tx
            .prepare_cached(
                "SELECT config_id, COUNT(*) FROM decks WHERE deleted_at IS NULL GROUP BY config_id",
            )
            .map_err(map_sqlite)?;
        statement
            .query_map([], |row| Ok((DeckConfigId(row.get(0)?), row.get(1)?)))
            .and_then(Iterator::collect)
            .map_err(map_sqlite)
    }

    fn soft_delete_cards_of_decks(&mut self, decks: &[DeckId], at: TimestampMs) -> AppResult<u32> {
        if decks.is_empty() {
            return Ok(0);
        }
        let sql = format!(
            "UPDATE cards SET deleted_at = ?, updated_at = ?, usn = ?, version = version + 1
             WHERE deck_id IN ({}) AND deleted_at IS NULL",
            placeholders(decks.len())
        );
        let mut args = vec![at.0, at.0, LOCAL_USN];
        args.extend(decks.iter().map(|deck| deck.0));
        let changed = self
            .tx
            .execute(&sql, params_from_iter(args))
            .map_err(map_sqlite)?;
        Ok(u32::try_from(changed).unwrap_or(u32::MAX))
    }

    fn soft_delete_orphan_notes(&mut self, at: TimestampMs) -> AppResult<Vec<NoteId>> {
        let orphans: Vec<NoteId> = {
            let mut statement = self
                .tx
                .prepare_cached(
                    "SELECT n.id FROM notes n WHERE n.deleted_at IS NULL
                       AND NOT EXISTS (SELECT 1 FROM cards c WHERE c.note_id = n.id
                                       AND c.deleted_at IS NULL)",
                )
                .map_err(map_sqlite)?;
            statement
                .query_map([], |row| row.get(0).map(NoteId))
                .and_then(Iterator::collect)
                .map_err(map_sqlite)?
        };
        for note in &orphans {
            self.tx
                .execute(
                    "UPDATE notes SET deleted_at = ?1, updated_at = ?1, usn = ?2, version = version + 1
                     WHERE id = ?3 AND deleted_at IS NULL",
                    params![at.0, LOCAL_USN, note.0],
                )
                .map_err(map_sqlite)?;
        }
        Ok(orphans)
    }

    fn all_tags(&mut self) -> AppResult<Vec<String>> {
        let mut statement = self
            .tx
            .prepare_cached(
                "SELECT DISTINCT t.name FROM tags t
                 JOIN note_tags nt ON nt.tag_id = t.id
                 JOIN notes n ON n.id = nt.note_id
                 WHERE n.deleted_at IS NULL ORDER BY t.name COLLATE NOCASE",
            )
            .map_err(map_sqlite)?;
        statement
            .query_map([], |row| row.get(0))
            .and_then(Iterator::collect)
            .map_err(map_sqlite)
    }

    fn notes_by_sort_field(
        &mut self,
        decks: &[DeckId],
        sort_field: &str,
        exclude: Option<NoteId>,
    ) -> AppResult<Vec<NoteMatch>> {
        if decks.is_empty() {
            return Ok(Vec::new());
        }
        let sql = format!(
            "SELECT n.id, json_extract(n.fields, '$.front'), MIN(c.deck_id) FROM notes n
             JOIN cards c ON c.note_id = n.id AND c.deleted_at IS NULL
             WHERE n.deleted_at IS NULL AND n.sort_field = ? AND n.id <> ?
               AND c.deck_id IN ({})
             GROUP BY n.id ORDER BY n.id LIMIT 20",
            placeholders(decks.len())
        );
        let mut args: Vec<Value> = vec![
            Value::Text(sort_field.to_owned()),
            Value::Integer(exclude.map_or(0, |note| note.0)),
        ];
        args.extend(decks.iter().map(|deck| Value::Integer(deck.0)));
        let mut statement = self.tx.prepare(&sql).map_err(map_sqlite)?;
        statement
            .query_map(params_from_iter(args), |row| {
                Ok(NoteMatch {
                    note_id: NoteId(row.get(0)?),
                    front: row.get(1)?,
                    deck_id: DeckId(row.get(2)?),
                })
            })
            .and_then(Iterator::collect)
            .map_err(map_sqlite)
    }

    fn word_notes(&mut self, decks: &[DeckId]) -> AppResult<Vec<WordNote>> {
        if decks.is_empty() {
            return Ok(Vec::new());
        }
        // Note-type kinds 1 and 2 are Basic and Basic-and-reversed (crates/domain/src/note.rs).
        let sql = format!(
            "SELECT n.id, json_extract(n.fields, '$.front'), json_extract(n.fields, '$.back'),
                    c.deck_id
             FROM notes n
             JOIN note_types nt ON nt.id = n.note_type_id
             JOIN cards c ON c.id = (SELECT c2.id FROM cards c2 WHERE c2.note_id = n.id
                                     AND c2.deleted_at IS NULL ORDER BY c2.template_ordinal LIMIT 1)
             WHERE n.deleted_at IS NULL AND nt.kind IN (1, 2) AND c.deck_id IN ({})",
            placeholders(decks.len())
        );
        let mut statement = self.tx.prepare(&sql).map_err(map_sqlite)?;
        statement
            .query_map(params_from_iter(decks.iter().map(|deck| deck.0)), |row| {
                Ok(WordNote {
                    note_id: NoteId(row.get(0)?),
                    front: row.get(1)?,
                    back: row.get(2)?,
                    deck_id: DeckId(row.get(3)?),
                })
            })
            .and_then(Iterator::collect)
            .map_err(map_sqlite)
    }

    fn set_grammar_tip_order(
        &mut self,
        tip: GrammarTipId,
        order: i64,
        at: TimestampMs,
    ) -> AppResult<()> {
        let changed = self
            .tx
            .execute(
                "UPDATE grammar_tips SET sort_order = ?1, updated_at = ?2, usn = ?3
                 WHERE id = ?4 AND deleted_at IS NULL",
                params![order, at.0, LOCAL_USN, tip.0],
            )
            .map_err(map_sqlite)?;
        if changed == 0 {
            return Err(AppError::NotFound {
                entity: "grammarTip",
            });
        }
        Ok(())
    }

    fn soft_delete_grammar_tip(&mut self, tip: GrammarTipId, at: TimestampMs) -> AppResult<()> {
        let changed = self
            .tx
            .execute(
                "UPDATE grammar_tips SET deleted_at = ?1, updated_at = ?1, usn = ?2
                 WHERE id = ?3 AND deleted_at IS NULL",
                params![at.0, LOCAL_USN, tip.0],
            )
            .map_err(map_sqlite)?;
        if changed == 0 {
            return Err(AppError::NotFound {
                entity: "grammarTip",
            });
        }
        Ok(())
    }

    fn update_manual_association_text(
        &mut self,
        id: AssociationId,
        text: &str,
        at: TimestampMs,
    ) -> AppResult<()> {
        // Provenance 3 is Manual (crates/domain/src/content.rs).
        let changed = self
            .tx
            .execute(
                "UPDATE associations SET text = ?1, updated_at = ?2, usn = ?3
                 WHERE id = ?4 AND provenance = 3 AND deleted_at IS NULL",
                params![text, at.0, LOCAL_USN, id.0],
            )
            .map_err(map_sqlite)?;
        if changed == 0 {
            return Err(AppError::NotFound {
                entity: "association",
            });
        }
        Ok(())
    }

    fn delete_word_link(&mut self, id: WordLinkId) -> AppResult<()> {
        let changed = self
            .tx
            .execute("DELETE FROM sentence_word_links WHERE id = ?1", [id.0])
            .map_err(map_sqlite)?;
        if changed == 0 {
            return Err(AppError::NotFound { entity: "wordLink" });
        }
        Ok(())
    }

    fn search_cards(
        &mut self,
        filter: &CardFilter,
        sort: CardSort,
        descending: bool,
        after: Option<&CardCursor>,
        limit: u32,
    ) -> AppResult<Vec<CardRow>> {
        let (mut from, mut args) = filter_sql(filter);
        let expression = sort_expression(sort);
        if let Some(cursor) = after {
            let (greater, value) = (if descending { "<" } else { ">" }, &cursor.value);
            let value = match (value, is_text_sort(sort)) {
                (SortValue::Text(text), true) => Value::Text(text.clone()),
                (SortValue::Number(number), false) => Value::Integer(*number),
                _ => {
                    return Err(
                        langualinker_domain::ValidationError::new("cursor", "format").into(),
                    );
                }
            };
            from.push_str(&format!(
                " AND ({expression} {greater} ? OR ({expression} = ? AND c.id {greater} ?))"
            ));
            args.extend([value.clone(), value, Value::Integer(cursor.card_id)]);
        }
        let card_columns = prefixed(CARD_COLUMNS, "c");
        let note_columns = prefixed(NOTE_COLUMNS, "n");
        let sql = format!(
            "SELECT {card_columns}, {note_columns}, d.name, COALESCE(t.direction, 0), {expression}
             {from}{} LIMIT ?",
            order_sql(sort, descending)
        );
        args.push(Value::Integer(i64::from(limit)));
        let card_width = CARD_COLUMNS.split(',').count();
        let note_width = NOTE_COLUMNS.split(',').count();
        let rows: Vec<(CardRow, String)> = {
            let mut statement = self.tx.prepare(&sql).map_err(map_sqlite)?;
            statement
                .query_map(params_from_iter(args), |row| {
                    let card = card_row(row)?;
                    let (note, fields) = note_row_at(row, card_width)?;
                    let base = card_width + note_width;
                    let direction =
                        code(base + 1, CardDirection::from_db_code(row.get(base + 1)?))?;
                    Ok((
                        CardRow {
                            card,
                            note,
                            deck_name: row.get(base)?,
                            direction,
                            sort_value: sort_value(row, base + 2, sort)?,
                        },
                        fields,
                    ))
                })
                .and_then(Iterator::collect)
                .map_err(map_sqlite)?
        };
        let mut result = Vec::with_capacity(rows.len());
        for (mut row, fields) in rows {
            row.note.fields = codec::note_fields_from_json(&fields)?;
            row.note.tags = self.note_tags(row.note.id)?;
            result.push(row);
        }
        Ok(result)
    }

    fn count_cards(&mut self, filter: &CardFilter) -> AppResult<u32> {
        let (from, args) = filter_sql(filter);
        let sql = format!("SELECT COUNT(*){from}");
        self.tx
            .query_row(&sql, params_from_iter(args), |row| row.get(0))
            .map_err(map_sqlite)
    }

    fn matching_card_ids(
        &mut self,
        filter: &CardFilter,
        sort: CardSort,
        descending: bool,
    ) -> AppResult<Vec<CardId>> {
        let (from, args) = filter_sql(filter);
        let sql = format!("SELECT c.id{from}{}", order_sql(sort, descending));
        let mut statement = self.tx.prepare(&sql).map_err(map_sqlite)?;
        statement
            .query_map(params_from_iter(args), |row| row.get(0).map(CardId))
            .and_then(Iterator::collect)
            .map_err(map_sqlite)
    }
}

/// Reads the note columns that follow the card columns of a browser row (same order as
/// `NOTE_COLUMNS`); the fields JSON is decoded by the caller.
fn note_row_at(
    row: &Row<'_>,
    o: usize,
) -> rusqlite::Result<(langualinker_domain::note::Note, String)> {
    use langualinker_domain::note::{Note, NoteFields};
    use langualinker_domain::{Guid, NoteTypeId};
    let guid_text: String = row.get(o + 1)?;
    let guid = Guid::parse(&guid_text).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            o + 1,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })?;
    Ok((
        Note {
            id: NoteId(row.get(o)?),
            guid,
            note_type_id: NoteTypeId(row.get(o + 2)?),
            fields: NoteFields {
                front: String::new(),
                back: String::new(),
            },
            sort_field: row.get(o + 4)?,
            tags: Vec::new(),
            created_at: TimestampMs(row.get(o + 5)?),
            updated_at: TimestampMs(row.get(o + 6)?),
            deleted_at: row.get::<_, Option<i64>>(o + 7)?.map(TimestampMs),
            usn: row.get(o + 8)?,
            version: row.get(o + 9)?,
        },
        row.get(o + 3)?,
    ))
}
