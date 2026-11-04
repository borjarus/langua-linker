//! Loading a content directory and linting it as a whole.

use std::collections::HashSet;
use std::io::{BufRead, Read};

use serde::de::DeserializeOwned;

use crate::check::{DeckChecker, Issues, Mode, check_catalog, check_manifest};
use crate::format::{CATALOG_FILE, Catalog, DeckManifest};
use crate::reader::NoteReader;
use crate::source::ContentSource;

/// Largest manifest or catalog accepted (they are small JSON objects).
const MAX_MANIFEST_BYTES: u64 = 64 * 1024;

/// Catalog and manifests of a content directory, decks in import order.
#[derive(Debug, Clone, PartialEq)]
pub struct Bundle {
    pub catalog: Catalog,
    pub decks: Vec<DeckManifest>,
}

fn read_json<T: DeserializeOwned>(
    source: &dyn ContentSource,
    name: &str,
    issues: &mut Issues,
) -> Option<T> {
    let mut text = String::new();
    let read = source.open(name).and_then(|reader| {
        reader
            .take(MAX_MANIFEST_BYTES + 1)
            .read_to_string(&mut text)
    });
    if let Err(error) = read {
        issues.error("file.missing", name, None, error.to_string());
        return None;
    }
    if text.len() as u64 > MAX_MANIFEST_BYTES {
        issues.error(
            "file.tooLarge",
            name,
            None,
            "manifest is larger than 64 KiB",
        );
        return None;
    }
    match serde_json::from_str(&text) {
        Ok(value) => Some(value),
        Err(error) => {
            let code = if error.to_string().starts_with("unknown field") {
                "manifest.unknownField"
            } else {
                "manifest.malformed"
            };
            issues.error(code, name, Some(error.line() as u32), error.to_string());
            None
        }
    }
}

impl Bundle {
    /// Reads and checks the catalog and every manifest. Fails when any of them has an error.
    pub fn load(source: &dyn ContentSource, mode: Mode) -> Result<(Self, Issues), Issues> {
        let mut issues = Issues::default();
        let Some(catalog) = read_json::<Catalog>(source, CATALOG_FILE, &mut issues) else {
            return Err(issues);
        };
        check_catalog(&catalog, CATALOG_FILE, &mut issues);
        let mut decks: Vec<DeckManifest> = Vec::new();
        for stem in &catalog.decks {
            let file = format!("{stem}.json");
            if let Some(manifest) = read_json::<DeckManifest>(source, &file, &mut issues) {
                let earlier: Vec<&DeckManifest> = decks.iter().collect();
                check_manifest(&manifest, &file, &earlier, mode, &mut issues);
                decks.push(manifest);
            }
        }
        if issues.has_errors() {
            return Err(issues);
        }
        Ok((Self { catalog, decks }, issues))
    }

    pub fn deck(&self, guid: &str) -> Option<&DeckManifest> {
        self.decks.iter().find(|deck| deck.deck_guid == guid)
    }

    /// Streams the note records of `deck`.
    pub fn notes<'s>(
        &self,
        source: &'s dyn ContentSource,
        deck: &DeckManifest,
    ) -> std::io::Result<NoteReader<Box<dyn BufRead + Send + 's>>> {
        source.open(&deck.notes_file).map(NoteReader::new)
    }

    /// Note GUIDs of the decks `deck` depends on (read without further checks).
    pub fn dependency_guids(
        &self,
        source: &dyn ContentSource,
        deck: &DeckManifest,
    ) -> Result<HashSet<String>, Issues> {
        let mut guids = HashSet::new();
        let mut issues = Issues::default();
        for dependency in &deck.depends_on {
            let Some(manifest) = self.deck(dependency) else {
                issues.error(
                    "deck.dependency",
                    &deck.notes_file,
                    None,
                    format!("unknown dependency {dependency}"),
                );
                continue;
            };
            match self.notes(source, manifest) {
                Ok(reader) => {
                    for (line, record) in reader {
                        match record {
                            Ok(record) => {
                                guids.insert(record.guid);
                            }
                            Err(error) => issues.error(
                                error.code,
                                &manifest.notes_file,
                                Some(line),
                                error.message,
                            ),
                        }
                    }
                }
                Err(error) => issues.error(
                    "file.missing",
                    &manifest.notes_file,
                    None,
                    error.to_string(),
                ),
            }
        }
        if issues.has_errors() {
            Err(issues)
        } else {
            Ok(guids)
        }
    }

    /// Streams and checks every record of `deck`. Returns the deck's note GUIDs.
    pub fn check_deck(
        &self,
        source: &dyn ContentSource,
        deck: &DeckManifest,
        seen_guids: &mut HashSet<String>,
        dependency_guids: &HashSet<String>,
        issues: &mut Issues,
    ) -> HashSet<String> {
        let mut reader = match self.notes(source, deck) {
            Ok(reader) => reader,
            Err(error) => {
                issues.error("file.missing", &deck.notes_file, None, error.to_string());
                return HashSet::new();
            }
        };
        let mut checker = DeckChecker::new(deck, seen_guids, dependency_guids);
        for (line, record) in &mut reader {
            match record {
                Ok(record) => checker.record(line, &record, issues),
                Err(error) => issues.error(error.code, &deck.notes_file, Some(line), error.message),
            }
        }
        let guids = std::mem::take(&mut checker.note_guids);
        checker.finish(&reader.sha256_hex(), issues);
        guids
    }
}

/// Lints a whole content directory: catalog, manifests, every record, cross-deck GUIDs and links.
pub fn lint(source: &dyn ContentSource, mode: Mode) -> Issues {
    let (bundle, mut issues) = match Bundle::load(source, mode) {
        Ok(loaded) => loaded,
        Err(issues) => return issues,
    };
    let mut seen = HashSet::new();
    let mut deck_guids: Vec<(String, HashSet<String>)> = Vec::new();
    for deck in &bundle.decks {
        let dependencies: HashSet<String> = deck_guids
            .iter()
            .filter(|(guid, _)| deck.depends_on.contains(guid))
            .flat_map(|(_, notes)| notes.iter().cloned())
            .collect();
        let notes = bundle.check_deck(source, deck, &mut seen, &dependencies, &mut issues);
        deck_guids.push((deck.deck_guid.clone(), notes));
    }
    issues
}
