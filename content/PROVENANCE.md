# Content and fixture provenance

Every asset that may ship in the app or is used as a test fixture is listed here. Release builds may only bundle assets whose **Release status** is `approved`; `content-lint --release` enforces it through each manifest's `review` and `license.rightsConfirmed` ([ADR-14](../docs/adr/ADR-14-bundled-content.md)).

| Asset | Origin | Declared license | Authorship / generation | Native review | Release status |
|---|---|---|---|---|---|
| `content/decks/de_words_a1.*` (43 notes, 86 cards) | KMP commit `c2fa62f`, converted to format v2 on 2026-11-04 | "Original LanguaLinker content — rights audit pending (D-04)" | fronts/backs: not recorded (author or LLM-assisted unknown); associations: LLM candidates, prompt `association-candidates-v1`, model `claude-opus-5-5`, 2026-11-04 | pending | **blocked** — confirm author, rights, review (D-04/ADR-14) |
| `content/decks/de_sentences_a1.*` (12) | as above | as above | fronts/backs/tips: not recorded; link ranges and word GUIDs derived mechanically from the KMP `wordLinks` | pending | **blocked** |
| `content/decks/en_words_a1.*` (46 notes, 92 cards) | as above | as above | as `de_words_a1` | pending | **blocked** |
| `content/decks/en_sentences_a1.*` (12) | as above | as above | as `de_sentences_a1` | pending | **blocked** |
| `content/tools/prompts/association-candidates-v1.md` | written for Phase 8 | project | project | — | tooling, not bundled |
| `tests/fixtures/kmp/content/*` | KMP commit `c2fa62f` (the original v1 files, byte-identical, moved from `content/decks`) | `source: original`, no attribution | not recorded | — | **test/reference only**, never bundled |
| `tests/fixtures/unicode/ranges.json` | written for Phase 8 (plan §14.1) | project | project | — | test only |
| `tests/fixtures/kmp/apkg/sample_german_a1_sentences.apkg` | KMP commit `c2fa62f` (added in "implementation of phase 3") | none | a real Anki export: `collection.anki21b` (zstd, schema 18) holds 432 notes (Polish front, German back), 864 cards, 5 329 reviews with FSRS state; `collection.anki2` is only Anki's "please update" stub | — | **test-only**, never bundled; input of the converter and importer tests |
| `tests/fixtures/kmp/schema/*` | KMP commit `c2fa62f` | project code | project | — | test/reference only |
| `tests/fixtures/fsrs/kmp-fsrs-spec.json` | derived from KMP `FsrsParameters.kt`/`FsrsTest.kt` | project code; FSRS default weights are published by open-spaced-repetition | project | — | test/reference only |
| `frontend/src/demo/fixtures.ts` | written for the Phase 2 demo | project code | project; includes sample mnemonics | — | demo only, excluded from release bundles |

## Conversion v1 → v2 (2026-11-04)

Text was NFC-normalized and trimmed; nothing was reworded. Note GUIDs are `<dew|des|enw|ens>-` + the first 12 hex characters of SHA-256(`<deck stem>:<front>`), fixed from now on. Association and tip GUIDs derive from their note (`-a1`, `-t1`…). Link ranges are the first left-to-right occurrence of each KMP `surfaceForm`; `lemma` is the KMP `wordCardRef` and `wordGuid` its word note. Manifests got `sha256`/counts from `content-lint --update-manifests`.

## Native review — where to look first

All 89 associations are candidates. The generating session flagged these as weakest (forced or partial sound-alikes, context-only images):

- **DE:** ich, uns, seit, gehen, heißen, möchten, können, der Abend, der Bahnhof, das Jahr, heute, wo, schön, zwei; context-only: in das, an dem, aus.
- **EN:** to come, where, what, every, Monday, much, name; context-only: this, the, a, from, do not, weather.
- **Facts to confirm:** *Sie* capitalisation, *an dem* → *am*, *aus* → *Ausgang*, *ein* ↔ *eins*, pronunciation note for *wie*, false-friend warnings (we/my, to, on), the splits under-stand / to-day / to-night / Pol-land (images, not etymology), *e-learning* (lernen, to learn), *mityng* (to meet), *IT* (it), colloquial *Dojczland* (Deutsch), tone of *drze się* (der).
- The KMP grammar tip of "Wie heißt du?" ("questions without a question word at the end…") reads oddly and should be checked with the other inherited tips.

## To close the content rights item (owner)

For each deck:
1. Who wrote fronts, backs and tips, and whether an LLM produced or assisted (model, date, prompt version if known).
2. Confirmation that no third-party dictionary, textbook or deck was copied.
3. Native-speaker review of every record (associations included): reviewer and date → `review` in the manifest.
4. Rights confirmed → `license.rightsConfirmed: true`; attribution text for the Licences screen, if any.
5. Final size (plan: 800–1 200 words and 300–500 sentences per language) — D-04.

Until then the decks may be used for development, demos and tests, but not bundled into a beta or release build.
