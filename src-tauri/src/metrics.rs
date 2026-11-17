//! Opt-in usage metrics in the shell (plan Phase 16 step 1, ADR-13). Not part of `AppApi`: the
//! commands are granted by the separate `metrics` capability, like the Phase 15 platform adapters.
//!
//! - The store ([`LocalTelemetry`]) follows the telemetry consent in the settings: on start,
//!   after `updateSettings`, and after a restore or reset replaced the settings.
//! - Ratings, undo, favorites and ended sessions are recorded by the study commands; provider
//!   calls by the LLM service; restores by [`MeteredEvents`]; a crash of the previous run on the
//!   next start; cold start and WebView errors by the WebView through the commands below.
//! - `get_usage_metrics` returns the local summary for the Privacy screen. Nothing is sent
//!   anywhere (D-08 is open).

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use langualinker_application::jobs::{JobEvent, JobKind, JobStatus};
use langualinker_application::ports::{CollectionScope, EventSink, Telemetry};
use langualinker_application::repositories::UnitOfWork;
use langualinker_application::settings as app_settings;
use langualinker_application::telemetry::{MetricEvent, MetricsSummary, TargetCheck};
use langualinker_contracts::{ErrorCode, ErrorDto};
use langualinker_domain::TimestampMs;
use langualinker_infrastructure::logging;
use langualinker_infrastructure::sqlite::SqliteDatabase;
use langualinker_infrastructure::telemetry::LocalTelemetry;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;

use crate::backend::{Backend, iso};
use crate::commands::{blocking, parse_input};
use crate::study::app_error;

/// Longest cold start accepted from the WebView; anything above is a broken measurement.
const MAX_COLD_START_MS: u32 = 600_000;
/// WebView errors recorded per process; a render loop must not fill the store.
const MAX_CLIENT_ERRORS: u32 = 20;

/// Makes the store follow the stored consent. Failures are logged; metrics never block.
pub fn sync_consent(db: &SqliteDatabase, telemetry: &LocalTelemetry) {
    match db.read(|tx| app_settings::load_doc(tx)) {
        Ok(doc) => {
            if let Err(error) = telemetry.set_enabled(doc.telemetry_opt_in) {
                tracing::warn!(code = error.code(), "metrics consent not applied");
            }
        }
        Err(error) => tracing::warn!(code = error.code(), "metrics consent not read"),
    }
}

/// Counts a crash the previous run left behind (once) and logs where it happened.
pub fn record_previous_crash(log_dir: &Path, telemetry: &dyn Telemetry) {
    if let Some(report) = logging::take_crash_report(log_dir) {
        tracing::warn!(
            app_version = report.app_version.as_str(),
            location = report.location.as_str(),
            "previous run ended in a crash"
        );
        telemetry.record(MetricEvent::NativeCrash);
    }
}

/// Passes every signal on and derives the restore metric from terminal job events. After a
/// restore or reset the stored consent may differ, so the store follows it again (on a separate
/// thread: the sink may be called while the database worker is busy).
pub struct MeteredEvents {
    pub inner: Arc<dyn EventSink>,
    pub telemetry: Arc<LocalTelemetry>,
    pub db: Arc<SqliteDatabase>,
}

impl EventSink for MeteredEvents {
    fn job_progress(&self, event: &JobEvent) {
        let terminal = matches!(event.status, JobStatus::Succeeded | JobStatus::Failed);
        if terminal && event.kind == JobKind::Restore {
            self.telemetry.record(MetricEvent::Restored {
                ok: event.status == JobStatus::Succeeded,
            });
        }
        if event.status == JobStatus::Succeeded
            && matches!(event.kind, JobKind::Restore | JobKind::Reset)
        {
            let (db, telemetry) = (self.db.clone(), self.telemetry.clone());
            let spawned = std::thread::Builder::new()
                .name("metrics-consent".into())
                .spawn(move || sync_consent(&db, &telemetry));
            if spawned.is_err() {
                tracing::warn!("metrics consent not re-read");
            }
        }
        self.inner.job_progress(event);
    }

    fn collection_changed(&self, scopes: &[CollectionScope], at: TimestampMs) {
        self.inner.collection_changed(scopes, at);
    }
}

// ---------------------------------------------------------------- commands

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StartupInput {
    cold_start_ms: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
enum ClientErrorKind {
    Error,
    UnhandledRejection,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ClientErrorInput {
    kind: ClientErrorKind,
}

static STARTUP_REPORTED: AtomicBool = AtomicBool::new(false);
static CLIENT_ERRORS: AtomicU32 = AtomicU32::new(0);

/// The WebView showed its first screen `coldStartMs` after navigation start. Counted once per
/// process (a WebView reload is not a cold start).
#[tauri::command]
pub async fn report_startup(backend: State<'_, Backend>, input: Value) -> Result<(), ErrorDto> {
    let input: StartupInput = parse_input(input)?;
    if input.cold_start_ms > MAX_COLD_START_MS {
        return Err(ErrorDto::new(ErrorCode::ValidationFailed).with_detail("field", "coldStartMs"));
    }
    if STARTUP_REPORTED.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    tracing::info!(duration_ms = input.cold_start_ms, "first screen shown");
    let store = backend.collection()?.telemetry.clone();
    blocking(move || {
        store.record(MetricEvent::AppStarted {
            cold_start_ms: input.cold_start_ms,
        });
        Ok(())
    })
    .await
}

/// An uncaught WebView error or rejected promise. Only its kind crosses IPC: messages and stacks
/// may contain card text.
#[tauri::command]
pub async fn report_client_error(
    backend: State<'_, Backend>,
    input: Value,
) -> Result<(), ErrorDto> {
    let input: ClientErrorInput = parse_input(input)?;
    if CLIENT_ERRORS.fetch_add(1, Ordering::SeqCst) >= MAX_CLIENT_ERRORS {
        return Ok(());
    }
    let code = match input.kind {
        ClientErrorKind::Error => "error",
        ClientErrorKind::UnhandledRejection => "unhandledRejection",
    };
    tracing::warn!(code, "WebView error");
    let store = backend.collection()?.telemetry.clone();
    blocking(move || {
        store.record(MetricEvent::ClientError);
        Ok(())
    })
    .await
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetDto {
    pub metric: &'static str,
    pub target: &'static str,
    pub value: Option<f64>,
    pub samples: u32,
    pub status: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RatingCountsDto {
    pub again: u32,
    pub hard: u32,
    pub good: u32,
    pub easy: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricsSummaryDto {
    pub since: String,
    pub days_observed: u32,
    pub active_days: u32,
    pub app_starts: u32,
    pub retained_d7: Option<bool>,
    pub cold_start_median_ms: Option<u32>,
    pub generations: u32,
    pub generation_failures: u32,
    pub generation_retries: u32,
    pub generation_p90_ms: Option<u32>,
    pub favorites: u32,
    pub favorite_rate: Option<f64>,
    pub native_crashes: u32,
    pub crash_free_rate: Option<f64>,
    pub client_errors: u32,
    pub sessions: u32,
    pub ratings: RatingCountsDto,
    pub undos: u32,
    pub restores: u32,
    pub restore_failures: u32,
    pub targets: Vec<TargetDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageMetricsDto {
    pub enabled: bool,
    /// Pseudonymous id of this installation's metrics; `null` while not opted in.
    pub install_id: Option<String>,
    pub summary: Option<MetricsSummaryDto>,
}

fn target_dto(check: &TargetCheck) -> TargetDto {
    TargetDto {
        metric: check.metric.as_str(),
        target: check.metric.target(),
        value: check.value,
        samples: check.samples,
        status: check.status.as_str(),
    }
}

pub fn summary_dto(summary: &MetricsSummary) -> MetricsSummaryDto {
    let [again, hard, good, easy] = summary.ratings;
    MetricsSummaryDto {
        since: summary.since.map(iso).unwrap_or_default(),
        days_observed: summary.days_observed,
        active_days: summary.active_days,
        app_starts: summary.app_starts,
        retained_d7: summary.retained_d7,
        cold_start_median_ms: summary.cold_start_median_ms,
        generations: summary.generations,
        generation_failures: summary.generation_failures,
        generation_retries: summary.generation_retries,
        generation_p90_ms: summary.generation_p90_ms,
        favorites: summary.favorites,
        favorite_rate: summary.favorite_rate,
        native_crashes: summary.native_crashes,
        crash_free_rate: summary.crash_free_rate,
        client_errors: summary.client_errors,
        sessions: summary.sessions,
        ratings: RatingCountsDto {
            again,
            hard,
            good,
            easy,
        },
        undos: summary.undos,
        restores: summary.restores,
        restore_failures: summary.restore_failures,
        targets: summary.targets.iter().map(target_dto).collect(),
    }
}

/// The local metrics summary (nothing while not opted in).
#[tauri::command]
pub async fn get_usage_metrics(backend: State<'_, Backend>) -> Result<UsageMetricsDto, ErrorDto> {
    let store = backend.collection()?.telemetry.clone();
    blocking(move || {
        let metrics = store.metrics().map_err(app_error)?;
        Ok(match metrics {
            Some(metrics) => UsageMetricsDto {
                enabled: true,
                install_id: Some(metrics.install_id),
                summary: Some(summary_dto(&metrics.summary)),
            },
            None => UsageMetricsDto {
                enabled: false,
                install_id: None,
                summary: None,
            },
        })
    })
    .await
}

#[cfg(test)]
mod tests {
    use langualinker_application::telemetry::summarize;

    use super::*;

    #[test]
    fn summary_dto_carries_numbers_and_codes_only() {
        let summary = summarize(
            &[],
            TimestampMs(1_772_438_400_000),
            TimestampMs(1_772_438_400_000),
        );
        let json = serde_json::to_value(summary_dto(&summary)).unwrap();
        assert_eq!(json["since"], "2026-03-02T08:00:00.000Z");
        assert_eq!(json["ratings"]["good"], 0);
        assert_eq!(json["targets"][0]["metric"], "retainedD7");
        assert_eq!(json["targets"][0]["status"], "cohortOnly");
        assert_eq!(json["targets"][1]["target"], "< 2000 ms");
    }

    #[test]
    fn client_error_input_accepts_kinds_only() {
        let ok: ClientErrorInput =
            serde_json::from_value(serde_json::json!({ "kind": "unhandledRejection" })).unwrap();
        assert_eq!(ok.kind, ClientErrorKind::UnhandledRejection);
        // A message never crosses IPC.
        assert!(
            serde_json::from_value::<ClientErrorInput>(
                serde_json::json!({ "kind": "error", "message": "der Hund" })
            )
            .is_err()
        );
    }
}
