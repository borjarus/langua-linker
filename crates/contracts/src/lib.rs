//! IPC transport contract shared with `frontend/src/api/types.ts`.
//! Until Phase 7 generates TypeScript from these types, both sides are kept aligned by tests.
//! Contract v1 is frozen in `v1/catalogue.json` and `v1/samples.json` (see `tests/contract_v1.rs`).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Contract version; must equal `API_VERSION` in `frontend/src/api/types.ts`.
pub const API_VERSION: &str = "1.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppInfoDto {
    pub api_version: String,
    pub app_name: String,
    pub app_version: String,
    pub mode: String,
}

/// Result of the SecretStore spike (ADR-07). Dev-variant diagnostics only — not part of `AppApi`.
/// Mirrored by `frontend/src/api/nativeDiagnostics.ts`. Never contains secret values.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretStoreProbeDto {
    /// Store vendor reported by the platform backend, or `none` when it could not be opened.
    pub backend: String,
    /// The marker written by the previous probe run was found, i.e. secrets survive a restart.
    pub persisted_from_previous_run: bool,
    pub steps: Vec<ProbeStepDto>,
}

/// Result of the file open/save spike (ADR-12). Dev-variant diagnostics only — not part of
/// `AppApi`. Mirrored by `frontend/src/api/nativeDiagnostics.ts`. Carries no file contents.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileProbeDto {
    /// The user closed the picker without choosing a file; `steps` is then empty.
    pub cancelled: bool,
    /// How the picker returned the file: a URL scheme (`content`, `file`) or `path`.
    pub location: Option<String>,
    /// Bytes read from the picked file, or written and read back by the save probe.
    pub bytes: Option<u64>,
    /// Content sniffed from the first bytes: `zip` (APKG), `text` or `binary`.
    pub kind: Option<String>,
    pub steps: Vec<ProbeStepDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeStepDto {
    pub name: String,
    pub ok: bool,
    pub error: Option<String>,
}

/// Error codes from plan §14.3; serialized as SCREAMING_SNAKE_CASE strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    ValidationFailed,
    VersionConflict,
    NotFound,
    Deleted,
    DbBusy,
    StorageFull,
    PermissionDenied,
    NetworkUnavailable,
    ProviderError,
    ProviderTimeout,
    InvalidProviderOutput,
    ModerationBlocked,
    QuotaExceeded,
    UnsupportedFormat,
    BackupCorrupt,
    SchemaTooNew,
    PreviewStale,
    Cancelled,
    NotImplemented,
    Internal,
}

impl ErrorCode {
    pub const ALL: [ErrorCode; 20] = [
        Self::ValidationFailed,
        Self::VersionConflict,
        Self::NotFound,
        Self::Deleted,
        Self::DbBusy,
        Self::StorageFull,
        Self::PermissionDenied,
        Self::NetworkUnavailable,
        Self::ProviderError,
        Self::ProviderTimeout,
        Self::InvalidProviderOutput,
        Self::ModerationBlocked,
        Self::QuotaExceeded,
        Self::UnsupportedFormat,
        Self::BackupCorrupt,
        Self::SchemaTooNew,
        Self::PreviewStale,
        Self::Cancelled,
        Self::NotImplemented,
        Self::Internal,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ValidationFailed => "VALIDATION_FAILED",
            Self::VersionConflict => "VERSION_CONFLICT",
            Self::NotFound => "NOT_FOUND",
            Self::Deleted => "DELETED",
            Self::DbBusy => "DB_BUSY",
            Self::StorageFull => "STORAGE_FULL",
            Self::PermissionDenied => "PERMISSION_DENIED",
            Self::NetworkUnavailable => "NETWORK_UNAVAILABLE",
            Self::ProviderError => "PROVIDER_ERROR",
            Self::ProviderTimeout => "PROVIDER_TIMEOUT",
            Self::InvalidProviderOutput => "INVALID_PROVIDER_OUTPUT",
            Self::ModerationBlocked => "MODERATION_BLOCKED",
            Self::QuotaExceeded => "QUOTA_EXCEEDED",
            Self::UnsupportedFormat => "UNSUPPORTED_FORMAT",
            Self::BackupCorrupt => "BACKUP_CORRUPT",
            Self::SchemaTooNew => "SCHEMA_TOO_NEW",
            Self::PreviewStale => "PREVIEW_STALE",
            Self::Cancelled => "CANCELLED",
            Self::NotImplemented => "NOT_IMPLEMENTED",
            Self::Internal => "INTERNAL",
        }
    }

    /// Default retry policy; the frontend mirrors it in `src/api/errors.ts`.
    pub fn retryable_by_default(self) -> bool {
        matches!(
            self,
            Self::DbBusy
                | Self::NetworkUnavailable
                | Self::ProviderError
                | Self::ProviderTimeout
                | Self::InvalidProviderOutput
                | Self::PreviewStale
        )
    }
}

/// User-facing error: the UI translates `message_key`; raw SQL or stack traces never cross IPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ErrorDto {
    pub code: ErrorCode,
    pub message_key: String,
    pub retryable: bool,
    pub details: Option<Map<String, Value>>,
}

impl ErrorDto {
    pub fn new(code: ErrorCode) -> Self {
        Self {
            code,
            message_key: format!("errors.{}", code.as_str()),
            retryable: code.retryable_by_default(),
            details: None,
        }
    }

    pub fn with_detail(mut self, key: &str, value: impl Into<Value>) -> Self {
        self.details
            .get_or_insert_with(Map::new)
            .insert(key.to_owned(), value.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const FRONTEND_TYPES: &str = include_str!("../../../frontend/src/api/types.ts");

    #[test]
    fn error_dto_matches_frontend_shape() {
        let dto = ErrorDto::new(ErrorCode::DbBusy).with_detail("entity", "deck");
        assert_eq!(
            serde_json::to_value(&dto).unwrap(),
            json!({
                "code": "DB_BUSY",
                "messageKey": "errors.DB_BUSY",
                "retryable": true,
                "details": { "entity": "deck" }
            })
        );
    }

    #[test]
    fn error_without_details_serializes_null() {
        let value = serde_json::to_value(ErrorDto::new(ErrorCode::BackupCorrupt)).unwrap();
        assert_eq!(value["details"], Value::Null);
        assert_eq!(value["retryable"], json!(false));
    }

    #[test]
    fn error_code_serialization_matches_as_str() {
        for code in ErrorCode::ALL {
            assert_eq!(serde_json::to_value(code).unwrap(), json!(code.as_str()));
        }
    }

    #[test]
    fn api_version_matches_frontend_contract() {
        let expected = format!("export const API_VERSION = '{API_VERSION}'");
        assert!(
            FRONTEND_TYPES.contains(&expected),
            "frontend API_VERSION differs from Rust contracts::API_VERSION"
        );
    }

    #[test]
    fn every_error_code_is_known_to_the_frontend() {
        for code in ErrorCode::ALL {
            let quoted = format!("'{}'", code.as_str());
            assert!(
                FRONTEND_TYPES.contains(&quoted),
                "{} missing in frontend ERROR_CODES",
                code.as_str()
            );
        }
    }
}
