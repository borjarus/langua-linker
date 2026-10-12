# ADR-01: Typed hash router for Svelte 5

- Status: Accepted (2026-10-12) — option chosen: in-house typed hash router
- Date: 2026-10-12
- Plan references: §4.5, Phase 2 step 16, D-09

## Context

The frontend is a static Vite SPA served from `tauri://`/`http://tauri.localhost` in native shells and from a static host in the browser demo. There is no path-resolving server, SvelteKit/SSR is out of scope, and native deep links must be validated before they reach the GUI. The router must support params, navigate/back/replace, an unsaved-changes guard, focus and scroll restoration, and mobile back behaviour.

## Decision

Use a small in-house hash router instead of a third-party library:

- `src/app/routes.ts` — framework-free route table (`ROUTE_PATTERNS`), typed params derived from patterns, `matchPath`, `buildPath`, `href`, and the deep-link adapter `deepLinkToPath`.
- `src/app/router.svelte.ts` — `Router` class with `$state` `current`/`kind`, `navigate`, `replace`, `back`, `addGuard`, `restoreScroll`.
- Programmatic navigation uses `pushState`/`replaceState` with an `llIndex` history state; anchors, hardware/browser back and manual edits arrive via `hashchange` and pass the same guards. A refused browser-driven move is undone with `history.go(delta)`.
- Guards are `{ isBlocking(): boolean; confirmLeave(to): Promise<boolean> }`; `isBlocking` also drives `beforeunload`.
- Path params are restricted to `[A-Za-z0-9_-]{1,128}`; anything else is `notFound`. Deep links accept only `langualinker://app/<route>` for an allow-list of routes and drop query parameters.
- The shell focuses the new screen's `h1[data-screen-heading]` after each non-initial navigation and calls `restoreScroll()` (`pop` restores the saved offset, `push`/`replace` go to top).

## Alternatives considered

- Third-party Svelte hash routers (e.g. `svelte-spa-router`): not spiked. Each would need a Svelte 5 compatibility spike (plan R19) and still require project code for typed params, async guards on browser back, and focus/scroll restoration. The requirement set is small enough to own; a library can replace `router.svelte.ts` later behind the same API.
- SvelteKit with `adapter-static`: excluded by plan §2 (SSR/server endpoints must not become implicit dependencies).
- History (path) routing: needs server rewrites; not available in all WebView asset protocols.

## Consequences

- ~200 lines owned by the project, covered by `tests/routes.test.ts` and `tests/router.svelte.test.ts`.
- Reverting a refused back navigation relies on `history.go`; verified in jsdom, still to be verified on Android back gesture/button (Phase 3 device smoke).
- Native deep-link delivery (plugin/event) is wired in Phase 15; the validation function already exists and is tested.
