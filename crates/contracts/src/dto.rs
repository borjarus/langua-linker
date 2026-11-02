//! DTOs whose source of truth is Rust (plan step 28). Each derives `TS`; `typescript()` renders
//! them into `frontend/src/generated/contracts.ts`, which `frontend/src/api/types.ts` re-exports.
//! A DTO moves here when its command becomes real; the rest of `types.ts` follows phase by phase.
//!
//! Integers are `u32`/`f64`: an `i64`/`u64` would render as `bigint` in TypeScript and fail the
//! frontend type check — the intended guard against ids or counts beyond 2^53 crossing as numbers.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ErrorDto;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum JobKind {
    PrepareContent,
    Import,
    Export,
    Backup,
    Restore,
    Reset,
    Llm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum JobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct JobProgressDto {
    pub done: u32,
    pub total: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobDto {
    pub job_id: String,
    pub kind: JobKind,
    pub status: JobStatus,
    /// Monotonic per job; consumers drop events with a lower sequence.
    pub sequence: u32,
    pub progress: Option<JobProgressDto>,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub error: Option<ErrorDto>,
    /// Set once the job finished (also on some failures, e.g. the safety copy of a failed restore).
    pub result: Option<JobResultDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobEventDto {
    pub job_id: String,
    pub kind: JobKind,
    pub sequence: u32,
    pub status: JobStatus,
    pub progress: Option<JobProgressDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ImportRowErrorDto {
    /// 1-based line in the file.
    pub row: u32,
    pub error: ErrorDto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ImportCountsDto {
    pub create: u32,
    pub update: u32,
    pub skip: u32,
    pub duplicate: u32,
    pub error: u32,
}

/// Outcome of a finished job, read with `getJobStatus`; job events never carry it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum JobResultDto {
    Import {
        deck_id: String,
        row_errors: Vec<ImportRowErrorDto>,
        create: u32,
        update: u32,
        skip: u32,
        duplicate: u32,
        error: u32,
    },
    Export {
        file_name: String,
        card_count: u32,
    },
    Backup {
        backup_id: String,
    },
    /// Present also on a failed restore: the safety copy made before anything changed.
    Restore {
        safety_backup_id: String,
    },
    Reset {
        safety_backup_id: Option<String>,
    },
    PrepareContent {
        deck_ids: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum CollectionScope {
    Decks,
    Cards,
    Notes,
    Settings,
    Stats,
    Backups,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CollectionChangedEventDto {
    pub scopes: Vec<CollectionScope>,
    /// Read signal only: refetch through AppApi.
    pub changed_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AboutDto {
    pub app_version: String,
    pub api_version: String,
    /// `null` when the database could not be opened.
    pub schema_version: Option<u32>,
    pub fsrs_version: String,
    pub content_version: String,
    pub platform: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct LogLineDto {
    pub at: String,
    pub level: LogLevel,
    pub target: String,
    /// Already redacted by the backend: no card content, prompts or secrets.
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiagnosticsReportDto {
    pub generated_at: String,
    pub lines: Vec<LogLineDto>,
    /// Number of values the backend replaced with `[redacted]`.
    pub redactions: u32,
}

/// Renders every Rust-owned DTO as one TypeScript module.
pub fn typescript() -> String {
    let cfg = ts_rs::Config::new();
    let declarations = [
        crate::ErrorCode::decl(&cfg),
        ErrorDto::decl(&cfg),
        crate::AppInfoDto::decl(&cfg),
        JobKind::decl(&cfg),
        JobStatus::decl(&cfg),
        JobProgressDto::decl(&cfg),
        JobDto::decl(&cfg),
        JobEventDto::decl(&cfg),
        ImportRowErrorDto::decl(&cfg),
        ImportCountsDto::decl(&cfg),
        JobResultDto::decl(&cfg),
        CollectionScope::decl(&cfg),
        CollectionChangedEventDto::decl(&cfg),
        AboutDto::decl(&cfg),
        LogLevel::decl(&cfg),
        LogLineDto::decl(&cfg),
        DiagnosticsReportDto::decl(&cfg),
    ];
    let mut out = String::from(
        "// Generated from crates/contracts (Rust is the DTO source, plan Phase 7 step 28).\n\
         // Do not edit: change the Rust type and run\n\
         //   UPDATE_CONTRACT=1 cargo test -p langualinker-contracts --test generated_typescript\n\n",
    );
    for declaration in declarations {
        out.push_str("export ");
        out.push_str(declaration.trim());
        out.push_str("\n\n");
    }
    out.truncate(out.trim_end().len());
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use serde::de::DeserializeOwned;
    use serde_json::{Value, json};

    use super::*;

    fn round_trip<T>(value: &T) -> Value
    where
        T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug,
    {
        let json = serde_json::to_value(value).unwrap();
        let back: T = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(&back, value);
        json
    }

    fn job_event() -> JobEventDto {
        JobEventDto {
            job_id: "12".into(),
            kind: JobKind::PrepareContent,
            sequence: 3,
            status: JobStatus::Running,
            progress: Some(JobProgressDto { done: 1, total: 4 }),
        }
    }

    #[test]
    fn enums_serialize_in_camel_case() {
        assert_eq!(json!(JobKind::PrepareContent), json!("prepareContent"));
        assert_eq!(json!(JobStatus::Cancelled), json!("cancelled"));
        assert_eq!(json!(CollectionScope::Backups), json!("backups"));
        assert_eq!(json!(LogLevel::Warn), json!("warn"));
    }

    #[test]
    fn job_event_uses_camel_case_fields() {
        assert_eq!(
            round_trip(&job_event()),
            json!({
                "jobId": "12",
                "kind": "prepareContent",
                "sequence": 3,
                "status": "running",
                "progress": { "done": 1, "total": 4 },
            })
        );
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let mut value = serde_json::to_value(job_event()).unwrap();
        value["extra"] = json!(1);
        assert!(serde_json::from_value::<JobEventDto>(value).is_err());
        assert!(
            serde_json::from_value::<JobProgressDto>(json!({"done": 1, "total": 2, "x": 0}))
                .is_err()
        );
    }

    #[test]
    fn integers_beyond_u32_are_rejected() {
        let too_big = json!({ "done": 4_294_967_296u64, "total": 1 });
        assert!(serde_json::from_value::<JobProgressDto>(too_big).is_err());
    }

    #[test]
    fn job_without_optional_parts_serializes_nulls() {
        let job = JobDto {
            job_id: "1".into(),
            kind: JobKind::Backup,
            status: JobStatus::Queued,
            sequence: 0,
            progress: None,
            started_at: "2026-03-02T08:00:00Z".into(),
            finished_at: None,
            error: None,
            result: None,
        };
        let json = round_trip(&job);
        assert_eq!(json["progress"], Value::Null);
        assert_eq!(json["finishedAt"], Value::Null);
        assert_eq!(json["result"], Value::Null);
        assert_eq!(json["startedAt"], "2026-03-02T08:00:00Z");
    }

    #[test]
    fn job_result_is_tagged_by_type() {
        let import = JobResultDto::Import {
            deck_id: "5".into(),
            row_errors: Vec::new(),
            create: 1,
            update: 2,
            skip: 3,
            duplicate: 4,
            error: 5,
        };
        assert_eq!(
            round_trip(&import),
            json!({
                "type": "import",
                "deckId": "5",
                "rowErrors": [],
                "create": 1,
                "update": 2,
                "skip": 3,
                "duplicate": 4,
                "error": 5,
            })
        );
        assert_eq!(
            round_trip(&JobResultDto::Export {
                file_name: "deck.csv".into(),
                card_count: 9,
            }),
            json!({ "type": "export", "fileName": "deck.csv", "cardCount": 9 })
        );
        assert_eq!(
            round_trip(&JobResultDto::PrepareContent {
                deck_ids: vec!["1".into()]
            }),
            json!({ "type": "prepareContent", "deckIds": ["1"] })
        );
    }

    #[test]
    fn every_result_variant_round_trips() {
        for result in [
            JobResultDto::Backup {
                backup_id: "b".into(),
            },
            JobResultDto::Restore {
                safety_backup_id: "s".into(),
            },
            JobResultDto::Reset {
                safety_backup_id: None,
            },
            JobResultDto::Reset {
                safety_backup_id: Some("s".into()),
            },
        ] {
            round_trip(&result);
        }
    }

    #[test]
    fn unknown_result_type_is_rejected() {
        assert!(serde_json::from_value::<JobResultDto>(json!({ "type": "nuke" })).is_err());
    }

    #[test]
    fn collection_changed_event_round_trips() {
        let event = CollectionChangedEventDto {
            scopes: vec![CollectionScope::Decks, CollectionScope::Stats],
            changed_at: "2026-03-02T08:00:00Z".into(),
        };
        assert_eq!(
            round_trip(&event),
            json!({ "scopes": ["decks", "stats"], "changedAt": "2026-03-02T08:00:00Z" })
        );
    }

    #[test]
    fn about_allows_a_missing_schema_version() {
        let about = AboutDto {
            app_version: "0.1.0".into(),
            api_version: "1".into(),
            schema_version: None,
            fsrs_version: "5".into(),
            content_version: "c1".into(),
            platform: "windows".into(),
        };
        let json = round_trip(&about);
        assert_eq!(json["schemaVersion"], Value::Null);
        assert_eq!(json["appVersion"], "0.1.0");
    }

    #[test]
    fn diagnostics_report_round_trips() {
        let report = DiagnosticsReportDto {
            generated_at: "2026-03-02T08:00:00Z".into(),
            lines: vec![LogLineDto {
                at: "2026-03-02T07:59:00Z".into(),
                level: LogLevel::Info,
                target: "jobs".into(),
                message: "started".into(),
            }],
            redactions: 2,
        };
        let json = round_trip(&report);
        assert_eq!(json["generatedAt"], "2026-03-02T08:00:00Z");
        assert_eq!(json["lines"][0]["level"], "info");
    }

    #[test]
    fn typescript_declares_every_dto_once() {
        let ts = typescript();
        for name in [
            "ErrorCode",
            "ErrorDto",
            "AppInfoDto",
            "JobKind",
            "JobStatus",
            "JobProgressDto",
            "JobDto",
            "JobEventDto",
            "ImportRowErrorDto",
            "ImportCountsDto",
            "JobResultDto",
            "CollectionScope",
            "CollectionChangedEventDto",
            "AboutDto",
            "LogLevel",
            "LogLineDto",
            "DiagnosticsReportDto",
        ] {
            let declaration = format!("export type {name} =");
            assert_eq!(ts.matches(&declaration).count(), 1, "{name}");
        }
        assert!(ts.ends_with("\n") && !ts.ends_with("\n\n"));
    }

    #[test]
    fn typescript_has_no_bigint() {
        assert!(!typescript().contains("bigint"));
    }
}
