# KMP → Rust migration manifest

- Date: 2026-10-12
- Source: KMP repository, commit `c2fa62fe0d260e71eef09d99ae5fd0c66b020151` (read-only; not tagged or modified)
- Decision context: [ADR-08](../adr/ADR-08-fresh-database.md) — fresh Rust database, no user-data migration. This project is a clean slate that reuses the KMP code and assets **as reference only**.
- Integrity: every copied file is listed with SHA-256 in [`scripts/fixtures.lock.json`](../../scripts/fixtures.lock.json) and checked by `node scripts/verify-fixtures.mjs` (CI). Copies were byte-compared with the source at copy time.

## 1. Copied artifacts (readable copies in this repository)

| Artifact | KMP path | Copy in this repo | Use |
|---|---|---|---|
| Content manifest | `shared/src/commonMain/composeResources/files/decks/manifest.json` | `content/decks/manifest.json` | Phase 8 input (to be upgraded to the plan's manifest format) |
| 4 deck headers | `…/decks/{de,en}_{words,sentences}_a1.json` | `content/decks/*.json` | Phase 8 input |
| 4 deck bodies, 113 records | `…/decks/*.ndjson` (DE words 43, DE sentences 12, EN words 46, EN sentences 12) | `content/decks/*.ndjson` | Phase 8 input; Phase 2 mock may adopt texts |
| SQLDelight schema (8 tables) | `shared/src/commonMain/sqldelight/com/mila/langualinker/database/*.sq` | `tests/fixtures/kmp/schema/*.sq` | Design reference for Phase 7 |
| Migration v1→v2 | `…/database/1.sqm` | `tests/fixtures/kmp/schema/1.sqm` | Historical reference only (ADR-08) |
| APKG sample | `shared/src/commonMain/assets/sample_german_a1_sentences.apkg` | `tests/fixtures/kmp/apkg/` | Phase 14 interoperability fixture (test-only, see provenance) |
| FSRS parameters + test specification | `fsrs/FsrsParameters.kt`, `commonTest/FsrsTest.kt` | `tests/fixtures/fsrs/kmp-fsrs-spec.json` | Phase 9 cross-check (not conformance vectors) |

Not copied (code, ported by reading the KMP source when the phase starts): `fsrs/*.kt`, `domain/model`, `domain/usecase`, `data/repository`, `data/importer`, `cliApp/ApkgReader.kt`, Compose UI, Gradle, Koin, platform factories.

## 2. Table manifest (KMP schema v2 → Rust schema v1)

Rust targets refer to plan Phase 7 steps 6–18. "Decision": keep / change / drop / new.

### `decks`
| KMP column | Type | Rust target | Decision |
|---|---|---|---|
| id | INTEGER PK | `decks.id` | keep (i64, string over IPC) |
| name | TEXT | `decks.name` | keep |
| language | TEXT | `decks.target_language` | change: split into L1/L2 (`native_language`, `target_language`) |
| type | TEXT 'Linguistic' | `deck_settings.deck_type` | change: DeckType lives with settings, hides never deletes |
| — | | guid, parent_id, description, config_id, content_version, bundled, created/updated/deleted, usn | new |

### `deck_settings`
| KMP column | Type | Rust target | Decision |
|---|---|---|---|
| deck_id | INTEGER PK | — | change: settings become shared presets (`deck_config`) referenced by `decks.config_id` (single config FK) |
| request_retention | REAL 0.9 | `deck_config.desired_retention` | keep (rename) |
| maximum_interval | INTEGER 36500 | `deck_config.maximum_interval_days` | keep |
| new_cards_per_day | INTEGER 20 | `deck_config.new_per_day` | keep |
| — | | 21 FSRS-6 weights, learning/relearning steps, review limit, fuzz, order, bury, leech, sync fields | new |

### `card_templates`
| KMP column | Type | Rust target | Decision |
|---|---|---|---|
| id, name, front_template, back_template | | `card_templates` | keep concept |
| deck_id | INTEGER | `card_templates.note_type_id` | change: templates belong to NoteType, not Deck |
| — | | guid, ordinal, version | new |

### `cards` (KMP mixes note content and schedule)
| KMP column | Type | Rust target | Decision |
|---|---|---|---|
| id | INTEGER PK | `cards.id` | keep |
| deck_id | INTEGER | `cards.deck_id` | keep (scheduling deck belongs to Card) |
| front, back, tags | TEXT | `notes.fields`, `note_tags` | change: content moves to Note (one Note → N Cards) |
| card_type | TEXT 'Sentence' | `notes.note_type_id` + `cards.template_ordinal` | change |
| position | INTEGER | `cards.position` (New only) | keep |
| due | INTEGER | `cards.due_at` (UTC ms, NULL for New) | change: KMP stores both `due` and `next_review_date` |
| next_review_date, last_review_date | TEXT ISO date | `cards.due_at`, `cards.last_review_at` (UTC ms) | change: day-granular text → epoch ms |
| stability, difficulty | REAL | `cards.stability`, `cards.difficulty` (NULL for New) | keep (f64) |
| retrievability | REAL | — | drop: computed from time/logs, not stored (plan step 16) |
| ease_factor | REAL 2.5 | `cards.legacy_ease` | keep only for Anki interop |
| average_interval | INTEGER | — | drop: computed |
| reps, lapses | INTEGER | same | keep |
| scheduled_days, elapsed_days | INTEGER | `cards.scheduled_days`, `cards.elapsed_days` | keep |
| card_state | TEXT 'New' | `cards.state` + `cards.queue` | change: separate CardState/QueueState, explicit DB codes |
| created_at | INTEGER | `cards.created_at` | keep |
| — | | guid, note_id, template_ordinal, flags, learning_step, buried_until, version, updated/deleted/usn | new |

### `associations`
| KMP column | Type | Rust target | Decision |
|---|---|---|---|
| card_id | INTEGER | `associations.note_id` | change: attach to Note (plan §14.4) |
| content | TEXT | `associations.text` | keep |
| type | TEXT 'Generated' | `associations.provenance` (bundled/generated/manual) | change |
| is_favorite | INTEGER | same + partial unique index `(note_id) WHERE is_favorite=1 AND deleted_at IS NULL` | change |
| created_at | INTEGER | same | keep |
| — | | L1/L2, prompt_version, model, moderation, cache_key, order, deleted_at | new |

### `grammar_tips`
| KMP column | Type | Rust target | Decision |
|---|---|---|---|
| card_id | INTEGER | `grammar_tips.note_id` | change |
| content, sort_order, source, created_at | | text, order, provenance, created_at | keep |

### `sentence_word_links`
| KMP column | Type | Rust target | Decision |
|---|---|---|---|
| sentence_card_id | INTEGER | `sentence_note_id` | change (card→note) |
| word_card_id | INTEGER NULL | `word_note_id` NULL + optional `preferred_word_card_id` | change |
| position_in_sentence | INTEGER (token index) | `start_index`, `end_index` (UTF-16, NFC, half-open) | change: token index is not convertible without re-tokenizing — recompute from `surface_form` with a content version bump (plan §14.1) |
| surface_form | TEXT | same + `lemma` | keep |
| resolution | TEXT 'Unresolved' | same enum (resolved/unresolved/ambiguous/userOverridden) | keep |

### `review_logs`
| KMP column | Type | Rust target | Decision |
|---|---|---|---|
| card_id, rating, reviewed_at | | same (`reviewed_at` UTC ms) | keep |
| scheduled_days, elapsed_days, stability, difficulty | | part of `card_after` snapshot | change: full `card_before`/`card_after`/scheduler delta |
| — | | session_id, unique operation_id, state_before, fsrs_version, parameters_hash, invalidated_at | new |

### New in Rust (no KMP equivalent)
`notes`, `note_types`, `tags`/`note_tags`, `study_sessions`, `session_items`, `review_undo_entries`, `settings`, `jobs`, `llm_requests` (outbox), `association_cache`, `media`, `anki_origin`, `content_overrides`, `import_batches`, FTS5 index.

### Migration history
KMP schema v2 has one migration (`1.sqm`, v1→v2): `sentence_word_links.word_card_id` became nullable and gained `resolution`; old `0` placeholders became `NULL`/Unresolved. Rust numbering starts at v1 independently; the lesson carried over: nullable FK + explicit resolution state from day one.

## 3. Bundled content findings (input for Phase 8 / D-04)

| Check | Result |
|---|---|
| Decks / records | 4 decks, 113 records (DE 43 + 12, EN 46 + 12) — matches the plan |
| Record fields | words: `front`, `back`, `tags`; sentences: + `grammarTips[]`, `wordLinks[{surfaceForm, wordCardRef}]` (109 links) |
| Note GUIDs | **missing** — Phase 8 must assign stable GUIDs before first release |
| Pre-generated associations | **missing** in all decks — required by MoSCoW "Must"; content work for Phase 8 |
| Word-link ranges | **missing** (surface form only) — compute UTF-16 ranges in the importer/linter |
| Grammar tips on word decks | none (only sentence decks) |
| License field | `{"source": "original", "attribution": null}` in every deck header — see `content/PROVENANCE.md` |
| Size vs. plan target | 113 records vs. proposed 800–1,200 words + 300–500 sentences per language (D-04) |

## 4. KMP test inventory (137 `@Test`)

| File | Tests | Area | Rust phase |
|---|---:|---|---|
| `commonTest/FsrsTest.kt` | 54 | FSRS algorithm (31) + scheduler (23) | 9 — captured in `kmp-fsrs-spec.json` |
| `jvmTest/SqlDelightRepositoryTest.kt` | 22 | repositories, cascade delete, favorite uniqueness, tip order, log paging | 7 |
| `commonTest/CsvImportTest.kt` | 15 | CSV delimiter/BOM/header/deck routing/due parsing | 14 |
| `jvmTest/BundledDeckImporterTest.kt` | 14 | importer order, idempotence, homographs, atomicity | 8 |
| `jvmTest/BundledContentLinterTest.kt` | 12 | content linter rules | 8 |
| `commonTest/ImportExportRoundTripTest.kt` | 9 | JSON/NDJSON/CSV round-trip | 12/14 |
| `jvmTest/DatabaseMigrationTest.kt` | 4 | v1→v2 migration | — (ADR-08) |
| `jvmTest/EnsureBundledDecksUseCaseTest.kt` | 4 | first-launch import idempotence | 8/10 |
| `SharedCommonTest`, `SharedLogicAndroidHostTest`, `SharedLogicIOSTest` | 3 | template examples | — |

The plan's "135 data-layer tests" does not match the source: 137 tests in total, of which ~80 touch data/import/export and 54 are FSRS. Test names are the behavioural checklist to port; the Kotlin code itself is not translated.

## 5. Open after Phase 0

- License/rights confirmation for content and the APKG (`content/PROVENANCE.md`, D-04/ADR-14).
- Pinned FSRS-6 reference implementation and numeric vectors (Phase 9); KMP default weights must be cross-checked against it.
