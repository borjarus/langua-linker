//! Statistics, settings, reset, backup and restore commands (plan Phase 12), mapped to contract
//! v1 DTOs. Mutations are idempotent by `operationId`; long work (backup, restore, reset) runs as
//! a durable job. A committed change emits `collection_changed` as a read signal.

use std::sync::Arc;

use langualinker_application::AppError;
use langualinker_application::llm;
use langualinker_application::ports::{CollectionScope, SecretStore};
use langualinker_application::repositories::{CollectionCounts, RecordedOperation, UnitOfWork};
use langualinker_application::settings::{
    self as app_settings, AI_KEY_SECRET, AI_PROVIDERS, BackupTarget, Consent, Settings,
    SettingsChange, Theme,
};
use langualinker_application::stats::{self, Stats};
use langualinker_contracts::ErrorDto;
use langualinker_contracts::dto::{
    AiProviderOptionDto, AiQuotaDto, AiSettingsDto, BackupDto, BackupInput, BackupKind,
    BackupLocation, BackupSettingsDto, CardStatesDto, CollectionCountsDto, ConsentState,
    DifficultyStabilityPointDto, ForecastAssumptionsDto, ForecastDayDto, JobDto, MaturityDto,
    NativeLanguage, PermissionState, RatioDto, ResetInput, ResetPreviewDto, ResetPreviewInput,
    ResetScope, RestoreInput, RestorePreviewDto, RestoreSource, ReviewDayDto, SettingsDto,
    ShareFileInput, ShareResultDto, ShareTarget, StabilityBucketDto, StatsDto, StatsInput,
    StatsRange, ThemePreference, TrueRetentionDto, UiLanguage, UpdateSettingsInput,
    ValidateRestoreInput,
};
use langualinker_domain::{DeckConfigId, DeckId, TimestampMs};
use langualinker_infrastructure::backup::{self, BackupInfo};
use serde_json::Value;
use tauri::{AppHandle, Runtime, State};

use crate::backend::{Backend, iso, job_dto};
use crate::commands::{blocking, operation_id, parse_input};
use crate::content::{domain_target_language, target_language};
use crate::files::PickedFiles;
use crate::study::{app_error, emit, id, text_id};

const OP_UPDATE_SETTINGS: &str = "updateSettings";

// ---------------------------------------------------------------- mapping

fn ratio_dto(ratio: stats::Ratio) -> RatioDto {
    RatioDto {
        passed: ratio.passed,
        total: ratio.total,
    }
}

fn clamp_ms(value: i64) -> u32 {
    u32::try_from(value.max(0)).unwrap_or(u32::MAX)
}

fn stats_dto(range: StatsRange, deck_id: Option<String>, stats: Stats) -> StatsDto {
    StatsDto {
        range,
        deck_id,
        reviews_by_day: stats
            .reviews_by_day
            .iter()
            .map(|day| ReviewDayDto {
                day: day.day.iso_date(),
                count: day.count,
                active_ms: clamp_ms(day.active_ms),
            })
            .collect(),
        forecast: stats
            .forecast
            .iter()
            .map(|day| ForecastDayDto {
                day: day.day.iso_date(),
                stored: day.stored,
                projected: day.projected,
            })
            .collect(),
        forecast_assumptions: ForecastAssumptionsDto {
            reviews_per_day: stats.reviews_per_day,
            new_per_day: stats.new_per_day,
            includes_new: false,
        },
        overdue: stats.overdue,
        card_states: CardStatesDto {
            new: stats.states.new,
            learning: stats.states.learning,
            review: stats.states.review,
            relearning: stats.states.relearning,
            suspended: stats.states.suspended,
        },
        maturity: MaturityDto {
            young: stats.young,
            mature: stats.mature,
        },
        mature_threshold_days: stats::MATURE_DAYS as u32,
        true_retention: TrueRetentionDto {
            young: stats.retention_young.map(ratio_dto),
            mature: stats.retention_mature.map(ratio_dto),
        },
        stability: stats
            .stability
            .iter()
            .map(|bucket| StabilityBucketDto {
                min_days: bucket.min_days,
                max_days: bucket.max_days,
                count: bucket.count,
            })
            .collect(),
        difficulty_stability: stats
            .memory
            .iter()
            .map(|point| DifficultyStabilityPointDto {
                card_id: text_id(point.card_id.0),
                stability: point.stability,
                difficulty: point.difficulty,
            })
            .collect(),
        streak_days: stats.streak_days,
        longest_streak_days: stats.longest_streak_days,
        generated_at: iso(stats.generated_at),
    }
}

fn ui_language_dto(language: app_settings::UiLanguage) -> UiLanguage {
    match language {
        app_settings::UiLanguage::Pl => UiLanguage::Pl,
        app_settings::UiLanguage::En => UiLanguage::En,
    }
}

fn domain_ui_language(language: UiLanguage) -> app_settings::UiLanguage {
    match language {
        UiLanguage::Pl => app_settings::UiLanguage::Pl,
        UiLanguage::En => app_settings::UiLanguage::En,
    }
}

fn theme_dto(theme: Theme) -> ThemePreference {
    match theme {
        Theme::System => ThemePreference::System,
        Theme::Light => ThemePreference::Light,
        Theme::Dark => ThemePreference::Dark,
    }
}

fn domain_theme(theme: ThemePreference) -> Theme {
    match theme {
        ThemePreference::System => Theme::System,
        ThemePreference::Light => Theme::Light,
        ThemePreference::Dark => Theme::Dark,
    }
}

fn consent_dto(consent: Consent) -> ConsentState {
    match consent {
        Consent::Unset => ConsentState::Unset,
        Consent::Granted => ConsentState::Granted,
        Consent::Declined => ConsentState::Declined,
    }
}

fn domain_consent(consent: ConsentState) -> Consent {
    match consent {
        ConsentState::Unset => Consent::Unset,
        ConsentState::Granted => Consent::Granted,
        ConsentState::Declined => Consent::Declined,
    }
}

fn target_dto(target: BackupTarget) -> BackupLocation {
    match target {
        BackupTarget::App => BackupLocation::App,
        BackupTarget::External => BackupLocation::External,
    }
}

pub(crate) fn has_api_key(secrets: Option<&Arc<dyn SecretStore>>) -> bool {
    secrets.is_some_and(|store| store.has(AI_KEY_SECRET).unwrap_or(false))
}

/// The settings with the AI key presence and today's local AI call count.
pub(crate) fn read_settings(
    db: &langualinker_infrastructure::sqlite::SqliteDatabase,
    secrets: Option<&Arc<dyn SecretStore>>,
    now: TimestampMs,
) -> Result<SettingsDto, ErrorDto> {
    let (settings, quota) = db
        .read(move |tx| Ok((app_settings::read(tx)?, llm::quota(tx, now)?)))
        .map_err(app_error)?;
    Ok(settings_dto(settings, has_api_key(secrets), quota))
}

fn settings_dto(settings: Settings, has_key: bool, quota: llm::Quota) -> SettingsDto {
    let doc = settings.doc;
    SettingsDto {
        ui_language: ui_language_dto(doc.ui_language),
        native_language: NativeLanguage::Pl,
        default_target_language: target_language(settings.default_target_language),
        theme: theme_dto(doc.theme),
        daily_goal: settings.daily_goal,
        time_zone: settings.time_zone,
        day_cutoff_hour: settings.day_cutoff_hour,
        default_preset_id: text_id(settings.default_preset_id.0),
        // Local notifications arrive in Phase 15; until then the screen offers no prompt.
        notifications: PermissionState::Unsupported,
        reminder_time: doc.reminder_time,
        tts_enabled: doc.tts_enabled,
        tts_rate: doc.tts_rate,
        ai: AiSettingsDto {
            consent: consent_dto(settings.ai_consent),
            provider: doc.ai.provider,
            model: doc.ai.model,
            endpoint: doc.ai.endpoint,
            has_api_key: has_key,
            providers: AI_PROVIDERS
                .iter()
                .map(|provider| AiProviderOptionDto {
                    id: provider.id.to_owned(),
                    name: provider.name.to_owned(),
                    requires_key: provider.requires_key,
                    models: provider.models.iter().map(|m| (*m).to_owned()).collect(),
                    custom_endpoint: provider.custom_endpoint,
                })
                .collect(),
            // The local daily call limit (cost guard, Phase 13); BYOK providers report none.
            quota: Some(AiQuotaDto {
                used: quota.used,
                limit: quota.limit,
                resets_at: iso(quota.resets_at),
            }),
        },
        telemetry_opt_in: doc.telemetry_opt_in,
        backup: BackupSettingsDto {
            automatic: doc.backup.automatic,
            retention: doc.backup.retention,
            target: target_dto(doc.backup.target),
            external_folder: doc.backup.external_folder,
        },
        version: doc.version,
    }
}

fn counts_dto(counts: CollectionCounts) -> CollectionCountsDto {
    CollectionCountsDto {
        decks: counts.decks,
        notes: counts.notes,
        cards: counts.cards,
        reviews: counts.reviews,
    }
}

fn backup_dto(backup: BackupInfo) -> BackupDto {
    BackupDto {
        id: backup.id,
        created_at: iso(TimestampMs(backup.manifest.created_at)),
        size_bytes: backup.size_bytes,
        schema_version: u32::try_from(backup.manifest.schema_version).unwrap_or(0),
        card_count: backup.manifest.counts.cards,
        location: match backup.location {
            backup::BackupLocation::App => BackupLocation::App,
            backup::BackupLocation::External => BackupLocation::External,
        },
        kind: match backup.manifest.kind {
            backup::BackupKind::Manual => BackupKind::Manual,
            backup::BackupKind::Automatic => BackupKind::Automatic,
            backup::BackupKind::Safety => BackupKind::Safety,
        },
    }
}

fn job(
    record: Result<langualinker_application::jobs::JobRecord, AppError>,
) -> Result<JobDto, ErrorDto> {
    record.map_err(app_error).and_then(|job| job_dto(&job))
}

// ---------------------------------------------------------------- statistics

#[tauri::command]
pub async fn get_stats(backend: State<'_, Backend>, input: Value) -> Result<StatsDto, ErrorDto> {
    let input: StatsInput = parse_input(input)?;
    let deck = input
        .deck_id
        .as_deref()
        .map(|value| id("deckId", value).map(DeckId))
        .transpose()?;
    let range = match input.range {
        StatsRange::OneMonth => stats::StatsRange::Month,
        StatsRange::ThreeMonths => stats::StatsRange::Quarter,
        StatsRange::OneYear => stats::StatsRange::Year,
        StatsRange::All => stats::StatsRange::All,
    };
    let collection = backend.collection()?;
    let (db, now) = (collection.db.clone(), collection.clock.now());
    let result = blocking(move || {
        db.read(move |tx| stats::stats(tx, now, deck, range))
            .map_err(app_error)
    })
    .await?;
    Ok(stats_dto(input.range, input.deck_id, result))
}

// ---------------------------------------------------------------- settings

#[tauri::command]
pub async fn get_settings(backend: State<'_, Backend>) -> Result<SettingsDto, ErrorDto> {
    let collection = backend.collection()?;
    let (db, secrets) = (collection.db.clone(), collection.secrets.clone());
    let now = collection.clock.now();
    blocking(move || read_settings(&db, secrets.as_ref(), now)).await
}

#[tauri::command]
pub async fn update_settings(
    backend: State<'_, Backend>,
    files: State<'_, PickedFiles>,
    input: Value,
) -> Result<SettingsDto, ErrorDto> {
    let input: UpdateSettingsInput = parse_input(input)?;
    operation_id(&input.operation_id)?;
    let patch = input.patch;
    let backup_folder = match patch.backup_folder_token.as_deref() {
        Some(token) => {
            let folder = files
                .get(token)
                .and_then(|file| Some((file.name, file.path?)))
                .ok_or_else(|| app_error(AppError::NotFound { entity: "folder" }))?;
            let path = folder
                .1
                .to_str()
                .ok_or_else(|| {
                    app_error(AppError::PermissionDenied {
                        reason: "externalFolder",
                    })
                })?
                .to_owned();
            Some((folder.0, path))
        }
        None => None,
    };
    let change = SettingsChange {
        ui_language: patch.ui_language.map(domain_ui_language),
        default_target_language: patch.default_target_language.map(domain_target_language),
        theme: patch.theme.map(domain_theme),
        daily_goal: patch.daily_goal,
        time_zone: patch.time_zone,
        day_cutoff_hour: patch.day_cutoff_hour,
        default_preset_id: patch
            .default_preset_id
            .as_deref()
            .map(|value| id("defaultPresetId", value).map(DeckConfigId))
            .transpose()?,
        reminder_time: patch.reminder_time,
        tts_enabled: patch.tts_enabled,
        tts_rate: patch.tts_rate,
        telemetry_opt_in: patch.telemetry_opt_in,
        ai_consent: patch.ai_consent.map(domain_consent),
        ai_provider: patch.ai_provider,
        ai_model: patch.ai_model,
        ai_endpoint: patch.ai_endpoint,
        backup_automatic: patch.backup_automatic,
        backup_retention: patch.backup_retention,
        backup_target: patch.backup_target.map(|target| match target {
            BackupLocation::App => BackupTarget::App,
            BackupLocation::External => BackupTarget::External,
        }),
        backup_folder,
    };
    let collection = backend.collection()?;
    let (db, now) = (collection.db.clone(), collection.clock.now());
    let operation = input.operation_id;
    let expected = input.expected_version;
    let effects = blocking(move || {
        db.write(move |tx| {
            // A retried request returns the current settings without applying the patch again.
            if tx.recorded_operation(&operation)?.is_some() {
                return Ok(None);
            }
            let effects = app_settings::update(tx, expected, change, now)?;
            tx.record_operation(
                &operation,
                &RecordedOperation {
                    operation: OP_UPDATE_SETTINGS.into(),
                    result_json: "null".into(),
                },
                now,
            )?;
            Ok(Some(effects))
        })
        .map_err(app_error)
    })
    .await?;
    if let Some(effects) = effects {
        if effects.remove_api_key
            && let Some(secrets) = collection.secrets.clone()
        {
            let removed = blocking(move || secrets.remove(AI_KEY_SECRET).map_err(app_error)).await;
            if removed.is_err() {
                tracing::warn!("AI key of the previous provider not removed");
            }
        }
        if effects.blocked_requests > 0 {
            tracing::info!(
                count = effects.blocked_requests,
                "AI requests blocked after consent withdrawal"
            );
        }
        let mut scopes = vec![CollectionScope::Settings];
        if effects.calendar_changed {
            scopes.extend([
                CollectionScope::Decks,
                CollectionScope::Cards,
                CollectionScope::Stats,
            ]);
        }
        emit(collection, &scopes, collection.clock.now());
    }
    let collection_db = collection.db.clone();
    let secrets = collection.secrets.clone();
    let now = collection.clock.now();
    blocking(move || read_settings(&collection_db, secrets.as_ref(), now)).await
}

// ---------------------------------------------------------------- reset

#[tauri::command]
pub async fn preview_reset(
    backend: State<'_, Backend>,
    input: Value,
) -> Result<ResetPreviewDto, ErrorDto> {
    let input: ResetPreviewInput = parse_input(input)?;
    let scope = match input.scope {
        ResetScope::Progress => backup::ResetScope::Progress,
        ResetScope::Everything => backup::ResetScope::Everything,
    };
    let backups = backend.collection()?.backups.clone();
    let preview = blocking(move || backups.preview_reset(scope).map_err(app_error)).await?;
    Ok(ResetPreviewDto {
        scope: input.scope,
        counts: counts_dto(preview.counts),
        confirmation_phrase: preview.phrase.to_owned(),
        latest_backup_at: preview.latest_backup_at.map(iso),
        preview_token: preview.token,
    })
}

#[tauri::command]
pub async fn reset_collection(
    backend: State<'_, Backend>,
    input: Value,
) -> Result<JobDto, ErrorDto> {
    let input: ResetInput = parse_input(input)?;
    operation_id(&input.operation_id)?;
    if input
        .safety_backup_id
        .as_deref()
        .is_some_and(|id| !backup::valid_id(id))
    {
        return Err(app_error(
            langualinker_domain::ValidationError::new("safetyBackupId", "format").into(),
        ));
    }
    let backups = backend.collection()?.backups.clone();
    blocking(move || {
        job(backups.reset(
            input.operation_id,
            &input.preview_token,
            input.safety_backup_id,
            input.confirmation_phrase,
        ))
    })
    .await
}

// ---------------------------------------------------------------- backups

#[tauri::command]
pub async fn list_backups(backend: State<'_, Backend>) -> Result<Vec<BackupDto>, ErrorDto> {
    let backups = backend.collection()?.backups.clone();
    blocking(move || {
        backups
            .list()
            .map(|list| list.into_iter().map(backup_dto).collect())
            .map_err(app_error)
    })
    .await
}

#[tauri::command]
pub async fn create_backup(backend: State<'_, Backend>, input: Value) -> Result<JobDto, ErrorDto> {
    let input: BackupInput = parse_input(input)?;
    operation_id(&input.operation_id)?;
    let backups = backend.collection()?.backups.clone();
    blocking(move || job(backups.create(Some(input.operation_id), backup::BackupKind::Manual)))
        .await
}

#[tauri::command]
pub async fn validate_restore(
    backend: State<'_, Backend>,
    files: State<'_, PickedFiles>,
    input: Value,
) -> Result<RestorePreviewDto, ErrorDto> {
    let input: ValidateRestoreInput = parse_input(input)?;
    let picked = match &input.source {
        RestoreSource::File { file_token } => Some(
            files
                .get(file_token)
                .and_then(|file| Some((file.name, file.path?)))
                .ok_or_else(|| app_error(AppError::NotFound { entity: "file" }))?,
        ),
        RestoreSource::Backup { .. } => None,
    };
    let backups = backend.collection()?.backups.clone();
    let preview = blocking(move || {
        match (input.source, picked) {
            (_, Some((name, path))) => backups.validate_file(&name, &path),
            (RestoreSource::Backup { backup_id }, None) => backups.validate_backup(&backup_id),
            (RestoreSource::File { .. }, None) => Err(AppError::NotFound { entity: "file" }),
        }
        .map_err(app_error)
    })
    .await?;
    Ok(RestorePreviewDto {
        file_name: preview.file_name,
        created_at: iso(preview.created_at),
        schema_version: u32::try_from(preview.schema_version).unwrap_or(0),
        counts: counts_dto(preview.counts),
        current: counts_dto(preview.current),
        compatible: true,
        issues: Vec::new(),
        preview_token: preview.token,
    })
}

#[tauri::command]
pub async fn restore_backup(backend: State<'_, Backend>, input: Value) -> Result<JobDto, ErrorDto> {
    let input: RestoreInput = parse_input(input)?;
    operation_id(&input.operation_id)?;
    let backups = backend.collection()?.backups.clone();
    blocking(move || job(backups.restore(input.operation_id, &input.preview_token))).await
}

/// Saves a copy of a backup outside the app sandbox through the system save dialog (plan Phase
/// 12 step 13). Exports arrive with CSV export in Phase 14; until then no export job exists.
#[tauri::command]
pub async fn share_file<R: Runtime>(
    app: AppHandle<R>,
    backend: State<'_, Backend>,
    input: Value,
) -> Result<ShareResultDto, ErrorDto> {
    let input: ShareFileInput = parse_input(input)?;
    operation_id(&input.operation_id)?;
    let backup_id = match input.target {
        ShareTarget::Backup { backup_id } => backup_id,
        ShareTarget::Export { .. } => {
            return Err(app_error(AppError::NotFound { entity: "export" }));
        }
    };
    let backups = backend.collection()?.backups.clone();
    let info = blocking(move || backups.find(&backup_id).map_err(app_error)).await?;
    let file_name = format!("langualinker-{}.{}", info.id, backup::BACKUP_EXTENSION);
    let shared =
        blocking(move || crate::files::save_copy(&app, &info.path, &file_name).map_err(app_error))
            .await?;
    Ok(ShareResultDto { shared })
}

/// Starts the automatic backup when it is due (app start and return to the foreground).
pub fn automatic_backup(backend: &Backend) {
    let Ok(collection) = backend.collection() else {
        return;
    };
    let backups = collection.backups.clone();
    tauri::async_runtime::spawn_blocking(move || match backups.start_automatic() {
        Ok(Some(job)) => tracing::info!(job_id = %job.id, "automatic backup started"),
        Ok(None) => {}
        Err(error) => tracing::warn!(code = error.code(), "automatic backup not started"),
    });
}
