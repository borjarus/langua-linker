//! Local diagnostics: rotating file logs with field redaction and a content-free crash report.
//!
//! Policy (plan §3, §5): logs never contain card content, prompts or secrets. Structured fields
//! are written only when their *name* is on [`ALLOWED_FIELDS`]; every other field value is
//! replaced by `<redacted>`. Messages must be static text — put variable data into fields.
//! Telemetry is not part of this module and stays disabled by default.

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use tracing::field::{Field, Visit};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{Builder as RollingBuilder, Rotation};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::field::RecordFields;
use tracing_subscriber::fmt::FormatFields;
use tracing_subscriber::fmt::format::Writer;

/// Field names whose values are safe to log (identifiers, codes, counts, timings).
pub const ALLOWED_FIELDS: &[&str] = &[
    "app_version",
    "code",
    "command",
    "count",
    "duration_ms",
    "job_id",
    "location",
    "operation_id",
    "phase",
    "platform",
    "schema_version",
    "status",
];

pub const REDACTED: &str = "<redacted>";
pub const LOG_FILE_PREFIX: &str = "langualinker";
pub const CRASH_REPORT_FILE: &str = "last-crash.json";
const MAX_LOG_FILES: usize = 7;

#[derive(Debug, thiserror::Error)]
pub enum LoggingError {
    #[error("cannot create log directory or file: {0}")]
    Io(#[from] std::io::Error),
    #[error("cannot open rolling log file: {0}")]
    Appender(#[from] tracing_appender::rolling::InitError),
    #[error("a global logger is already installed")]
    AlreadyInstalled,
}

/// Formats event and span fields, keeping values only for allow-listed field names.
#[derive(Debug, Default, Clone, Copy)]
pub struct RedactingFields;

impl<'writer> FormatFields<'writer> for RedactingFields {
    fn format_fields<R: RecordFields>(&self, writer: Writer<'writer>, fields: R) -> fmt::Result {
        let mut visitor = RedactingVisitor {
            writer,
            first: true,
            result: Ok(()),
        };
        fields.record(&mut visitor);
        visitor.result
    }
}

struct RedactingVisitor<'writer> {
    writer: Writer<'writer>,
    first: bool,
    result: fmt::Result,
}

impl Visit for RedactingVisitor<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if self.result.is_err() {
            return;
        }
        let separator = if self.first { "" } else { " " };
        self.first = false;
        let name = field.name();
        self.result = if name == "message" {
            write!(self.writer, "{separator}{value:?}")
        } else if ALLOWED_FIELDS.contains(&name) {
            write!(self.writer, "{separator}{name}={value:?}")
        } else {
            write!(self.writer, "{separator}{name}={REDACTED}")
        };
    }
}

/// Keeps the background log writer alive; dropping it flushes pending lines.
#[must_use = "dropping the guard stops file logging"]
pub struct LoggingGuard {
    _worker: WorkerGuard,
}

/// Installs the global logger: daily rotation, at most 7 files, no ANSI, redacted fields.
/// The level comes from `LANGUALINKER_LOG` (default `info`).
pub fn init_file_logging(dir: &Path) -> Result<LoggingGuard, LoggingError> {
    std::fs::create_dir_all(dir)?;
    let appender = RollingBuilder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix(LOG_FILE_PREFIX)
        .filename_suffix("log")
        .max_log_files(MAX_LOG_FILES)
        .build(dir)?;
    let (writer, worker) = tracing_appender::non_blocking(appender);
    let filter =
        EnvFilter::try_from_env("LANGUALINKER_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(writer)
        .with_ansi(false)
        .fmt_fields(RedactingFields)
        .try_init()
        .map_err(|_| LoggingError::AlreadyInstalled)?;
    Ok(LoggingGuard { _worker: worker })
}

/// Records panics as a small JSON report with app version and source location only.
/// The panic payload is deliberately omitted: it may contain user content.
pub fn install_crash_hook(dir: PathBuf, app_version: &'static str) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|location| format!("{}:{}", location.file(), location.line()))
            .unwrap_or_default();
        // Best effort: a failing crash report must never mask the original panic.
        let _ = write_crash_report(&dir, app_version, &location);
        tracing::error!(app_version, location = %location, "panic");
        previous(info);
    }));
}

fn write_crash_report(dir: &Path, app_version: &str, location: &str) -> std::io::Result<()> {
    let occurred_at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default();
    let report = serde_json::json!({
        "appVersion": app_version,
        "location": location,
        "occurredAtMs": occurred_at_ms,
    });
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join(CRASH_REPORT_FILE), report.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::fmt::MakeWriter;

    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<Vec<u8>>>);

    impl Write for Capture {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for Capture {
        type Writer = Capture;

        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    fn captured(log: impl FnOnce()) -> String {
        let capture = Capture::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(capture.clone())
            .with_ansi(false)
            .fmt_fields(RedactingFields)
            .finish();
        tracing::subscriber::with_default(subscriber, log);
        String::from_utf8(capture.0.lock().unwrap().clone()).unwrap()
    }

    #[test]
    fn content_and_secrets_are_redacted_while_safe_fields_remain() {
        let output = captured(|| {
            tracing::info!(
                front = "der Kühlschrank",
                api_key = "sk-live-123",
                prompt = "Generate a mnemonic",
                code = "DB_BUSY",
                duration_ms = 42,
                "note saved"
            );
        });
        assert!(output.contains("note saved"), "{output}");
        assert!(output.contains("code=\"DB_BUSY\""), "{output}");
        assert!(output.contains("duration_ms=42"), "{output}");
        for secret in ["Kühlschrank", "sk-live-123", "Generate a mnemonic"] {
            assert!(!output.contains(secret), "leaked {secret}: {output}");
        }
        assert_eq!(output.matches(REDACTED).count(), 3, "{output}");
    }

    #[test]
    fn span_fields_are_redacted_too() {
        let output = captured(|| {
            let span =
                tracing::info_span!("import", file_name = "moje-slowka.csv", job_id = "job-7");
            let _entered = span.enter();
            tracing::info!(count = 3, "rows parsed");
        });
        assert!(output.contains("job_id=\"job-7\""), "{output}");
        assert!(!output.contains("moje-slowka"), "{output}");
    }

    #[test]
    fn controlled_crash_writes_a_report_without_the_panic_message() {
        let dir = std::env::temp_dir().join(format!("ll-crash-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let default_hook = std::panic::take_hook();
        install_crash_hook(dir.clone(), "9.9.9-test");
        let outcome = std::panic::catch_unwind(|| panic!("secret card text: zażółć"));
        let _ = std::panic::take_hook();
        std::panic::set_hook(default_hook);

        assert!(outcome.is_err());
        let report = std::fs::read_to_string(dir.join(CRASH_REPORT_FILE)).expect("crash report");
        assert!(report.contains("\"appVersion\":\"9.9.9-test\""), "{report}");
        assert!(report.contains("logging.rs"), "{report}");
        assert!(!report.contains("zażółć"), "{report}");
        assert!(!report.contains("secret card text"), "{report}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
