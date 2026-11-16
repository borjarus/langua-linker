//! CSV import and deck export jobs (plan Phase 14, ADR-19).
//!
//! - **Preview** (`previewImport`): the file is parsed and planned against a read snapshot;
//!   nothing is written. The preview is remembered with the collection fingerprint.
//! - **Commit** (`commitImport`, job `import`): refused as `PREVIEW_STALE` when the token is not
//!   the last preview's or the collection changed since. The job plans again inside one write
//!   transaction and applies the plan there; a cancel or an error rolls everything back.
//! - **Export** (`exportDeck`, job `export`): one read snapshot, then the CSV or `.apkg` is
//!   written to a hidden temporary file and renamed into `exports/<jobId>.<ext>`, so a kill
//!   leaves no half file. `shareFile` copies it out through the save dialog. The newest
//!   [`EXPORTS_KEPT`] exports are kept.

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use langualinker_application::jobs::{JobId, JobKind, JobRecord, JobService, JobStatus, NewJob};
use langualinker_application::ports::{Clock, CollectionScope, EventSink, Rng};
use langualinker_application::repositories::UnitOfWork;
use langualinker_application::transfer::{
    self, CsvFile, CsvOptions, CsvOutcome, CsvPlan, ExportFormat, ExportSnapshot,
};
use langualinker_application::{AppError, AppResult};
use langualinker_domain::DeckId;
use langualinker_interop::anki::Losses;
use langualinker_interop::anki::codec::AnkiClock;
use langualinker_interop::anki::schema11::{self, Export};
use sha2::{Digest, Sha256};

use crate::backup::io_error;
use crate::sqlite::SqliteDatabase;

/// Finished exports kept for sharing.
pub const EXPORTS_KEPT: usize = 5;

/// What the Transfer screen shows before an import.
#[derive(Debug, Clone, PartialEq)]
pub struct CsvPreview {
    pub token: String,
    pub file: Arc<CsvFile>,
    pub plan: CsvPlan,
}

/// Result of an export job.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportOutcome {
    pub file_name: String,
    pub format: ExportFormat,
    /// Cards in the file (CSV: rows, one per note).
    pub card_count: u32,
    pub losses: Losses,
}

/// Renders job results as contract JSON; supplied by the composition root.
#[derive(Clone, Copy)]
pub struct TransferEncoders {
    pub import: fn(&CsvOutcome) -> String,
    pub export: fn(&ExportOutcome) -> String,
}

#[derive(Debug, Clone)]
pub struct ExportRequest {
    pub deck_id: DeckId,
    pub format: ExportFormat,
    pub include_scheduling: bool,
    pub formula_guard: bool,
}

struct StoredPreview {
    token: String,
    file: Arc<CsvFile>,
    options: CsvOptions,
    fingerprint: String,
}

#[derive(Clone)]
pub struct TransferService {
    db: Arc<SqliteDatabase>,
    jobs: JobService<SqliteDatabase>,
    events: Arc<dyn EventSink>,
    clock: Arc<dyn Clock>,
    rng: Arc<dyn Rng>,
    exports: PathBuf,
    encode: TransferEncoders,
    preview: Arc<Mutex<Option<StoredPreview>>>,
}

/// A file name without path separators or characters the platforms refuse.
pub fn safe_file_name(name: &str, extension: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.' | '(' | ')') {
                c
            } else {
                '_'
            }
        })
        .take(80)
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').trim();
    let stem = if cleaned.is_empty() { "deck" } else { cleaned };
    format!("{stem}.{extension}")
}

impl TransferService {
    pub fn new(
        db: Arc<SqliteDatabase>,
        jobs: JobService<SqliteDatabase>,
        events: Arc<dyn EventSink>,
        clock: Arc<dyn Clock>,
        rng: Arc<dyn Rng>,
        exports: PathBuf,
        encode: TransferEncoders,
    ) -> Self {
        // Temporary files of a killed export are never resumed.
        if let Ok(entries) = fs::read_dir(&exports) {
            for entry in entries.flatten() {
                if entry.file_name().to_string_lossy().starts_with('.') {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
        Self {
            db,
            jobs,
            events,
            clock,
            rng,
            exports,
            encode,
            preview: Arc::default(),
        }
    }

    /// Parses and plans an import; nothing is written.
    pub fn preview_csv(
        &self,
        file_name: &str,
        bytes: &[u8],
        options: CsvOptions,
    ) -> AppResult<CsvPreview> {
        let file = Arc::new(transfer::read_csv(file_name, bytes, &options)?);
        let (plan, fingerprint) = {
            let file = file.clone();
            let options = options.clone();
            self.db.read(move |tx| {
                let plan = transfer::plan_csv(tx, &file, &options)?;
                Ok((plan, tx.collection_fingerprint()?))
            })?
        };
        let digest = Sha256::digest(format!(
            "{}|{fingerprint}|{options:?}|{}",
            hex(&Sha256::digest(bytes)),
            self.clock.now().0
        ));
        let token = hex(&digest[..12]);
        *self
            .preview
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(StoredPreview {
            token: token.clone(),
            file: file.clone(),
            options,
            fingerprint,
        });
        Ok(CsvPreview { token, file, plan })
    }

    /// Starts the import of the last preview (job `import`).
    pub fn commit_csv(&self, operation_id: String, token: &str) -> AppResult<JobRecord> {
        let recorded = {
            let operation = operation_id.clone();
            self.db.read(move |tx| tx.recorded_operation(&operation))?
        };
        if let Some(recorded) = recorded {
            return self.jobs.get(&JobId(recorded.result_json));
        }
        let (file, options, fingerprint) = {
            let stored = self
                .preview
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            match stored.as_ref() {
                Some(stored) if stored.token == token => (
                    stored.file.clone(),
                    stored.options.clone(),
                    stored.fingerprint.clone(),
                ),
                _ => return Err(AppError::PreviewStale),
            }
        };
        if self.db.read(|tx| tx.collection_fingerprint())? != fingerprint {
            return Err(AppError::PreviewStale);
        }
        let total = u64::try_from(file.records.len()).unwrap_or(u64::MAX).max(1);
        let service = self.clone();
        self.jobs.start(
            NewJob {
                kind: JobKind::Import,
                operation_id: Some(operation_id),
                total: Some(total),
            },
            Box::new(move |context| {
                context.progress(0, total)?;
                let cancel = context.cancel_token();
                let rng = service.rng.clone();
                let now = service.clock.now();
                let outcome = service.db.write(move |tx| {
                    if tx.collection_fingerprint()? != fingerprint {
                        return Err(AppError::PreviewStale);
                    }
                    let plan = transfer::plan_csv(tx, &file, &options)?;
                    transfer::apply_csv(tx, rng.as_ref(), &plan, now, &cancel)
                })?;
                service.events.collection_changed(
                    &[
                        CollectionScope::Decks,
                        CollectionScope::Notes,
                        CollectionScope::Cards,
                        CollectionScope::Stats,
                    ],
                    now,
                );
                context.progress(total, total)?;
                Ok(Some((service.encode.import)(&outcome)))
            }),
        )
    }

    /// Starts an export (job `export`). An unknown or deleted deck fails before the job.
    pub fn export(&self, operation_id: String, request: ExportRequest) -> AppResult<JobRecord> {
        let recorded = {
            let operation = operation_id.clone();
            self.db.read(move |tx| tx.recorded_operation(&operation))?
        };
        if let Some(recorded) = recorded {
            return self.jobs.get(&JobId(recorded.result_json));
        }
        let deck_id = request.deck_id;
        // A deleted deck is `NOT_FOUND` too: `exportDeck` declares no `DELETED` (contract 1.x).
        self.db.read(move |tx| match tx.deck(deck_id)? {
            Some(deck) if deck.deleted_at.is_none() => Ok(()),
            _ => Err(AppError::NotFound { entity: "deck" }),
        })?;
        let service = self.clone();
        self.jobs.start(
            NewJob {
                kind: JobKind::Export,
                operation_id: Some(operation_id),
                total: Some(2),
            },
            Box::new(move |context| {
                let with_reviews = request.format == ExportFormat::Apkg;
                let snapshot = service
                    .db
                    .read(move |tx| transfer::export_snapshot(tx, request.deck_id, with_reviews))?;
                context.progress(1, 2)?;
                let id = context.id().clone();
                let outcome = service.write_export(&id, &snapshot, &request)?;
                context.begin_commit()?;
                service.prune(&id);
                context.progress(2, 2)?;
                Ok(Some((service.encode.export)(&outcome)))
            }),
        )
    }

    fn write_export(
        &self,
        id: &JobId,
        snapshot: &ExportSnapshot,
        request: &ExportRequest,
    ) -> AppResult<ExportOutcome> {
        fs::create_dir_all(&self.exports).map_err(|error| io_error(&error, "exports"))?;
        let extension = request.format.extension();
        let temp = self.exports.join(format!(".{id}.{extension}"));
        let target = self.exports.join(format!("{id}.{extension}"));
        let now = self.clock.now();
        let written = (|| -> AppResult<(u32, Losses)> {
            let file = File::create(&temp).map_err(|error| io_error(&error, "exports"))?;
            match request.format {
                ExportFormat::Csv => {
                    let (bytes, losses, rows) = transfer::csv_export(
                        snapshot,
                        request.include_scheduling,
                        request.formula_guard,
                    );
                    let mut out = BufWriter::new(file);
                    out.write_all(&bytes)
                        .and_then(|()| out.flush())
                        .and_then(|()| out.get_ref().sync_all())
                        .map_err(|error| io_error(&error, "exports"))?;
                    Ok((rows, losses))
                }
                ExportFormat::Apkg => {
                    let origin = snapshot
                        .notes
                        .iter()
                        .map(|(note, _)| note.created_at)
                        .chain(snapshot.cards.iter().map(|card| card.created_at))
                        .chain(snapshot.reviews.iter().map(|log| log.reviewed_at))
                        .chain(snapshot.cards.iter().filter_map(|card| card.due_at))
                        .chain(std::iter::once(now))
                        .min()
                        .unwrap_or(now);
                    let export = Export {
                        decks: &snapshot.decks,
                        notes: &snapshot.notes,
                        cards: &snapshot.cards,
                        reviews: &snapshot.reviews,
                        clock: AnkiClock::starting_at(snapshot.calendar, origin),
                        now,
                    };
                    let mut out = BufWriter::new(file);
                    let written = schema11::write_apkg(&export, &self.exports, &mut out).map_err(
                        |error| match error {
                            schema11::WriteError::Io(error) => io_error(&error, "exports"),
                            other => AppError::Internal(format!("apkg export: {other}")),
                        },
                    )?;
                    out.flush()
                        .and_then(|()| out.get_ref().sync_all())
                        .map_err(|error| io_error(&error, "exports"))?;
                    let mut losses = snapshot.losses.clone();
                    losses.merge(&written.losses);
                    Ok((written.cards, losses))
                }
            }
        })();
        let (card_count, losses) = match written {
            Ok(written) => written,
            Err(error) => {
                let _ = fs::remove_file(&temp);
                return Err(error);
            }
        };
        fs::rename(&temp, &target).map_err(|error| io_error(&error, "exports"))?;
        let deck_name = snapshot
            .decks
            .first()
            .map_or("deck", |deck| deck.name.as_str());
        tracing::info!(job_id = %id, format = extension, cards = card_count, "deck exported");
        Ok(ExportOutcome {
            file_name: safe_file_name(deck_name, extension),
            format: request.format,
            card_count,
            losses,
        })
    }

    /// Keeps the newest exports (and the one just written).
    fn prune(&self, keep: &JobId) {
        let Ok(entries) = fs::read_dir(&self.exports) else {
            return;
        };
        let mut files: Vec<(std::time::SystemTime, PathBuf)> = entries
            .flatten()
            .filter(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
            .filter_map(|entry| {
                let modified = entry.metadata().and_then(|meta| meta.modified()).ok()?;
                Some((modified, entry.path()))
            })
            .collect();
        files.sort_by(|a, b| b.0.cmp(&a.0));
        for (_, path) in files.into_iter().skip(EXPORTS_KEPT) {
            let is_current = path
                .file_stem()
                .is_some_and(|stem| stem.to_string_lossy() == keep.0);
            if !is_current {
                let _ = fs::remove_file(path);
            }
        }
    }

    /// The file of a finished export and the name to offer when saving it.
    pub fn export_file(&self, job_id: &JobId) -> AppResult<(PathBuf, String)> {
        let job = self.jobs.get(job_id)?;
        if job.kind != JobKind::Export || job.status != JobStatus::Succeeded {
            return Err(AppError::NotFound { entity: "export" });
        }
        let file_name = job
            .result_json
            .as_deref()
            .and_then(|json| serde_json::from_str::<serde_json::Value>(json).ok())
            .and_then(|value| value["fileName"].as_str().map(str::to_owned))
            .ok_or(AppError::NotFound { entity: "export" })?;
        [ExportFormat::Csv, ExportFormat::Apkg]
            .iter()
            .map(|format| {
                self.exports
                    .join(format!("{}.{}", job_id.0, format.extension()))
            })
            .find(|path| path.is_file())
            .map(|path| (path, file_name))
            .ok_or(AppError::NotFound { entity: "export" })
    }

    pub fn exports_dir(&self) -> &Path {
        &self.exports
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_are_safe() {
        assert_eq!(safe_file_name("Deutsch A1", "csv"), "Deutsch A1.csv");
        assert_eq!(safe_file_name("../a/b:c", "apkg"), "_a_b_c.apkg");
        assert_eq!(safe_file_name("...", "csv"), "deck.csv");
        assert_eq!(safe_file_name("Żółw", "csv"), "Żółw.csv");
    }
}
