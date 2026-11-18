//! FSRS optimizer: history of a preset's cards and stored runs (schema v6, plan Phase 18).

use langualinker_application::optimizer::{NewOptimizerRun, OptimizerLog, OptimizerRun, RunStatus};
use langualinker_application::repositories::OptimizerRepository;
use langualinker_application::{AppError, AppResult};
use langualinker_domain::review::{Rating, ReviewKind};
use langualinker_domain::{CardId, DeckConfigId, TimestampMs};
use rusqlite::{OptionalExtension, Row, params};

use super::SqliteRepos;
use crate::sqlite::{codec, map_sqlite};

const RUN_COLUMNS: &str = "id, preset_id, status, optimizer_version, fsrs_version, dataset_hash,
    weights_before, weights_proposed, report, created_at, applied_at, reverted_at";

/// Raw row; codes and JSON are decoded outside rusqlite's error type.
struct RunRow {
    id: i64,
    preset_id: i64,
    status: i64,
    optimizer_version: String,
    fsrs_version: String,
    dataset_hash: String,
    weights_before: String,
    weights_proposed: String,
    report: String,
    created_at: i64,
    applied_at: Option<i64>,
    reverted_at: Option<i64>,
}

fn run_row(row: &Row<'_>) -> rusqlite::Result<RunRow> {
    Ok(RunRow {
        id: row.get(0)?,
        preset_id: row.get(1)?,
        status: row.get(2)?,
        optimizer_version: row.get(3)?,
        fsrs_version: row.get(4)?,
        dataset_hash: row.get(5)?,
        weights_before: row.get(6)?,
        weights_proposed: row.get(7)?,
        report: row.get(8)?,
        created_at: row.get(9)?,
        applied_at: row.get(10)?,
        reverted_at: row.get(11)?,
    })
}

impl RunRow {
    fn decode(self) -> AppResult<OptimizerRun> {
        Ok(OptimizerRun {
            id: self.id,
            preset_id: DeckConfigId(self.preset_id),
            status: RunStatus::from_db_code(self.status)?,
            optimizer_version: self.optimizer_version,
            fsrs_version: self.fsrs_version,
            dataset_hash: self.dataset_hash,
            weights_before: codec::f64_list_from_json(&self.weights_before, "weights_before")?,
            weights_proposed: codec::f64_list_from_json(
                &self.weights_proposed,
                "weights_proposed",
            )?,
            report_json: self.report,
            created_at: TimestampMs(self.created_at),
            applied_at: self.applied_at.map(TimestampMs),
            reverted_at: self.reverted_at.map(TimestampMs),
        })
    }
}

impl OptimizerRepository for SqliteRepos<'_> {
    fn optimizer_logs(&mut self, preset: DeckConfigId) -> AppResult<Vec<OptimizerLog>> {
        let mut statement = self
            .tx
            .prepare_cached(
                "SELECT l.card_id, l.rating, l.kind, l.reviewed_at, l.invalidated_at IS NOT NULL
                   FROM review_logs l
                   JOIN cards c ON c.id = l.card_id
                   JOIN decks d ON d.id = c.deck_id
                  WHERE d.config_id = ?1 AND c.deleted_at IS NULL AND d.deleted_at IS NULL
                  ORDER BY l.card_id, l.reviewed_at, l.id",
            )
            .map_err(map_sqlite)?;
        let rows: Vec<(i64, i64, i64, i64, bool)> = statement
            .query_map([preset.0], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })
            .and_then(Iterator::collect)
            .map_err(map_sqlite)?;
        rows.into_iter()
            .map(|(card, rating, kind, at, invalidated)| {
                Ok(OptimizerLog {
                    card_id: CardId(card),
                    rating: Rating::from_db_code(rating)?,
                    kind: ReviewKind::from_db_code(kind)?,
                    reviewed_at: TimestampMs(at),
                    invalidated,
                })
            })
            .collect()
    }

    fn insert_optimizer_run(&mut self, run: &NewOptimizerRun) -> AppResult<i64> {
        self.tx
            .execute(
                "INSERT INTO optimizer_runs (preset_id, status, optimizer_version, fsrs_version,
                     dataset_hash, weights_before, weights_proposed, report, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    run.preset_id.0,
                    RunStatus::Proposed.db_code(),
                    run.optimizer_version,
                    run.fsrs_version,
                    run.dataset_hash,
                    codec::f64_list_to_json(&run.weights_before),
                    codec::f64_list_to_json(&run.weights_proposed),
                    run.report_json,
                    run.created_at.0,
                ],
            )
            .map_err(map_sqlite)?;
        Ok(self.tx.last_insert_rowid())
    }

    fn optimizer_run(&mut self, id: i64) -> AppResult<Option<OptimizerRun>> {
        let sql = format!("SELECT {RUN_COLUMNS} FROM optimizer_runs WHERE id = ?1");
        self.tx
            .query_row(&sql, [id], run_row)
            .optional()
            .map_err(map_sqlite)?
            .map(RunRow::decode)
            .transpose()
    }

    fn optimizer_runs(&mut self, preset: DeckConfigId, limit: u32) -> AppResult<Vec<OptimizerRun>> {
        let sql = format!(
            "SELECT {RUN_COLUMNS} FROM optimizer_runs WHERE preset_id = ?1
              ORDER BY created_at DESC, id DESC LIMIT ?2"
        );
        let mut statement = self.tx.prepare_cached(&sql).map_err(map_sqlite)?;
        let rows: Vec<RunRow> = statement
            .query_map(params![preset.0, limit], run_row)
            .and_then(Iterator::collect)
            .map_err(map_sqlite)?;
        rows.into_iter().map(RunRow::decode).collect()
    }

    fn set_optimizer_run_status(
        &mut self,
        id: i64,
        status: RunStatus,
        at: TimestampMs,
    ) -> AppResult<()> {
        let sql = match status {
            RunStatus::Applied => {
                "UPDATE optimizer_runs SET status = ?1, applied_at = ?2 WHERE id = ?3"
            }
            RunStatus::Reverted => {
                "UPDATE optimizer_runs SET status = ?1, reverted_at = ?2 WHERE id = ?3"
            }
            RunStatus::Proposed => {
                "UPDATE optimizer_runs SET status = ?1 WHERE id = ?3 AND ?2 = ?2"
            }
        };
        let changed = self
            .tx
            .execute(sql, params![status.db_code(), at.0, id])
            .map_err(map_sqlite)?;
        if changed == 1 {
            Ok(())
        } else {
            Err(AppError::NotFound {
                entity: "optimizerRun",
            })
        }
    }
}
