//! Contract v1 freeze (plan Phase 6): the Rust side of the dual-adapter contract tests.
//!
//! `v1/catalogue.json` and `v1/samples.json` are recorded from the demo adapter by
//! `frontend/tests/contractV1.test.ts`; the Tauri adapter replays the same samples there. Here the
//! same files are checked against the Rust contract, so the Phase 7 commands have one fixed target:
//! each command must accept the recorded `input` and return a payload of the recorded shape.

use std::collections::{BTreeSet, HashSet};

use langualinker_contracts::dto::{
    AboutDto, CollectionChangedEventDto, DiagnosticsReportDto, JobDto, JobEventDto,
};
use langualinker_contracts::{API_VERSION, AppInfoDto, ErrorCode, ErrorDto};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

const CATALOGUE: &str = include_str!("../v1/catalogue.json");
const SAMPLES: &str = include_str!("../v1/samples.json");

const KINDS: [&str; 4] = ["query", "mutation", "job", "subscription"];
const EVENTS: [&str; 2] = ["job_progress", "collection_changed"];
const PERMISSIONS: [&str; 5] = ["none", "file", "notifications", "network", "secret"];

fn catalogue() -> Value {
    serde_json::from_str(CATALOGUE).expect("catalogue.json is JSON")
}

fn samples() -> Value {
    serde_json::from_str(SAMPLES).expect("samples.json is JSON")
}

fn operations(catalogue: &Value) -> &Map<String, Value> {
    catalogue["operations"]
        .as_object()
        .expect("operations object")
}

fn strings(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .expect("array")
        .iter()
        .map(|item| item.as_str().expect("string"))
        .collect()
}

fn is_snake_case(command: &str) -> bool {
    !command.is_empty()
        && command
            .split('_')
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_lowercase()))
}

/// `YYYY-MM-DDTHH:MM:SS.mmmZ`, the only timestamp form that crosses IPC.
fn is_iso_utc(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 24
        && bytes.iter().enumerate().all(|(index, byte)| match index {
            4 | 7 => *byte == b'-',
            10 => *byte == b'T',
            13 | 16 => *byte == b':',
            19 => *byte == b'.',
            23 => *byte == b'Z',
            _ => byte.is_ascii_digit(),
        })
}

fn is_study_day(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 10
        && bytes.iter().enumerate().all(|(index, byte)| match index {
            4 | 7 => *byte == b'-',
            _ => byte.is_ascii_digit(),
        })
}

/// Plan §4.1 transport rules; mirrors `frontend/tests/contract/transport.ts`.
fn transport_problems(value: &Value, path: &str, problems: &mut Vec<String>) {
    match value {
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                transport_problems(item, &format!("{path}[{index}]"), problems);
            }
        }
        Value::Object(fields) => {
            for (key, field) in fields {
                let field_path = format!("{path}.{key}");
                let is_id = (key == "id" || key.ends_with("Id")) && key != "operationId";
                if is_id && !(field.is_string() || field.is_null()) {
                    problems.push(format!("{field_path}: id must be a string"));
                }
                if key.ends_with("Ids")
                    && field
                        .as_array()
                        .is_some_and(|ids| ids.iter().any(|id| !id.is_string()))
                {
                    problems.push(format!("{field_path}: ids must be strings"));
                }
                if key.ends_with("At") && !field.as_str().map_or(field.is_null(), is_iso_utc) {
                    problems.push(format!("{field_path}: not UTC ISO-8601 with milliseconds"));
                }
                if key == "day" && !field.as_str().is_some_and(is_study_day) {
                    problems.push(format!("{field_path}: not a YYYY-MM-DD study day"));
                }
                transport_problems(field, &field_path, problems);
            }
        }
        _ => {}
    }
}

#[test]
fn frozen_files_carry_the_current_api_version() {
    assert_eq!(catalogue()["apiVersion"], API_VERSION);
    assert_eq!(samples()["apiVersion"], API_VERSION);
}

#[test]
fn catalogue_error_codes_match_rust_in_order() {
    let rust: Vec<&str> = ErrorCode::ALL.iter().map(|code| code.as_str()).collect();
    assert_eq!(strings(&catalogue()["errorCodes"]), rust);
}

#[test]
fn catalogue_operations_are_well_formed() {
    let catalogue = catalogue();
    let mut commands = HashSet::new();
    for (name, spec) in operations(&catalogue) {
        let command = spec["command"].as_str().expect("command");
        assert!(
            commands.insert(command),
            "{name}: duplicate command {command}"
        );
        assert!(
            is_snake_case(command),
            "{name}: {command} is not snake_case"
        );
        let kind = spec["kind"].as_str().expect("kind");
        assert!(KINDS.contains(&kind), "{name}: unknown kind {kind}");
        assert!(
            PERMISSIONS.contains(&spec["permission"].as_str().expect("permission")),
            "{name}: unknown permission"
        );
        for code in strings(&spec["errors"]) {
            assert!(
                serde_json::from_value::<ErrorCode>(Value::from(code)).is_ok(),
                "{name}: unknown error {code}"
            );
        }
        let emits = strings(&spec["emits"]);
        assert!(
            emits.iter().all(|event| EVENTS.contains(event)),
            "{name}: unknown event"
        );
        if kind == "job" {
            assert!(
                emits.contains(&"job_progress"),
                "{name}: a job must emit job_progress"
            );
        }
        if kind == "subscription" {
            assert!(
                EVENTS.contains(&command),
                "{name}: subscribes to an unknown event"
            );
        }
    }
}

#[test]
fn every_callable_operation_has_a_sample_with_its_command() {
    let (catalogue, samples) = (catalogue(), samples());
    let operations = operations(&catalogue);
    let mut covered = BTreeSet::new();
    for (index, call) in samples["calls"]
        .as_array()
        .expect("calls")
        .iter()
        .enumerate()
    {
        let name = call["operation"].as_str().expect("operation");
        let spec = &operations
            .get(name)
            .unwrap_or_else(|| panic!("#{index}: {name} not in the catalogue"));
        assert_eq!(call["command"], spec["command"], "#{index} {name}");
        assert!(
            call["args"].as_array().is_some_and(|args| args.len() <= 1),
            "#{index} {name}: one input at most"
        );
        assert!(
            call.get("output").is_some() != call.get("error").is_some(),
            "#{index} {name}: output xor error"
        );
        covered.insert(name);
    }
    let missing: Vec<&String> = operations
        .iter()
        .filter(|(name, spec)| spec["kind"] != "subscription" && !covered.contains(name.as_str()))
        .map(|(name, _)| name)
        .collect();
    assert!(
        missing.is_empty(),
        "operations without a sample: {missing:?}"
    );
}

#[test]
fn samples_follow_the_transport_rules() {
    let mut problems = Vec::new();
    transport_problems(&samples(), "$", &mut problems);
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn error_samples_are_rust_error_dtos_with_declared_codes() {
    let (catalogue, samples) = (catalogue(), samples());
    let operations = operations(&catalogue);
    let mut seen = 0;
    for call in samples["calls"].as_array().expect("calls") {
        let Some(error) = call.get("error") else {
            continue;
        };
        let name = call["operation"].as_str().expect("operation");
        let dto: ErrorDto = serde_json::from_value(error.clone()).expect("ErrorDto");
        assert_eq!(dto.message_key, format!("errors.{}", dto.code.as_str()));
        assert!(
            strings(&operations[name]["errors"]).contains(&dto.code.as_str()),
            "{name}: {} is not declared",
            dto.code.as_str()
        );
        // Rust serializes the same JSON the frontend received.
        assert_eq!(serde_json::to_value(&dto).unwrap(), *error);
        seen += 1;
    }
    assert!(seen > 0, "v1 freezes at least one error payload");
}

#[test]
fn app_info_sample_round_trips_through_the_rust_dto() {
    let samples = samples();
    let call = samples["calls"]
        .as_array()
        .expect("calls")
        .iter()
        .find(|call| call["operation"] == "getAppInfo")
        .expect("getAppInfo sample");
    let dto: AppInfoDto = serde_json::from_value(call["output"].clone()).expect("AppInfoDto");
    assert_eq!(dto.api_version, API_VERSION);
    assert_eq!(serde_json::to_value(&dto).unwrap(), call["output"]);
}

/// JSON numbers compared by value: an `f64` field writes `0.0` where the recorded sample has `0`;
/// JavaScript reads both as the same number.
fn integral(value: Value) -> Value {
    match value {
        Value::Number(number) => match number.as_f64() {
            Some(float)
                if float.fract() == 0.0
                    && float.abs() < 9.0e15
                    && !number.is_i64()
                    && !number.is_u64() =>
            {
                Value::from(float as i64)
            }
            _ => Value::Number(number),
        },
        Value::Array(items) => Value::Array(items.into_iter().map(integral).collect()),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, value)| (key, integral(value)))
                .collect(),
        ),
        other => other,
    }
}

/// Checklist rule 2: the Rust output DTO reads every frozen output of the command and writes the
/// same JSON back (`deny_unknown_fields` makes a renamed or extra field fail here).
fn round_trips<T: DeserializeOwned + Serialize>(operation: &str) -> usize {
    let samples = samples();
    let mut count = 0;
    for call in samples["calls"].as_array().expect("calls") {
        if call["operation"] != operation || call.get("output").is_none() {
            continue;
        }
        let dto: T = serde_json::from_value(call["output"].clone())
            .unwrap_or_else(|error| panic!("{operation}: {error}"));
        assert_eq!(
            integral(serde_json::to_value(&dto).unwrap()),
            integral(call["output"].clone()),
            "{operation}"
        );
        count += 1;
    }
    count
}

#[test]
fn phase7_outputs_round_trip_through_their_rust_dtos() {
    assert!(
        round_trips::<JobDto>("getJobStatus") >= 6,
        "every job result variant"
    );
    assert!(round_trips::<JobDto>("cancelJob") >= 1);
    assert_eq!(round_trips::<AboutDto>("getAboutInfo"), 1);
    assert_eq!(
        round_trips::<DiagnosticsReportDto>("getDiagnosticsReport"),
        1
    );
}

#[test]
fn phase8_outputs_round_trip_through_their_rust_dtos() {
    use langualinker_contracts::dto::{AttributionDto, DashboardDto, OnboardingStateDto};
    assert!(round_trips::<OnboardingStateDto>("getOnboardingState") >= 1);
    assert!(round_trips::<DashboardDto>("getDashboard") >= 1);
    assert!(round_trips::<Vec<AttributionDto>>("getAttributions") >= 1);
    assert!(round_trips::<JobDto>("completeOnboarding") >= 1);
}

#[test]
fn deck_import_samples_round_trip_through_their_rust_dtos() {
    use langualinker_contracts::dto::{
        CommitDeckImportInput, DeckImportPreviewDto, FileHandleDto, PickFileInput,
        PreviewDeckImportInput,
    };
    assert!(round_trips::<DeckImportPreviewDto>("previewDeckImport") >= 1);
    assert!(round_trips::<JobDto>("commitDeckImport") >= 1);
    assert!(round_trips::<Option<FileHandleDto>>("pickFile") >= 1);
    let samples = samples();
    for call in samples["calls"].as_array().expect("calls") {
        let input = call["args"][0].clone();
        let back = match call["operation"].as_str() {
            Some("pickFile") => serde_json::to_value(
                serde_json::from_value::<PickFileInput>(input.clone()).unwrap(),
            ),
            Some("previewDeckImport") => serde_json::to_value(
                serde_json::from_value::<PreviewDeckImportInput>(input.clone()).unwrap(),
            ),
            Some("commitDeckImport") => serde_json::to_value(
                serde_json::from_value::<CommitDeckImportInput>(input.clone()).unwrap(),
            ),
            _ => continue,
        };
        assert_eq!(back.unwrap(), input);
    }
}

#[test]
fn phase8_inputs_deserialize_into_their_rust_dtos() {
    use langualinker_contracts::dto::CompleteOnboardingInput;
    let samples = samples();
    let mut seen = 0;
    for call in samples["calls"].as_array().expect("calls") {
        if call["operation"] == "completeOnboarding" {
            let input = call["args"][0].clone();
            let dto: CompleteOnboardingInput = serde_json::from_value(input.clone()).unwrap();
            assert_eq!(serde_json::to_value(&dto).unwrap(), input);
            seen += 1;
        }
    }
    assert!(seen >= 1);
}

#[test]
fn phase10_outputs_round_trip_through_their_rust_dtos() {
    use langualinker_contracts::dto::{
        AssociationDto, CardActionResultDto, NoteContentDto, RatingPreviewDto, ReviewResultDto,
        SessionSummaryDto, StudySessionDto, WordLinkDto,
    };
    assert!(round_trips::<StudySessionDto>("startStudy") >= 1);
    assert!(round_trips::<StudySessionDto>("resumeStudy") >= 1);
    assert!(round_trips::<StudySessionDto>("updateStudySession") >= 1);
    assert!(round_trips::<RatingPreviewDto>("getRatingPreview") >= 1);
    assert!(round_trips::<ReviewResultDto>("rateCard") >= 1);
    assert!(round_trips::<ReviewResultDto>("undoReview") >= 1);
    assert!(round_trips::<CardActionResultDto>("applyCardAction") >= 1);
    assert!(round_trips::<SessionSummaryDto>("getSessionSummary") >= 1);
    assert!(round_trips::<NoteContentDto>("getNoteContent") >= 1);
    assert!(round_trips::<Vec<AssociationDto>>("setFavoriteAssociation") >= 1);
    assert!(round_trips::<WordLinkDto>("resolveWordLink") >= 1);
}

#[test]
fn phase10_inputs_deserialize_into_their_rust_dtos() {
    use langualinker_contracts::dto::{
        CardActionInput, PreviewInput, RateCardInput, ResolveWordLinkInput,
        SetFavoriteAssociationInput, StartStudyInput, UndoInput, UpdateStudySessionInput,
    };
    fn back<T: DeserializeOwned + Serialize>(input: &Value) -> Value {
        serde_json::to_value(serde_json::from_value::<T>(input.clone()).unwrap()).unwrap()
    }
    let samples = samples();
    let mut seen = 0;
    for call in samples["calls"].as_array().expect("calls") {
        let input = &call["args"][0];
        let read = match call["operation"].as_str() {
            Some("startStudy") => back::<StartStudyInput>(input),
            Some("updateStudySession") => back::<UpdateStudySessionInput>(input),
            Some("getRatingPreview") => back::<PreviewInput>(input),
            Some("rateCard") => back::<RateCardInput>(input),
            Some("undoReview") => back::<UndoInput>(input),
            Some("applyCardAction") => back::<CardActionInput>(input),
            Some("setFavoriteAssociation") => back::<SetFavoriteAssociationInput>(input),
            Some("resolveWordLink") => back::<ResolveWordLinkInput>(input),
            // Bare ids.
            Some("resumeStudy" | "getSessionSummary" | "getNoteContent") => {
                let _: String = serde_json::from_value(input.clone()).expect("id string");
                input.clone()
            }
            _ => continue,
        };
        assert_eq!(&read, input, "{}", call["operation"]);
        seen += 1;
    }
    assert!(seen >= 11);
}

#[test]
fn phase7_inputs_are_a_bare_job_id() {
    let samples = samples();
    for call in samples["calls"].as_array().expect("calls") {
        if call["operation"] == "getJobStatus" || call["operation"] == "cancelJob" {
            let args = call["args"].as_array().expect("args");
            assert_eq!(args.len(), 1);
            let _: String = serde_json::from_value(args[0].clone()).expect("job id string");
        }
    }
}

#[test]
fn event_samples_round_trip_through_their_rust_dtos() {
    let samples = samples();
    let job: JobEventDto =
        serde_json::from_value(samples["events"]["job_progress"].clone()).unwrap();
    assert_eq!(
        serde_json::to_value(&job).unwrap(),
        samples["events"]["job_progress"]
    );
    let changed: CollectionChangedEventDto =
        serde_json::from_value(samples["events"]["collection_changed"].clone()).unwrap();
    assert_eq!(
        serde_json::to_value(&changed).unwrap(),
        samples["events"]["collection_changed"]
    );
}

#[test]
fn phase11_outputs_round_trip_through_their_rust_dtos() {
    use langualinker_contracts::dto::{
        BulkPreviewDto, BulkResultDto, CardPageDto, DeckOverviewDto, DeleteDeckResultDto,
        DeleteNoteResultDto, DuplicateCheckDto, EditorDataDto, NoteDetailsDto, PresetDto,
        WordMatchDto,
    };
    assert!(round_trips::<DeckOverviewDto>("getDeck") >= 1);
    assert!(round_trips::<DeckOverviewDto>("createDeck") >= 1);
    assert!(round_trips::<DeckOverviewDto>("updateDeck") >= 1);
    assert!(round_trips::<DeleteDeckResultDto>("deleteDeck") >= 1);
    assert!(round_trips::<Vec<PresetDto>>("listPresets") >= 1);
    assert!(round_trips::<PresetDto>("savePreset") >= 1);
    assert!(round_trips::<NoteDetailsDto>("getNoteDetails") >= 1);
    assert!(round_trips::<EditorDataDto>("getEditorData") >= 1);
    assert!(round_trips::<DuplicateCheckDto>("checkDuplicate") >= 1);
    assert!(round_trips::<NoteDetailsDto>("saveNote") >= 1);
    assert!(round_trips::<DeleteNoteResultDto>("deleteNote") >= 1);
    assert!(round_trips::<NoteDetailsDto>("updateNoteContent") >= 1);
    assert!(round_trips::<Vec<WordMatchDto>>("searchWords") >= 1);
    assert!(round_trips::<CardPageDto>("searchCards") >= 1);
    assert!(round_trips::<BulkPreviewDto>("previewBulkAction") >= 1);
    assert!(round_trips::<BulkResultDto>("applyBulkAction") >= 1);
}

#[test]
fn phase11_inputs_deserialize_into_their_rust_dtos() {
    use langualinker_contracts::dto::{
        BulkActionInput, BulkPreviewInput, CreateDeckInput, DeleteDeckInput, DeleteNoteInput,
        DuplicateCheckInput, SaveNoteInput, SavePresetInput, SearchInput, SearchWordsInput,
        UpdateDeckInput, UpdateNoteContentInput,
    };
    fn back<T: DeserializeOwned + Serialize>(input: &Value) -> Value {
        serde_json::to_value(serde_json::from_value::<T>(input.clone()).unwrap()).unwrap()
    }
    let samples = samples();
    let mut seen = 0;
    for call in samples["calls"].as_array().expect("calls") {
        let input = &call["args"][0];
        let read = match call["operation"].as_str() {
            Some("createDeck") => back::<CreateDeckInput>(input),
            Some("updateDeck") => back::<UpdateDeckInput>(input),
            Some("deleteDeck") => back::<DeleteDeckInput>(input),
            Some("savePreset") => back::<SavePresetInput>(input),
            Some("checkDuplicate") => back::<DuplicateCheckInput>(input),
            Some("saveNote") => back::<SaveNoteInput>(input),
            Some("deleteNote") => back::<DeleteNoteInput>(input),
            Some("updateNoteContent") => back::<UpdateNoteContentInput>(input),
            Some("searchWords") => back::<SearchWordsInput>(input),
            Some("searchCards") => back::<SearchInput>(input),
            Some("previewBulkAction") => back::<BulkPreviewInput>(input),
            Some("applyBulkAction") => back::<BulkActionInput>(input),
            // Bare ids.
            Some("getDeck" | "getNoteDetails") => {
                let _: String = serde_json::from_value(input.clone()).expect("id string");
                input.clone()
            }
            _ => continue,
        };
        assert_eq!(&read, input, "{}", call["operation"]);
        seen += 1;
    }
    assert!(seen >= 14);
}

#[test]
fn a_deck_patch_tells_a_missing_parent_from_a_null_one() {
    use langualinker_contracts::dto::DeckPatchDto;
    let missing: DeckPatchDto = serde_json::from_value(serde_json::json!({"name": "A"})).unwrap();
    assert_eq!(missing.parent_id, None);
    let root: DeckPatchDto = serde_json::from_value(serde_json::json!({"parentId": null})).unwrap();
    assert_eq!(root.parent_id, Some(None));
    let moved: DeckPatchDto = serde_json::from_value(serde_json::json!({"parentId": "4"})).unwrap();
    assert_eq!(moved.parent_id, Some(Some("4".to_owned())));
    assert_eq!(
        serde_json::to_value(&root).unwrap(),
        serde_json::json!({"parentId": null})
    );
    assert_eq!(
        serde_json::to_value(&missing).unwrap(),
        serde_json::json!({"name": "A"})
    );
}
