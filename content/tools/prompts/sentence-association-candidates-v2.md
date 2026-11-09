# Prompt `sentence-association-candidates-v2`

Versioned prompt for **candidate** mnemonic associations of bundled sentence notes (plan Phase 8
step 6, ADR-14). v2 extends
[`sentence-association-candidates-v1`](sentence-association-candidates-v1.md) with Spanish;
German and English records keep v1. Like [`association-candidates-v2`](association-candidates-v2.md),
its output is never release content by itself: every candidate is reviewed by a native Polish
speaker with knowledge of the target language before the deck's `review.status` may become
`approved` (see [content/PROVENANCE.md](../../PROVENANCE.md)).

Changing the text below requires a new prompt id (`sentence-association-candidates-v3`, …);
records keep the id they were produced with in `associations[].promptVersion`.

## Input

One sentence note per item: target language (`de`, `en` or `es`), the L2 sentence, its Polish
translation and, when present, the grammar tips of the note.

## Instructions given to the model

> You write mnemonic associations for Polish-speaking adult beginners (CEFR A1) learning German,
> English or Spanish from translation drills. For each sentence, write **one** association in
> Polish that helps the learner produce the foreign sentence from its Polish meaning and remember
> its key word or structure.
>
> Rules:
> 1. Polish only, except for the foreign words you build on; quote them exactly as they appear in
>    the sentence.
> 2. One or two short sentences, at most 200 characters. A vivid, concrete scene beats an
>    explanation.
> 3. Anchor the association on the sentence's key element: a word that is hard to recall, a
>    false friend, word order, a case ending, a separable prefix, ser vs estar, a stem change
>    or a fixed phrase. Use a real
>    Polish sound-alike or a well-known loanword when one exists; otherwise a memorable image of
>    the situation.
> 4. Never state grammar, gender or etymology that is not true; do not contradict the grammar
>    tips of the note.
> 5. Safe for every adult audience: no violence, sexual content, drugs, insults, politics,
>    religion, brands, real people, or stereotypes about nations, genders or groups.
> 6. Do not copy text from dictionaries, textbooks or other decks; write original sentences.
>
> Output JSON only: `{ "<guid>": "<association>" }` for every input sentence, in input order.

## Review checklist (native reviewer)

- The association fits the sentence and its Polish meaning; the sound-alike is real.
- No false grammar, gender or etymology claims; consistent with the note's grammar tips.
- Tone is neutral and adult; nothing offensive or brand-related.
- Length ≤ 200 characters (the linter enforces the GUI limit of 500).
