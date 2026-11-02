//! JSON columns: note fields, preset steps and weights, review snapshots, job details.
//! Field names are fixed here; the serialized form is part of the schema.

use langualinker_application::jobs::JobDetail;
use langualinker_application::{AppError, AppResult};
use langualinker_domain::TimestampMs;
use langualinker_domain::card::{CardFlag, CardFsrsState, CardState, QueueState};
use langualinker_domain::note::NoteFields;
use langualinker_domain::review::CardSnapshot;
use serde_json::{Map, Value, json};

fn bad(column: &str) -> AppError {
    AppError::Internal(format!("malformed JSON in {column}"))
}

pub fn note_fields_to_json(fields: &NoteFields) -> String {
    json!({ "front": fields.front, "back": fields.back }).to_string()
}

pub fn note_fields_from_json(text: &str) -> AppResult<NoteFields> {
    let value: Value = serde_json::from_str(text).map_err(|_| bad("notes.fields"))?;
    let field = |name: &str| {
        value
            .get(name)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| bad("notes.fields"))
    };
    Ok(NoteFields {
        front: field("front")?,
        back: field("back")?,
    })
}

pub fn u32_list_to_json(values: &[u32]) -> String {
    Value::from(values.to_vec()).to_string()
}

pub fn u32_list_from_json(text: &str, column: &str) -> AppResult<Vec<u32>> {
    serde_json::from_str(text).map_err(|_| bad(column))
}

pub fn f64_list_to_json(values: &[f64]) -> String {
    Value::from(values.to_vec()).to_string()
}

pub fn f64_list_from_json(text: &str, column: &str) -> AppResult<Vec<f64>> {
    serde_json::from_str(text).map_err(|_| bad(column))
}

pub fn snapshot_to_json(snapshot: &CardSnapshot) -> String {
    json!({
        "state": snapshot.state.db_code(),
        "queue": snapshot.queue.db_code(),
        "dueAt": snapshot.due_at.map(|at| at.0),
        "position": snapshot.position,
        "stability": snapshot.fsrs.stability,
        "difficulty": snapshot.fsrs.difficulty,
        "lastReviewAt": snapshot.fsrs.last_review_at.map(|at| at.0),
        "scheduledDays": snapshot.fsrs.scheduled_days,
        "elapsedDays": snapshot.fsrs.elapsed_days,
        "legacyEase": snapshot.legacy_ease,
        "reps": snapshot.reps,
        "lapses": snapshot.lapses,
        "learningStep": snapshot.learning_step,
        "buriedUntil": snapshot.buried_until.map(|at| at.0),
        "flag": snapshot.flag.db_code(),
        "version": snapshot.version,
    })
    .to_string()
}

pub fn snapshot_from_json(text: &str) -> AppResult<CardSnapshot> {
    let value: Map<String, Value> = serde_json::from_str(text).map_err(|_| bad("card snapshot"))?;
    let int = |name: &str| {
        value
            .get(name)
            .and_then(Value::as_i64)
            .ok_or_else(|| bad(name))
    };
    let opt_int = |name: &str| -> AppResult<Option<i64>> {
        match value.get(name) {
            None | Some(Value::Null) => Ok(None),
            Some(other) => other.as_i64().map(Some).ok_or_else(|| bad(name)),
        }
    };
    let opt_float = |name: &str| -> AppResult<Option<f64>> {
        match value.get(name) {
            None | Some(Value::Null) => Ok(None),
            Some(other) => other.as_f64().map(Some).ok_or_else(|| bad(name)),
        }
    };
    Ok(CardSnapshot {
        state: CardState::from_db_code(int("state")?)?,
        queue: QueueState::from_db_code(int("queue")?)?,
        due_at: opt_int("dueAt")?.map(TimestampMs),
        position: int("position")?,
        fsrs: CardFsrsState {
            stability: opt_float("stability")?,
            difficulty: opt_float("difficulty")?,
            last_review_at: opt_int("lastReviewAt")?.map(TimestampMs),
            scheduled_days: int("scheduledDays")?,
            elapsed_days: int("elapsedDays")?,
        },
        legacy_ease: opt_int("legacyEase")?,
        reps: int("reps")?,
        lapses: int("lapses")?,
        learning_step: opt_int("learningStep")?
            .map(|step| u32::try_from(step).map_err(|_| bad("learningStep")))
            .transpose()?,
        buried_until: opt_int("buriedUntil")?.map(TimestampMs),
        flag: CardFlag::from_db_code(int("flag")?)?,
        version: int("version")?,
    })
}

pub fn job_details_to_json(details: &[(String, JobDetail)]) -> String {
    let map: Map<String, Value> = details
        .iter()
        .map(|(key, value)| {
            let value = match value {
                JobDetail::Text(text) => Value::from(text.as_str()),
                JobDetail::Number(number) => Value::from(*number),
            };
            (key.clone(), value)
        })
        .collect();
    Value::Object(map).to_string()
}

pub fn job_details_from_json(text: &str) -> AppResult<Vec<(String, JobDetail)>> {
    let map: Map<String, Value> =
        serde_json::from_str(text).map_err(|_| bad("jobs.error_details"))?;
    map.into_iter()
        .map(|(key, value)| {
            let detail = match value {
                Value::String(text) => JobDetail::Text(text),
                Value::Number(number) => {
                    JobDetail::Number(number.as_i64().ok_or_else(|| bad("jobs.error_details"))?)
                }
                _ => return Err(bad("jobs.error_details")),
            };
            Ok((key, detail))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_round_trips_with_nulls_and_values() {
        let snapshot = CardSnapshot {
            state: CardState::Relearning,
            queue: QueueState::DayLearn,
            due_at: Some(TimestampMs(1_772_438_400_000)),
            position: 4,
            fsrs: CardFsrsState {
                stability: Some(12.5),
                difficulty: Some(6.25),
                last_review_at: None,
                scheduled_days: 3,
                elapsed_days: 11,
            },
            legacy_ease: None,
            reps: 9,
            lapses: 2,
            learning_step: Some(0),
            buried_until: None,
            flag: CardFlag::Blue,
            version: 17,
        };
        assert_eq!(
            snapshot_from_json(&snapshot_to_json(&snapshot)).unwrap(),
            snapshot
        );
    }

    #[test]
    fn note_fields_keep_unicode() {
        let fields = NoteFields {
            front: "Kühlschrank 🧊".into(),
            back: "lodówka\n„cytat”".into(),
        };
        assert_eq!(
            note_fields_from_json(&note_fields_to_json(&fields)).unwrap(),
            fields
        );
        assert!(note_fields_from_json("{\"front\":1}").is_err());
    }
}
