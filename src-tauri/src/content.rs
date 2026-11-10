//! Bundled content compiled into the app (no asset path on Android/iOS) and the mapping of the
//! content, onboarding and dashboard read models to contract DTOs (Phase 8).

use langualinker_application::content::{
    ImportOutcome, OnboardingState, PackagePreview, PrepareOutcome,
};
use langualinker_application::dashboard::Dashboard;
use langualinker_content::ArchiveSource;
use langualinker_content::check::{parse_deck_type, parse_note_type, parse_target_language};
use langualinker_content::format::ContentKind;
use langualinker_contracts::dto::{
    AttributionDto, AttributionScope, BundledDeckDto, ContentState, ContentWarningCode,
    ContentWarningDto, DashboardDto, DeckCapabilitiesDto, DeckContentKind, DeckCountsDto,
    DeckImportOriginDto, DeckImportPreviewDto, DeckImportWarningDto, DeckSummaryDto, DeckType,
    JobResultDto, NativeLanguage, NoteTypeKind, OnboardingStateDto, OnboardingStep,
    ResumableSessionDto, ResumableStatus, TargetLanguage,
};
use langualinker_domain::deck;
use langualinker_domain::note;

use crate::backend::iso;

/// The encrypted bundled content (`content/decks/decks.7z`, ADR-14), compiled into the app so
/// Android/iOS need no asset path. It is decrypted in memory on the first read.
pub static BUNDLED_ARCHIVE: &[u8] = include_bytes!("../../content/decks/decks.7z");

/// Archive password, embedded by `build.rs` from `LANGUA_DECKS_PASSWORD` / `.env`.
const BUNDLED_PASSWORD: &str = env!("LANGUA_DECKS_PASSWORD");

/// The bundled content source; nothing is decrypted until the content service reads a file.
pub fn bundled() -> ArchiveSource {
    ArchiveSource::embedded(BUNDLED_ARCHIVE, BUNDLED_PASSWORD)
}

/// Main runtime dependencies shown on the Licences screen. A generated full list (cargo-about,
/// npm licence report) replaces it with the release audit (plan Phase 16/17).
const DEPENDENCIES: &[(&str, &str, &str)] = &[
    ("Tauri", "MIT OR Apache-2.0", "https://tauri.app"),
    ("Svelte", "MIT", "https://svelte.dev"),
    ("i18next", "MIT", "https://www.i18next.com"),
    ("TanStack Virtual", "MIT", "https://tanstack.com/virtual"),
    (
        "SQLite",
        "Public Domain",
        "https://sqlite.org/copyright.html",
    ),
    ("rusqlite", "MIT", "https://github.com/rusqlite/rusqlite"),
    ("serde", "MIT OR Apache-2.0", "https://serde.rs"),
    (
        "unicode-normalization",
        "MIT OR Apache-2.0",
        "https://github.com/unicode-rs/unicode-normalization",
    ),
    (
        "chrono",
        "MIT OR Apache-2.0",
        "https://github.com/chronotope/chrono",
    ),
    ("tracing", "MIT", "https://github.com/tokio-rs/tracing"),
    (
        "RustCrypto sha2",
        "MIT OR Apache-2.0",
        "https://github.com/RustCrypto/hashes",
    ),
    (
        "keyring-core",
        "MIT OR Apache-2.0",
        "https://github.com/open-source-cooperative/keyring-core",
    ),
];

pub fn attributions(content: Vec<(String, String, Option<String>)>) -> Vec<AttributionDto> {
    let mut list: Vec<AttributionDto> = DEPENDENCIES
        .iter()
        .map(|(name, license, url)| AttributionDto {
            name: (*name).to_owned(),
            license: (*license).to_owned(),
            url: Some((*url).to_owned()),
            scope: AttributionScope::Dependency,
        })
        .collect();
    list.extend(
        content
            .into_iter()
            .map(|(name, license, url)| AttributionDto {
                name,
                license,
                url,
                scope: AttributionScope::Content,
            }),
    );
    list
}

pub fn target_language(language: deck::TargetLanguage) -> TargetLanguage {
    match language {
        deck::TargetLanguage::De => TargetLanguage::De,
        deck::TargetLanguage::En => TargetLanguage::En,
        deck::TargetLanguage::Es => TargetLanguage::Es,
    }
}

pub fn domain_target_language(language: TargetLanguage) -> deck::TargetLanguage {
    match language {
        TargetLanguage::De => deck::TargetLanguage::De,
        TargetLanguage::En => deck::TargetLanguage::En,
        TargetLanguage::Es => deck::TargetLanguage::Es,
    }
}

fn native_language(language: deck::NativeLanguage) -> NativeLanguage {
    match language {
        deck::NativeLanguage::Pl => NativeLanguage::Pl,
    }
}

pub fn deck_type(value: deck::DeckType) -> DeckType {
    match value {
        deck::DeckType::Simple => DeckType::Simple,
        deck::DeckType::TextWithAssociations => DeckType::TextWithAssociations,
        deck::DeckType::Linguistic => DeckType::Linguistic,
    }
}

pub fn capabilities(value: deck::DeckType) -> DeckCapabilitiesDto {
    let caps = value.capabilities();
    DeckCapabilitiesDto {
        associations: caps.associations,
        grammar_tips: caps.grammar_tips,
        word_links: caps.word_links,
        tts: caps.tts,
    }
}

pub fn clamp(value: i64) -> u32 {
    u32::try_from(value.max(0)).unwrap_or(u32::MAX)
}

pub fn onboarding_dto(state: OnboardingState) -> OnboardingStateDto {
    OnboardingStateDto {
        completed: state.completed,
        step: if state.completed {
            OnboardingStep::Ready
        } else {
            OnboardingStep::NativeLanguage
        },
        available_decks: state
            .available
            .into_iter()
            .map(|deck| BundledDeckDto {
                guid: deck.guid,
                name: deck.name,
                description: deck.description,
                target_language: target_language(deck.target_language),
                deck_type: deck_type(deck.deck_type),
                note_count: deck.note_count,
                card_count: deck.card_count,
                level: deck.level,
            })
            .collect(),
        installed_deck_guids: state.installed,
        native_language: native_language(state.native_language),
        target_languages: state
            .target_languages
            .into_iter()
            .map(target_language)
            .collect(),
        daily_goal: state.daily_goal,
    }
}

pub fn dashboard_dto(dashboard: Dashboard) -> DashboardDto {
    DashboardDto {
        decks: dashboard
            .decks
            .into_iter()
            .map(|item| DeckSummaryDto {
                id: item.deck.id.to_string(),
                guid: item.deck.guid.as_str().to_owned(),
                name: item.deck.name,
                parent_id: item.deck.parent_id.map(|id| id.to_string()),
                target_language: target_language(item.deck.target_language),
                native_language: native_language(item.deck.native_language),
                deck_type: deck_type(item.deck.deck_type),
                capabilities: capabilities(item.deck.deck_type),
                counts: DeckCountsDto {
                    new: item.counts.new,
                    learning: item.counts.learning,
                    review: item.counts.review,
                    overdue: item.counts.overdue,
                    total: item.counts.total,
                },
                progress: item.progress,
                bundled: item.deck.bundled,
                content_state: if item.preparing {
                    ContentState::Preparing
                } else {
                    ContentState::Ready
                },
                version: clamp(item.deck.version),
            })
            .collect(),
        streak_days: dashboard.streak_days,
        studied_today: dashboard.studied_today,
        due_today: dashboard.due_today,
        daily_goal: dashboard.daily_goal,
        resumable: dashboard.resumable.map(|open| ResumableSessionDto {
            session_id: open.session_id.0.to_string(),
            deck_id: open.deck_id.map(|id| id.0.to_string()),
            deck_name: open.deck_name,
            status: if open.paused {
                ResumableStatus::Paused
            } else {
                ResumableStatus::Active
            },
            reviewed_count: open.reviewed_count,
            remaining: open.remaining,
        }),
        next_due_at: dashboard.next_due_at.map(iso),
        generated_at: iso(dashboard.generated_at),
    }
}

fn note_type(kind: note::NoteTypeKind) -> NoteTypeKind {
    match kind {
        note::NoteTypeKind::Basic => NoteTypeKind::Basic,
        note::NoteTypeKind::BasicAndReversed => NoteTypeKind::BasicAndReversed,
        note::NoteTypeKind::Sentence => NoteTypeKind::Sentence,
        note::NoteTypeKind::SentenceAndReversed => NoteTypeKind::SentenceAndReversed,
    }
}

/// The checked package as the Transfer screen shows it.
pub fn deck_import_preview_dto(preview: PackagePreview) -> DeckImportPreviewDto {
    let manifest = preview.manifest;
    DeckImportPreviewDto {
        file_name: preview.file_name,
        deck_guid: manifest.deck_guid,
        name: manifest.name,
        description: manifest.description,
        // The package passed the content checks, so these names are known.
        target_language: parse_target_language(&manifest.target_language)
            .map_or(TargetLanguage::De, target_language),
        deck_type: parse_deck_type(&manifest.deck_type).map_or(DeckType::Linguistic, deck_type),
        note_type: parse_note_type(&manifest.note_type).map_or(NoteTypeKind::Basic, note_type),
        content_kind: match manifest.content_kind {
            ContentKind::Words => DeckContentKind::Words,
            ContentKind::Sentences => DeckContentKind::Sentences,
        },
        level: manifest.level,
        origin: manifest.origin.map(|origin| DeckImportOriginDto {
            kind: origin.kind,
            file_name: origin.file_name,
            converted_on: origin.converted_at,
            tool: origin.tool,
        }),
        notes: preview.notes,
        cards: preview.cards,
        scheduled_cards: preview.scheduled,
        suspended_cards: preview.suspended,
        reviews: preview.reviews,
        new_notes: preview.new_notes,
        existing_notes: preview.existing_notes,
        existing_deck_id: preview.existing_deck.map(|id| id.to_string()),
        warnings: preview
            .warnings
            .into_iter()
            .take(50)
            .map(|(code, line)| DeckImportWarningDto {
                code: code.to_owned(),
                line,
            })
            .collect(),
        preview_token: preview.token,
    }
}

/// `JobResultDto::Import` of a deck package: `skip` counts notes already present and unchanged.
pub fn encode_import(outcome: &ImportOutcome) -> String {
    let result = JobResultDto::Import {
        deck_id: outcome.deck_id.to_string(),
        row_errors: Vec::new(),
        create: outcome.created,
        update: outcome.updated,
        skip: outcome.unchanged,
        duplicate: 0,
        error: 0,
    };
    serde_json::to_string(&result).expect("job result serializes")
}

/// `JobResultDto::PrepareContent` as stored in `jobs.result`.
pub fn encode_outcome(outcome: &PrepareOutcome) -> String {
    let result = JobResultDto::PrepareContent {
        deck_ids: outcome.deck_ids.iter().map(ToString::to_string).collect(),
        created: outcome.created,
        updated: outcome.updated,
        unchanged: outcome.unchanged,
        kept: outcome.kept,
        archived: outcome.archived,
        warnings: outcome
            .warnings
            .iter()
            .filter_map(|warning| {
                let code = match warning.code {
                    "overrideConflict" => ContentWarningCode::OverrideConflict,
                    "deckDeleted" => ContentWarningCode::DeckDeleted,
                    _ => return None,
                };
                Some(ContentWarningDto {
                    code,
                    deck_guid: warning.deck_guid.clone(),
                    note_guid: warning.note_guid.clone(),
                    field: warning.field.clone(),
                })
            })
            .collect(),
    };
    serde_json::to_string(&result).expect("job result serializes")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::Path;

    use langualinker_application::content::ContentWarning;
    use langualinker_content::{ContentSource, Mode, lint};
    use langualinker_domain::DeckId;

    use super::*;

    #[test]
    fn content_directory_holds_only_the_encrypted_archive() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../content/decks");
        let on_disk: BTreeSet<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(on_disk, BTreeSet::from(["decks.7z".to_owned()]));
    }

    #[test]
    fn embedded_archive_holds_the_catalog_and_every_deck() {
        let source = bundled();
        let names: BTreeSet<String> = source.names().unwrap().into_iter().collect();
        let mut catalog = String::new();
        std::io::Read::read_to_string(&mut source.open("catalog.json").unwrap(), &mut catalog)
            .unwrap();
        let catalog: langualinker_content::format::Catalog =
            serde_json::from_str(&catalog).unwrap();
        let mut expected = BTreeSet::from(["catalog.json".to_owned()]);
        for stem in &catalog.decks {
            expected.insert(format!("{stem}.json"));
            expected.insert(format!("{stem}.ndjson"));
        }
        assert_eq!(names, expected);
    }

    #[test]
    fn embedded_content_passes_the_dev_gate() {
        let issues = lint(&bundled(), Mode::Dev);
        assert!(issues.0.is_empty(), "{:?}", issues.0);
    }

    #[test]
    fn outcome_encodes_as_the_contract_result() {
        let outcome = PrepareOutcome {
            deck_ids: vec![DeckId(3)],
            created: 2,
            warnings: vec![ContentWarning {
                code: "overrideConflict",
                deck_guid: "d".into(),
                note_guid: Some("n".into()),
                field: Some("back".into()),
            }],
            ..PrepareOutcome::default()
        };
        let json: serde_json::Value = serde_json::from_str(&encode_outcome(&outcome)).unwrap();
        assert_eq!(json["type"], "prepareContent");
        assert_eq!(json["deckIds"], serde_json::json!(["3"]));
        assert_eq!(json["warnings"][0]["code"], "overrideConflict");
    }

    #[test]
    fn attributions_list_dependencies_then_content() {
        let list = attributions(vec![("Deck".into(), "Licence".into(), None)]);
        assert_eq!(list.last().unwrap().scope, AttributionScope::Content);
        assert!(
            list.iter()
                .any(|item| item.name == "Tauri" && item.scope == AttributionScope::Dependency)
        );
    }
}
