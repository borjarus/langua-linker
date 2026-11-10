//! Settings (plan Phase 12 steps 6–7).
//!
//! The settings screens edit one versioned document (`settings` key). Three values live where the
//! rest of the app already reads them: the daily goal (`dailyGoal`, dashboard and onboarding) and
//! the study-day calendar (IANA zone and cutoff in `collection`, the scheduler). An update writes
//! all of them in one transaction and bumps the document version once, so a stale form gets
//! `VERSION_CONFLICT` instead of overwriting a newer change.
//!
//! Review logs are UTC instants: changing the zone or the cutoff re-groups them into study days,
//! it never rewrites history. Consents (AI, telemetry) are recorded with the version of the text
//! the user agreed to and the time; withdrawing AI consent blocks queued requests in the same
//! transaction, before any of them can reach the network.

use langualinker_domain::calendar::StudyCalendar;
use langualinker_domain::deck::TargetLanguage;
use langualinker_domain::{DeckConfigId, TimestampMs, ValidationError};
use serde::{Deserialize, Serialize};

use crate::content::{DAILY_GOAL_KEY, DEFAULT_DAILY_GOAL, onboarding_target_languages};
use crate::repositories::Repositories;
use crate::{AppError, AppResult};

/// Settings key of the settings document.
pub const SETTINGS_KEY: &str = "settings";
/// Version of the AI and telemetry consent texts; a new text asks again.
pub const CONSENT_VERSION: u32 = 1;
/// Secret name of the AI provider key in the platform store (ADR-07).
pub const AI_KEY_SECRET: &str = "ai.apiKey";

pub const MIN_DAILY_GOAL: u32 = 1;
pub const MAX_DAILY_GOAL: u32 = 500;
pub const MIN_BACKUP_RETENTION: u32 = 1;
pub const MAX_BACKUP_RETENTION: u32 = 50;
pub const DEFAULT_BACKUP_RETENTION: u32 = 3;
const MAX_MODEL_CHARS: usize = 100;
const MAX_ENDPOINT_CHARS: usize = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UiLanguage {
    #[default]
    Pl,
    En,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Consent {
    #[default]
    Unset,
    Granted,
    Declined,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BackupTarget {
    #[default]
    App,
    External,
}

/// An AI provider the settings offer (the client arrives in Phase 13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiProvider {
    pub id: &'static str,
    pub name: &'static str,
    pub requires_key: bool,
    pub models: &'static [&'static str],
    pub custom_endpoint: bool,
}

pub const AI_PROVIDERS: &[AiProvider] = &[
    AiProvider {
        id: "anthropic",
        name: "Anthropic",
        requires_key: true,
        models: &["claude-haiku-5-5", "claude-sonnet-5-5"],
        custom_endpoint: false,
    },
    AiProvider {
        id: "gemini",
        name: "Google Gemini",
        requires_key: true,
        models: &[
            "gemini-3.8-flash",
            "gemini-3.7-flash",
            "gemini-3-flash-preview",
        ],
        custom_endpoint: false,
    },
    AiProvider {
        id: "openaiCompatible",
        name: "OpenAI-compatible",
        requires_key: true,
        models: &[],
        custom_endpoint: true,
    },
];

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AiSettings {
    pub consent: Consent,
    /// Consent text version the answer applies to; an older one counts as unset.
    pub consent_version: Option<u32>,
    pub consent_at: Option<i64>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub endpoint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BackupSettings {
    pub automatic: bool,
    pub retention: u32,
    pub target: BackupTarget,
    /// Display name of the external folder (never shown as a path).
    pub external_folder: Option<String>,
    /// Native path of the external folder; Rust-side only.
    pub external_path: Option<String>,
}

impl Default for BackupSettings {
    fn default() -> Self {
        Self {
            automatic: true,
            retention: DEFAULT_BACKUP_RETENTION,
            target: BackupTarget::App,
            external_folder: None,
            external_path: None,
        }
    }
}

/// The stored settings document. Missing fields take their defaults, so older documents load.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SettingsDoc {
    pub ui_language: UiLanguage,
    /// Code (`de`, `en`, `es`); `None` follows the onboarding choice.
    pub default_target_language: Option<String>,
    pub theme: Theme,
    /// `None` = the built-in preset.
    pub default_preset_id: Option<i64>,
    /// `HH:MM`, local time.
    pub reminder_time: Option<String>,
    pub tts_enabled: bool,
    pub tts_rate: f64,
    pub telemetry_opt_in: bool,
    pub telemetry_consent_version: Option<u32>,
    pub telemetry_consent_at: Option<i64>,
    pub ai: AiSettings,
    pub backup: BackupSettings,
    pub version: u32,
}

impl Default for SettingsDoc {
    fn default() -> Self {
        Self {
            ui_language: UiLanguage::Pl,
            default_target_language: None,
            theme: Theme::System,
            default_preset_id: None,
            reminder_time: None,
            tts_enabled: true,
            tts_rate: 1.0,
            telemetry_opt_in: false,
            telemetry_consent_version: None,
            telemetry_consent_at: None,
            ai: AiSettings::default(),
            backup: BackupSettings::default(),
            version: 1,
        }
    }
}

/// Settings as the screens show them: the document plus the values stored elsewhere.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub doc: SettingsDoc,
    pub default_target_language: TargetLanguage,
    pub daily_goal: u32,
    pub time_zone: String,
    pub day_cutoff_hour: u32,
    pub default_preset_id: DeckConfigId,
    /// AI consent as it applies now (an answer to an older text counts as unset).
    pub ai_consent: Consent,
}

/// A change to the settings; `None` keeps the value. Inner `Option`s clear a value.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SettingsChange {
    pub ui_language: Option<UiLanguage>,
    pub default_target_language: Option<TargetLanguage>,
    pub theme: Option<Theme>,
    pub daily_goal: Option<u32>,
    pub time_zone: Option<String>,
    pub day_cutoff_hour: Option<u32>,
    pub default_preset_id: Option<DeckConfigId>,
    pub reminder_time: Option<Option<String>>,
    pub tts_enabled: Option<bool>,
    pub tts_rate: Option<f64>,
    pub telemetry_opt_in: Option<bool>,
    pub ai_consent: Option<Consent>,
    pub ai_provider: Option<String>,
    pub ai_model: Option<Option<String>>,
    pub ai_endpoint: Option<Option<String>>,
    pub backup_automatic: Option<bool>,
    pub backup_retention: Option<u32>,
    pub backup_target: Option<BackupTarget>,
    /// A folder the user picked: display name and native path.
    pub backup_folder: Option<(String, String)>,
}

/// What an update did besides storing the document.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SettingsEffects {
    /// The provider changed: its stored key belongs to the previous one and is removed.
    pub remove_api_key: bool,
    /// LLM requests blocked because AI consent was withdrawn.
    pub blocked_requests: u64,
    /// Zone or cutoff changed: today's counts change.
    pub calendar_changed: bool,
}

pub fn load_doc(tx: &mut dyn Repositories) -> AppResult<SettingsDoc> {
    match tx.setting(SETTINGS_KEY)? {
        Some(json) => serde_json::from_str(&json)
            .map_err(|_| AppError::Internal("settings document is malformed".into())),
        None => Ok(SettingsDoc::default()),
    }
}

fn store_doc(tx: &mut dyn Repositories, doc: &SettingsDoc, at: TimestampMs) -> AppResult<()> {
    let json = serde_json::to_string(doc)
        .map_err(|_| AppError::Internal("settings document not serializable".into()))?;
    tx.put_setting(SETTINGS_KEY, &json, at)
}

fn target_language(code: &str) -> Option<TargetLanguage> {
    TargetLanguage::ALL
        .iter()
        .copied()
        .find(|language| language.code() == code)
}

/// The preset new decks get: the chosen one while it exists, otherwise the built-in one.
pub fn default_preset(tx: &mut dyn Repositories) -> AppResult<DeckConfigId> {
    let doc = load_doc(tx)?;
    if let Some(id) = doc.default_preset_id
        && let Some(config) = tx.config(DeckConfigId(id))?
        && config.deleted_at.is_none()
    {
        return Ok(config.id);
    }
    Ok(tx.default_config()?.id)
}

pub fn read(tx: &mut dyn Repositories) -> AppResult<Settings> {
    let doc = load_doc(tx)?;
    let default_target_language = match doc.default_target_language.as_deref() {
        Some(code) => target_language(code),
        None => onboarding_target_languages(tx)?
            .first()
            .and_then(|code| target_language(code)),
    }
    .unwrap_or(TargetLanguage::De);
    let daily_goal = tx
        .setting(DAILY_GOAL_KEY)?
        .and_then(|json| serde_json::from_str::<u32>(&json).ok())
        .unwrap_or(DEFAULT_DAILY_GOAL);
    let (time_zone, cutoff_minutes) = tx.collection_calendar()?;
    let ai_consent = if doc.ai.consent_version == Some(CONSENT_VERSION) {
        doc.ai.consent
    } else {
        Consent::Unset
    };
    Ok(Settings {
        default_target_language,
        daily_goal,
        time_zone,
        day_cutoff_hour: cutoff_minutes / 60,
        default_preset_id: default_preset(tx)?,
        ai_consent,
        doc,
    })
}

/// Adopts the device time zone on the first start (the schema default is UTC). Later starts, and
/// a zone the user chose, are left alone. Returns whether the zone was set.
pub fn initialize(
    tx: &mut dyn Repositories,
    device_zone: &str,
    at: TimestampMs,
) -> AppResult<bool> {
    if tx.setting(SETTINGS_KEY)?.is_some() {
        return Ok(false);
    }
    let (zone, cutoff) = tx.collection_calendar()?;
    let adopt =
        zone == "UTC" && device_zone != "UTC" && StudyCalendar::new(device_zone, cutoff).is_ok();
    if adopt {
        tx.set_collection_calendar(device_zone, cutoff)?;
    }
    store_doc(tx, &SettingsDoc::default(), at)?;
    Ok(adopt)
}

fn invalid(field: &'static str, reason: &'static str) -> AppError {
    ValidationError::new(field, reason).into()
}

fn in_range(field: &'static str, value: u32, min: u32, max: u32) -> AppResult<()> {
    if (min..=max).contains(&value) {
        Ok(())
    } else {
        Err(invalid(field, "range"))
    }
}

/// `HH:MM` with a 24-hour clock.
fn valid_time(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 5
        && bytes[2] == b':'
        && text[..2].parse::<u32>().is_ok_and(|hour| hour < 24)
        && text[3..].parse::<u32>().is_ok_and(|minute| minute < 60)
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 2 || byte.is_ascii_digit())
}

/// An HTTPS URL with a host and no credentials, spaces or control characters.
fn valid_endpoint(text: &str) -> bool {
    let Some(rest) = text.strip_prefix("https://") else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    text.chars().count() <= MAX_ENDPOINT_CHARS
        && !host.is_empty()
        && !host.contains('@')
        && !text.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// Applies a change at the version the form read. The document version goes up by one.
pub fn update(
    tx: &mut dyn Repositories,
    expected_version: u32,
    change: SettingsChange,
    at: TimestampMs,
) -> AppResult<SettingsEffects> {
    let mut doc = load_doc(tx)?;
    if doc.version != expected_version {
        return Err(AppError::VersionConflict {
            entity: "settings",
            current: i64::from(doc.version),
        });
    }
    let mut effects = SettingsEffects::default();

    if let Some(language) = change.ui_language {
        doc.ui_language = language;
    }
    if let Some(language) = change.default_target_language {
        doc.default_target_language = Some(language.code().to_owned());
    }
    if let Some(theme) = change.theme {
        doc.theme = theme;
    }
    if let Some(goal) = change.daily_goal {
        in_range("dailyGoal", goal, MIN_DAILY_GOAL, MAX_DAILY_GOAL)?;
        tx.put_setting(DAILY_GOAL_KEY, &goal.to_string(), at)?;
    }
    if change.time_zone.is_some() || change.day_cutoff_hour.is_some() {
        let (zone, cutoff) = tx.collection_calendar()?;
        let new_zone = change.time_zone.unwrap_or_else(|| zone.clone());
        let new_cutoff = match change.day_cutoff_hour {
            Some(hour) => {
                in_range("dayCutoffHour", hour, 0, 23)?;
                hour * 60
            }
            None => cutoff,
        };
        StudyCalendar::new(&new_zone, new_cutoff).map_err(|_| invalid("timeZone", "unknown"))?;
        if (new_zone.as_str(), new_cutoff) != (zone.as_str(), cutoff) {
            tx.set_collection_calendar(&new_zone, new_cutoff)?;
            effects.calendar_changed = true;
        }
    }
    if let Some(id) = change.default_preset_id {
        match tx.config(id)? {
            Some(config) if config.deleted_at.is_none() => doc.default_preset_id = Some(id.0),
            _ => return Err(invalid("defaultPresetId", "notFound")),
        }
    }
    if let Some(reminder) = change.reminder_time {
        if let Some(time) = &reminder
            && !valid_time(time)
        {
            return Err(invalid("reminderTime", "format"));
        }
        doc.reminder_time = reminder;
    }
    if let Some(enabled) = change.tts_enabled {
        doc.tts_enabled = enabled;
    }
    if let Some(rate) = change.tts_rate {
        if !rate.is_finite() || !(0.5..=1.5).contains(&rate) {
            return Err(invalid("ttsRate", "range"));
        }
        doc.tts_rate = rate;
    }
    if let Some(opt_in) = change.telemetry_opt_in
        && opt_in != doc.telemetry_opt_in
    {
        doc.telemetry_opt_in = opt_in;
        doc.telemetry_consent_version = Some(CONSENT_VERSION);
        doc.telemetry_consent_at = Some(at.0);
    }

    if let Some(consent) = change.ai_consent {
        doc.ai.consent = consent;
        doc.ai.consent_version = Some(CONSENT_VERSION);
        doc.ai.consent_at = Some(at.0);
        if consent != Consent::Granted {
            effects.blocked_requests = tx.block_pending_llm_requests(at)?;
        }
    }
    if let Some(id) = change.ai_provider {
        let provider = AI_PROVIDERS
            .iter()
            .find(|provider| provider.id == id)
            .ok_or_else(|| invalid("aiProvider", "unknown"))?;
        if doc.ai.provider.as_deref() != Some(provider.id) {
            // Another provider starts from its first suggested model; a key belongs to one
            // provider.
            doc.ai.provider = Some(provider.id.to_owned());
            doc.ai.model = provider.models.first().map(|model| (*model).to_owned());
            if !provider.custom_endpoint {
                doc.ai.endpoint = None;
            }
            effects.remove_api_key = true;
        }
    }
    if let Some(model) = change.ai_model {
        let model = model
            .map(|text| text.trim().to_owned())
            .filter(|text| !text.is_empty());
        if model.as_ref().is_some_and(|text| {
            text.chars().count() > MAX_MODEL_CHARS || text.chars().any(char::is_control)
        }) {
            return Err(invalid("aiModel", "tooLong"));
        }
        doc.ai.model = model;
    }
    if let Some(endpoint) = change.ai_endpoint {
        let endpoint = endpoint
            .map(|text| text.trim().to_owned())
            .filter(|text| !text.is_empty());
        if endpoint
            .as_deref()
            .is_some_and(|text| !valid_endpoint(text))
        {
            return Err(invalid("aiEndpoint", "https"));
        }
        doc.ai.endpoint = endpoint;
    }

    if let Some(automatic) = change.backup_automatic {
        doc.backup.automatic = automatic;
    }
    if let Some(retention) = change.backup_retention {
        in_range(
            "backupRetention",
            retention,
            MIN_BACKUP_RETENTION,
            MAX_BACKUP_RETENTION,
        )?;
        doc.backup.retention = retention;
    }
    if let Some((name, path)) = change.backup_folder {
        doc.backup.external_folder = Some(name);
        doc.backup.external_path = Some(path);
        doc.backup.target = BackupTarget::External;
    }
    if let Some(target) = change.backup_target {
        if target == BackupTarget::External && doc.backup.external_path.is_none() {
            return Err(invalid("backupTarget", "noFolder"));
        }
        doc.backup.target = target;
    }

    doc.version += 1;
    store_doc(tx, &doc, at)?;
    Ok(effects)
}

/// The AI key was stored or removed (the key itself lives in the platform store): the document
/// version goes up so an open form re-reads `hasApiKey` instead of overwriting a newer state.
pub fn api_key_changed(tx: &mut dyn Repositories, at: TimestampMs) -> AppResult<()> {
    let mut doc = load_doc(tx)?;
    doc.version += 1;
    store_doc(tx, &doc, at)
}

/// Phrase typed to confirm a reset without a fresh backup, in the UI language.
pub fn confirmation_phrase(language: UiLanguage, everything: bool) -> &'static str {
    match (language, everything) {
        (UiLanguage::Pl, false) => "RESETUJ POSTĘP",
        (UiLanguage::Pl, true) => "USUŃ WSZYSTKO",
        (UiLanguage::En, false) => "RESET PROGRESS",
        (UiLanguage::En, true) => "DELETE EVERYTHING",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reminder_times_use_a_24_hour_clock() {
        for good in ["00:00", "07:30", "23:59"] {
            assert!(valid_time(good), "{good}");
        }
        for bad in ["24:00", "7:30", "07:60", "07-30", "0a:30", ""] {
            assert!(!valid_time(bad), "{bad}");
        }
    }

    #[test]
    fn endpoints_must_be_https_without_credentials() {
        assert!(valid_endpoint("https://llm.example.com/v1"));
        assert!(!valid_endpoint("http://llm.example.com/v1"));
        assert!(!valid_endpoint("https://user:pw@llm.example.com"));
        assert!(!valid_endpoint("https:///path"));
        assert!(!valid_endpoint("https://a b"));
    }

    #[test]
    fn older_documents_load_with_defaults() {
        let doc: SettingsDoc = serde_json::from_str(r#"{"theme":"dark","version":4}"#).unwrap();
        assert_eq!(doc.theme, Theme::Dark);
        assert_eq!(doc.version, 4);
        assert_eq!(doc.backup.retention, DEFAULT_BACKUP_RETENTION);
        assert!(doc.tts_enabled);
    }

    #[test]
    fn confirmation_phrases_follow_the_ui_language() {
        assert_eq!(confirmation_phrase(UiLanguage::Pl, false), "RESETUJ POSTĘP");
        assert_eq!(
            confirmation_phrase(UiLanguage::En, true),
            "DELETE EVERYTHING"
        );
    }
}
