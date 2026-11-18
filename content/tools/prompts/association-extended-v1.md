# Prompt: extended word association (v1)

`promptVersion`: `association-extended-v1` (2026-11-18). Produces the optional `extended` text of a word note's
association (`associations[].extended`, format v2, Markdown). The short `text` stays as it is; the
extended version is shown below it, collapsed by default.

## Input

One word note: `front` (L2 word, with article for nouns), `back` (Polish meaning), `tags` (part of
speech, level), the current short association, and the target language (`de`, `en` or `es`).

## Output

Polish Markdown, 3 500–8 000 characters, NFC, no HTML, no tables, no images, no links. Allowed:
`##` headings, paragraphs, `**bold**`, `*italic*`, `-` lists, `>` quotes, `---` rules. Exactly these
nine sections, in this order, separated by `---`:

1. `## 1. Rozkład dźwiękowy (TSZ – Technika Słów Zastępczych)` — pronunciation in IPA, split into
   sound parts, what each part sounds like in Polish (or a known L2/English word), then
   **Najlepsze skojarzenie dźwiękowe** and **Najlepsze podejście**.
2. `## 2. Główna historyjka (obraz + akcja + emocje) – najlepsze skojarzenie` — a vivid scene
   (**Wyobraź sobie:**), with **Akcja:**, **Emocja:** and **Skojarzenie:** lines. The short
   association may be reused and expanded.
3. `## 3. Szybsze skojarzenie (dźwiękowe – najlepsze dla początkujących)` — a short sound-alike
   story and a `>` **Łatwiejsze:** one-liner.
4. `## 4. Alternatywne skojarzenie (…)` — a second route: for DE and ES via English, for EN via
   German or a Polish loanword; name the route in the heading.
5. `## 5. Łańcuszek skojarzeń (LMS) – wersja ostateczna` — one or two `>` chains with arrows.
6. `## 6. Zdanie do utrwalenia` — two short A1/A2 sentences in the target language, each as a
   `>` quote with the Polish translation in brackets and one line of imagery.
7. `## 7. Rodzajnik – **der/die/das X** (rodzaj)` for German nouns, `## 7. Rodzaj – **el/la X**`
   for Spanish nouns, otherwise `## 7. Forma i gramatyka` (conjugation, comparison, plural, word
   order, typical collocations — whatever the learner needs at A1). Gender memory images where a
   gender exists.
8. `## 8. Indeks kulturowy (…)` — one small scene anchored in the target culture (DE:
   Oktoberfest, Bavaria, Berlin…; EN: London, a pub, the Underground…; ES: tapas bar, fiesta,
   Madrid…) where the word is said aloud.
9. `## 9. Pokrewne słowa` — 3–6 related words (same family, compounds, opposites) with Polish
   meanings, then **Zapamiętaj:** with one closing sentence.

## Rules

- Facts must be true: IPA, article, gender, plural, conjugation, translations, etymology. A
  sound-alike is an image, never presented as etymology. Do not invent words.
- Polish typography: „cudzysłów”, en dash `–` with spaces.
- The target word is always in **bold** when it is the subject of a line.
- No stereotypes or anything unsuitable for a general audience.
