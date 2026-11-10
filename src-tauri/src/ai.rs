//! AI association commands (plan Phase 13, ADR-18): `generate_association` queues requests in the
//! outbox and answers with the `llm` job; `set_ai_api_key` writes the BYOK key to the platform
//! secret store. No command ever returns the key, and no prompt or answer is logged.

use langualinker_application::AppError;
use langualinker_application::repositories::{RecordedOperation, UnitOfWork};
use langualinker_application::settings::{self as app_settings, AI_KEY_SECRET};
use langualinker_contracts::ErrorDto;
use langualinker_contracts::dto::{
    AiSettingsDto, GenerateAssociationInput, JobDto, SetAiApiKeyInput,
};
use langualinker_domain::{NoteId, ValidationError};
use serde_json::Value;
use tauri::State;

use crate::backend::{Backend, job_dto};
use crate::commands::{blocking, operation_id, parse_input};
use crate::settings::read_settings;
use crate::study::{app_error, emit, id};

const OP_SET_AI_KEY: &str = "setAiApiKey";
const MIN_KEY_CHARS: usize = 8;
const MAX_KEY_CHARS: usize = 512;

#[tauri::command]
pub async fn generate_association(
    backend: State<'_, Backend>,
    input: Value,
) -> Result<JobDto, ErrorDto> {
    let input: GenerateAssociationInput = parse_input(input)?;
    operation_id(&input.operation_id)?;
    let note = NoteId(id("noteId", &input.note_id)?);
    let llm = backend.collection()?.llm.clone();
    blocking(move || {
        llm.generate(&input.operation_id, note, input.variants)
            .map_err(app_error)
            .and_then(|job| job_dto(&job))
    })
    .await
}

/// A key is one token of printable characters; it is never echoed back, not even in errors.
fn validated_key(key: Option<String>) -> Result<Option<String>, ErrorDto> {
    let Some(key) = key else {
        return Ok(None);
    };
    let key = key.trim().to_owned();
    let length = key.chars().count();
    if !(MIN_KEY_CHARS..=MAX_KEY_CHARS).contains(&length)
        || key.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(app_error(ValidationError::new("apiKey", "format").into()));
    }
    Ok(Some(key))
}

#[tauri::command]
pub async fn set_ai_api_key(
    backend: State<'_, Backend>,
    input: Value,
) -> Result<AiSettingsDto, ErrorDto> {
    let input: SetAiApiKeyInput = parse_input(input)?;
    operation_id(&input.operation_id)?;
    let key = validated_key(input.api_key)?;
    let collection = backend.collection()?;
    let secrets = collection.secrets.clone().ok_or_else(|| {
        app_error(AppError::PermissionDenied {
            reason: "secretStore",
        })
    })?;
    let db = collection.db.clone();
    let operation = input.operation_id;
    let replayed = {
        let (db, operation) = (db.clone(), operation.clone());
        blocking(move || {
            db.read(move |tx| Ok(tx.recorded_operation(&operation)?.is_some()))
                .map_err(app_error)
        })
        .await?
    };
    if !replayed {
        // The store first: a retry after a failure in between writes the same key again.
        let store = secrets.clone();
        blocking(move || {
            match key {
                Some(key) => store.save(AI_KEY_SECRET, &key),
                None => store.remove(AI_KEY_SECRET).map(drop),
            }
            .map_err(app_error)
        })
        .await?;
        let now = collection.clock.now();
        let (db, operation) = (db.clone(), operation.clone());
        blocking(move || {
            db.write(move |tx| {
                if tx.recorded_operation(&operation)?.is_some() {
                    return Ok(());
                }
                app_settings::api_key_changed(tx, now)?;
                tx.record_operation(
                    &operation,
                    &RecordedOperation {
                        operation: OP_SET_AI_KEY.into(),
                        result_json: "null".into(),
                    },
                    now,
                )
            })
            .map_err(app_error)
        })
        .await?;
        emit(
            collection,
            &[langualinker_application::ports::CollectionScope::Settings],
            now,
        );
        tracing::info!("AI key changed");
    }
    let now = collection.clock.now();
    let secrets = collection.secrets.clone();
    // Only the AI part comes back, with `hasApiKey`; the key itself never does.
    blocking(move || read_settings(&db, secrets.as_ref(), now).map(|settings| settings.ai)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_single_printable_tokens() {
        assert_eq!(validated_key(None).unwrap(), None);
        assert_eq!(
            validated_key(Some("  sk-ant-api03-abcdef  ".into())).unwrap(),
            Some("sk-ant-api03-abcdef".into())
        );
        for bad in [
            "short",
            "has space inside",
            "tab\tkey12345",
            &"x".repeat(513),
        ] {
            let error = validated_key(Some(bad.to_owned())).unwrap_err();
            let json = serde_json::to_string(&error).unwrap();
            assert!(!json.contains(bad), "the key is never echoed: {json}");
            assert_eq!(error.details.unwrap()["field"], "apiKey");
        }
    }
}
