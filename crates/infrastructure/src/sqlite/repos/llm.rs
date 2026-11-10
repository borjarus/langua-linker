//! LLM outbox, association cache and call log (schema v4, plan Phase 13).

use langualinker_application::AppResult;
use langualinker_application::repositories::{
    CachedAssociation, LlmCall, LlmRepository, LlmRequestRow, LlmRequestStatus, NewLlmRequest,
};
use langualinker_domain::{AssociationId, NoteId, TimestampMs};
use rusqlite::types::Type;
use rusqlite::{OptionalExtension, Row, params};

use super::SqliteRepos;
use crate::sqlite::map_sqlite;

const REQUEST_COLUMNS: &str = "id, note_id, job_id, operation_id, variant, status, attempts,
    offline_retries, next_attempt_at, prompt_kind, prompt_version, provider, model, cache_key,
    source_hash, association_id, error_code, created_at, updated_at";

fn request_row(row: &Row<'_>) -> rusqlite::Result<LlmRequestRow> {
    let status: String = row.get(5)?;
    Ok(LlmRequestRow {
        id: row.get(0)?,
        note_id: NoteId(row.get(1)?),
        job_id: row.get(2)?,
        operation_id: row.get(3)?,
        variant: row.get(4)?,
        status: LlmRequestStatus::parse(&status).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(5, Type::Text, Box::new(error))
        })?,
        attempts: row.get(6)?,
        offline_retries: row.get(7)?,
        next_attempt_at: row.get::<_, Option<i64>>(8)?.map(TimestampMs),
        prompt_kind: row.get(9)?,
        prompt_version: row.get(10)?,
        provider: row.get(11)?,
        model: row.get(12)?,
        cache_key: row.get(13)?,
        source_hash: row.get(14)?,
        association_id: row.get::<_, Option<i64>>(15)?.map(AssociationId),
        error_code: row.get(16)?,
        created_at: TimestampMs(row.get(17)?),
        updated_at: TimestampMs(row.get(18)?),
    })
}

impl SqliteRepos<'_> {
    fn requests_where(
        &mut self,
        condition: &str,
        param: &dyn rusqlite::ToSql,
    ) -> AppResult<Vec<LlmRequestRow>> {
        let sql =
            format!("SELECT {REQUEST_COLUMNS} FROM llm_requests WHERE {condition} ORDER BY id");
        let mut statement = self.tx.prepare_cached(&sql).map_err(map_sqlite)?;
        statement
            .query_map([param], request_row)
            .and_then(Iterator::collect)
            .map_err(map_sqlite)
    }
}

impl LlmRepository for SqliteRepos<'_> {
    fn insert_llm_request(
        &mut self,
        request: &NewLlmRequest,
        at: TimestampMs,
    ) -> AppResult<Option<i64>> {
        let inserted = self
            .tx
            .execute(
                "INSERT OR IGNORE INTO llm_requests (note_id, operation_id, variant, status,
                     prompt_kind, prompt_version, provider, model, cache_key, source_hash,
                     created_at, updated_at)
                 VALUES (?1, ?2, ?3, 'queued', ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)",
                params![
                    request.note_id.0,
                    request.operation_id,
                    request.variant,
                    request.prompt_kind,
                    request.prompt_version,
                    request.provider,
                    request.model,
                    request.cache_key,
                    request.source_hash,
                    at.0,
                ],
            )
            .map_err(map_sqlite)?;
        Ok((inserted == 1).then(|| self.tx.last_insert_rowid()))
    }

    fn llm_request(&mut self, id: i64) -> AppResult<Option<LlmRequestRow>> {
        let sql = format!("SELECT {REQUEST_COLUMNS} FROM llm_requests WHERE id = ?1");
        self.tx
            .query_row(&sql, [id], request_row)
            .optional()
            .map_err(map_sqlite)
    }

    fn llm_requests_of_operation(&mut self, operation_id: &str) -> AppResult<Vec<LlmRequestRow>> {
        self.requests_where("operation_id = ?1", &operation_id)
    }

    fn llm_requests_with_status(
        &mut self,
        status: LlmRequestStatus,
    ) -> AppResult<Vec<LlmRequestRow>> {
        self.requests_where("status = ?1", &status.as_str())
    }

    fn visible_llm_requests(&mut self, note: NoteId) -> AppResult<Vec<LlmRequestRow>> {
        self.requests_where(
            "note_id = ?1 AND status IN ('queued', 'sending', 'failed', 'blocked')",
            &note.0,
        )
    }

    fn due_llm_requests(&mut self, now: TimestampMs, limit: u32) -> AppResult<Vec<i64>> {
        let mut statement = self
            .tx
            .prepare_cached(
                "SELECT id FROM llm_requests
                 WHERE status = 'queued' AND (next_attempt_at IS NULL OR next_attempt_at <= ?1)
                 ORDER BY id LIMIT ?2",
            )
            .map_err(map_sqlite)?;
        statement
            .query_map(params![now.0, limit], |row| row.get(0))
            .and_then(Iterator::collect)
            .map_err(map_sqlite)
    }

    fn update_llm_request(&mut self, row: &LlmRequestRow, at: TimestampMs) -> AppResult<()> {
        self.tx
            .execute(
                "UPDATE llm_requests SET job_id = ?2, status = ?3, attempts = ?4,
                     offline_retries = ?5, next_attempt_at = ?6, prompt_kind = ?7,
                     prompt_version = ?8, provider = ?9, model = ?10, cache_key = ?11,
                     association_id = ?12, error_code = ?13, updated_at = ?14
                 WHERE id = ?1",
                params![
                    row.id,
                    row.job_id,
                    row.status.as_str(),
                    row.attempts,
                    row.offline_retries,
                    row.next_attempt_at.map(|at| at.0),
                    row.prompt_kind,
                    row.prompt_version,
                    row.provider,
                    row.model,
                    row.cache_key,
                    row.association_id.map(|id| id.0),
                    row.error_code,
                    at.0,
                ],
            )
            .map(drop)
            .map_err(map_sqlite)
    }

    fn used_cache_keys(&mut self, note: NoteId) -> AppResult<Vec<String>> {
        let mut statement = self
            .tx
            .prepare_cached(
                "SELECT cache_key FROM associations WHERE note_id = ?1 AND cache_key IS NOT NULL
                 UNION
                 SELECT cache_key FROM llm_requests WHERE note_id = ?1",
            )
            .map_err(map_sqlite)?;
        statement
            .query_map([note.0], |row| row.get(0))
            .and_then(Iterator::collect)
            .map_err(map_sqlite)
    }

    fn cached_association(&mut self, cache_key: &str) -> AppResult<Option<CachedAssociation>> {
        self.tx
            .query_row(
                "SELECT text, model, prompt_version FROM association_cache
                 WHERE cache_key = ?1 AND moderation = 1",
                [cache_key],
                |row| {
                    Ok(CachedAssociation {
                        text: row.get(0)?,
                        model: row.get(1)?,
                        prompt_version: row.get(2)?,
                    })
                },
            )
            .optional()
            .map_err(map_sqlite)
    }

    fn put_cached_association(
        &mut self,
        cache_key: &str,
        entry: &CachedAssociation,
        at: TimestampMs,
    ) -> AppResult<()> {
        // Only moderated answers reach the cache (moderation 1 = passed).
        self.tx
            .execute(
                "INSERT OR REPLACE INTO association_cache
                     (cache_key, text, model, prompt_version, moderation, created_at, expires_at)
                 VALUES (?1, ?2, ?3, ?4, 1, ?5, NULL)",
                params![
                    cache_key,
                    entry.text,
                    entry.model,
                    entry.prompt_version,
                    at.0
                ],
            )
            .map(drop)
            .map_err(map_sqlite)
    }

    fn insert_llm_call(&mut self, call: &LlmCall) -> AppResult<()> {
        self.tx
            .execute(
                "INSERT INTO llm_calls (request_id, started_at, duration_ms, provider, model,
                     outcome)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    call.request_id,
                    call.started_at.0,
                    call.duration_ms.max(0),
                    call.provider,
                    call.model,
                    call.outcome,
                ],
            )
            .map(drop)
            .map_err(map_sqlite)
    }

    fn llm_calls_since(&mut self, since: TimestampMs) -> AppResult<u32> {
        self.tx
            .query_row(
                "SELECT COUNT(*) FROM llm_calls WHERE started_at >= ?1 AND outcome <> 'notSent'",
                [since.0],
                |row| row.get(0),
            )
            .map_err(map_sqlite)
    }

    fn llm_call_durations(&mut self, since: TimestampMs) -> AppResult<Vec<i64>> {
        let mut statement = self
            .tx
            .prepare_cached(
                "SELECT duration_ms FROM llm_calls WHERE started_at >= ?1 AND outcome = 'ok'
                 ORDER BY duration_ms",
            )
            .map_err(map_sqlite)?;
        statement
            .query_map([since.0], |row| row.get(0))
            .and_then(Iterator::collect)
            .map_err(map_sqlite)
    }
}
