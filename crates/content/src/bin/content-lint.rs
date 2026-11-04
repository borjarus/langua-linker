//! Content linter (plan Phase 8 step 14).
//!
//! ```text
//! cargo run -p langualinker-content --bin content-lint -- content/decks            # dev gate (CI)
//! cargo run -p langualinker-content --bin content-lint -- content/decks --release  # beta/release gate
//! cargo run -p langualinker-content --bin content-lint -- content/decks --update-manifests
//! ```
//!
//! `--update-manifests` rewrites `sha256`, `expectedNotes` and `expectedCards` of every manifest
//! from its notes file (after an editorial change), then lints. Exit code 1 on any error.
//!
//! A file instead of a directory is linted as an import package (`apkg-convert` output):
//! `content-lint my-deck.ndjson`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use langualinker_content::check::{Mode, Severity, parse_note_type};
use langualinker_content::format::{CATALOG_FILE, Catalog, DeckManifest};
use langualinker_content::reader::NoteReader;
use langualinker_content::{DirSource, Issues, Package, lint};

fn update_manifests(dir: &Path) -> Result<(), String> {
    let catalog: Catalog = read(&dir.join(CATALOG_FILE))?;
    for stem in &catalog.decks {
        let path = dir.join(format!("{stem}.json"));
        let mut manifest: DeckManifest = read(&path)?;
        let file =
            std::fs::File::open(dir.join(&manifest.notes_file)).map_err(|e| e.to_string())?;
        let mut reader = NoteReader::new(std::io::BufReader::new(file));
        let notes = (&mut reader).count() as u32;
        let templates =
            parse_note_type(&manifest.note_type).map_or(1, |kind| kind.templates().len() as u32);
        manifest.sha256 = reader.sha256_hex();
        manifest.expected_notes = notes;
        manifest.expected_cards = notes * templates;
        let json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
        std::fs::write(&path, format!("{json}\n")).map_err(|e| e.to_string())?;
        println!("updated {}", path.display());
    }
    Ok(())
}

fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

fn main() -> ExitCode {
    let mut dir: Option<PathBuf> = None;
    let mut mode = Mode::Dev;
    let mut update = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--release" => mode = Mode::Release,
            "--update-manifests" => update = true,
            "-h" | "--help" => {
                println!("usage: content-lint <content-dir> [--release] [--update-manifests]");
                return ExitCode::SUCCESS;
            }
            other if dir.is_none() && !other.starts_with('-') => dir = Some(PathBuf::from(other)),
            other => {
                eprintln!("unknown argument {other}");
                return ExitCode::from(2);
            }
        }
    }
    let Some(dir) = dir else {
        eprintln!("usage: content-lint <content-dir> [--release] [--update-manifests]");
        return ExitCode::from(2);
    };
    if update && let Err(error) = update_manifests(&dir) {
        eprintln!("error: {error}");
        return ExitCode::FAILURE;
    }
    let issues = if dir.is_file() {
        lint_package(&dir)
    } else {
        lint(&DirSource::new(&dir), mode)
    };
    for issue in &issues.0 {
        println!("{issue}");
    }
    let errors = issues
        .0
        .iter()
        .filter(|issue| issue.severity == Severity::Error)
        .count();
    let warnings = issues.0.len() - errors;
    let gate = if mode == Mode::Release {
        "release"
    } else {
        "dev"
    };
    println!("content-lint ({gate}): {errors} error(s), {warnings} warning(s)");
    if errors > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn lint_package(path: &Path) -> Issues {
    let mut issues = Issues::default();
    let file = path.display().to_string();
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            issues.error("file.missing", &file, None, error.to_string());
            return issues;
        }
    };
    match Package::parse(&bytes) {
        Err(kind) => {
            let message = match kind {
                langualinker_content::NotAPackage::Zip => {
                    "a ZIP archive (.apkg?) — convert it with apkg-convert first"
                }
                langualinker_content::NotAPackage::TooLarge => "larger than 64 MiB",
                langualinker_content::NotAPackage::Unknown => "no package header on the first line",
            };
            issues.error("package.format", &file, None, message);
            issues
        }
        Ok(Err(header_issues)) => header_issues,
        Ok(Ok(package)) => {
            let check = package.check();
            println!(
                "{}: {} notes, {} cards ({} with progress, {} suspended/buried), {} reviews",
                package.manifest.name,
                check.notes,
                check.cards,
                check.scheduled,
                check.suspended,
                check.reviews
            );
            check.issues
        }
    }
}
