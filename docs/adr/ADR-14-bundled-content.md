# ADR-14: Bundled content — format v2, importer, linter and licensing gate

- Status: Proposed (2026-11-04); engineering part implemented in Phase 8. The content decision itself (D-04: size, authors, rights, native reviewer) is open and blocks beta/release.
- Plan references: Phase 8 steps 1–22, §14.1 (Unicode ranges); D-04, D-05; risks R2, R6, R10, R12

## Context

The app must start offline with DE/EN decks that already carry associations, grammar tips and sentence→word links. Phase 0 copied 113 KMP records (43 + 12 DE, 46 + 12 EN) without note GUIDs, link ranges or associations, and with unconfirmed rights ([PROVENANCE.md](../../content/PROVENANCE.md)). The plan's target size (800–1 200 words and 300–500 sentences per language, natively reviewed) is editorial work, not code.

## Decision

### Format v2 (`content/decks`, `crates/content/src/format.rs`)

1. `catalog.json` lists the decks in import order (word decks before the sentence decks that link to them). One JSON manifest per deck: `schemaVersion` 2, `deckGuid`, `contentVersion` (`YYYY.MM.DD`, compared as text), name/description, CEFR level, L1/L2, `deckType`, `noteType`, `contentKind` (`words`/`sentences`), `dependsOn`, `notesFile`, `sha256`, `expectedNotes`, `expectedCards`, `license { name, attribution, url, rightsConfirmed }`, `review { status, reviewer, reviewedAt }`.
2. One NDJSON line per note: note `guid` (stable, ≤ 48 characters), `front`, `back`, `tags`, `associations[] { guid, text, promptVersion, model }`, `grammarTips[] { guid, text }`, `wordLinks[] { start, end, surfaceForm, lemma, wordGuid }`. Every type rejects unknown fields; an unknown `schemaVersion` is rejected.
3. Card GUIDs are generated at install (random, ≠ note GUID). Note count and card count are separate (`expectedCards` = notes × templates; word decks use `BasicAndReversed`, sentence decks `Sentence`).
4. Links reference word notes by GUID, never by text, so homographs and multiword units (`ins` → lemma `in das`) are unambiguous. Ranges are UTF-16 code units over NFC text, half-open — the rule of plan §14.1, implemented once in Rust (`crates/domain/src/text.rs`) and once in TypeScript (`frontend/src/services/text.ts`), both tested against `tests/fixtures/unicode/ranges.json`.

### One set of checks for CI and install time (`crates/content/src/check.rs`)

5. `content-lint` (CI, dev gate) and the importer run the same checks: malformed lines, unknown fields/versions, languages, licence name, duplicate GUIDs across decks, duplicate fronts (homographs are a warning), empty fields, NFC/whitespace/markup, lengths (associations and tips ≤ 500 characters, the editor's limit), ranges (bounds, surrogate pairs, surface, overlap), unresolved `wordGuid`, note/card counts and the checksum. Requirements depend on the content kind: every word note needs an association; every sentence links at least one word; tips are optional per note.
6. `content-lint --release` additionally requires `review.status = approved` with reviewer and date and `license.rightsConfirmed`. The current content fails it on purpose until D-04 is closed.

### Importer (`crates/application/src/content.rs`)

7. **Pending list.** `completeOnboarding` stores the answers and adds the chosen, not yet installed decks to the `content.pendingDecks` setting; startup adds installed decks whose bundled `contentVersion` is newer. A `prepareContent` job installs the list in catalog order. One job writes at a time.
8. **Validate, then one transaction per deck.** Every pending deck is streamed and validated before anything is written (cancellable, persisted progress per 250 records). Then each deck is streamed again inside one write transaction, which also removes the deck from the pending list. A kill or error rolls the deck back as a whole; there is no staging table to clean. After the commit phase starts, a cancel is ignored (ADR-09). Restart resumes the remaining decks automatically.
9. **Idempotent.** Notes, associations and tips are matched by GUID; one live card per (note, template). A second run creates nothing and writes no scheduling column (`due`, S/D, reps, lapses, queue).
10. **User data wins.** Fields named in `content_overrides` (`front`, `back`, `tags`, `noteType`, `wordLinks`, `association:<guid>`, `tip:<guid>`) are kept; a differing bundled value is reported as an `overrideConflict` warning in the job result. Deleted notes, associations and tips stay deleted; a deleted bundled deck is not reinstalled (`deckDeleted`). Name, description and type of an installed deck belong to the user.
11. **Removed items (keep/archive policy).** Schema v2 adds `bundled_notes` (provenance per note). A note a newer version no longer ships is **kept** when it has progress (reps, non-new state or a review log) or user edits, and otherwise its cards are **archived** (history intact). If a later version ships it again, the importer restores exactly the cards it archived itself. Nothing is deleted.
12. **Links.** A sentence deck installed without its word deck keeps its links `unresolved` with the lemma; installing or updating the word deck queues the installed sentence decks again and resolves them.
13. **Result.** `JobResultDto::PrepareContent` (contract 1.1) carries `deckIds`, `created/updated/unchanged/kept/archived` and `warnings`. Invalid content fails the job with `VALIDATION_FAILED { field: "content", reason: <check code> }`; file and line go to the log, never card text.

### Content in the app

14. The shell embeds `content/decks` with `include_bytes!` (no asset paths on Android/iOS); a test fails when a file is added to the directory but not embedded. `getAttributions` lists the deck licences from the manifests plus the main dependencies (a generated full list follows with the release audit).

### Authoring pipeline

15. Candidate associations come from a versioned prompt (`content/tools/prompts/association-candidates-v1.md`); each record names `promptVersion` and `model`. Every item needs native review before the deck may be marked approved.

### Import packages and the Anki converter (owner decision 2026-11-04)

16. **Package.** A learner's own deck travels as one NDJSON file: line 1 `{"package": 1, "manifest": {…}}` with `source: "import"` and an `origin` (`kind`, `fileName`, `convertedAt`, `tool`, `sourceDecks`), then note records. Only packages may carry `cards[]` (per template: state, queue, `dueAtMs`, position, `scheduledDays`, FSRS stability/difficulty, ease ‰, reps, lapses, last review, flag) and `reviews[]` (rating, kind, interval, ease, duration, oldest first). Bundled content with progress is rejected (`scheduling.bundled`); packages need no associations or word links and depend on no deck. `content-lint <file>` lints a package.
17. **Converter `apkg-convert`** (`crates/anki`, desktop only — the app never links its ZIP/zstd code). Reads `collection.anki21b` (zstd, schema 18, custom `unicase` collation registered), `collection.anki21` or `collection.anki2` (schema 11, JSON note types); refuses the compatibility stub. Options: `--language de|en`, `--kind words|sentences`, `--reverse`, `--name`, `--deck-type`, `--level`, `--anki-deck` (with subdecks), `--front-field`/`--back-field` (index or name), `--no-scheduling`, `--no-reviews`, `--strict`, `--list`. HTML fields become plain NFC text (block tags → lines; sound/image references removed and counted). Cloze notes, empty fronts and exact duplicates are skipped and reported; notes the content rules reject are dropped and listed (or fail with `--strict`).
18. **Scheduling mapping.** `type` → state; `queue` keeps suspended and both buried kinds (preview/inconsistent queues fall back to the state's natural queue); filtered-deck cards use their home deck and `odue`. `due` of a new card is its position; a value above 10⁹ is epoch seconds (intraday learning); otherwise days since `col.crt` → UTC ms. `ivl` → `scheduledDays` (negative seconds → 0), `factor` ≥ 1300 → SM-2 ease (under FSRS the column holds difficulty×100 and is dropped), flags 0–7, FSRS `s`/`d` from `cards.data`. Revlog rows with a rating become history; rows without (manual rescheduling) are skipped. **Not carried:** the position inside learning steps (a learning card restarts its steps), media files, card templates/styling, deck options (the deck gets the default preset), Anki's sibling burying state beyond the queue.
19. **Import in the app** (Transfer screen, contract 1.2). `pickFile({purpose: 'import'})` reads the picked file once (≤ 64 MiB) into memory behind an opaque token; `previewDeckImport` checks it with the same rules and reports notes, cards, cards with progress, suspended/buried, reviews, new vs existing notes and warnings; `commitDeckImport` imports it in one transaction as a job `import` (result `JobResultDto::Import`). A `.apkg` picked directly is `UNSUPPORTED_FORMAT { format: "apkg" }` with conversion instructions. Only the latest preview can be committed; a collection change since the preview is `PREVIEW_STALE`.
20. **Progress semantics.** Scheduling and history apply only to cards the import creates. Importing the same package again (same deck and note GUIDs, derived from the Anki deck name and note GUID) adds new notes, updates unedited content and never touches cards that already exist, so progress made in the app wins. Imported review logs carry `fsrs_version = "import"`; their before/after snapshots are reconstructed from the revlog row and they are never undoable.

## Consequences

- The 113 converted records plus 89 candidate associations are development content: usable in dev/demo/tests, blocked for beta/release by the release gate.
- Import of 5 000 notes (10 000 cards) including validation takes 2.6 s on the development PC (release build, in-memory database, `imports_five_thousand_notes_within_budget`); memory is O(longest line) for parsing plus the GUID set of one deck. The device measurement belongs to the Phase 8 device run.
- The study day of the dashboard uses the collection cutoff in UTC until the scheduler (Phase 9) owns time zones; there is no resumable session before Phase 10.
- The real Anki export in `tests/fixtures/kmp/apkg` (schema 18, 432 notes, 864 cards, 5 329 reviews, FSRS state on 812 cards) converts without loss reports beyond flattened formatting and imports with its due dates, FSRS state and history (`anki_package_imports_with_its_scheduling_and_history`).
- APKG stays a desktop conversion step in v1; reading `.apkg` inside the app remains v1.1 (ADR-11/D-05). CSV import stays in Phase 14.
- Open: D-04 (size, authors, reviewer, rights), the native review of the candidates, the device run of the first-start flow and of the package import (Android `content://`, iOS security-scoped files).
