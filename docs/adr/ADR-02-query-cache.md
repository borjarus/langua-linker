# ADR-02: Custom query/cache service instead of a query library

- Status: Accepted (2026-10-12) — option chosen: framework-neutral QueryClient + runes binding useQuery
- Date: 2026-10-12
- Plan references: §3 (data and cache), Phase 2 step 13, R19, R20

## Context

Screens need loading/empty/ready/error states, de-duplication of concurrent reads, stale-while-revalidate after mutations, protection against late responses after navigation, and refresh on the `collection_changed` read signal. The plan allows `@tanstack/svelte-query` only after a Svelte 5 compatibility spike and forbids React bindings.

## Decision

Implement a framework-neutral `QueryClient` (`src/services/queryClient.ts`) plus a runes binding `useQuery` (`src/services/query.svelte.ts`):

- Keys are arrays serialized to JSON; prefix invalidation matches whole key parts (`['deck']` does not match `['decks']`).
- In-flight requests per key are shared; errors are never cached.
- `useQuery` exposes `state: UiState<T>` (`loading | empty | ready | error`), `stale`, `fetching`, `refreshError`, `refresh()`. A failed refresh keeps cached data and reports `refreshError`.
- `$effect` in `useQuery` only (re)subscribes when the key changes and invalidates late responses via a generation counter; it never runs mutations.
- The root subscribes `AppApi.onCollectionChanged` → `queries.invalidate()`; mounted queries refetch, unmounted ones refetch on next mount.
- Mutations stay explicit calls on `AppApi` from event handlers (with `operationId`), not part of the query layer.

## Alternatives considered

- `@tanstack/svelte-query` v5/v6: capable, but its Svelte 5 runes API was still settling; adopting it would require a dedicated spike and adds ~13 kB gz. Can replace this service later behind the same `useQuery` shape.
- Plain `$state` per screen: duplicates loading/error/race handling in every screen.

## Consequences

- Tested in `tests/query.svelte.test.ts` (dedupe, no error caching, prefix invalidation, unmount, race on key change).
- No offline persistence of the cache: local data lives in SQLite (Rust); the cache is an in-memory view.
- Revisit if screens need pagination primitives beyond the browser's custom paging (Phase 4, ADR-03).
