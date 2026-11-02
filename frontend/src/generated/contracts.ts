// Generated from crates/contracts (Rust is the DTO source, plan Phase 7 step 28).
// Do not edit: change the Rust type and run
//   UPDATE_CONTRACT=1 cargo test -p langualinker-contracts --test generated_typescript

export type ErrorCode = "VALIDATION_FAILED" | "VERSION_CONFLICT" | "NOT_FOUND" | "DELETED" | "DB_BUSY" | "STORAGE_FULL" | "PERMISSION_DENIED" | "NETWORK_UNAVAILABLE" | "PROVIDER_ERROR" | "PROVIDER_TIMEOUT" | "INVALID_PROVIDER_OUTPUT" | "MODERATION_BLOCKED" | "QUOTA_EXCEEDED" | "UNSUPPORTED_FORMAT" | "BACKUP_CORRUPT" | "SCHEMA_TOO_NEW" | "PREVIEW_STALE" | "CANCELLED" | "NOT_IMPLEMENTED" | "INTERNAL";

export type ErrorDto = { code: ErrorCode, 
/**
 * i18n key; the UI translates this, never raw SQL or stack traces.
 */
messageKey: string, retryable: boolean, details: Record<string, string | number | boolean | null> | null, };

export type AppInfoDto = { apiVersion: string, appName: string, appVersion: string, 
/**
 * Native build variant (`dev` | `beta` | `release`); the in-memory mock reports `demo`.
 */
mode: 'demo' | 'dev' | 'beta' | 'release', };

export type JobKind = "prepareContent" | "import" | "export" | "backup" | "restore" | "reset" | "llm";

export type JobStatus = "queued" | "running" | "succeeded" | "failed" | "cancelled";

export type JobProgressDto = { done: number, total: number, };

export type JobDto = { jobId: string, kind: JobKind, status: JobStatus, 
/**
 * Monotonic per job; consumers drop events with a lower sequence.
 */
sequence: number, progress: JobProgressDto | null, startedAt: string, finishedAt: string | null, error: ErrorDto | null, 
/**
 * Set once the job finished (also on some failures, e.g. the safety copy of a failed restore).
 */
result: JobResultDto | null, };

export type JobEventDto = { jobId: string, kind: JobKind, sequence: number, status: JobStatus, progress: JobProgressDto | null, };

export type ImportRowErrorDto = { 
/**
 * 1-based line in the file.
 */
row: number, error: ErrorDto, };

export type ImportCountsDto = { create: number, update: number, skip: number, duplicate: number, error: number, };

export type JobResultDto = { "type": "import", deckId: string, rowErrors: Array<ImportRowErrorDto>, create: number, update: number, skip: number, duplicate: number, error: number, } | { "type": "export", fileName: string, cardCount: number, } | { "type": "backup", backupId: string, } | { "type": "restore", safetyBackupId: string, } | { "type": "reset", safetyBackupId: string | null, } | { "type": "prepareContent", deckIds: Array<string>, };

export type CollectionScope = "decks" | "cards" | "notes" | "settings" | "stats" | "backups";

export type CollectionChangedEventDto = { scopes: Array<CollectionScope>, 
/**
 * Read signal only: refetch through AppApi.
 */
changedAt: string, };

export type AboutDto = { appVersion: string, apiVersion: string, 
/**
 * `null` when the database could not be opened.
 */
schemaVersion: number | null, fsrsVersion: string, contentVersion: string, platform: string, };

export type LogLevel = "error" | "warn" | "info" | "debug";

export type LogLineDto = { at: string, level: LogLevel, target: string, 
/**
 * Already redacted by the backend: no card content, prompts or secrets.
 */
message: string, };

export type DiagnosticsReportDto = { generatedAt: string, lines: Array<LogLineDto>, 
/**
 * Number of values the backend replaced with `[redacted]`.
 */
redactions: number, };
