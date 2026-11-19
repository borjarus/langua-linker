# Content and fixture provenance

Every asset that may ship in the app or is used as a test fixture is listed here. Release builds may only bundle assets whose **Release status** is `approved`; `content-lint --release` enforces it through each manifest's `review` and `license.rightsConfirmed` ([ADR-14](../docs/adr/ADR-14-bundled-content.md)).

| Asset | Origin | Declared license | Authorship / generation | Native review | Release status |
|---|---|---|---|---|---|
| `content/tools/prompts/association-candidates-v1.md` | written for Phase 8 | project | project | — | tooling, not bundled |
| `content/tools/prompts/association-extended-v1.md` | written 2026-11-18 from the owner's nine-section template | project | project | — | tooling, not bundled |
| `content/tools/prompts/*-candidates-v2.md` | v1 prompts extended with Spanish (2026-11-09) | project | project | — | tooling, not bundled |
| `tests/fixtures/kmp/content.7z` | KMP commit `c2fa62f` (the original v1 files, byte-identical, moved from `content/decks`; encrypted like the bundled content, ADR-14 §14) | `source: original`, no attribution | not recorded | — | **test/reference only**, never bundled |
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

## Extended associations (2026-11-18)

Every word note's association carries an `extended` Markdown text generated with `association-extended-v1` (claude-opus-5-5). Like the short candidates, all of them need native review. The generating sessions flagged etymology and culture claims to verify first, among them: *trafić* ← *treffen*, *kosztować* ← *kosten*/Latin *constare*, *musieć* ← *müssen*, *sztymować* ← *stimmen*; cognates seit–since, Abend–even, wohnen–wont, Stadt–stead, wirklich–work, schön–sheen, schlecht–slight, Zeit–tide, warten–ward, lieben–love–lubić; *tschüs* ← Low German *adjüs*; plurals *Euros*/*Cents* in casual speech; Berlin from an Old Slavic word for marsh (labelled as a hypothesis); culture details (Zugspitze 2 962 m, Cologne Cathedral 157 m, Bremen cathedral-step sweeping at 30, the bottle deposit amount).

## Chapters and levels (2026-11-19)

The sentence decks list their chapters (`sections` in the manifests: 38 German, 36 English, 35 Spanish, tags `lekcja-01`…). The chapter order and topics follow the tables of contents of the source grammar books the lessons were built from; the titles were reworded for the app (claude-opus-5-5, 2026-11-19) so they name the same grammar topic without reusing the books' wording. Levels: `A1-A2` for both Spanish decks (the Spanish grammar book covers A1–A2), `A1` for the others. All six decks went to `contentVersion` 2026.11.19 so installed decks receive the chapters and levels. Native review should check that each title still matches its chapter's tips.

## To close the content rights item (owner)

For each deck:
1. Who wrote fronts, backs and tips, and whether an LLM produced or assisted (model, date, prompt version if known).
2. Confirmation that no third-party dictionary, textbook or deck was copied.
3. Native-speaker review of every record (associations included): reviewer and date → `review` in the manifest.
4. Rights confirmed → `license.rightsConfirmed: true`; attribution text for the Licences screen, if any.
5. Final size (plan: 800–1 200 words and 300–500 sentences per language) — D-04.

Until then the decks may be used for development, demos and tests, but not bundled into a beta or release build.
