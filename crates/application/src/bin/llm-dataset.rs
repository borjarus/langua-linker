//! Exports the training data of the on-device association model (ADR-23).
//!
//! ```text
//! cargo run -p langualinker-application --bin llm-dataset -- content/decks-src target/llm-dataset
//! cargo run -p langualinker-application --bin llm-dataset -- content/decks target/llm-dataset --eval-percent 5
//! ```
//!
//! Reads every deck of the catalog (plain files, or `decks.7z` decrypted with
//! `LANGUA_DECKS_PASSWORD` / `.env`) and writes `train.jsonl`, `eval.jsonl` and `stats.json`.
//! Each line is the app's own prompt for a note with one of its stored associations as the
//! answer (see `llm::dataset`). Nothing leaves the machine; the output contains deck text, so
//! keep it out of git (the default under `target/` is ignored).

use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use langualinker_application::llm::dataset::{self, Skip};
use langualinker_content::archive::{ARCHIVE_FILE, PASSWORD_VAR, find_password};
use langualinker_content::check::{parse_note_type, parse_target_language};
use langualinker_content::format::{CATALOG_FILE, Catalog, DeckManifest};
use langualinker_content::reader::NoteReader;
use langualinker_content::{ArchiveSource, ContentSource, DirSource};
use langualinker_domain::deck::NativeLanguage;

const USAGE: &str = "usage: llm-dataset <content dir> <out dir> [--eval-percent <0-50>]";

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    let mut positional = Vec::new();
    let mut eval_percent = 5u8;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg == "--eval-percent" {
            eval_percent = args
                .next()
                .and_then(|value| value.parse().ok())
                .filter(|value| *value <= 50)
                .ok_or(USAGE)?;
        } else {
            positional.push(PathBuf::from(arg));
        }
    }
    let [content, out] = positional.as_slice() else {
        return Err(USAGE.into());
    };
    let source = open(content)?;
    std::fs::create_dir_all(out).map_err(|error| error.to_string())?;
    let mut train = writer(&out.join("train.jsonl"))?;
    let mut eval = writer(&out.join("eval.jsonl"))?;

    let catalog: Catalog = read_json(source.as_ref(), CATALOG_FILE)?;
    let mut counts: BTreeMap<String, BTreeMap<&'static str, u32>> = BTreeMap::new();
    for stem in &catalog.decks {
        let manifest: DeckManifest = read_json(source.as_ref(), &format!("{stem}.json"))?;
        let target = parse_target_language(&manifest.target_language)
            .ok_or_else(|| format!("{stem}: unknown target language"))?;
        let note_type = parse_note_type(&manifest.note_type)
            .ok_or_else(|| format!("{stem}: unknown note type"))?;
        let reader = source
            .open(&manifest.notes_file)
            .map_err(|error| format!("{}: {error}", manifest.notes_file))?;
        let tally = counts.entry(stem.clone()).or_default();
        for (line, record) in NoteReader::new(reader) {
            let record = record.map_err(|error| format!("{stem} line {line}: {error:?}"))?;
            for association in &record.associations {
                let built = dataset::example(
                    &record.guid,
                    note_type,
                    NativeLanguage::Pl,
                    target,
                    &record.front,
                    &record.back,
                    &association.text,
                );
                let key = match built {
                    Ok(example) => {
                        let held_out = dataset::is_eval(&record.guid, eval_percent);
                        let file = if held_out { &mut eval } else { &mut train };
                        writeln!(file, "{}", dataset::to_json(&example))
                            .map_err(|error| error.to_string())?;
                        if held_out { "eval" } else { "train" }
                    }
                    Err(Skip::Empty) => "skippedEmpty",
                    Err(Skip::TooLong) => "skippedTooLong",
                    Err(Skip::Moderation) => "skippedModeration",
                };
                *tally.entry(key).or_default() += 1;
            }
        }
    }
    train.flush().map_err(|error| error.to_string())?;
    eval.flush().map_err(|error| error.to_string())?;
    let stats = serde_json::json!({ "evalPercent": eval_percent, "decks": counts });
    std::fs::write(
        out.join("stats.json"),
        serde_json::to_string_pretty(&stats).unwrap_or_default(),
    )
    .map_err(|error| error.to_string())?;
    println!(
        "{}",
        serde_json::to_string_pretty(&stats).unwrap_or_default()
    );
    Ok(())
}

/// Plain files, or the encrypted archive when the directory holds one.
fn open(dir: &Path) -> Result<Box<dyn ContentSource>, String> {
    let archive = dir.join(ARCHIVE_FILE);
    if !archive.is_file() {
        return Ok(Box::new(DirSource::new(dir)));
    }
    let password = find_password(&std::env::current_dir().unwrap_or_default())
        .ok_or_else(|| format!("no password — set {PASSWORD_VAR} or add it to .env"))?;
    ArchiveSource::open_path(&archive, password)
        .map(|source| Box::new(source) as Box<dyn ContentSource>)
        .map_err(|error| format!("{}: {error}", archive.display()))
}

fn read_json<T: serde::de::DeserializeOwned>(
    source: &dyn ContentSource,
    name: &str,
) -> Result<T, String> {
    let mut text = String::new();
    let mut reader = source
        .open(name)
        .map_err(|error| format!("{name}: {error}"))?;
    loop {
        let mut line = String::new();
        let read = reader
            .read_line(&mut line)
            .map_err(|error| format!("{name}: {error}"))?;
        if read == 0 {
            break;
        }
        text.push_str(&line);
    }
    serde_json::from_str(&text).map_err(|error| format!("{name}: {error}"))
}

fn writer(path: &Path) -> Result<std::io::BufWriter<std::fs::File>, String> {
    std::fs::File::create(path)
        .map(std::io::BufWriter::new)
        .map_err(|error| format!("{}: {error}", path.display()))
}
