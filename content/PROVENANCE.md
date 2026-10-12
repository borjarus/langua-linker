# Content and fixture provenance

Every asset that may ship in the app or is used as a test fixture is listed here. Release builds may only bundle assets whose **Release status** is `approved`.

| Asset | Origin | Declared license | Authorship / generation | Native review | Release status |
|---|---|---|---|---|---|
| `content/decks/de_words_a1.*` (43) | KMP commit `c2fa62f` | `source: original`, no attribution | not recorded (author or LLM-assisted unknown) | not recorded | **blocked** — confirm author, rights, review (D-04/ADR-14) |
| `content/decks/de_sentences_a1.*` (12) | KMP commit `c2fa62f` | `source: original`, no attribution | not recorded | not recorded | **blocked** |
| `content/decks/en_words_a1.*` (46) | KMP commit `c2fa62f` | `source: original`, no attribution | not recorded | not recorded | **blocked** |
| `content/decks/en_sentences_a1.*` (12) | KMP commit `c2fa62f` | `source: original`, no attribution | not recorded | not recorded | **blocked** |
| `tests/fixtures/kmp/apkg/sample_german_a1_sentences.apkg` | KMP commit `c2fa62f` (added in "implementation of phase 3") | none | unknown; 1 note, 0 reviews in the legacy `collection.anki2` | — | **test-only**, never bundled. Note: `collection.anki2` is Anki's "please update" stub; real data is in `collection.anki21b` (zstd) — relevant for Phase 14 |
| `tests/fixtures/kmp/schema/*` | KMP commit `c2fa62f` | project code | project | — | test/reference only |
| `tests/fixtures/fsrs/kmp-fsrs-spec.json` | derived from KMP `FsrsParameters.kt`/`FsrsTest.kt` | project code; FSRS default weights are published by open-spaced-repetition | project | — | test/reference only |
| `frontend/src/demo/fixtures.ts` | written for the Phase 2 demo | project code | project; includes sample mnemonics | — | demo only, excluded from release bundles |

## To close the content rights item (owner)

For each deck record:
1. Who wrote it, and whether an LLM produced or assisted (model, date, prompt version if known).
2. Confirmation that no third-party dictionary, textbook or deck was copied.
3. Native-speaker review: reviewer and date.
4. Attribution text for the in-app Licenses screen, if any.

Until then the decks may be used for development, demos and tests, but not bundled into a beta or release build.
