//! Application errors. Each variant maps to exactly one contract `ErrorCode` in the composition
//! root; messages never cross IPC, and `details` carry only field names, codes and counts.

use langualinker_domain::{UnknownCode, ValidationError};

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum AppError {
    #[error("validation failed: {0}")]
    Validation(#[from] ValidationError),
    #[error("{entity} changed since it was read (current version {current})")]
    VersionConflict { entity: &'static str, current: i64 },
    #[error("{entity} not found")]
    NotFound { entity: &'static str },
    #[error("{entity} was deleted")]
    Deleted { entity: &'static str },
    #[error("database is busy")]
    DbBusy,
    #[error("storage is full")]
    StorageFull,
    #[error("database schema {found} is newer than supported {supported}")]
    SchemaTooNew { found: i64, supported: i64 },
    #[error("permission denied: {reason}")]
    PermissionDenied { reason: &'static str },
    #[error("preview is stale")]
    PreviewStale,
    #[error("cancelled")]
    Cancelled,
    #[error("{operation} is not implemented yet")]
    NotImplemented { operation: &'static str },
    /// Unexpected failure; the text is for logs only (it must not contain user content).
    #[error("internal error: {0}")]
    Internal(String),
}

impl From<UnknownCode> for AppError {
    fn from(error: UnknownCode) -> Self {
        Self::Internal(error.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;

/// A value in `ErrorDto.details`: only names, codes and numbers — never user content.
#[derive(Debug, Clone, PartialEq)]
pub enum DetailValue {
    Text(&'static str),
    Number(i64),
}

impl AppError {
    /// Contract error code (`ErrorCode` in `crates/contracts`).
    pub fn code(&self) -> &'static str {
        match self {
            Self::Validation(_) => "VALIDATION_FAILED",
            Self::VersionConflict { .. } => "VERSION_CONFLICT",
            Self::NotFound { .. } => "NOT_FOUND",
            Self::Deleted { .. } => "DELETED",
            Self::DbBusy => "DB_BUSY",
            Self::StorageFull => "STORAGE_FULL",
            Self::SchemaTooNew { .. } => "SCHEMA_TOO_NEW",
            Self::PermissionDenied { .. } => "PERMISSION_DENIED",
            Self::PreviewStale => "PREVIEW_STALE",
            Self::Cancelled => "CANCELLED",
            Self::NotImplemented { .. } => "NOT_IMPLEMENTED",
            Self::Internal(_) => "INTERNAL",
        }
    }

    /// Safe details for the UI. `Internal` deliberately exposes nothing.
    pub fn details(&self) -> Vec<(&'static str, DetailValue)> {
        use DetailValue::{Number, Text};
        match self {
            Self::Validation(error) => {
                vec![("field", Text(error.field)), ("reason", Text(error.reason))]
            }
            Self::VersionConflict { entity, current } => {
                vec![
                    ("entity", Text(entity)),
                    ("currentVersion", Number(*current)),
                ]
            }
            Self::NotFound { entity } | Self::Deleted { entity } => vec![("entity", Text(entity))],
            Self::SchemaTooNew { found, supported } => {
                vec![("found", Number(*found)), ("supported", Number(*supported))]
            }
            Self::PermissionDenied { reason } => vec![("reason", Text(reason))],
            Self::NotImplemented { operation } => vec![("operation", Text(operation))],
            Self::DbBusy
            | Self::StorageFull
            | Self::PreviewStale
            | Self::Cancelled
            | Self::Internal(_) => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_errors_expose_no_details() {
        let error = AppError::Internal("UNIQUE constraint failed: notes.guid".into());
        assert_eq!(error.code(), "INTERNAL");
        assert!(error.details().is_empty());
    }

    #[test]
    fn validation_details_name_the_field_not_the_value() {
        let error = AppError::from(ValidationError::new("front", "tooLong"));
        assert_eq!(error.code(), "VALIDATION_FAILED");
        assert_eq!(
            error.details(),
            [
                ("field", DetailValue::Text("front")),
                ("reason", DetailValue::Text("tooLong"))
            ]
        );
    }
}
