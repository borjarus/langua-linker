//! Phase 1 native spikes (ADR-07, ADR-12): secret store and files. Notifications, TTS and sharing
//! are probed from the WebView (`DiagnosticsScreen.svelte`). Commands here are registered in every
//! build but granted only by the `diagnostics` capability, which only the dev variant enables; they
//! also refuse to run outside the dev variant.

use std::fmt::Display;
use std::io::{self, Read, Write};

use langualinker_contracts::{
    ErrorCode, ErrorDto, FileProbeDto, ProbeStepDto, SecretStoreProbeDto,
};
use langualinker_infrastructure::secrets::{self, SecretStore};
use tauri::{AppHandle, Runtime};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_fs::{FilePath, FsExt, OpenOptions};

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
    dev_only()?;
    let service = service.0.clone();
    // Keystore/Keychain calls block; keep them off the async workers and the main thread.
    blocking(move || match SecretStore::native(&service) {
        Ok(store) => run_probe(&store),
        Err(error) => SecretStoreProbeDto {
            backend: "none".to_owned(),
            persisted_from_previous_run: false,
            steps: vec![step("open", Err(error))],
        },
    })
    .await
}

fn dev_only() -> Result<(), ErrorDto> {
    if APP_VARIANT == "dev" {
        Ok(())
    } else {
        Err(ErrorDto::new(ErrorCode::PermissionDenied))
    }
}

async fn blocking<T: Send + 'static>(
    task: impl FnOnce() -> T + Send + 'static,
) -> Result<T, ErrorDto> {
    tauri::async_runtime::spawn_blocking(task)
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

fn step<E: Display>(name: &str, result: Result<(), E>) -> ProbeStepDto {
    ProbeStepDto {
        name: name.to_owned(),
        ok: result.is_ok(),
        error: result.err().map(|error| error.to_string()),
    }
}

/// Written by the save probe; non-ASCII so the round trip also checks the byte path.
const BACKUP_SAMPLE: &str = "langualinker-probe-backup v1\nzażółć gęślą jaźń · Grüße · 🔑\n";
const BACKUP_FILE_NAME: &str = "langualinker-probe-backup.txt";
/// Bytes kept from the start of a picked file to tell CSV text from an APKG (zip) archive.
const SNIFF_LEN: usize = 512;

/// Picks a CSV/APKG (or, unfiltered, any) file and streams it through Rust, the way the Phase 8
/// importer will read it: Android returns a `content://` URI, iOS a security-scoped `file://` URL.
/// The report carries the location kind, size and sniffed type — never the name or contents.
#[tauri::command]
pub async fn probe_file_open<R: Runtime>(
    app: AppHandle<R>,
    filtered: bool,
) -> Result<FileProbeDto, ErrorDto> {
    dev_only()?;
    // The blocking picker must not run on the main thread, which drives the dialog.
    blocking(move || {
        let mut dialog = app.dialog().file();
        if filtered {
            dialog = dialog.add_filter("CSV / APKG", &["csv", "apkg"]);
        }
        let Some(path) = dialog.blocking_pick_file() else {
            return cancelled_file_probe();
        };
        let mut report = picked_file_probe(&path);
        let mut options = OpenOptions::new();
        options.read(true);
        let measured = app.fs().open(path.clone(), options).and_then(measure);
        match measured {
            Ok((bytes, head)) => {
                report.bytes = Some(bytes);
                report.kind = Some(sniff(&head).to_owned());
                report.steps.push(step::<io::Error>("openAndRead", Ok(())));
            }
            Err(error) => report.steps.push(step("openAndRead", Err(error))),
        }
        release_access(&app, path, &mut report);
        report
    })
    .await
}

/// Lets the user choose where to save a small backup-like file, writes it through Rust and reads
/// it back. On iOS the picker exports an empty placeholder first (plugin behaviour) and returns
/// the exported URL, so this also shows whether writing there afterwards works.
#[tauri::command]
pub async fn probe_file_save<R: Runtime>(app: AppHandle<R>) -> Result<FileProbeDto, ErrorDto> {
    dev_only()?;
    blocking(move || {
        let Some(path) = app
            .dialog()
            .file()
            .set_file_name(BACKUP_FILE_NAME)
            .add_filter("Text", &["txt"])
            .blocking_save_file()
        else {
            return cancelled_file_probe();
        };
        let mut report = picked_file_probe(&path);
        let mut write = OpenOptions::new();
        write.write(true).create(true).truncate(true);
        let written = app
            .fs()
            .open(path.clone(), write)
            .and_then(|mut file| file.write_all(BACKUP_SAMPLE.as_bytes()));
        let written_ok = written.is_ok();
        report.steps.push(step("write", written));
        if written_ok {
            let mut read = OpenOptions::new();
            read.read(true);
            let read_back = app.fs().open(path.clone(), read).and_then(|mut file| {
                let mut content = Vec::new();
                file.read_to_end(&mut content)?;
                Ok(content)
            });
            match read_back {
                Ok(content) => {
                    report.bytes = Some(content.len() as u64);
                    let same = content == BACKUP_SAMPLE.as_bytes();
                    let checked = if same { Ok(()) } else { Err("content differs") };
                    report.steps.push(step("readBack", checked));
                }
                Err(error) => report.steps.push(step("readBack", Err(error))),
            }
        }
        release_access(&app, path, &mut report);
        report
    })
    .await
}

fn cancelled_file_probe() -> FileProbeDto {
    FileProbeDto {
        cancelled: true,
        location: None,
        bytes: None,
        kind: None,
        steps: Vec::new(),
    }
}

fn picked_file_probe(path: &FilePath) -> FileProbeDto {
    FileProbeDto {
        cancelled: false,
        location: Some(location(path)),
        bytes: None,
        kind: None,
        steps: vec![step::<io::Error>("pick", Ok(()))],
    }
}

fn location(path: &FilePath) -> String {
    match path {
        FilePath::Url(url) => url.scheme().to_owned(),
        FilePath::Path(_) => "path".to_owned(),
    }
}

/// iOS keeps security-scoped access open until it is released explicitly.
#[cfg(target_os = "ios")]
fn release_access<R: Runtime>(app: &AppHandle<R>, path: FilePath, report: &mut FileProbeDto) {
    if matches!(&path, FilePath::Url(url) if url.scheme() == "file") {
        let released = app.fs().stop_accessing_security_scoped_resource(path);
        report.steps.push(step("releaseAccess", released));
    }
}

#[cfg(not(target_os = "ios"))]
fn release_access<R: Runtime>(_app: &AppHandle<R>, _path: FilePath, _report: &mut FileProbeDto) {}

/// Streams the whole file (the importer must not need it in memory) and keeps its first bytes.
fn measure(mut reader: impl Read) -> io::Result<(u64, Vec<u8>)> {
    let mut buffer = vec![0; 64 * 1024];
    let mut head = Vec::with_capacity(SNIFF_LEN);
    let mut total = 0u64;
    loop {
        let read = match reader.read(&mut buffer) {
            Ok(0) => return Ok((total, head)),
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        let missing = SNIFF_LEN - head.len();
        head.extend_from_slice(&buffer[..read.min(missing)]);
        total += read as u64;
    }
}

fn sniff(head: &[u8]) -> &'static str {
    if head.starts_with(b"PK\x03\x04") {
        return "zip";
    }
    if head.is_empty() {
        return "empty";
    }
    // A multi-byte character cut off by the sample boundary is still text.
    match std::str::from_utf8(head) {
        Ok(text) if !text.contains('\0') => "text",
        Err(error) if error.error_len().is_none() => "text",
        _ => "binary",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measure_streams_everything_and_keeps_only_the_head() {
        let data = vec![b'a'; 200_000];
        let (bytes, head) = measure(data.as_slice()).unwrap();
        assert_eq!(bytes, 200_000);
        assert_eq!(head.len(), SNIFF_LEN);
    }

    #[test]
    fn sniff_tells_csv_from_apkg() {
        assert_eq!(sniff(b"front,back\nHaus,house\n"), "text");
        assert_eq!(sniff("Grüße,".as_bytes()), "text");
        assert_eq!(sniff(&"ż".as_bytes()[..1]), "text");
        assert_eq!(sniff(b"PK\x03\x04\x14\x00"), "zip");
        assert_eq!(sniff(&[0xff, 0xfe, 0x00]), "binary");
        assert_eq!(sniff(b""), "empty");
    }

    #[test]
    fn file_report_carries_the_scheme_not_the_name() {
        let url: FilePath = "content://com.android.providers/document/secret-name.csv"
            .parse()
            .unwrap();
        let report = picked_file_probe(&url);
        assert_eq!(report.location.as_deref(), Some("content"));
        let json = serde_json::to_string(&report).unwrap();
        assert!(!json.contains("secret-name"), "{json}");
        let path = FilePath::from(std::path::PathBuf::from("/tmp/secret-name.csv"));
        assert_eq!(location(&path), "path");
    }

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
