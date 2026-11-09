//! Converts an Anki `.apkg` export into a LanguaLinker import package.
//!
//! ```text
//! apkg-convert deck.apkg --list
//! apkg-convert deck.apkg --language de --kind words --reverse --name "Niemiecki — moje słówka" -o moje.ndjson
//! apkg-convert deck.apkg --language en --kind sentences --anki-deck "English::Sentences" \
//!     --front-field Front --back-field Back --deck-type TextWithAssociations -o zdania.ndjson
//! ```
//!
//! Run from the repository with `cargo run -p langualinker-anki --bin apkg-convert -- …`.

use std::path::PathBuf;
use std::process::ExitCode;

use langualinker_anki::{FieldRef, Options, Report, convert, read_apkg};
use langualinker_content::format::ContentKind;
use langualinker_domain::deck::{DeckType, TargetLanguage};

const USAGE: &str = "usage: apkg-convert <deck.apkg> --language de|en|es [options]

  -o, --out <file>          output package (default: <deck>.ndjson next to the .apkg)
  --language de|en|es       target language (L2) of the deck            [required]
  --kind words|sentences    word or sentence notes                       [words]
  --reverse                 two cards per note: front→back and back→front
  --name <text>             deck name in LanguaLinker      [Anki deck or file name]
  --description <text>      deck description
  --deck-type <type>        Simple | TextWithAssociations | Linguistic   [Linguistic]
  --level <A1..C2>          CEFR level shown in the app                  [A1]
  --anki-deck <name>        only this Anki deck and its subdecks         [all]
  --front-field <n|name>    Anki field for the front (0-based or name)   [0]
  --back-field <n|name>     Anki field for the back                      [1]
  --deck-guid <guid>        stable deck id (default derived from the name)
  --no-scheduling           import every card as new
  --no-reviews              keep the scheduling state, drop the review history
  --strict                  fail instead of skipping notes the content rules reject
  --list                    show Anki decks, note types and counts, then exit";

fn field(value: &str) -> FieldRef {
    value
        .parse()
        .map_or_else(|_| FieldRef::Name(value.to_owned()), FieldRef::Index)
}

fn print_report(report: &Report) {
    println!("notes:                 {}", report.notes);
    println!(
        "cards:                 {} ({} with progress, {} suspended/buried, {} with FSRS state)",
        report.cards, report.cards_with_progress, report.suspended_or_buried, report.fsrs_cards
    );
    println!("reviews:               {}", report.reviews);
    let skipped = [
        ("cloze notes (not supported)", report.skipped_cloze),
        ("notes with an empty front", report.skipped_empty_front),
        (
            "notes without the chosen fields",
            report.skipped_missing_field,
        ),
        (
            "duplicate notes (same front and back)",
            report.skipped_duplicates,
        ),
        (
            "cards of templates the note type lacks",
            report.dropped_cards,
        ),
        (
            "  … of them with progress",
            report.dropped_cards_with_progress,
        ),
        ("  … and their reviews", report.dropped_reviews),
        (
            "manual reschedule entries (no rating)",
            report.manual_reviews_skipped,
        ),
        (
            "fields whose formatting was flattened",
            report.fields_with_markup,
        ),
        ("sound/image references removed", report.media_refs_removed),
        (
            "media files not converted",
            report.media_files_not_converted,
        ),
        (
            "tags dropped (too long, too many, repeated)",
            report.tags_dropped,
        ),
        (
            "learning cards (restart their current steps)",
            report.learning_cards,
        ),
    ];
    for (label, count) in skipped {
        if count > 0 {
            println!("{label:<46} {count}");
        }
    }
    if !report.skipped_invalid.is_empty() {
        println!(
            "notes rejected by the content rules:          {}",
            report.skipped_invalid.len()
        );
        for (front, code) in report.skipped_invalid.iter().take(20) {
            println!("  [{code}] {front}");
        }
    }
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let mut input: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut list = false;
    let mut language: Option<TargetLanguage> = None;
    let mut options = Options {
        name: None,
        description: None,
        language: TargetLanguage::De,
        kind: ContentKind::Words,
        reverse: false,
        deck_type: DeckType::Linguistic,
        level: "A1".into(),
        anki_deck: None,
        front_field: FieldRef::Index(0),
        back_field: FieldRef::Index(1),
        scheduling: true,
        reviews: true,
        deck_guid: None,
        file_name: String::new(),
        today: chrono::Utc::now().date_naive(),
        strict: false,
    };
    macro_rules! value {
        ($flag:expr) => {
            match args.next() {
                Some(value) => value,
                None => {
                    eprintln!("{} needs a value\n\n{USAGE}", $flag);
                    return ExitCode::from(2);
                }
            }
        };
    }
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            "--list" => list = true,
            "-o" | "--out" => out = Some(PathBuf::from(value!(arg))),
            "--language" => {
                language = match value!(arg).as_str() {
                    "de" => Some(TargetLanguage::De),
                    "en" => Some(TargetLanguage::En),
                    "es" => Some(TargetLanguage::Es),
                    other => {
                        eprintln!("unsupported language {other} (de|en|es)");
                        return ExitCode::from(2);
                    }
                }
            }
            "--kind" => {
                options.kind = match value!(arg).as_str() {
                    "words" | "word" => ContentKind::Words,
                    "sentences" | "sentence" => ContentKind::Sentences,
                    other => {
                        eprintln!("unknown kind {other} (words|sentences)");
                        return ExitCode::from(2);
                    }
                }
            }
            "--reverse" => options.reverse = true,
            "--name" => options.name = Some(value!(arg)),
            "--description" => options.description = Some(value!(arg)),
            "--deck-type" => {
                options.deck_type = match value!(arg).as_str() {
                    "Simple" => DeckType::Simple,
                    "TextWithAssociations" => DeckType::TextWithAssociations,
                    "Linguistic" => DeckType::Linguistic,
                    other => {
                        eprintln!("unknown deck type {other}");
                        return ExitCode::from(2);
                    }
                }
            }
            "--level" => options.level = value!(arg),
            "--anki-deck" => options.anki_deck = Some(value!(arg)),
            "--front-field" => options.front_field = field(&value!(arg)),
            "--back-field" => options.back_field = field(&value!(arg)),
            "--deck-guid" => options.deck_guid = Some(value!(arg)),
            "--no-scheduling" => options.scheduling = false,
            "--no-reviews" => options.reviews = false,
            "--strict" => options.strict = true,
            other if input.is_none() && !other.starts_with('-') => {
                input = Some(PathBuf::from(other))
            }
            other => {
                eprintln!("unknown argument {other}\n\n{USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    let Some(input) = input else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let collection = match read_apkg(&input) {
        Ok(collection) => collection,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::FAILURE;
        }
    };
    if list {
        println!(
            "Anki schema {}, {} notes, {} cards, {} reviews, {} media files",
            collection.schema,
            collection.notes.len(),
            collection.cards.len(),
            collection.reviews.len(),
            collection.media_files
        );
        println!("\ndecks (cards):");
        for (name, count) in collection.cards_per_deck() {
            println!("  {name} ({count})");
        }
        println!("\nnote types:");
        let mut types: Vec<_> = collection.note_types.values().collect();
        types.sort_by(|a, b| a.name.cmp(&b.name));
        for kind in types {
            let cloze = if kind.cloze {
                ", cloze — not supported"
            } else {
                ""
            };
            println!(
                "  {} — fields: {}; templates: {}{cloze}",
                kind.name,
                kind.fields.join(", "),
                kind.templates.join(", ")
            );
        }
        return ExitCode::SUCCESS;
    }
    let Some(language) = language else {
        eprintln!("--language is required\n\n{USAGE}");
        return ExitCode::from(2);
    };
    options.language = language;
    options.file_name = input
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let out = out.unwrap_or_else(|| input.with_extension("ndjson"));
    match convert(&collection, &options) {
        Ok(conversion) => {
            if let Err(error) = std::fs::write(&out, &conversion.package) {
                eprintln!("error: cannot write {}: {error}", out.display());
                return ExitCode::FAILURE;
            }
            println!(
                "wrote {} — deck \"{}\" ({}, {})",
                out.display(),
                conversion.manifest.name,
                conversion.manifest.note_type,
                conversion.manifest.target_language
            );
            print_report(&conversion.report);
            println!("\nImport it in LanguaLinker: Settings → Import & export → Deck package.");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
