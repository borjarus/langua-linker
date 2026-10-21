<script lang="ts">
  import { createVirtualizer } from '@tanstack/svelte-virtual'
  import type { Snippet } from 'svelte'
  import { untrack } from 'svelte'

  interface Props {
    itemCount: number
    /** Row height in rem, so rows grow with the text scale. */
    rowHeightRem?: number
    label: string
    overscan?: number
    /** Called when the rendered window changes; drives page fetching. */
    onrange?: (start: number, end: number) => void
    row: Snippet<[number]>
  }

  let { itemCount, rowHeightRem = 3.5, label, overscan = 6, onrange, row }: Props = $props()

  let viewport = $state<HTMLElement>()
  let rootFontPx = $state(16)

  const rowHeight = $derived(rowHeightRem * rootFontPx)

  // Options are pushed in the effect below; the store only needs a valid initial shape.
  const virtualizer = createVirtualizer<HTMLElement, HTMLElement>({
    count: 0,
    getScrollElement: () => viewport ?? null,
    estimateSize: () => rowHeight,
  })

  const items = $derived($virtualizer.getVirtualItems())
  const offset = $derived(items[0]?.start ?? 0)

  $effect(() => {
    // Read every input here so the effect re-runs when the viewport mounts or the text scale changes.
    const element = viewport ?? null
    const size = rowHeight
    const options = { count: itemCount, overscan, getScrollElement: () => element, estimateSize: () => size }
    untrack(() => {
      $virtualizer.setOptions(options)
      // Rows have a fixed height; a new root font size invalidates every cached size.
      $virtualizer.measure()
    })
  })

  $effect(() => {
    const measure = () => {
      rootFontPx = Number.parseFloat(getComputedStyle(document.documentElement).fontSize) || 16
    }
    measure()
    if (typeof ResizeObserver === 'undefined') return
    const observer = new ResizeObserver(measure)
    observer.observe(document.documentElement)
    return () => observer.disconnect()
  })

  $effect(() => {
    const first = items[0]
    const last = items.at(-1)
    if (first && last) untrack(() => onrange?.(first.index, last.index + 1))
  })

  /** Scrolls the minimum needed to show row `index` (keyboard navigation). */
  export function scrollToIndex(index: number): void {
    $virtualizer.scrollToIndex(index, { align: 'auto' })
  }
</script>

<!-- The scroll container must be focusable so keyboard users can scroll it (WCAG 2.1.1). -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div class="viewport" bind:this={viewport} role="list" aria-label={label} tabindex="0">
  <div class="spacer" style:height="{$virtualizer.getTotalSize()}px">
    <div class="window" style:transform="translateY({offset}px)">
      {#each items as item (item.key)}
        <div class="item" role="listitem" aria-setsize={itemCount} aria-posinset={item.index + 1} data-index={item.index} style:height="{item.size}px">
          {@render row(item.index)}
        </div>
      {/each}
    </div>
  </div>
</div>

<style>
  .viewport {
    position: relative;
    height: min(60dvh, 32rem);
    overflow-y: auto;
    overscroll-behavior: contain;
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface);
  }

  .spacer {
    position: relative;
  }

  .window {
    position: absolute;
    inset-inline: 0;
    top: 0;
  }

  .item {
    display: flex;
    align-items: center;
    overflow: hidden;
    border-bottom: 1px solid var(--color-border);
  }
</style>
