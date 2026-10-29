//! Contract v1 freeze (plan Phase 6): the Rust side of the dual-adapter contract tests.
//!
//! `v1/catalogue.json` and `v1/samples.json` are recorded from the demo adapter by
//! `frontend/tests/contractV1.test.ts`; the Tauri adapter replays the same samples there. Here the
//! same files are checked against the Rust contract, so the Phase 7 commands have one fixed target:
//! each command must accept the recorded `input` and return a payload of the recorded shape.

use std::collections::{BTreeSet, HashSet};

use langualinker_contracts::{API_VERSION, AppInfoDto, ErrorCode, ErrorDto};
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
