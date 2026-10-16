# Svelte 5 frontend patterns (Phase 2 baseline)

Status: accepted (2026-10-12) together with ADR-01, ADR-02 and ADR-05.

## Layers

| Layer | Location | Depends on | Rule |
|---|---|---|---|
| Contract | `src/api/types.ts`, `appApi.ts`, `operations.ts`, `errors.ts` | nothing | Plain TS, no Svelte/Tauri imports. |
| Adapters | `src/api/tauriAppApi.ts`, `src/demo/*` | contract | Only `tauriAppApi.ts` imports `@tauri-apps/*` (ESLint `no-restricted-imports`). |
| Services | `src/services/*` | contract | i18n, query cache, theme, toasts, operation ids. |
| App | `src/app/*` | services, adapters | Bootstrap, context, router, shell. Chooses the adapter once. |
| Components | `src/components/*` | services via context | Design system; no data fetching. |
| Features | `src/features/*` | everything above | Screens (`.svelte`) and controllers (`.svelte.ts`). |

## Context and profiles

- `createServices()` builds one isolated graph (`AppServices`) per root: API adapter, `I18n`, `Router`, `QueryClient`, `ThemeController`, `ToastController`, optional `DemoControls`.
- `provideServices()` / `useServices()` use `setContext`/`getContext` with the `APP_SERVICES` symbol; tests pass their own graph through `render(..., { context })`.
- Profile comes from `VITE_APP_PROFILE` (`demo` default, `dev`, `beta`, `release`). Demo → `MockAppApi`; others → Tauri adapter. In `beta`/`release` the demo branch is statically false, so the mock chunk and the gallery are not emitted; `npm run verify:release` enforces it.
- An IPC error never switches adapters; the Tauri adapter rejects with `AppError`.

## Runes

- Components: `<script lang="ts">`, `$props()` with an explicit `Props` interface, callback props (`onrate`, `onclose`), snippets for composition (`actions`, `row`, `ready`), `$props.id()` for ARIA ids.
- `$derived` for anything computable; writable `$derived` for props that the user may locally override (`Disclosure`, `SearchInput`).
- `$effect` only for subscriptions, listeners, timers, focus and read queries. It returns its cleanup. Mutations are never started from `$effect`.
- Async listener registration handles unmount-before-registration (`tauriAppApi.subscribe`, tested).
- Attachments (`{@attach focusTrap(...)}`) for DOM behaviour with teardown.

## Mutations

- Called from explicit event handlers with an `operationId` from `newOperationId()`.
- Retrying the same intent reuses its `operationId`; a different intent gets a new one.
- Pending state disables the trigger (e.g. rating buttons while `saving`).
- Reference implementation: `src/features/study/studyController.svelte.ts` (plan §4.3 state machine plus pause/end/lost), tested in `tests/studyController.svelte.test.ts`, including "reactive reads never repeat a mutation".
- A rating commit waits for the in-flight preview of the shown card; it never sends an empty or foreign `previewToken`.
- Background jobs that a screen starts (association generation) live in a small controller (`associationGenerator.svelte.ts`); the component owns the `onJobEvent` subscription in `$effect` and forwards events. Events are signals: the controller refetches through `AppApi`.
- Leaving a screen with unfinished work uses a router guard plus a dialog (`EditorScreen` draft guard, `StudyScreen` leave guard). Explicit exits the user already chose (edit card, add word, summary) skip the guard.

## Screen state

- Reads use `useQuery` and render through `QueryView` (loading → `Skeleton`, error → `ErrorState` with retry if `retryable`, empty → screen-specific `EmptyState` with a CTA, ready).
- Each route change remounts the screen (`{#key}` in `RouteOutlet`) so subscriptions start clean.

## Platform ports outside `AppApi`

- Text-to-speech is `SpeechService` (`src/services/speech.ts`) in `AppServices`: `available` / `noVoice` / `unsupported`. The demo and the WebView use the Web Speech engine where present; the mock never simulates audio; the native plugin replaces it in Phase 15 (ADR-12).

## Accessibility baseline

- Every screen has one `h1[data-screen-heading]` (focused after navigation); skip link to `main`.
- Visible `:focus-visible` ring, touch targets ≥ `--touch-target` (3rem = 48 CSS px), rem-based sizes for 200% text, `prefers-reduced-motion` honoured by tokens.
- Status is never colour/emoji only: icons are `aria-hidden` and paired with text.
- Dialogs: `role="dialog"`, `aria-modal`, labelled by title, focus trap, Escape, focus restore.
- Study moves focus after user input only: to the answer after reveal, to the next question after a rating. Keyboard shortcuts (Space, 1–4) ignore inputs, dialogs, modifier keys and auto-repeat; Space on a focused button keeps its native meaning.
