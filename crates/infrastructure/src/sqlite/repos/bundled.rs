//! Bundled-content provenance (schema v2) and the dashboard read models.

use langualinker_application::repositories::{
    BundledContentRepository, BundledNote, DashboardQueries, DeckCardCounts,
};
use langualinker_application::{AppError, AppResult};
use langualinker_domain::card::{CardState, QueueState};
use langualinker_domain::content::{Association, GrammarTip};
use langualinker_domain::deck::Deck;
use langualinker_domain::{
    AssociationId, DeckId, GrammarTipId, Guid, NoteId, SessionId, TimestampMs,
};
use rusqlite::{OptionalExtension, params};

use super::{
    ASSOCIATION_COLUMNS, DECK_COLUMNS, LOCAL_USN, SqliteRepos, association_row, deck_row, guid,
};
use crate::sqlite::map_sqlite;

/// Interval (days) from which a card counts as mature.
const MATURE_DAYS: i64 = 21;
const DAY_MS: i64 = 86_400_000;

fn bundled_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<BundledNote> {
    Ok(BundledNote {
        note_id: NoteId(row.get(0)?),
        deck_guid: row.get(1)?,
        content_version: row.get(2)?,
        removed_in_version: row.get(3)?,
        archived_by_import: row.get(4)?,
    })
}

impl BundledContentRepository for SqliteRepos<'_> {
    fn deck_by_guid(&mut self, guid: &Guid) -> AppResult<Option<Deck>> {
        let sql = format!(
            "SELECT {DECK_COLUMNS} FROM decks d JOIN deck_settings s ON s.deck_id = d.id WHERE d.guid = ?1"
        );
        self.tx
            .query_row(&sql, [guid.as_str()], deck_row)
            .optional()
            .map_err(map_sqlite)
    }

    fn bundled_note(&mut self, note: NoteId) -> AppResult<Option<BundledNote>> {
        self.tx
            .prepare_cached(
                "SELECT note_id, deck_guid, content_version, removed_in_version, archived_by_import
                 FROM bundled_notes WHERE note_id = ?1",
            )
            .and_then(|mut statement| statement.query_row([note.0], bundled_row).optional())
            .map_err(map_sqlite)
    }

    fn put_bundled_note(&mut self, note: &BundledNote) -> AppResult<()> {
        self.tx
            .prepare_cached(
                "INSERT INTO bundled_notes (note_id, deck_guid, content_version, removed_in_version,
                     archived_by_import)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (note_id) DO UPDATE SET deck_guid = excluded.deck_guid,
                     content_version = excluded.content_version,
                     removed_in_version = excluded.removed_in_version,
                     archived_by_import = excluded.archived_by_import",
            )
            .and_then(|mut statement| {
                statement.execute(params![
                    note.note_id.0,
                    note.deck_guid,
                    note.content_version,
                    note.removed_in_version,
                    note.archived_by_import,
                ])
            })
            .map(drop)
            .map_err(map_sqlite)
    }

    fn bundled_notes_of_deck(&mut self, deck_guid: &str) -> AppResult<Vec<(Guid, BundledNote)>> {
        let mut statement = self
            .tx
            .prepare_cached(
                "SELECT b.note_id, b.deck_guid, b.content_version, b.removed_in_version,
                     b.archived_by_import, n.guid
                 FROM bundled_notes b JOIN notes n ON n.id = b.note_id
                 WHERE b.deck_guid = ?1 AND n.deleted_at IS NULL ORDER BY b.note_id",
            )
            .map_err(map_sqlite)?;
        statement
            .query_map([deck_guid], |row| Ok((guid(row, 5)?, bundled_row(row)?)))
            .and_then(Iterator::collect)
            .map_err(map_sqlite)
    }

    fn content_overrides(&mut self, note: NoteId) -> AppResult<Vec<String>> {
        let mut statement = self
            .tx
            .prepare_cached("SELECT field FROM content_overrides WHERE note_id = ?1 ORDER BY field")
            .map_err(map_sqlite)?;
        statement
            .query_map([note.0], |row| row.get(0))
            .and_then(Iterator::collect)
            .map_err(map_sqlite)
    }

    fn put_content_override(
        &mut self,
        note: NoteId,
        field: &str,
        original_content_version: &str,
        at: TimestampMs,
    ) -> AppResult<()> {
        self.tx
            .execute(
                "INSERT INTO content_overrides (note_id, field, original_content_version, updated_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (note_id, field) DO UPDATE SET updated_at = excluded.updated_at",
                params![note.0, field, original_content_version, at.0],
            )
            .map(drop)
            .map_err(map_sqlite)
    }

    fn association_by_guid(&mut self, guid: &Guid) -> AppResult<Option<Association>> {
        let sql = format!("SELECT {ASSOCIATION_COLUMNS} FROM associations WHERE guid = ?1");
        self.tx
            .prepare_cached(&sql)
            .and_then(|mut statement| {
                statement
                    .query_row([guid.as_str()], association_row)
                    .optional()
            })
            .map_err(map_sqlite)
    }

    fn update_association_text(
        &mut self,
        id: AssociationId,
        text: &str,
        at: TimestampMs,
    ) -> AppResult<()> {
        let text = langualinker_domain::validation::required_text(
            "text",
            text,
            langualinker_domain::validation::MAX_SNIPPET_CHARS,
        )?;
        let changed = self
            .tx
            .execute(
                "UPDATE associations SET text = ?1, updated_at = ?2, usn = ?3
                 WHERE id = ?4 AND deleted_at IS NULL",
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

    fn grammar_tip_by_guid(&mut self, tip_guid: &Guid) -> AppResult<Option<GrammarTip>> {
        let row = self
            .tx
            .query_row(
                "SELECT note_id, deleted_at, id FROM grammar_tips WHERE guid = ?1",
                [tip_guid.as_str()],
                |row| {
                    Ok((
                        NoteId(row.get(0)?),
                        row.get::<_, Option<i64>>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(map_sqlite)?;
        let Some((note, deleted_at, id)) = row else {
            return Ok(None);
        };
        if deleted_at.is_none() {
            let tips = langualinker_application::repositories::ContentRepository::grammar_tips(
                self, note,
            )?;
            return Ok(tips.into_iter().find(|tip| tip.id.0 == id));
        }
        self.tx
            .query_row(
                "SELECT id, guid, note_id, text, provenance, sort_order, created_at, updated_at,
                     deleted_at, usn FROM grammar_tips WHERE id = ?1",
                [id],
                |row| {
                    Ok(GrammarTip {
                        id: GrammarTipId(row.get(0)?),
                        guid: guid(row, 1)?,
                        note_id: NoteId(row.get(2)?),
                        text: row.get(3)?,
                        provenance: super::code(
                            4,
                            langualinker_domain::content::TipProvenance::from_db_code(row.get(4)?),
                        )?,
                        sort_order: row.get(5)?,
                        created_at: super::ts(row, 6)?,
                        updated_at: super::ts(row, 7)?,
                        deleted_at: super::opt_ts(row, 8)?,
                        usn: row.get(9)?,
                    })
                },
            )
            .optional()
            .map_err(map_sqlite)
    }

    fn update_grammar_tip_text(
        &mut self,
        id: GrammarTipId,
        text: &str,
        at: TimestampMs,
    ) -> AppResult<()> {
        let text = langualinker_domain::validation::required_text(
            "text",
            text,
            langualinker_domain::validation::MAX_SNIPPET_CHARS,
        )?;
        let changed = self
            .tx
            .execute(
                "UPDATE grammar_tips SET text = ?1, updated_at = ?2, usn = ?3
                 WHERE id = ?4 AND deleted_at IS NULL",
                params![text, at.0, LOCAL_USN, id.0],
            )
            .map_err(map_sqlite)?;
        if changed == 0 {
            return Err(AppError::NotFound {
                entity: "grammarTip",
            });
        }
        Ok(())
    }

    fn set_note_cards_archived(
        &mut self,
        note: NoteId,
        archived_at: Option<TimestampMs>,
        at: TimestampMs,
    ) -> AppResult<u64> {
        let changed = match archived_at {
            Some(archived) => self.tx.execute(
                "UPDATE cards SET archived_at = ?1, updated_at = ?2, usn = ?3, version = version + 1
                 WHERE note_id = ?4 AND deleted_at IS NULL AND archived_at IS NULL",
                params![archived.0, at.0, LOCAL_USN, note.0],
            ),
            None => self.tx.execute(
                "UPDATE cards SET archived_at = NULL, updated_at = ?1, usn = ?2, version = version + 1
                 WHERE note_id = ?3 AND deleted_at IS NULL AND archived_at IS NOT NULL",
                params![at.0, LOCAL_USN, note.0],
            ),
        }
        .map_err(map_sqlite)?;
        Ok(changed as u64)
    }

    fn collection_fingerprint(&mut self) -> AppResult<String> {
        self.fingerprint()
    }

    fn note_has_progress(&mut self, note: NoteId) -> AppResult<bool> {
        self.tx
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM cards WHERE note_id = ?1 AND (reps > 0 OR state <> 0))
                     OR EXISTS (SELECT 1 FROM review_logs l JOIN cards c ON c.id = l.card_id
                                WHERE c.note_id = ?1)",
                [note.0],
                |row| row.get(0),
            )
            .map_err(map_sqlite)
    }
}

impl SqliteRepos<'_> {
    pub(crate) fn fingerprint(&self) -> AppResult<String> {
        self.tx
            .query_row(
                "SELECT (SELECT COUNT(*) || ':' || COALESCE(MAX(updated_at), 0) FROM decks) || '/' ||
                        (SELECT COUNT(*) || ':' || COALESCE(MAX(updated_at), 0) FROM notes) || '/' ||
                        (SELECT COUNT(*) || ':' || COALESCE(MAX(updated_at), 0) FROM cards)",
                [],
                |row| row.get(0),
            )
            .map_err(map_sqlite)
    }
}

impl DashboardQueries for SqliteRepos<'_> {
    fn deck_card_counts(
        &mut self,
        day_start: TimestampMs,
        day_end: TimestampMs,
    ) -> AppResult<Vec<(DeckId, DeckCardCounts)>> {
        let studyable = format!(
            "queue NOT IN ({}, {}, {})",
            QueueState::Suspended.db_code(),
            QueueState::SchedBuried.db_code(),
            QueueState::UserBuried.db_code()
        );
        let sql = format!(
            "SELECT deck_id,
                 SUM(state = {new} AND {studyable}),
                 SUM(state IN ({learning}, {relearning}) AND {studyable} AND due_at < ?2),
                 SUM(state = {review} AND {studyable} AND due_at < ?2),
                 SUM(state = {review} AND {studyable} AND due_at < ?1),
                 COUNT(*),
                 SUM(state = {review} AND scheduled_days >= {MATURE_DAYS})
             FROM cards WHERE deleted_at IS NULL AND archived_at IS NULL
             GROUP BY deck_id ORDER BY deck_id",
            new = CardState::New.db_code(),
            learning = CardState::Learning.db_code(),
            relearning = CardState::Relearning.db_code(),
            review = CardState::Review.db_code(),
        );
        let mut statement = self.tx.prepare_cached(&sql).map_err(map_sqlite)?;
        let count = |row: &rusqlite::Row<'_>, index: usize| -> rusqlite::Result<u32> {
            Ok(row
                .get::<_, Option<i64>>(index)?
                .unwrap_or(0)
                .clamp(0, u32::MAX.into()) as u32)
        };
        statement
            .query_map([day_start.0, day_end.0], |row| {
                Ok((
                    DeckId(row.get(0)?),
                    DeckCardCounts {
                        new: count(row, 1)?,
                        learning: count(row, 2)?,
                        review: count(row, 3)?,
                        overdue: count(row, 4)?,
                        total: count(row, 5)?,
                        mature: count(row, 6)?,
                    },
                ))
            })
            .and_then(Iterator::collect)
            .map_err(map_sqlite)
    }

    fn day_cutoff_minutes(&mut self) -> AppResult<u32> {
        self.tx
            .query_row(
                "SELECT day_cutoff_minutes FROM collection WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map(|minutes| minutes.unwrap_or(240))
            .map_err(map_sqlite)
    }

    fn reviews_since(&mut self, since: TimestampMs) -> AppResult<u32> {
        self.tx
            .query_row(
                "SELECT COUNT(*) FROM review_logs WHERE reviewed_at >= ?1 AND invalidated_at IS NULL",
                [since.0],
                |row| row.get::<_, i64>(0),
            )
            .map(|count| count.clamp(0, u32::MAX.into()) as u32)
            .map_err(map_sqlite)
    }

    fn review_days(&mut self, cutoff_minutes: u32, limit: u32) -> AppResult<Vec<i64>> {
        let mut statement = self
            .tx
            .prepare_cached(
                "SELECT DISTINCT (reviewed_at - ?1) / ?2 AS day FROM review_logs
                 WHERE invalidated_at IS NULL ORDER BY day DESC LIMIT ?3",
            )
            .map_err(map_sqlite)?;
        statement
            .query_map(
                params![i64::from(cutoff_minutes) * 60_000, DAY_MS, limit],
                |row| row.get(0),
            )
            .and_then(Iterator::collect)
            .map_err(map_sqlite)
    }

    fn next_due_after(&mut self, after: TimestampMs) -> AppResult<Option<TimestampMs>> {
        self.tx
            .query_row(
                &format!(
                    "SELECT MIN(due_at) FROM cards WHERE due_at >= ?1 AND deleted_at IS NULL
                         AND archived_at IS NULL AND queue NOT IN ({}, {}, {})",
                    QueueState::Suspended.db_code(),
                    QueueState::SchedBuried.db_code(),
                    QueueState::UserBuried.db_code()
                ),
                [after.0],
                |row| row.get::<_, Option<i64>>(0),
            )
            .map(|due| due.map(TimestampMs))
            .map_err(map_sqlite)
    }

    fn has_review_between(&mut self, from: TimestampMs, to: TimestampMs) -> AppResult<bool> {
        self.tx
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM review_logs
                     WHERE reviewed_at >= ?1 AND reviewed_at < ?2 AND invalidated_at IS NULL)",
                params![from.0, to.0],
                |row| row.get(0),
            )
            .map_err(map_sqlite)
    }

    fn first_reviewed_session(&mut self) -> AppResult<Option<SessionId>> {
        self.tx
            .query_row(
                "SELECT session_id FROM review_logs
                 WHERE session_id IS NOT NULL AND invalidated_at IS NULL
                 ORDER BY reviewed_at, id LIMIT 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map(|id| id.map(SessionId))
            .map_err(map_sqlite)
    }
}
