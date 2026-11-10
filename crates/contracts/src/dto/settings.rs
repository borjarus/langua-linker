//! Statistics, settings, reset, backup and restore DTOs (plan Phase 12). Re-exported from
//! [`crate::dto`].

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{NativeLanguage, TargetLanguage, double_option};
use crate::ErrorDto;

// ---------------------------------------------------------------- statistics

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum StatsRange {
    #[serde(rename = "1m")]
    OneMonth,
    #[serde(rename = "3m")]
    ThreeMonths,
    #[serde(rename = "1y")]
    OneYear,
    #[serde(rename = "all")]
    All,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StatsInput {
    /// `null` = the whole collection; a deck includes its subdecks.
    pub deck_id: Option<String>,
    pub range: StatsRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RatioDto {
    pub passed: u32,
    pub total: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewDayDto {
    pub day: String,
    pub count: u32,
    pub active_ms: u32,
}

/// One study day of the due forecast.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForecastDayDto {
    pub day: String,
    /// Cards whose stored due date falls on this day (day 0 also holds the overdue ones).
    pub stored: u32,
    /// Reviews expected on this day once the deck limits carry the backlog forward.
    pub projected: u32,
}

/// What the projected forecast assumes; shown next to the chart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForecastAssumptionsDto {
    pub reviews_per_day: u32,
    pub new_per_day: u32,
    /// New cards are not part of the forecast: they are introduced by the user's daily limit.
    pub includes_new: bool,
}

/// Cards per state; suspended cards are counted only under `Suspended`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct CardStatesDto {
    pub new: u32,
    pub learning: u32,
    pub review: u32,
    pub relearning: u32,
    pub suspended: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MaturityDto {
    pub young: u32,
    pub mature: u32,
}

/// `null` = not enough data; never rendered as 0%.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrueRetentionDto {
    pub young: Option<RatioDto>,
    pub mature: Option<RatioDto>,
}

/// Stability histogram bucket in days, `[minDays, maxDays)`; `maxDays: null` is open-ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StabilityBucketDto {
    pub min_days: u32,
    pub max_days: Option<u32>,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DifficultyStabilityPointDto {
    pub card_id: String,
    /// Days.
    pub stability: f64,
    /// FSRS difficulty, 1–10.
    pub difficulty: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StatsDto {
    pub range: StatsRange,
    pub deck_id: Option<String>,
    /// Every study day of the range, oldest first, including days without reviews.
    pub reviews_by_day: Vec<ReviewDayDto>,
    pub forecast: Vec<ForecastDayDto>,
    pub forecast_assumptions: ForecastAssumptionsDto,
    /// Cards already past their due day; part of `forecast[0].stored`.
    pub overdue: u32,
    pub card_states: CardStatesDto,
    /// Review cards split by interval: young below `matureThresholdDays`, mature from it.
    pub maturity: MaturityDto,
    pub mature_threshold_days: u32,
    pub true_retention: TrueRetentionDto,
    pub stability: Vec<StabilityBucketDto>,
    /// Studied cards only; the advanced D-vs-S view.
    pub difficulty_stability: Vec<DifficultyStabilityPointDto>,
    pub streak_days: u32,
    pub longest_streak_days: u32,
    pub generated_at: String,
}

// ---------------------------------------------------------------- settings

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum UiLanguage {
    Pl,
    En,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ThemePreference {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum PermissionState {
    Granted,
    Denied,
    Prompt,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ConsentState {
    Unset,
    Granted,
    Declined,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AiProviderOptionDto {
    pub id: String,
    pub name: String,
    pub requires_key: bool,
    /// Suggested models; empty = the user types the model name.
    pub models: Vec<String>,
    /// The user supplies an HTTPS endpoint (OpenAI-compatible servers).
    pub custom_endpoint: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AiQuotaDto {
    pub used: u32,
    pub limit: u32,
    pub resets_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AiSettingsDto {
    pub consent: ConsentState,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub endpoint: Option<String>,
    /// Secrets never cross into JavaScript; only their presence does.
    pub has_api_key: bool,
    pub providers: Vec<AiProviderOptionDto>,
    /// `null` = the provider reports no limit.
    pub quota: Option<AiQuotaDto>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum BackupLocation {
    App,
    External,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackupSettingsDto {
    /// A backup is made automatically once a day after studying.
    pub automatic: bool,
    /// Newest backups kept; older ones are removed (safety copies are kept separately).
    pub retention: u32,
    pub target: BackupLocation,
    /// Display name of the chosen external folder; never a raw path.
    pub external_folder: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettingsDto {
    pub ui_language: UiLanguage,
    pub native_language: NativeLanguage,
    /// Suggested target language for new decks; each deck keeps its own.
    pub default_target_language: TargetLanguage,
    pub theme: ThemePreference,
    pub daily_goal: u32,
    /// IANA zone used to compute the study day.
    pub time_zone: String,
    /// Hour (0–23) at which a new study day starts.
    pub day_cutoff_hour: u32,
    /// Preset given to new decks.
    pub default_preset_id: String,
    pub notifications: PermissionState,
    pub reminder_time: Option<String>,
    pub tts_enabled: bool,
    /// Speech rate, 0.5–1.5.
    pub tts_rate: f64,
    pub ai: AiSettingsDto,
    pub telemetry_opt_in: bool,
    pub backup: BackupSettingsDto,
    pub version: u32,
}

/// Fields an `updateSettings` changes; a missing field keeps its value.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettingsPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ui_language: Option<UiLanguage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub default_target_language: Option<TargetLanguage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub theme: Option<ThemePreference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub daily_goal: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub time_zone: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub day_cutoff_hour: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub default_preset_id: Option<String>,
    /// `null` turns the reminder off.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "double_option"
    )]
    #[ts(optional, as = "Option<Option<String>>")]
    pub reminder_time: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tts_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tts_rate: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub telemetry_opt_in: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ai_consent: Option<ConsentState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ai_provider: Option<String>,
    /// `null` clears the model.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "double_option"
    )]
    #[ts(optional, as = "Option<Option<String>>")]
    pub ai_model: Option<Option<String>>,
    /// HTTPS only; `null` clears it.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "double_option"
    )]
    #[ts(optional, as = "Option<Option<String>>")]
    pub ai_endpoint: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub backup_automatic: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub backup_retention: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub backup_target: Option<BackupLocation>,
    /// File token of a folder chosen with `pickFile({ purpose: 'backupFolder' })`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub backup_folder_token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateSettingsInput {
    pub operation_id: String,
    pub expected_version: u32,
    pub patch: SettingsPatch,
}

// ---------------------------------------------------------------- reset / backup / restore

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ResetScope {
    Progress,
    Everything,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResetPreviewInput {
    pub scope: ResetScope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CollectionCountsDto {
    pub decks: u32,
    pub notes: u32,
    pub cards: u32,
    pub reviews: u32,
}

/// What a reset would remove; the user confirms it with a fresh backup or the phrase.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResetPreviewDto {
    pub scope: ResetScope,
    pub counts: CollectionCountsDto,
    /// Typed by the user when no safety backup is made.
    pub confirmation_phrase: String,
    pub latest_backup_at: Option<String>,
    pub preview_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResetInput {
    pub operation_id: String,
    pub preview_token: String,
    /// A backup created for this reset (job result); either it or the phrase is required.
    pub safety_backup_id: Option<String>,
    pub confirmation_phrase: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum BackupKind {
    Manual,
    Automatic,
    Safety,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackupDto {
    pub id: String,
    pub created_at: String,
    /// Archive size; a JSON number (sizes above 4 GiB stay exact up to 2^53).
    #[ts(type = "number")]
    pub size_bytes: u64,
    pub schema_version: u32,
    pub card_count: u32,
    pub location: BackupLocation,
    pub kind: BackupKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackupInput {
    pub operation_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ShareTarget {
    Backup { backup_id: String },
    Export { job_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShareFileInput {
    pub operation_id: String,
    pub target: ShareTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShareResultDto {
    /// `false` when the user closed the share sheet; not an error.
    pub shared: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RestoreSource {
    File { file_token: String },
    Backup { backup_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValidateRestoreInput {
    pub source: RestoreSource,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestorePreviewDto {
    pub file_name: String,
    pub created_at: String,
    pub schema_version: u32,
    pub counts: CollectionCountsDto,
    /// What the restore replaces.
    pub current: CollectionCountsDto,
    pub compatible: bool,
    pub issues: Vec<ErrorDto>,
    pub preview_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestoreInput {
    pub operation_id: String,
    pub preview_token: String,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn stats_ranges_keep_their_short_names() {
        assert_eq!(json!(StatsRange::OneMonth), json!("1m"));
        assert_eq!(json!(StatsRange::All), json!("all"));
        let input: StatsInput =
            serde_json::from_value(json!({"deckId": null, "range": "3m"})).unwrap();
        assert_eq!(input.range, StatsRange::ThreeMonths);
    }

    #[test]
    fn card_states_are_keyed_like_the_card_state_enum() {
        let states = CardStatesDto {
            new: 1,
            suspended: 2,
            ..CardStatesDto::default()
        };
        let value = json!(states);
        assert_eq!(value["New"], 1);
        assert_eq!(value["Suspended"], 2);
    }

    #[test]
    fn settings_patch_tells_missing_from_null() {
        let cleared: SettingsPatch =
            serde_json::from_value(json!({ "aiEndpoint": null, "dailyGoal": 25 })).unwrap();
        assert_eq!(cleared.ai_endpoint, Some(None));
        assert_eq!(cleared.ai_model, None);
        assert_eq!(cleared.daily_goal, Some(25));
        assert_eq!(
            json!(cleared),
            json!({ "dailyGoal": 25, "aiEndpoint": null })
        );
        assert!(
            serde_json::from_value::<SettingsPatch>(json!({ "nativeLanguage": "pl" })).is_err()
        );
    }

    #[test]
    fn sources_and_share_targets_are_tagged_by_kind() {
        let source: RestoreSource =
            serde_json::from_value(json!({ "kind": "backup", "backupId": "b1" })).unwrap();
        assert_eq!(
            source,
            RestoreSource::Backup {
                backup_id: "b1".into()
            }
        );
        let target: ShareTarget =
            serde_json::from_value(json!({ "kind": "export", "jobId": "7" })).unwrap();
        assert_eq!(target, ShareTarget::Export { job_id: "7".into() });
    }
}
