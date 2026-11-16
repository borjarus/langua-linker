//! CSV import and deck export commands (plan Phase 14, ADR-19): `previewImport`,
//! `commitImport` (job `import`) and `exportDeck` (job `export`); `shareFile` for exports lives
//! with the backups in `settings.rs`.

use langualinker_application::AppError;
use langualinker_application::transfer::{
    CsvOptions, CsvOutcome, CsvTarget, DuplicatePolicy as AppDuplicatePolicy,
    ExportFormat as AppFormat,
};
use langualinker_contracts::ErrorDto;
use langualinker_contracts::dto::{
    CommitImportInput, CsvColumnTarget, CsvDelimiter, CsvEncoding, DuplicatePolicy,
    ExportDeckInput, ExportFormat, ExportLossDto, ImportCountsDto, ImportInput, ImportPreviewDto,
    ImportRowErrorDto, JobDto, JobResultDto,
};
use langualinker_domain::DeckId;
use langualinker_infrastructure::transfer::{CsvPreview, ExportOutcome, ExportRequest};
use langualinker_interop::csv::{Delimiter, Encoding};
use serde_json::Value;
use tauri::State;

use crate::backend::{Backend, error_dto, job_dto};
use crate::commands::{blocking, operation_id, parse_input};
use crate::files::PickedFiles;
use crate::study::{app_error, id};

fn delimiter_dto(value: Delimiter) -> CsvDelimiter {
    match value {
        Delimiter::Comma => CsvDelimiter::Comma,
        Delimiter::Semicolon => CsvDelimiter::Semicolon,
        Delimiter::Tab => CsvDelimiter::Tab,
    }
}

fn delimiter(value: CsvDelimiter) -> Delimiter {
    match value {
        CsvDelimiter::Comma => Delimiter::Comma,
        CsvDelimiter::Semicolon => Delimiter::Semicolon,
        CsvDelimiter::Tab => Delimiter::Tab,
    }
}

fn encoding(value: CsvEncoding) -> Encoding {
    match value {
        CsvEncoding::Utf8 => Encoding::Utf8,
        CsvEncoding::Utf16Le => Encoding::Utf16Le,
        CsvEncoding::Utf16Be => Encoding::Utf16Be,
        CsvEncoding::Windows1252 => Encoding::Windows1252,
    }
}

const TARGETS: [(CsvColumnTarget, CsvTarget); 12] = [
    (CsvColumnTarget::Front, CsvTarget::Front),
    (CsvColumnTarget::Back, CsvTarget::Back),
    (CsvColumnTarget::Tags, CsvTarget::Tags),
    (CsvColumnTarget::Ignore, CsvTarget::Ignore),
    (CsvColumnTarget::Guid, CsvTarget::Guid),
    (CsvColumnTarget::Deck, CsvTarget::Deck),
    (CsvColumnTarget::NoteType, CsvTarget::NoteType),
    (CsvColumnTarget::Due, CsvTarget::Due),
    (CsvColumnTarget::Interval, CsvTarget::Interval),
    (CsvColumnTarget::Reps, CsvTarget::Reps),
    (CsvColumnTarget::Lapses, CsvTarget::Lapses),
    (CsvColumnTarget::Position, CsvTarget::Position),
];

fn target(value: CsvColumnTarget) -> CsvTarget {
    TARGETS
        .iter()
        .find(|(dto, _)| *dto == value)
        .map(|(_, target)| *target)
        .unwrap_or(CsvTarget::Ignore)
}

fn target_dto(value: CsvTarget) -> CsvColumnTarget {
    TARGETS
        .iter()
        .find(|(_, target)| *target == value)
        .map(|(dto, _)| *dto)
        .unwrap_or(CsvColumnTarget::Ignore)
}

fn policy(value: DuplicatePolicy) -> AppDuplicatePolicy {
    match value {
        DuplicatePolicy::Skip => AppDuplicatePolicy::Skip,
        DuplicatePolicy::Update => AppDuplicatePolicy::Update,
        DuplicatePolicy::Duplicate => AppDuplicatePolicy::Duplicate,
    }
}

fn format(value: AppFormat) -> ExportFormat {
    match value {
        AppFormat::Csv => ExportFormat::Csv,
        AppFormat::Apkg => ExportFormat::Apkg,
    }
}

fn row_errors(errors: &[langualinker_application::transfer::RowError]) -> Vec<ImportRowErrorDto> {
    errors
        .iter()
        .map(|row| ImportRowErrorDto {
            row: row.line,
            error: error_dto(&AppError::Validation(row.error.clone())),
        })
        .collect()
}

/// `JobResultDto::Import` of a CSV import as stored in `jobs.result`.
pub fn encode_import(outcome: &CsvOutcome) -> String {
    let result = JobResultDto::Import {
        deck_id: outcome.deck_id.0.to_string(),
        row_errors: row_errors(&outcome.row_errors),
        create: outcome.counts.create,
        update: outcome.counts.update,
        skip: outcome.counts.skip,
        duplicate: outcome.counts.duplicate,
        error: outcome.counts.error,
    };
    serde_json::to_string(&result).expect("job result serializes")
}

/// `JobResultDto::Export` as stored in `jobs.result`.
pub fn encode_export(outcome: &ExportOutcome) -> String {
    let result = JobResultDto::Export {
        file_name: outcome.file_name.clone(),
        card_count: outcome.card_count,
        format: Some(format(outcome.format)),
        losses: outcome
            .losses
            .iter()
            .map(|(loss, count)| ExportLossDto {
                code: loss.code().to_owned(),
                count,
            })
            .collect(),
    };
    serde_json::to_string(&result).expect("job result serializes")
}

fn preview_dto(preview: CsvPreview, has_header: bool) -> ImportPreviewDto {
    let file = &preview.file;
    let columns = (0..file.columns)
        .map(|index| {
            file.header
                .as_ref()
                .and_then(|header| header.get(index))
                .map_or_else(|| (index + 1).to_string(), |name| name.trim().to_owned())
        })
        .collect();
    let sample_rows = file
        .records
        .iter()
        .take(langualinker_application::transfer::SAMPLE_ROWS)
        .map(|record| {
            (0..file.columns)
                .map(|index| record.fields.get(index).cloned().unwrap_or_default())
                .collect()
        })
        .collect();
    let plan = &preview.plan;
    ImportPreviewDto {
        file_name: file.file_name.clone(),
        detected_delimiter: file.detected.map(delimiter_dto),
        delimiter: delimiter_dto(file.delimiter),
        encoding: file.encoding.label().to_owned(),
        has_bom: file.has_bom,
        has_header: has_header || file.header.is_some(),
        columns,
        sample_rows,
        mapping: plan.mapping.iter().copied().map(target_dto).collect(),
        total_rows: u32::try_from(file.records.len()).unwrap_or(u32::MAX),
        counts: ImportCountsDto {
            create: plan.counts.create,
            update: plan.counts.update,
            skip: plan.counts.skip,
            duplicate: plan.counts.duplicate,
            error: plan.counts.error,
        },
        row_errors: row_errors(&plan.row_errors),
        target_deck_id: plan.target_deck_id.map(|deck| deck.0.to_string()),
        target_deck_name: plan.target_deck_name.clone(),
        new_decks: plan.new_decks.clone(),
        preview_token: preview.token,
    }
}

#[tauri::command]
pub async fn preview_import(
    backend: State<'_, Backend>,
    files: State<'_, PickedFiles>,
    input: Value,
) -> Result<ImportPreviewDto, ErrorDto> {
    let input: ImportInput = parse_input(input)?;
    let file = files
        .get(&input.file_token)
        .ok_or_else(|| error_dto(&AppError::NotFound { entity: "file" }))?;
    let target_deck = input
        .target_deck_id
        .as_deref()
        .map(|value| id("targetDeckId", value).map(DeckId))
        .transpose()?;
    let options = CsvOptions {
        delimiter: input.delimiter.map(delimiter),
        encoding: input.encoding.map(encoding),
        has_header: input.has_header,
        mapping: input
            .mapping
            .map(|mapping| mapping.into_iter().map(target).collect()),
        duplicate_policy: policy(input.duplicate_policy),
        target_deck,
        formula_guard: input.formula_guard.unwrap_or(false),
    };
    let has_header = input.has_header;
    let transfer = backend.collection()?.transfer.clone();
    blocking(move || {
        transfer
            .preview_csv(&file.name, &file.bytes, options)
            .map(|preview| preview_dto(preview, has_header))
            .map_err(app_error)
    })
    .await
}

#[tauri::command]
pub async fn commit_import(backend: State<'_, Backend>, input: Value) -> Result<JobDto, ErrorDto> {
    let input: CommitImportInput = parse_input(input)?;
    operation_id(&input.operation_id)?;
    let transfer = backend.collection()?.transfer.clone();
    blocking(move || {
        transfer
            .commit_csv(input.operation_id, &input.preview_token)
            .map_err(app_error)
            .and_then(|job| job_dto(&job))
    })
    .await
}

#[tauri::command]
pub async fn export_deck(backend: State<'_, Backend>, input: Value) -> Result<JobDto, ErrorDto> {
    let input: ExportDeckInput = parse_input(input)?;
    operation_id(&input.operation_id)?;
    let request = ExportRequest {
        deck_id: DeckId(id("deckId", &input.deck_id)?),
        format: match input.format {
            ExportFormat::Csv => AppFormat::Csv,
            ExportFormat::Apkg => AppFormat::Apkg,
        },
        include_scheduling: input.include_scheduling,
        formula_guard: input.formula_guard.unwrap_or(false),
    };
    let transfer = backend.collection()?.transfer.clone();
    blocking(move || {
        transfer
            .export(input.operation_id, request)
            .map_err(app_error)
            .and_then(|job| job_dto(&job))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_column_target_maps_both_ways() {
        for (dto, target_value) in TARGETS {
            assert_eq!(target(dto), target_value);
            assert_eq!(target_dto(target_value), dto);
        }
        assert_eq!(TARGETS.len(), CsvTarget::ALL.len());
    }
}
