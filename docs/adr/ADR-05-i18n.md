# ADR-05: i18next core with a runes adapter

- Status: Accepted (2026-10-12) — option chosen: i18next core with a runes adapter
- Date: 2026-10-12
- Plan references: §4.5, Phase 2 steps 17–18, D-06

## Context

The UI ships in Polish and English, needs CLDR plurals (Polish: one/few/many/other), number/date formatting, immediate reaction to language changes in every component (dialogs, toasts, charts), typed keys, and a CI check against literal strings in `.svelte` files. react-i18next is excluded.

## Decision

- `i18next` core (`createInstance()` per root, no global singleton) initialised synchronously before the first render.
- `I18n` class in `src/services/i18n.svelte.ts`: `language` is `$state`; `t()` reads it, so any template or `$derived` calling `t` re-renders on `setLanguage()`. Interpolation escaping is off because Svelte escapes text.
- Resources are TypeScript modules: `src/locales/en.ts` is the key source; `TranslationKey` folds plural suffixes into base keys; `pl.ts` is typed as `LocaleResource` and checked by tests for key parity, CLDR plural categories, placeholder parity and error-code coverage.
- Counts use `{{count, number}}` so large numbers are localised (`50 000` / `50,000`).
- Data-driven keys (e.g. `ErrorDto.messageKey`) go through `tDynamic(key, fallback)`.
- `scripts/check-ui-strings.mjs` parses every `.svelte` file with the Svelte compiler and fails on letter-containing text nodes or static `aria-label`/`title`/`placeholder`/`alt`/`label` values. Exceptions must be listed in the script's `ALLOWED` set.
- UI language is a local preference per profile (`ll:<profile>:language`) until settings persistence arrives in Phase 12.

## Consequences

- Locale files are code, so typos in keys are compile errors; translators edit TS objects (acceptable for two languages; revisit with a TMS).
- RTL: content is rendered with `dir="auto"`; the UI itself is LTR-only (PL/EN).
- Tests: `tests/i18n.test.ts`.
