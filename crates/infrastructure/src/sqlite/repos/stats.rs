//! Statistics read models and collection-wide writes of Phase 12 (calendar, consent withdrawal,
//! progress reset). Deck scopes travel as one JSON array parameter (`json_each`), so the SQL text
//! never depends on user input.

use langualinker_application::repositories::{
    CardFact, CollectionCounts, MaintenanceRepository, ReviewFact, StatsQueries,
};
use langualinker_application::{AppError, AppResult};
use langualinker_domain::card::{CardState, QueueState};
use langualinker_domain::review::{Rating, ReviewKind};
use langualinker_domain::{CardId, DeckId, TimestampMs};
use rusqlite::{Connection, OptionalExtension, params};

use super::{LOCAL_USN, SqliteRepos};
use crate::sqlite::map_sqlite;

/// `NULL` for every deck, otherwise a JSON array of deck ids.
fn scope(decks: Option<&[DeckId]>) -> Option<String> {
    decks.map(|ids| {
        let ids: Vec<i64> = ids.iter().map(|id| id.0).collect();
        serde_json::to_string(&ids).unwrap_or_else(|_| "[]".to_owned())
    })
}

const IN_SCOPE: &str = "(?1 IS NULL OR c.deck_id IN (SELECT value FROM json_each(?1)))";

fn decode<T>(
    value: i64,
    decode: impl Fn(i64) -> Result<T, langualinker_domain::UnknownCode>,
) -> rusqlite::Result<T> {
    decode(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Integer,
            Box::new(error),
        )
    })
}

/// Size of a collection; also read from staged backup files, hence a plain connection.
pub(crate) fn collection_counts(conn: &Connection) -> AppResult<CollectionCounts> {
    conn.query_row(
        "SELECT (SELECT COUNT(*) FROM decks WHERE deleted_at IS NULL),
                (SELECT COUNT(DISTINCT n.id) FROM notes n JOIN cards c ON c.note_id = n.id
                  WHERE n.deleted_at IS NULL AND c.deleted_at IS NULL AND c.archived_at IS NULL),
                (SELECT COUNT(*) FROM cards WHERE deleted_at IS NULL AND archived_at IS NULL),
                (SELECT COUNT(*) FROM review_logs WHERE invalidated_at IS NULL)",
        [],
        |row| {
            let count = |index: usize| -> rusqlite::Result<u32> {
                Ok(u32::try_from(row.get::<_, i64>(index)?).unwrap_or(u32::MAX))
            };
            Ok(CollectionCounts {
                decks: count(0)?,
                notes: count(1)?,
                cards: count(2)?,
                reviews: count(3)?,
            })
        },
    )
    .map_err(map_sqlite)
}

impl StatsQueries for SqliteRepos<'_> {
    fn review_facts(
        &mut self,
        decks: Option<&[DeckId]>,
        since: TimestampMs,
    ) -> AppResult<Vec<ReviewFact>> {
        let sql = format!(
            "SELECT l.card_id, l.reviewed_at, l.duration_ms, l.rating, l.kind,
                    json_extract(l.card_before, '$.state'),
                    COALESCE(json_extract(l.card_before, '$.scheduledDays'), 0),
                    json_extract(l.card_before, '$.dueAt')
               FROM review_logs l JOIN cards c ON c.id = l.card_id
              WHERE l.invalidated_at IS NULL AND l.reviewed_at >= ?2 AND {IN_SCOPE}
              ORDER BY l.reviewed_at, l.id"
        );
        let mut statement = self.tx.prepare_cached(&sql).map_err(map_sqlite)?;
        let rows = statement
            .query_map(params![scope(decks), since.0], |row| {
                Ok(ReviewFact {
                    card_id: CardId(row.get(0)?),
                    reviewed_at: TimestampMs(row.get(1)?),
                    duration_ms: row.get(2)?,
                    rating: decode(row.get(3)?, Rating::from_db_code)?,
                    kind: decode(row.get(4)?, ReviewKind::from_db_code)?,
                    state_before: decode(row.get(5)?, CardState::from_db_code)?,
                    interval_before_days: row.get(6)?,
                    due_before: row.get::<_, Option<i64>>(7)?.map(TimestampMs),
                })
            })
            .map_err(map_sqlite)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(map_sqlite)
    }

    fn card_facts(&mut self, decks: Option<&[DeckId]>) -> AppResult<Vec<CardFact>> {
        let sql = format!(
            "SELECT c.id, c.deck_id, c.state, c.queue, c.due_at, c.stability, c.difficulty,
                    c.scheduled_days
               FROM cards c
              WHERE c.deleted_at IS NULL AND c.archived_at IS NULL AND {IN_SCOPE}
              ORDER BY c.id"
        );
        let mut statement = self.tx.prepare_cached(&sql).map_err(map_sqlite)?;
        let rows = statement
            .query_map(params![scope(decks)], |row| {
                Ok(CardFact {
                    card_id: CardId(row.get(0)?),
                    deck_id: DeckId(row.get(1)?),
                    state: decode(row.get(2)?, CardState::from_db_code)?,
                    queue: decode(row.get(3)?, QueueState::from_db_code)?,
                    due_at: row.get::<_, Option<i64>>(4)?.map(TimestampMs),
                    stability: row.get(5)?,
                    difficulty: row.get(6)?,
                    interval_days: row.get(7)?,
                })
            })
            .map_err(map_sqlite)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(map_sqlite)
    }

    fn has_review_in(
        &mut self,
        decks: Option<&[DeckId]>,
        from: TimestampMs,
        to: TimestampMs,
    ) -> AppResult<bool> {
        let sql = format!(
            "SELECT EXISTS (SELECT 1 FROM review_logs l JOIN cards c ON c.id = l.card_id
                             WHERE l.invalidated_at IS NULL AND l.reviewed_at >= ?2
                               AND l.reviewed_at < ?3 AND {IN_SCOPE})"
        );
        self.tx
            .query_row(&sql, params![scope(decks), from.0, to.0], |row| row.get(0))
            .map_err(map_sqlite)
    }

    fn first_review_at(&mut self, decks: Option<&[DeckId]>) -> AppResult<Option<TimestampMs>> {
        let sql = format!(
            "SELECT MIN(l.reviewed_at) FROM review_logs l JOIN cards c ON c.id = l.card_id
              WHERE l.invalidated_at IS NULL AND {IN_SCOPE}"
        );
        self.tx
            .query_row(&sql, params![scope(decks)], |row| {
                row.get::<_, Option<i64>>(0)
            })
            .map(|at| at.map(TimestampMs))
            .map_err(map_sqlite)
    }
}

impl MaintenanceRepository for SqliteRepos<'_> {
    fn set_collection_calendar(&mut self, zone: &str, cutoff_minutes: u32) -> AppResult<()> {
        let changed = self
            .tx
            .execute(
                "UPDATE collection SET time_zone = ?1, day_cutoff_minutes = ?2, usn = ?3 WHERE id = 1",
                params![zone, cutoff_minutes, LOCAL_USN],
            )
            .map_err(map_sqlite)?;
        if changed == 1 {
            Ok(())
        } else {
            Err(AppError::Internal("collection row missing".into()))
        }
    }

    fn block_pending_llm_requests(&mut self, at: TimestampMs) -> AppResult<u64> {
        self.tx
            .execute(
                "UPDATE llm_requests SET status = 'blocked', error_code = 'CONSENT_WITHDRAWN',
                     next_attempt_at = NULL, updated_at = ?1
                 WHERE status IN ('queued', 'sending')",
                [at.0],
            )
            .map(|count| count as u64)
            .map_err(map_sqlite)
    }

    fn collection_counts(&mut self) -> AppResult<CollectionCounts> {
        collection_counts(self.tx)
    }

    fn reset_progress(&mut self, at: TimestampMs) -> AppResult<u64> {
        self.tx
            .execute_batch(
                "DELETE FROM review_undo_entries;
                 DELETE FROM review_logs;
                 DELETE FROM session_items;
                 DELETE FROM study_sessions;",
            )
            .map_err(map_sqlite)?;
        // New cards keep their order and come first; studied cards follow in their old order.
        self.tx
            .execute(
                "WITH ordered AS (
                     SELECT id, ROW_NUMBER() OVER (
                         ORDER BY CASE WHEN state = 0 THEN 0 ELSE 1 END, position, id) AS pos
                       FROM cards WHERE deleted_at IS NULL)
                 UPDATE cards SET position = ordered.pos
                   FROM ordered WHERE ordered.id = cards.id",
                [],
            )
            .map_err(map_sqlite)?;
        self.tx
            .execute(
                "UPDATE cards SET state = 0,
                     queue = CASE WHEN queue = ?1 THEN ?1 ELSE ?2 END,
                     due_at = NULL, stability = NULL, difficulty = NULL, legacy_ease = NULL,
                     last_review_at = NULL, scheduled_days = 0, elapsed_days = 0, reps = 0,
                     lapses = 0, learning_step = NULL, buried_until = NULL,
                     updated_at = ?3, usn = ?4, version = version + 1
                 WHERE deleted_at IS NULL",
                params![
                    QueueState::Suspended.db_code(),
                    QueueState::New.db_code(),
                    at.0,
                    LOCAL_USN
                ],
            )
            .map(|count| count as u64)
            .map_err(map_sqlite)
    }

    fn newest_review_at(&mut self) -> AppResult<Option<TimestampMs>> {
        self.tx
            .query_row(
                "SELECT MAX(reviewed_at) FROM review_logs WHERE invalidated_at IS NULL",
                [],
                |row| row.get::<_, Option<i64>>(0),
            )
            .optional()
            .map(|at| at.flatten().map(TimestampMs))
            .map_err(map_sqlite)
    }
}
