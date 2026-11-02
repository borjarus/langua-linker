//! Reads the app's own log files for the About screen's diagnostic report.
//!
//! Lines are already redacted when written ([`crate::logging`]); this reader redacts again before
//! anything crosses IPC: every `key=value` whose key is not on [`ALLOWED_FIELDS`] loses its value.

use std::path::Path;

use chrono::{DateTime, SecondsFormat, Utc};

use crate::logging::{ALLOWED_FIELDS, LOG_FILE_PREFIX, REDACTED};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    /// UTC ISO-8601 with milliseconds.
    pub at: String,
    pub level: LogLevel,
    pub target: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LogReport {
    pub lines: Vec<LogLine>,
    /// Redaction markers in the returned lines.
    pub redactions: usize,
}

/// The newest `limit` lines across the rotated log files in `dir`, oldest first. A missing
/// directory yields an empty report; unparsable lines are skipped.
pub fn read_recent(dir: &Path, limit: usize) -> std::io::Result<LogReport> {
    let mut files: Vec<_> = match std::fs::read_dir(dir) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(LOG_FILE_PREFIX) && name.ends_with(".log"))
            })
            .collect(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(LogReport::default());
        }
        Err(error) => return Err(error),
    };
    // Daily rotation names files `<prefix>.<YYYY-MM-DD>.log`, so names sort chronologically.
    files.sort();
    let mut lines = Vec::new();
    for file in files.iter().rev() {
        let text = std::fs::read_to_string(file)?;
        let mut parsed: Vec<LogLine> = text.lines().filter_map(parse_line).collect();
        parsed.reverse();
        lines.extend(parsed);
        if lines.len() >= limit {
            break;
        }
    }
    lines.truncate(limit);
    lines.reverse();
    let redactions = lines
        .iter()
        .map(|line| line.message.matches(REDACTED).count())
        .sum();
    Ok(LogReport { lines, redactions })
}

/// Parses one line of the `tracing_subscriber::fmt` format:
/// `<rfc3339> <LEVEL> [span{fields}:]* <target>: <message> [key=value]*`.
pub fn parse_line(line: &str) -> Option<LogLine> {
    let line = line.trim();
    let (timestamp, rest) = line.split_once(' ')?;
    let rest = rest.trim_start();
    let (level, rest) = rest.split_once(' ')?;
    let level = match level {
        "ERROR" => LogLevel::Error,
        "WARN" => LogLevel::Warn,
        "INFO" => LogLevel::Info,
        "DEBUG" | "TRACE" => LogLevel::Debug,
        _ => return None,
    };
    let at = DateTime::parse_from_rfc3339(timestamp)
        .ok()?
        .with_timezone(&Utc)
        .to_rfc3339_opts(SecondsFormat::Millis, true);
    let mut segments = rest.trim_start().split(": ");
    let mut target = segments.next()?;
    // Span prefixes (`name{fields}`) come before the target.
    while target.ends_with('}') && target.contains('{') {
        target = segments.next()?;
    }
    let message = segments.collect::<Vec<_>>().join(": ");
    Some(LogLine {
        at,
        level,
        target: target.to_owned(),
        message: redact_fields(&message),
    })
}

/// Replaces the value of every `key=value` token whose key is not allow-listed.
fn redact_fields(message: &str) -> String {
    let mut result = String::with_capacity(message.len());
    let mut rest = message;
    while let Some(eq) = rest.find('=') {
        let key_start = rest[..eq]
            .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.'))
            .map_or(0, |index| index + 1);
        let key = &rest[key_start..eq];
        let value_len = value_length(&rest[eq + 1..]);
        result.push_str(&rest[..=eq]);
        let value = &rest[eq + 1..eq + 1 + value_len];
        if key.is_empty() || ALLOWED_FIELDS.contains(&key) || value == REDACTED {
            result.push_str(value);
        } else {
            result.push_str(REDACTED);
        }
        rest = &rest[eq + 1 + value_len..];
    }
    result.push_str(rest);
    result
}

/// Length of a field value: a quoted string (with escapes) or up to the next space.
fn value_length(text: &str) -> usize {
    if let Some(quoted) = text.strip_prefix('"') {
        let mut escaped = false;
        for (index, c) in quoted.char_indices() {
            match c {
                '\\' if !escaped => escaped = true,
                '"' if !escaped => return index + 2,
                _ => escaped = false,
            }
        }
        text.len()
    } else {
        text.find(' ').unwrap_or(text.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_fmt_layout_with_spans() {
        let line = parse_line(
            "2026-10-30T09:47:24.123456Z  INFO import{job_id=\"7\"}: langualinker_lib::jobs: rows parsed count=3",
        )
        .unwrap();
        assert_eq!(line.at, "2026-10-30T09:47:24.123Z");
        assert_eq!(line.level, LogLevel::Info);
        assert_eq!(line.target, "langualinker_lib::jobs");
        assert_eq!(line.message, "rows parsed count=3");
    }

    #[test]
    fn unknown_fields_are_redacted_again() {
        let line = parse_line(
            "2026-10-30T09:47:24Z ERROR app: save failed front=\"der Hund\" code=\"DB_BUSY\" note=[redacted]",
        )
        .unwrap();
        assert_eq!(
            line.message,
            "save failed front=[redacted] code=\"DB_BUSY\" note=[redacted]"
        );
        assert_eq!(line.level, LogLevel::Error);
    }

    #[test]
    fn quoted_values_with_spaces_and_escapes_are_one_value() {
        assert_eq!(
            redact_fields(r#"x prompt="a \"b\" c" status="ok""#),
            r#"x prompt=[redacted] status="ok""#
        );
    }

    #[test]
    fn garbage_lines_are_skipped() {
        assert!(parse_line("").is_none());
        assert!(parse_line("not a log line").is_none());
        assert!(parse_line("2026-10-30T09:47:24Z NOTICE x: y").is_none());
    }

    #[test]
    fn reads_the_newest_lines_across_files() {
        let dir = tempfile::tempdir().unwrap();
        let line = |n: u32| format!("2026-10-30T09:00:{n:02}Z  INFO app: line {n}\n");
        std::fs::write(
            dir.path().join("langualinker.2026-10-29.log"),
            (0..3).map(line).collect::<String>(),
        )
        .unwrap();
        std::fs::write(
            dir.path().join("langualinker.2026-10-30.log"),
            (3..5).map(line).collect::<String>(),
        )
        .unwrap();
        std::fs::write(dir.path().join("other.log"), line(9)).unwrap();
        let report = read_recent(dir.path(), 3).unwrap();
        let messages: Vec<_> = report.lines.iter().map(|l| l.message.as_str()).collect();
        assert_eq!(messages, ["line 2", "line 3", "line 4"]);
        assert_eq!(report.redactions, 0);
        assert!(
            read_recent(&dir.path().join("missing"), 3)
                .unwrap()
                .lines
                .is_empty()
        );
    }
}
