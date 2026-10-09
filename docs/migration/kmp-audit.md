# KMP Audit and Migration Manifest

Audit date: 2026-10-09

## Source repository

KMP reference remains at `C:\Users\m.puczynski\IdeaProjects\LanguaLinker`. It is not modified or deleted by this repository setup.

## Verified assets

| Category | KMP source | Migration action |
| --- | --- | --- |
| FSRS | `shared/src/commonMain/kotlin/com/mila/langualinker/fsrs/` | Port and validate against vectors before real scheduling. |
| Domain models | `shared/src/commonMain/kotlin/com/mila/langualinker/domain/model/` | Port intentionally to Rust domain crate; do not copy UI assumptions. |
| Use cases | `shared/src/commonMain/kotlin/com/mila/langualinker/domain/usecase/` | Reimplement as Rust application use cases. |
| SQL schema | `shared/src/commonMain/sqldelight/com/mila/langualinker/database/*.sq` | Replace with fresh numbered Rust/SQLite migrations. |
| SQL migration | `shared/src/commonMain/sqldelight/com/mila/langualinker/database/1.sqm` | Preserve as v1-to-v2 fixture/reference; not same migration numbering as Rust schema. |
| Repositories | `shared/src/commonMain/kotlin/com/mila/langualinker/data/repository/` | Replace SQLDelight implementations with Rust infrastructure adapters. |
| Importers | `shared/src/commonMain/kotlin/com/mila/langualinker/data/importer/` | Port schema and test cases; retain content-version semantics. |
| Bundled content | `shared/src/commonMain/composeResources/files/decks/` | Copy only after license/content review; preserve manifest, JSON, and NDJSON as fixtures. |
| APKG fixture | `shared/src/commonMain/assets/sample_german_a1_sentences.apkg` | Retain as interoperability fixture after license/security review. |
| Tests | `shared/src/commonTest`, `shared/src/jvmTest`, `shared/src/androidHostTest`, `shared/src/iosTest` | Translate FSRS, importer, migration, repository, and round-trip behavior into Rust test vectors. |

The KMP project has schema version 2 and an explicit `1 -> 2` migration. It uses SQLDelight, not `rusqlite`.

## Classification

- Preserve: source content, content manifest, APKG fixture, migration fixtures, behavior vectors.
- Port: FSRS, scheduler behavior, domain models, import/export rules, tests.
- Replace: Compose UI, Gradle, SQLDelight drivers, Koin modules, platform factories.
- Do not delete: KMP sources or databases before a separate backup and user-install audit.

## Open decisions blocking data migration

1. Confirm whether production KMP databases exist. Without them, Rust starts with fresh schema v1; with them, use backup -> staging conversion -> validation -> safe swap.
2. Inventory real database fixtures, their schema versions, GUID fields, review history, settings, and bundled-content overrides.
3. Approve licenses and provenance before copying deck content or APKG fixtures.
4. Pin FSRS reference implementation, parameter set, and conformance vectors before porting scheduler code.
5. Decide Android/iOS toolchain and native secret-store plugin after physical-device spikes.

## Phase 0 exit evidence

This audit establishes reference assets and avoids inherited KMP completion status. It does not claim a Rust port, migration, or production-data compatibility.