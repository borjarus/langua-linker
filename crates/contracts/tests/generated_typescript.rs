//! Contract drift check (plan step 28): the committed TypeScript must equal what the Rust DTOs
//! generate. CI fails on drift; regenerate with
//! `UPDATE_CONTRACT=1 cargo test -p langualinker-contracts --test generated_typescript`.

use std::path::PathBuf;

fn generated_file() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../frontend/src/generated/contracts.ts")
}

#[test]
fn committed_typescript_matches_the_rust_dtos() {
    let expected = langualinker_contracts::dto::typescript();
    let path = generated_file();
    if std::env::var("UPDATE_CONTRACT").is_ok_and(|value| value == "1") {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &expected).unwrap();
        return;
    }
    let committed = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert!(
        committed == expected,
        "frontend/src/generated/contracts.ts is out of date with the Rust DTOs; run \
         UPDATE_CONTRACT=1 cargo test -p langualinker-contracts --test generated_typescript"
    );
}
