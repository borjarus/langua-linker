# ADR-19: CSV import/export and Anki `.apkg` export

- Status: Proposed (2026-11-16); implemented in Phase 14 steps 1–3 ("14a"). Acceptance after the APKG opens in a tested Anki desktop version (counts, content, directions, due dates compared) and the 5–10 external APKG fixtures exist. Step 4 (optional `.apkg` import, v1.1) is not part of this decision.
- Plan references: Phase 14 steps 1–15 and DoD; D-05 (APKG scope); ADR-11/ADR-14 (APKG import stays the desktop converter in v1); ADR-15 (study calendar); ADR-17 (backup is the lossless form, `shareFile`)

## Context

Phase 5 built the Transfer screen (CSV import with preview, export panel) on the demo adapter; contract 1.2 declared `previewImport`, `commitImport` and `exportDeck` as pending (`NOT_IMPLEMENTED`). Phase 8 added the `.ndjson` deck package import and the desktop `apkg-convert` tool. Phase 14a makes CSV import, CSV export and `.apkg` export real.

## Decision

### Crate layout

1. A new crate `langualinker-interop` holds format code without collection access: `csv` (decoding, directives, detection, reader, writer) and `anki::codec` (card columns), plus `anki::schema11` (the `.apkg` writer) behind the default feature `apkg` (SQLite, ZIP, SHA-1). The application layer links it without that feature; only the infrastructure writes `.apkg` files.
2. `application::transfer` plans and applies an import inside the caller's transaction and reads the export snapshot; `infrastructure::transfer::TransferService` owns the preview, the jobs and the export files; the shell (`src-tauri/src/transfer.rs`) maps DTOs.

### CSV import (step 1)

3. **Decoding**: a BOM decides (UTF-8, UTF-16 LE/BE); without one valid UTF-8 is UTF-8, anything else Windows-1252. The UI may force an encoding; a forced encoding that does not fit (or contradicts the BOM) is `VALIDATION_FAILED { field: "file", reason: "encoding" }`, never a guess. Files with NUL bytes, ZIPs (`apkg`) and SQLite files are `UNSUPPORTED_FORMAT`.
4. **Delimiter**: comma, semicolon and tab are tried on the first 100 records with quotes respected; only a single candidate with a consistent column count ≥ 2 counts as detected. Otherwise `detectedDelimiter` is `null` and the UI picker decides (the reader uses a comma meanwhile). Anki text-export directives (`#separator`, `#columns`, `#html`) are honoured and skipped; HTML fields are reduced to text (scripts and styles dropped — imported HTML is never rendered).
5. **Reader**: RFC 4180 (quoted delimiters, `""`, line breaks in quotes; CRLF/LF/CR); a quote inside an unquoted field is literal; blank lines are skipped; limits 32 MiB, 100 000 rows, 64 columns, 256 KiB per field (file errors with the line). Field limits of the collection (10 000 characters, tags) are row errors.
6. **Mapping**: `front`, `back`, `tags` (space-separated), `guid`, `deck` (`Parent::Child`), `noteType`, `due`, `interval`, `reps`, `lapses`, `position`, `ignore`. Header names suggest the mapping (English, Polish, German, Anki), else front/back/tags by position. Exactly one front and one back; other targets at most once.
7. **Matching and policies**: a GUID matches the note with that GUID anywhere; otherwise the front matches a note of the row's deck (with subdecks) equal after trim, NFC and case folding — the editor's duplicate rule. Rows of the same file match each other. `skip` keeps the match; `update` writes the back (and the front for a GUID match) and, when mapped, the tags — never the schedule or the note type; bundled notes record content overrides; `duplicate` creates a new note (new GUID when the GUID matched). A GUID of a deleted note is a row error. Foreign GUIDs (Anki base91) map to a stable derived GUID so a second import updates instead of duplicating.
8. **Scheduling (step 3)**: applies only to created notes and only to the forward card. A `due` (ISO 8601; a date is the start of that study day, a date-time without zone is UTC) makes a review card with `interval` (default: days until due, ≥ 1), `reps` (≥ 1), `lapses`; no FSRS memory, so the scheduler treats it as a legacy card (like imported Anki cards). An empty `due` keeps the card new; `position` orders it after the existing new cards. Basic CSV carries no FSRS memory and no history.
9. **Preview = commit**: `previewImport` plans against a read snapshot and writes nothing; it stores the parsed file, the options and the collection fingerprint. `commitImport` is `PREVIEW_STALE` for another token or a changed collection; the job plans again inside one write transaction and applies the plan there (decks, notes, cards). Cancel is checked every 100 rows and rolls everything back; the commit phase starts as the transaction's last step. There is no staging table: one transaction is the staging.
10. **Decks**: `targetDeckId: null` creates a deck named after the file (deck paths whose first level has that name go into it). Missing levels of `deck` paths are created with the target deck's languages and type (else the onboarding language, a simple deck) and the default preset. `newDecks` lists them in the preview (contract 1.3).
11. **Formula guard (import)**: optional `formulaGuard` removes exactly the `'` a guarded export put before `=`, `+`, `-`, `@`, tab or CR.

### CSV export

12. One row per note: `guid, front, back, tags, deck, noteType`, with scheduling `due` (ISO UTC with milliseconds, empty for new), `interval`, `reps`, `lapses`, `position` (new cards) of the **forward** card. UTF-8 with BOM, CRLF, quotes where needed. The **formula guard is an explicit option** (`formulaGuard`): with it, fields starting with `=`, `+`, `-`, `@`, tab or CR get a leading `'`; without it (the raw variant, default) every field is the exact text. A text that itself starts with `'=` does not survive a guarded round trip; the raw variant is lossless.

### Anki codec (step 2)

13. `due` is read by queue: new → position; learning (1) → epoch seconds; review (2) and day-learning (3) → study days since `col.crt`; suspended/buried by `type` (learning values above 10⁹ are seconds — Anki's own heuristic). Filtered cards decode to `odid`/`odue`. Days are **calendar study days** of the collection's zone and cutoff (DST-safe), not `(due − crt) / 86400`; `crt` is the start of a study day, `rollover` the cutoff in whole hours (a non-hour cutoff is reported), `creationOffset` the zone offset in minutes west.
14. Version awareness: scheduler v1 relearning (`type 2` in a learning queue) decodes as relearning, v1 has no queue −3 (an error, not a guess); unknown `type`/`queue` values are errors. FSRS memory is read from `cards.data` (`s`, `d`). Decoded cards keep their raw columns (`RawOrigin`) for the optional import (step 4).

### `.apkg` export (step 3)

15. Legacy schema 11 (`collection.anki2`, `col.ver = 11`, `schedVer = 2`) plus an empty `media` map in a deflated ZIP — the layout every Anki 2.1+ imports. Note GUIDs are kept; fields are HTML-escaped and joined with U+001F; `sfld` is the front, `csum` the first 32 bits of SHA-1; tags space-separated (whitespace in a tag → `_`, reported). One note type per built-in kind with a fixed id (forward `{{Front}}`, reverse `{{Back}}`), so a re-export updates the same Anki note type. Deck names keep the hierarchy (`Root::Child`). Ids are epoch milliseconds (creation times; a collision takes the next id after the highest taken).
16. Cards through the codec: `ivl`, legacy `factor` (2500 when unknown), reps/lapses, flags, `left` = one step (the position inside the steps is not portable, reported), FSRS memory as card data `{"s","d"}` — read by Anki 23.10+, **not verified across versions**, reported as `fsrsMemoryUnverified`. The review log (not undone reviews) becomes `revlog` rows (rating, intervals before/after, factor, time capped at 60 s, kind); the FSRS snapshot of each review has no column (`reviewSnapshots`).
17. **Loss report** (`JobResultDto.export.losses`, contract 1.3): associations, grammar tips, word links and archived cards for every format; CSV adds review history, FSRS memory, flags and reverse-card progress. Codes: `associations`, `grammarTips`, `wordLinks`, `reviewHistory`, `reviewSnapshots`, `fsrsMemory`, `fsrsMemoryUnverified`, `learningSteps`, `buryExpiry`, `cutoffRounded`, `archivedCards`, `tagWhitespace`, `reverseScheduling`, `flags`. The own backup (ADR-17) stays the lossless form.

### Files and sharing

18. Exports are written to a hidden temporary file in `<data>/exports`, synced and renamed to `<jobId>.<csv|apkg>`; a kill leaves no half file (temporaries are removed at start). The newest five are kept. `shareFile { kind: "export", jobId }` copies the file out through the save dialog with the offered name `<deck>.csv|apkg`; an unknown or unfinished export is `NOT_FOUND`.
19. Errors stay within contract 1.x: a deleted deck in `exportDeck` is `NOT_FOUND` (the operation declares no `DELETED`).

### Contract 1.3 (minor)

20. New Rust DTOs (generated TypeScript): `ImportInput` (+ optional `encoding`, `formulaGuard`), `ImportPreviewDto` (+ `newDecks`), `CommitImportInput`, `ExportDeckInput` (`format: "csv" | "apkg"`, optional `formulaGuard`), `ExportLossDto`, `CsvDelimiter`, `CsvEncoding`, `DuplicatePolicy`, `CsvColumnTarget` (+ 8 targets), `ExportFormat`; `JobResultDto.export` gains optional `format` and `losses` (a 1.2 result still reads). The frontend and backend ship together; a new `CsvColumnTarget` member in the preview output is a documented deviation from the "new enum member in an output is major" rule.

## Measurements (release build, Windows desktop, 2026-11-16)

20 000 rows: preview 65 ms, import (one transaction) 3.5 s, CSV export 0.5 s (1.7 MB), APKG export 1.1 s (1.4 MB); a cancel right after the start leaves the collection unchanged (`crates/infrastructure/tests/transfer.rs`, `cargo test --release … twenty -- --nocapture`). Phone numbers follow with the device run.

## Consequences

- CSV import and both exports work offline on every platform; the phones carry the SQLite/ZIP writer (zlib-rs, no C toolchain).
- The `.apkg` is verified by reading it back with the desktop converter's reader (schema, notes, fields, GUIDs, tags, card types/queues, due ranges, revlog count, deck names, note types). Opening it in Anki desktop is a manual acceptance step not automated here.
- Only one external APKG fixture exists (`tests/fixtures/kmp/apkg`); the plan's 5–10 legal external fixtures remain open, as does the optional import (step 4).

## Alternatives considered

- **Writing schema 18 (`collection.anki21b`, zstd, protobuf config)**: newer Anki reads schema 11 as well; the plan orders legacy first and newer formats only after a tested variant.
- **A staging table for imports**: one write transaction gives the same all-or-nothing result with less state; progress inside the transaction is not persisted (the job shows 0 → done), which is acceptable at ~3.5 s for 20 000 rows.
- **One CSV row per card**: rows per note are what spreadsheets and Anki text exports use; reverse-card progress is reported as a loss instead.
