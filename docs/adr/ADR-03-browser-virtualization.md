# ADR-03: Card browser paging and virtualization

- Status: Accepted (2026-10-21) — option chosen: `@tanstack/svelte-virtual` behind the existing `VirtualList` API, with contract paging; Android verified on the emulator, iOS check open (see Consequences)
- Date: 2026-10-20
- Plan references: Phase 4 steps 10–14, Phase 11 steps 16–17, ADR-02

## Context

The browser must handle a 50 000-card collection (plan Phase 4 step 13) without loading every DTO or rendering every row, keep selection and keyboard focus usable while rows are recycled, and work in Android System WebView and WKWebView at 200% text. Step 14 asks for a virtualization library to be chosen after a Svelte 5 spike, without React wrappers.

## Spike

Candidates were checked against Svelte 5 runes support, dependency weight, variable/rem-based row heights (text scale), and keyboard focus when the focused row scrolls out of the window:

- `@tanstack/svelte-virtual`: framework-neutral core (`@tanstack/virtual-core`, no dependencies) with a thin Svelte adapter; supports Svelte 5 (peer `^5`), fixed and measured (`measureElement`) row sizes, overscan and `scrollToIndex` with minimal-scroll alignment. Cost: the adapter is store-based, so runes code reads it through `$virtualizer` and pushes options with `setOptions`. Chosen: a maintained engine for range calculation and scroll handling, and a path to variable-height rows without rewriting the component.
- `svelte-virtual-list` and similar community packages: Svelte 3/4 component APIs (slots, `export let`), not maintained for runes mode.
- The in-house `components/VirtualList.svelte` (Phase 2): fixed row height in rem (grows with text scale), overscan, `onrange` callback for page fetching, `aria-setsize`/`aria-posinset`, focusable scroll container. Its public API and markup are kept as the wrapper around TanStack, so the 50k DOM-bound test and screens carry over unchanged.

### Frame profiling (2026-10-21)

`e2e/virtualization.spec.ts` opens the card browser on the `large` demo scenario (50 000 matches) at 200% text and scrolls the list one step per animation frame: a slow fling (200 px/frame, 300 frames) and a fast one (2 000 px/frame, 300 frames, crossing ~40 pages of 100 cards). Frame gaps from `requestAnimationFrame`, three serial runs each (`--workers=1`), Windows desktop, both implementations on the same build:

| Configuration | Fling | TanStack p50 / p95 / max (ms) | In-house p50 / p95 / max (ms) |
|---|---|---|---|
| Chromium | slow | 17 / 17 / 33–50 | 17 / 17 / 33–50 |
| Chromium | fast | 17 / 17 / 33 | 17 / 17 / 33 |
| Chromium, 4× CPU throttling | slow | 17 / 33–50 / 200–367 | 17 / 33–50 / 200–350 |
| Chromium, 4× CPU throttling | fast | 17 / 133–183 / 350–450 | 17 / 133–200 / 267–367 |
| WebKit (Playwright, Windows) | slow | 75–79 / 112–116 / 141–174 | 73–80 / 101–120 / 140–156 |
| WebKit (Playwright, Windows) | fast | 98–105 / 137–138 / 177–238 | 98–104 / 130–141 / 174–181 |

At most 16 rows are rendered at any time. The two engines are indistinguishable, so the library adds no measurable cost. Idle WebKit frames are 15/30 ms (p50/p95), so its slowdown while scrolling comes from rendering and paging rows, not from range calculation. The same applies to the fast fling under throttling, where both engines show long frames when new pages arrive. The spec gates the frame budget in desktop Chromium only (p50 ≤ 20 ms, p95 ≤ 34 ms, max ≤ 150 ms) and attaches the other runs' numbers to the report, because Playwright's WebKit build and CPU throttling do not predict phone behaviour.

## Decision

Use `@tanstack/svelte-virtual` (pinned `3.13.40`) as the engine inside `components/VirtualList.svelte`, keeping the component's props (`itemCount`, `rowHeightRem`, `overscan`, `onrange`, `row` snippet) and markup (`role="list"`/`listitem`, `aria-setsize`/`aria-posinset`, focusable scroll container), so screens do not change. The store-based adapter is read through `$virtualizer` in runes mode; options are pushed with `setOptions` from an effect, and `measure()` resets cached sizes when the root font size (text scale) changes. A maintained library takes over range calculation, scroll observation and scroll-to-index, and leaves room for variable-height rows (`measureElement`) later.

- `scrollToIndex(index)` delegates to the virtualizer with `align: 'auto'` (scrolls the minimum needed to show a row); rows carry `data-index`. The browser moves focus with ↑/↓ by scrolling first and focusing the same control (checkbox or link) in the newly rendered row.
- Paging follows the contract: `BrowserController` requests only the 100-card pages the visible window touches (`onrange`), keeps rows in a sparse `$state.raw` array, and drops responses of an earlier search (generation counter).
- Selection is not tied to rendered rows: either explicit card ids, or "all matches of this filter" minus exclusions, sent as `BulkSelection { kind: 'query', filterVersion, excludedCardIds }`. The backend resolves it against its filter snapshot and rejects a stale snapshot with `PREVIEW_STALE`. Every bulk action is previewed (`previewBulkAction`) before it is applied.
- Rows have a fixed height (4.5 rem) and single-line, ellipsized text, so recycled rows never change height.

## Consequences

- One runtime dependency (`@tanstack/svelte-virtual` + `@tanstack/virtual-core`, no transitive deps). jsdom has no layout, so `tests/setup.ts` reports a 480 px `offsetHeight` for the list's scroll container and maps `Element.scrollTo` onto `scrollTop` plus a `scroll` event. `tests/components.test.ts` (50k list renders a bounded DOM) and `tests/management.test.ts` (large scenario: 50 000 matches, fewer than 60 rendered rows; keyboard navigation; bulk preview/confirm) and `e2e/virtualization.spec.ts` (frame profiling) cover the behaviour.
- Fixed row height means long content is truncated in the list; the full text is on the card details.
- The adapter's store bridge stays until TanStack ships a runes-native adapter; migrating to it is internal to `VirtualList`.
- Fast flings on a slow CPU drop frames while pages arrive (see the profiling table), in both engines. If the device run confirms this, reduce per-row render cost or prefetch the next page in `onrange` rather than changing the engine.
- Android check (2026-10-21, emulator, `android:dev`, `large` scenario, 200% text): momentum scrolling through the 50 000 matches is smooth with `onrange` paging, ticking checkboxes works while scrolling, and TalkBack announces the list and the row positions. No frame drops from the profiling table were noticeable.
- Open: the same check in WKWebView with VoiceOver (iOS simulator or device), and a physical Android device during the Phase 6 device run (G6); physical iPhone once one is available. Variable-height rows would switch the component to `measureElement`.
- Phase 11 replaces the offset cursor of the mock with a keyset cursor and a stable tie-break (`card id`), without changing the screen.
