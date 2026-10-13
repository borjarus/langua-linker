//! Phase 1 native spikes (ADR-07, ADR-12). Commands here are registered in every build but granted
//! only by the `diagnostics` capability, which only the dev variant enables; they also refuse to run
//! outside the dev variant.

use langualinker_contracts::{ErrorCode, ErrorDto, ProbeStepDto, SecretStoreProbeDto};
use langualinker_infrastructure::secrets::{self, SecretStore};

use crate::APP_VARIANT;

/// Platform-store service name: the app identifier of the running variant.
pub struct SecretService(pub String);

const PROBE_SECRET: &str = "diagnostics-probe";
/// Left in the store on purpose, so the next launch can report persistence across a restart.
const PROBE_MARKER: &str = "diagnostics-probe-marker";
const FIRST_VALUE: &str = "ll-probe-1";
/// Non-ASCII on purpose: the Android store encrypts bytes, Keychain stores data blobs.
const SECOND_VALUE: &str = "ll-probe-2 zażółć 🔑";

/// Save/read/overwrite/delete round trip against the platform store. The report carries only
/// step names, pass/fail and error descriptions — never the test values.
#[tauri::command]
pub async fn probe_secret_store(
    service: tauri::State<'_, SecretService>,
) -> Result<SecretStoreProbeDto, ErrorDto> {
    if APP_VARIANT != "dev" {
        return Err(ErrorDto::new(ErrorCode::PermissionDenied));
    }
    let service = service.0.clone();
    // Keystore/Keychain calls block; keep them off the async workers and the main thread.
    tauri::async_runtime::spawn_blocking(move || match SecretStore::native(&service) {
        Ok(store) => run_probe(&store),
        Err(error) => SecretStoreProbeDto {
            backend: "none".to_owned(),
            persisted_from_previous_run: false,
            steps: vec![step("open", Err(error))],
        },
    })
    .await
    .map_err(|_| ErrorDto::new(ErrorCode::Internal))
}

pub fn run_probe(store: &SecretStore) -> SecretStoreProbeDto {
    let (persisted, marker_read) = match store.has(PROBE_MARKER) {
        Ok(found) => (found, Ok(())),
        Err(error) => (false, Err(error)),
    };
    let expect = |name: &str, expected: Option<&str>| {
        let actual = store.get(PROBE_SECRET);
        let matches = matches!(&actual, Ok(value) if value.as_deref() == expected);
        step(name, actual.and_then(|_| check(matches)))
    };
    let mut steps = vec![step("markerRead", marker_read)];
    steps.push(step("cleanup", store.delete(PROBE_SECRET).map(|_| ())));
    steps.push(expect("absentBeforeWrite", None));
    steps.push(step("write", store.set(PROBE_SECRET, FIRST_VALUE)));
    steps.push(expect("readBack", Some(FIRST_VALUE)));
    steps.push(step("overwrite", store.set(PROBE_SECRET, SECOND_VALUE)));
    steps.push(expect("readOverwritten", Some(SECOND_VALUE)));
    steps.push(step("delete", store.delete(PROBE_SECRET).and_then(check)));
    steps.push(expect("absentAfterDelete", None));
    steps.push(step("markerWrite", store.set(PROBE_MARKER, "1")));
    SecretStoreProbeDto {
        backend: store.backend(),
        persisted_from_previous_run: persisted,
        steps,
    }
}

fn check(passed: bool) -> secrets::Result<()> {
    if passed {
        Ok(())
    } else {
        Err(secrets::SecretStoreError::Platform(
            "unexpected value".to_owned(),
        ))
    }
}

fn step(name: &str, result: secrets::Result<()>) -> ProbeStepDto {
    ProbeStepDto {
        name: name.to_owned(),
        ok: result.is_ok(),
        error: result.err().map(|error| error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_store() -> SecretStore {
        SecretStore::with_store(keyring_core::mock::Store::new().unwrap(), "test")
    }

    #[test]
    fn probe_passes_on_a_working_store_and_detects_the_marker_next_time() {
        let store = mock_store();
        let first = run_probe(&store);
        assert!(first.steps.iter().all(|step| step.ok), "{first:?}");
        assert!(!first.persisted_from_previous_run);
        let second = run_probe(&store);
        assert!(second.persisted_from_previous_run);
    }

    #[test]
    fn report_never_contains_the_test_values() {
        let report = serde_json::to_string(&run_probe(&mock_store())).unwrap();
        assert!(
            !report.contains(FIRST_VALUE) && !report.contains("zażółć"),
            "{report}"
        );
    }
}
