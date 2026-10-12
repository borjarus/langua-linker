<script lang="ts">
  import type { Snippet } from 'svelte'

  interface Props {
    itemCount: number
    /** Row height in rem, so rows grow with the text scale. */
    rowHeightRem?: number
    label: string
    overscan?: number
    /** Called when the visible window changes; drives page fetching. */
    onrange?: (start: number, end: number) => void
    row: Snippet<[number]>
  }

  let { itemCount, rowHeightRem = 3.5, label, overscan = 6, onrange, row }: Props = $props()

  let viewport = $state<HTMLElement>()
  let scrollTop = $state(0)
  let viewportHeight = $state(480)
  let rootFontPx = $state(16)

  const rowHeight = $derived(rowHeightRem * rootFontPx)
  const start = $derived(Math.max(0, Math.floor(scrollTop / rowHeight) - overscan))
  const end = $derived(Math.min(itemCount, Math.ceil((scrollTop + viewportHeight) / rowHeight) + overscan))
  const indexes = $derived(Array.from({ length: Math.max(0, end - start) }, (_, offset) => start + offset))

  $effect(() => {
    if (!viewport) return
    const measure = () => {
      rootFontPx = Number.parseFloat(getComputedStyle(document.documentElement).fontSize) || 16
      viewportHeight = viewport?.clientHeight || viewportHeight
    }
    measure()
    if (typeof ResizeObserver === 'undefined') return
    const observer = new ResizeObserver(measure)
    observer.observe(viewport)
    observer.observe(document.documentElement)
    return () => observer.disconnect()
  })

  $effect(() => {
    onrange?.(start, end)
  })
</script>

<!-- The scroll container must be focusable so keyboard users can scroll it (WCAG 2.1.1). -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div
  class="viewport"
  bind:this={viewport}
  role="list"
  aria-label={label}
  tabindex="0"
  onscroll={(event) => (scrollTop = event.currentTarget.scrollTop)}
>
  <div class="spacer" style:height="{itemCount * rowHeight}px">
    <div class="window" style:transform="translateY({start * rowHeight}px)">
      {#each indexes as index (index)}
        <div class="item" role="listitem" aria-setsize={itemCount} aria-posinset={index + 1} style:height="{rowHeight}px">
          {@render row(index)}
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
