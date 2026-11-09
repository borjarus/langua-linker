# Prompt `association-candidates-v2`

Versioned prompt for **candidate** mnemonic associations of bundled word notes (plan Phase 8
step 6, ADR-14). v2 extends [`association-candidates-v1`](association-candidates-v1.md) with
Spanish; German and English records keep v1. Output of this prompt is never release content by
itself: every candidate is reviewed by a native Polish speaker with knowledge of the target
language before the deck's `review.status` may become `approved` (see
[content/PROVENANCE.md](../../PROVENANCE.md)).

Changing the text below requires a new prompt id (`association-candidates-v3`, …); records keep
the id they were produced with in `associations[].promptVersion`.

## Input

One word note per item: target language (`de`, `en` or `es`), `front` (the L2 word or phrase,
German and Spanish nouns with their article), `back` (Polish meaning, sometimes with a
grammatical hint in parentheses) and tags (part of speech, CEFR level).

## Instructions given to the model

> You write mnemonic associations for Polish-speaking adult beginners (CEFR A1) learning German,
> English or Spanish. For each word, write **one** association in Polish that links how the
> foreign word **sounds or looks** to its **Polish meaning**, so that the learner recalls the
> meaning when seeing the word.
>
> Rules:
> 1. Polish only, except for the foreign word itself and the sound-alike you build on. Write the
>    foreign word exactly as given in `front` (keep the article of German and Spanish nouns).
> 2. One or two short sentences, at most 200 characters. A vivid, concrete image beats an
>    explanation.
> 3. Build on a real Polish sound-alike or a well-known loanword or internationalism when one
>    exists; otherwise use the shape of the word. Never invent a fake etymology, and do not state
>    linguistic facts that are not true.
> 4. Where the article is part of `front`, you may hint at the gender (German der = męski,
>    die = żeński, das = nijaki; Spanish el = męski, la = żeński), but do not contradict it.
> 5. Safe for every adult audience: no violence, sexual content, drugs, insults, politics,
>    religion, brands, real people, or stereotypes about nations, genders or groups.
> 6. Do not copy text from dictionaries, textbooks or other decks; write original sentences.
> 7. If no honest association comes to mind, write a short memorable image that uses the
>    meaning in context instead of a forced sound-alike.
>
> Output JSON only: `{ "<front>": "<association>" }` for every input word, in input order.

## Review checklist (native reviewer)

- The sound-alike is real for a Polish speaker and the meaning is correct.
- No false grammar, gender or etymology claims.
- Tone is neutral and adult; nothing offensive or brand-related.
- Length ≤ 200 characters (the linter enforces the GUI limit of 500).
