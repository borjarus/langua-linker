//! FSRS optimizer commands (plan Phase 18, ADR-22), mapped to contract 1.5 DTOs.
//!
//! `start_optimizer` returns a job (`optimize`) that trains on the device; `get_optimizer` is the
//! durable truth about a preset's runs. Apply and revert change only the preset's weights.

use langualinker_application::optimizer::{
    self, DatasetCounts, OptimizerView, RunAction, RunMetrics, RunStatus, RunView,
};
use langualinker_application::ports::CollectionScope;
use langualinker_application::repositories::UnitOfWork;
use langualinker_contracts::ErrorDto;
use langualinker_contracts::dto::{
    GetOptimizerInput, JobDto, JobResultDto, OptimizerDatasetDto, OptimizerDto,
    OptimizerMetricsDto, OptimizerRunActionInput, OptimizerRunDto, OptimizerRunStatus, PresetDto,
    StartOptimizerInput,
};
use langualinker_domain::DeckConfigId;
use langualinker_domain::fsrs::DEFAULT_WEIGHTS;
use langualinker_domain::optimizer::{MINIMUM_REVIEWS, RECOMMENDED_REVIEWS};
use serde_json::Value;
use tauri::State;

use crate::backend::{Backend, iso, job_dto};
use crate::commands::{blocking, operation_id, parse_input};
use crate::content::clamp;
use crate::manage::preset_dto;
use crate::study::{app_error, emit, id, text_id};

/// `JobResultDto` of a finished run, stored with the job.
pub fn encode_run(run_id: i64) -> String {
    serde_json::to_string(&JobResultDto::Optimize {
        run_id: text_id(run_id),
    })
    .unwrap_or_default()
}

fn dataset_dto(counts: DatasetCounts) -> OptimizerDatasetDto {
    OptimizerDatasetDto {
        reviews: counts.reviews,
        cards: counts.cards,
        usable: counts.usable,
        excluded_undone: counts.excluded_undone,
        excluded_manual: counts.excluded_manual,
        excluded_incomplete: counts.excluded_incomplete,
    }
}

fn metrics_dto(metrics: RunMetrics) -> OptimizerMetricsDto {
    OptimizerMetricsDto {
        log_loss: metrics.log_loss,
        rmse: metrics.rmse,
        daily_reviews: metrics.daily_reviews,
        median_interval_days: metrics.median_interval_days,
    }
}

fn status_dto(status: RunStatus) -> OptimizerRunStatus {
    match status {
        RunStatus::Proposed => OptimizerRunStatus::Proposed,
        RunStatus::Applied => OptimizerRunStatus::Applied,
        RunStatus::Reverted => OptimizerRunStatus::Reverted,
    }
}

fn run_dto(view: RunView) -> OptimizerRunDto {
    let RunView {
        run,
        report,
        can_apply,
        can_revert,
    } = view;
    OptimizerRunDto {
        run_id: text_id(run.id),
        status: status_dto(run.status),
        created_at: iso(run.created_at),
        applied_at: run.applied_at.map(iso),
        reverted_at: run.reverted_at.map(iso),
        optimizer_version: run.optimizer_version,
        fsrs_version: run.fsrs_version,
        dataset: dataset_dto(report.dataset),
        training_reviews: report.training_reviews,
        validation_reviews: report.validation_reviews,
        weights_before: run.weights_before,
        weights_proposed: run.weights_proposed,
        current: metrics_dto(report.current),
        proposed: metrics_dto(report.proposed),
        recommended: report.recommended,
        few_reviews: report.few_reviews,
        can_apply,
        can_revert,
    }
}

fn optimizer_dto(view: OptimizerView) -> OptimizerDto {
    OptimizerDto {
        preset_id: text_id(view.preset.id.0),
        preset_name: view.preset.name,
        preset_version: clamp(view.preset.version),
        desired_retention: view.preset.options.desired_retention,
        current_weights: view.preset.options.weights,
        default_weights: DEFAULT_WEIGHTS.to_vec(),
        dataset: dataset_dto(view.dataset),
        minimum_reviews: MINIMUM_REVIEWS,
        recommended_reviews: RECOMMENDED_REVIEWS,
        running_job_id: view.running_job.map(|job| job.0),
        runs: view.runs.into_iter().map(run_dto).collect(),
    }
}

#[tauri::command]
pub async fn get_optimizer(
    backend: State<'_, Backend>,
    input: Value,
) -> Result<OptimizerDto, ErrorDto> {
    let input: GetOptimizerInput = parse_input(input)?;
    let preset = DeckConfigId(id("presetId", &input.preset_id)?);
    let service = backend.collection()?.optimizer.clone();
    blocking(move || service.view(preset).map(optimizer_dto).map_err(app_error)).await
}

#[tauri::command]
pub async fn start_optimizer(
    backend: State<'_, Backend>,
    input: Value,
) -> Result<JobDto, ErrorDto> {
    let input: StartOptimizerInput = parse_input(input)?;
    operation_id(&input.operation_id)?;
    let preset = DeckConfigId(id("presetId", &input.preset_id)?);
    let service = backend.collection()?.optimizer.clone();
    blocking(move || {
        service
            .start(input.operation_id, preset)
            .map_err(app_error)
            .and_then(|job| job_dto(&job))
    })
    .await
}

async fn switch(
    backend: State<'_, Backend>,
    input: Value,
    apply: bool,
) -> Result<PresetDto, ErrorDto> {
    let input: OptimizerRunActionInput = parse_input(input)?;
    operation_id(&input.operation_id)?;
    let action = RunAction {
        operation_id: input.operation_id,
        run_id: id("runId", &input.run_id)?,
        expected_version: i64::from(input.expected_version),
    };
    let collection = backend.collection()?;
    let (db, now) = (collection.db.clone(), collection.clock.now());
    let preset = blocking(move || {
        db.write(move |tx| {
            if apply {
                optimizer::apply(tx, &action, now)
            } else {
                optimizer::revert(tx, &action, now)
            }
        })
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

#[tauri::command]
pub async fn apply_optimizer_run(
    backend: State<'_, Backend>,
    input: Value,
) -> Result<PresetDto, ErrorDto> {
    switch(backend, input, true).await
}

#[tauri::command]
pub async fn revert_optimizer_run(
    backend: State<'_, Backend>,
    input: Value,
) -> Result<PresetDto, ErrorDto> {
    switch(backend, input, false).await
}
