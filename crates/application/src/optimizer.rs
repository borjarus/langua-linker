//! FSRS optimizer use cases (plan Phase 18 step 2, ADR-22): read the preset's history, run the
//! local optimizer as a cancellable job, keep every result, and apply or revert one with the
//! learner's consent. History never leaves the device and is never changed; applying writes only
//! the preset's weights (no card is rescheduled), so a revert restores the weights before exactly.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use langualinker_domain::calendar::StudyCalendar;
use langualinker_domain::deck::DeckConfig;
use langualinker_domain::fsrs::{FSRS_VERSION, WEIGHT_COUNT};
use langualinker_domain::optimizer::{
    self, Dataset, DatasetStats, LoggedReview, MINIMUM_REVIEWS, Metrics, OPTIMIZER_VERSION,
    OptimizeError, OptimizeRequest,
};
use langualinker_domain::review::{Rating, ReviewKind};
use langualinker_domain::{CardId, DeckConfigId, TimestampMs, ValidationError};
use serde::{Deserialize, Serialize};

use crate::jobs::{JobId, JobKind, JobRecord, JobService, NewJob};
use crate::learning;
use crate::management;
use crate::ports::Clock;
use crate::repositories::{Repositories, UnitOfWork};
use crate::{AppError, AppResult};

const OP_APPLY: &str = "applyOptimizerRun";
const OP_REVERT: &str = "revertOptimizerRun";
/// Runs shown per preset, newest first.
const HISTORY_LIMIT: u32 = 20;
/// Progress is persisted at most this many times per run (each update is a write).
const PROGRESS_UPDATES: u64 = 50;

// ---------------------------------------------------------------- records

/// One logged answer of a card of the preset (`review_logs`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OptimizerLog {
    pub card_id: CardId,
    pub rating: Rating,
    pub kind: ReviewKind,
    pub reviewed_at: TimestampMs,
    pub invalidated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStatus {
    Proposed,
    Applied,
    Reverted,
}

impl RunStatus {
    pub fn db_code(self) -> i64 {
        match self {
            Self::Proposed => 0,
            Self::Applied => 1,
            Self::Reverted => 2,
        }
    }

    pub fn from_db_code(code: i64) -> AppResult<Self> {
        match code {
            0 => Ok(Self::Proposed),
            1 => Ok(Self::Applied),
            2 => Ok(Self::Reverted),
            _ => Err(AppError::Internal(format!(
                "unknown optimizer run status {code}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewOptimizerRun {
    pub preset_id: DeckConfigId,
    pub optimizer_version: String,
    pub fsrs_version: String,
    pub dataset_hash: String,
    pub weights_before: Vec<f64>,
    pub weights_proposed: Vec<f64>,
    /// [`RunReport`] as JSON.
    pub report_json: String,
    pub created_at: TimestampMs,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OptimizerRun {
    pub id: i64,
    pub preset_id: DeckConfigId,
    pub status: RunStatus,
    pub optimizer_version: String,
    pub fsrs_version: String,
    pub dataset_hash: String,
    pub weights_before: Vec<f64>,
    pub weights_proposed: Vec<f64>,
    pub report_json: String,
    pub created_at: TimestampMs,
    pub applied_at: Option<TimestampMs>,
    pub reverted_at: Option<TimestampMs>,
}

/// Dataset counts as stored with a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasetCounts {
    pub reviews: u32,
    pub cards: u32,
    pub usable: u32,
    pub excluded_undone: u32,
    pub excluded_manual: u32,
    pub excluded_incomplete: u32,
}

impl From<DatasetStats> for DatasetCounts {
    fn from(stats: DatasetStats) -> Self {
        Self {
            reviews: stats.reviews,
            cards: stats.cards,
            usable: stats.usable,
            excluded_undone: stats.excluded_undone,
            excluded_manual: stats.excluded_manual,
            excluded_incomplete: stats.excluded_incomplete,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunMetrics {
    pub log_loss: f64,
    pub rmse: f64,
    pub daily_reviews: f64,
    pub median_interval_days: f64,
}

impl From<Metrics> for RunMetrics {
    fn from(metrics: Metrics) -> Self {
        Self {
            log_loss: metrics.validation.log_loss,
            rmse: metrics.validation.rmse,
            daily_reviews: metrics.workload.daily_reviews,
            median_interval_days: metrics.workload.median_interval_days,
        }
    }
}

/// What a run measured (`optimizer_runs.report`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunReport {
    pub dataset: DatasetCounts,
    pub training_reviews: u32,
    pub validation_reviews: u32,
    pub current: RunMetrics,
    pub proposed: RunMetrics,
    pub recommended: bool,
    pub few_reviews: bool,
}

/// A run as the screen shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct RunView {
    pub run: OptimizerRun,
    pub report: RunReport,
    pub can_apply: bool,
    pub can_revert: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OptimizerView {
    pub preset: DeckConfig,
    pub dataset: DatasetCounts,
    pub running_job: Option<JobId>,
    pub runs: Vec<RunView>,
}

// ---------------------------------------------------------------- reads

fn live_preset(tx: &mut dyn Repositories, id: DeckConfigId) -> AppResult<DeckConfig> {
    tx.config(id)?
        .filter(|config| config.deleted_at.is_none())
        .ok_or(AppError::NotFound { entity: "preset" })
}

/// Chronological card histories of the preset's live cards, with study days in the collection
/// calendar (the scheduler's elapsed-day rule).
pub fn dataset(tx: &mut dyn Repositories, preset: DeckConfigId) -> AppResult<Dataset> {
    let (zone, cutoff) = tx.collection_calendar()?;
    let calendar = StudyCalendar::new(&zone, cutoff).unwrap_or_else(|_| StudyCalendar::utc(cutoff));
    let logs = tx
        .optimizer_logs(preset)?
        .into_iter()
        .map(|log| LoggedReview {
            card: log.card_id.0,
            rating: log.rating,
            kind: log.kind,
            day: calendar.day_of(log.reviewed_at).0,
            at: log.reviewed_at.0,
            undone: log.invalidated,
        })
        .collect();
    Ok(optimizer::build_dataset(logs))
}

fn run_view(run: OptimizerRun, weights: &[f64]) -> AppResult<RunView> {
    let report: RunReport = serde_json::from_str(&run.report_json)
        .map_err(|error| AppError::Internal(format!("optimizer report: {error}")))?;
    let can_apply = run.status == RunStatus::Proposed && run.weights_before == weights;
    let can_revert = run.status == RunStatus::Applied && run.weights_proposed == weights;
    Ok(RunView {
        run,
        report,
        can_apply,
        can_revert,
    })
}

/// GetOptimizer without the running job: the preset, what its history offers and the runs.
pub fn view(tx: &mut dyn Repositories, preset_id: DeckConfigId) -> AppResult<OptimizerView> {
    let preset = live_preset(tx, preset_id)?;
    let dataset = dataset(tx, preset_id)?;
    let runs = tx
        .optimizer_runs(preset_id, HISTORY_LIMIT)?
        .into_iter()
        .map(|run| run_view(run, &preset.options.weights))
        .collect::<AppResult<Vec<_>>>()?;
    Ok(OptimizerView {
        preset,
        dataset: dataset.stats.into(),
        running_job: None,
        runs,
    })
}

// ---------------------------------------------------------------- apply / revert

#[derive(Debug, Clone, PartialEq)]
pub struct RunAction {
    pub operation_id: String,
    pub run_id: i64,
    /// Preset version the screen showed.
    pub expected_version: i64,
}

fn find_preset(tx: &mut dyn Repositories, id: DeckConfigId) -> AppResult<(DeckConfig, u32)> {
    management::presets(tx)?
        .into_iter()
        .find(|(config, _)| config.id == id)
        .ok_or(AppError::NotFound { entity: "preset" })
}

/// Shared by apply and revert: checks the run's status and that the preset still has the
/// weights the action starts from, then writes the other weight vector.
fn switch_weights(
    tx: &mut dyn Repositories,
    action: &RunAction,
    operation: &str,
    now: TimestampMs,
) -> AppResult<(DeckConfig, u32)> {
    if learning::replayed(tx, &action.operation_id, operation)? {
        let id = tx
            .recorded_operation(&action.operation_id)?
            .and_then(|recorded| recorded.result_json.parse::<i64>().ok())
            .ok_or_else(|| AppError::Internal("recorded operation without an id".into()))?;
        return find_preset(tx, DeckConfigId(id));
    }
    let run = tx.optimizer_run(action.run_id)?.ok_or(AppError::NotFound {
        entity: "optimizerRun",
    })?;
    let mut preset = live_preset(tx, run.preset_id)?;
    if preset.version != action.expected_version {
        return Err(AppError::VersionConflict {
            entity: "preset",
            current: preset.version,
        });
    }
    let applying = operation == OP_APPLY;
    let (required, from, to) = if applying {
        (
            RunStatus::Proposed,
            &run.weights_before,
            &run.weights_proposed,
        )
    } else {
        (
            RunStatus::Applied,
            &run.weights_proposed,
            &run.weights_before,
        )
    };
    if run.status != required {
        return Err(ValidationError::new("runId", "status").into());
    }
    if preset.options.weights != *from {
        return Err(ValidationError::new("runId", "stale").into());
    }
    preset.options.weights = to.clone();
    let saved = tx.update_config(&preset, now)?;
    let status = if applying {
        RunStatus::Applied
    } else {
        RunStatus::Reverted
    };
    tx.set_optimizer_run_status(run.id, status, now)?;
    learning::record(
        tx,
        &action.operation_id,
        operation,
        saved.id.0.to_string(),
        now,
    )?;
    find_preset(tx, saved.id)
}

/// ApplyOptimizerRun: the preset takes the proposed weights from its next rating on. Due dates
/// and memory states of existing cards stay as they are (no reschedule without consent).
pub fn apply(
    tx: &mut dyn Repositories,
    action: &RunAction,
    now: TimestampMs,
) -> AppResult<(DeckConfig, u32)> {
    switch_weights(tx, action, OP_APPLY, now)
}

/// RevertOptimizerRun: back to the weights the preset had before the run was applied.
pub fn revert(
    tx: &mut dyn Repositories,
    action: &RunAction,
    now: TimestampMs,
) -> AppResult<(DeckConfig, u32)> {
    switch_weights(tx, action, OP_REVERT, now)
}

// ---------------------------------------------------------------- the job

/// Renders the job result (`JobResultDto` `optimize`) for a stored run id.
pub type EncodeRun = fn(i64) -> String;

/// Starts optimizer jobs (one per preset at a time) and answers GetOptimizer with the running job.
pub struct OptimizerService<U: UnitOfWork> {
    db: Arc<U>,
    jobs: JobService<U>,
    clock: Arc<dyn Clock>,
    encode: EncodeRun,
    running: Arc<Mutex<HashMap<DeckConfigId, JobId>>>,
    /// Serialises starts, so two taps cannot start two runs of one preset.
    starting: Arc<Mutex<()>>,
}

impl<U: UnitOfWork> Clone for OptimizerService<U> {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            jobs: self.jobs.clone(),
            clock: self.clock.clone(),
            encode: self.encode,
            running: self.running.clone(),
            starting: self.starting.clone(),
        }
    }
}

/// Removes the preset from the running registry however the job ends.
struct RunningGuard {
    running: Arc<Mutex<HashMap<DeckConfigId, JobId>>>,
    preset: DeckConfigId,
}

impl Drop for RunningGuard {
    fn drop(&mut self) {
        if let Ok(mut running) = self.running.lock() {
            running.remove(&self.preset);
        }
    }
}

impl<U: UnitOfWork> OptimizerService<U> {
    pub fn new(db: Arc<U>, jobs: JobService<U>, clock: Arc<dyn Clock>, encode: EncodeRun) -> Self {
        Self {
            db,
            jobs,
            clock,
            encode,
            running: Arc::default(),
            starting: Arc::default(),
        }
    }

    fn running_job(&self, preset: DeckConfigId) -> Option<JobId> {
        let job = self.running.lock().ok()?.get(&preset).cloned()?;
        // The registry entry goes away when the work returns; the job row a moment later.
        match self.jobs.get(&job) {
            Ok(record) if !record.status.is_finished() => Some(job),
            _ => None,
        }
    }

    pub fn view(&self, preset: DeckConfigId) -> AppResult<OptimizerView> {
        let mut view = self.db.read(move |tx| view(tx, preset))?;
        view.running_job = self.running_job(preset);
        Ok(view)
    }

    /// StartOptimizer: a job that trains on the preset's history and stores a proposed run. A
    /// run already in progress for the preset is returned instead of a second one.
    pub fn start(&self, operation_id: String, preset: DeckConfigId) -> AppResult<JobRecord> {
        learning::check_operation_id(&operation_id)?;
        let _starting = self
            .starting
            .lock()
            .map_err(|_| AppError::Internal("optimizer start lock".into()))?;
        if let Some(job) = self.running_job(preset) {
            return self.jobs.get(&job);
        }
        let usable = self.db.read(move |tx| {
            live_preset(tx, preset)?;
            Ok(dataset(tx, preset)?.stats.usable)
        })?;
        if usable < MINIMUM_REVIEWS {
            return Err(ValidationError::new("presetId", "notEnoughReviews").into());
        }
        let service = self.clone();
        let guard = RunningGuard {
            running: self.running.clone(),
            preset,
        };
        let job = self.jobs.start(
            NewJob {
                kind: JobKind::Optimize,
                operation_id: Some(operation_id),
                total: None,
            },
            Box::new(move |context| {
                let _guard = guard;
                service.work(preset, context)
            }),
        )?;
        // A run that already finished leaves a stale entry; `running_job` checks the status.
        if !job.status.is_finished()
            && let Ok(mut running) = self.running.lock()
        {
            running.insert(preset, job.id.clone());
        }
        Ok(job)
    }

    fn work(
        &self,
        preset_id: DeckConfigId,
        context: &crate::jobs::JobContext<U>,
    ) -> AppResult<Option<String>> {
        let (preset, dataset) = self.db.read(move |tx| {
            let preset = live_preset(tx, preset_id)?;
            Ok((preset, dataset(tx, preset_id)?))
        })?;
        let mut current_weights = [0.0; WEIGHT_COUNT];
        if preset.options.weights.len() != WEIGHT_COUNT {
            return Err(AppError::Internal("preset weights".into()));
        }
        current_weights.copy_from_slice(&preset.options.weights);
        let request = OptimizeRequest {
            dataset: &dataset,
            current_weights,
            desired_retention: preset.options.desired_retention,
            maximum_interval_days: preset.options.maximum_interval_days,
        };
        let mut stopped: Option<AppError> = None;
        let mut reported = u64::MAX;
        let result = optimizer::optimize(&request, |done, total| {
            let bucket = done * PROGRESS_UPDATES / total.max(1);
            let outcome = if bucket != reported {
                reported = bucket;
                context.progress(done, total)
            } else {
                context.check_cancelled()
            };
            match outcome {
                Ok(()) => true,
                Err(error) => {
                    stopped = Some(error);
                    false
                }
            }
        });
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(OptimizeError::Cancelled) => {
                return Err(stopped.unwrap_or(AppError::Cancelled));
            }
            Err(OptimizeError::NotEnoughReviews) => {
                return Err(ValidationError::new("presetId", "notEnoughReviews").into());
            }
        };
        let report = RunReport {
            dataset: dataset.stats.into(),
            training_reviews: outcome.training_reviews,
            validation_reviews: outcome.validation_reviews,
            current: outcome.current.into(),
            proposed: outcome.proposed.into(),
            recommended: outcome.recommended,
            few_reviews: outcome.few_reviews,
        };
        let run = NewOptimizerRun {
            preset_id,
            optimizer_version: OPTIMIZER_VERSION.into(),
            fsrs_version: FSRS_VERSION.into(),
            dataset_hash: dataset.fingerprint.clone(),
            weights_before: preset.options.weights.clone(),
            weights_proposed: outcome.weights.to_vec(),
            report_json: serde_json::to_string(&report)
                .map_err(|error| AppError::Internal(format!("optimizer report: {error}")))?,
            created_at: self.clock.now(),
        };
        context.begin_commit()?;
        let id = self.db.write(move |tx| tx.insert_optimizer_run(&run))?;
        tracing::info!(
            run_id = id,
            usable = report.dataset.usable,
            recommended = report.recommended,
            "optimizer run finished"
        );
        Ok(Some((self.encode)(id)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_status_codes_round_trip() {
        for status in [RunStatus::Proposed, RunStatus::Applied, RunStatus::Reverted] {
            assert_eq!(RunStatus::from_db_code(status.db_code()).unwrap(), status);
        }
        assert!(RunStatus::from_db_code(3).is_err());
    }

    #[test]
    fn reports_round_trip_as_json() {
        let metrics = RunMetrics {
            log_loss: 0.35,
            rmse: 0.03,
            daily_reviews: 12.5,
            median_interval_days: 9.0,
        };
        let report = RunReport {
            dataset: DatasetCounts {
                usable: 1200,
                ..DatasetCounts::default()
            },
            training_reviews: 960,
            validation_reviews: 240,
            current: metrics,
            proposed: metrics,
            recommended: false,
            few_reviews: false,
        };
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"trainingReviews\":960"));
        assert_eq!(serde_json::from_str::<RunReport>(&json).unwrap(), report);
    }
}
