//! Integration test in its own process: installs the global file logger once.

use langualinker_infrastructure::logging::{LOG_FILE_PREFIX, REDACTED, init_file_logging};

#[test]
fn writes_redacted_lines_to_a_rotating_file() {
    let dir = std::env::temp_dir().join(format!("ll-log-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    let guard = init_file_logging(&dir).expect("logger installs");
    tracing::info!(
        app_version = "0.1.0",
        front = "die Straßenbahnhaltestelle",
        "app started"
    );
    drop(guard);

    let log = std::fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(LOG_FILE_PREFIX)
        })
        .expect("log file created");
    let content = std::fs::read_to_string(log).unwrap();
    assert!(content.contains("app started"), "{content}");
    assert!(content.contains("app_version=\"0.1.0\""), "{content}");
    assert!(content.contains(REDACTED), "{content}");
    assert!(!content.contains("Straßenbahnhaltestelle"), "{content}");
    assert!(!content.contains('\u{1b}'), "no ANSI escapes in files");

    assert!(
        init_file_logging(&dir).is_err(),
        "second install is rejected"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
