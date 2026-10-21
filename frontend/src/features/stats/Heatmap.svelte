<script lang="ts">
  import type { ReviewDayDto } from '../../api/types'
  import { useServices } from '../../app/context'
  import { heatmapCells, stepIndex, type HeatCell } from './charts'

  interface Props {
    days: ReviewDayDto[]
    label: string
    formatDay: (day: string) => string
    /** Reports how many weeks fit, so the caption and the table describe exactly what is drawn. */
    onweeks?: (weeks: number) => void
  }

  let { days, label, formatDay, onweeks }: Props = $props()
  const { i18n, theme } = useServices()
  const ids = $props.id()
  const GAP = 2
  const LEVEL_OPACITY = [0, 0.25, 0.5, 0.75, 1] as const
  const WEEK_KEYS: Record<string, string> = { ArrowUp: 'ArrowLeft', ArrowDown: 'ArrowRight', ArrowLeft: 'PageUp', ArrowRight: 'PageDown' }

  let element = $state<HTMLDivElement>()
  let measured = $state(0)
  let fontPx = $state(13)
  let active = $state<number | null>(null)
  let focused = $state(false)
  let announcement = $state('')

  $effect(() => {
    void measured
    void theme.textScale
    if (element) fontPx = Number.parseFloat(getComputedStyle(element).fontSize) || 13
  })

  const width = $derived(measured > 0 ? measured : 320)
  // Cells stay at least 12 px (and grow with text) — fewer weeks on a phone instead of specks.
  const cell = $derived(Math.max(12, Math.round(fontPx * 0.95)))
  const labelWidth = $derived(Math.ceil(fontPx * 2.4))
  const weeks = $derived(Math.max(1, Math.min(53, Math.floor((width - labelWidth) / (cell + GAP)))))
  const cells = $derived(heatmapCells(days, weeks))
  const height = $derived(7 * (cell + GAP))
  const current = $derived(active === null ? null : (cells[active] ?? null))
  // Monday, Wednesday, Friday of the week of 2024-01-01 (a Monday), named in the UI language.
  const weekdayNames = $derived([0, 2, 4].map((offset) => ({ row: offset, name: i18n.formatDate(`2024-01-0${1 + offset}T12:00:00Z`, { weekday: 'short', timeZone: 'UTC' }) })))

  $effect(() => onweeks?.(weeks))

  const describe = (item: HeatCell) => `${formatDay(item.day)}: ${i18n.t('stats.reviewsCount', { count: item.count })}`

  function onkeydown(event: KeyboardEvent) {
    // Cells are in day order and rows are weekdays: ↑/↓ move a day, ←/→ a week.
    const next = stepIndex(WEEK_KEYS[event.key] ?? event.key, active, cells.length, 7)
    if (next === null) return
    event.preventDefault()
    active = next
    const item = cells[next]
    if (item) announcement = describe(item)
  }

  function onpointermove(event: PointerEvent) {
    const box = (event.currentTarget as SVGSVGElement).getBoundingClientRect()
    const week = Math.floor((event.clientX - box.left - labelWidth) / (cell + GAP))
    const weekday = Math.floor((event.clientY - box.top) / (cell + GAP))
    const index = cells.findIndex((item) => item.week === week && item.weekday === weekday)
    active = index < 0 ? null : index
  }
</script>

<!-- A labelled, focusable group: arrow keys read values into the live region (the table view has them all). -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
  class="heatmap"
  bind:this={element}
  bind:clientWidth={measured}
  role="group"
  tabindex="0"
  aria-label={label}
  aria-describedby="{ids}-hint"
  {onkeydown}
  onfocus={() => {
    focused = true
    active ??= cells.length - 1
  }}
  onblur={() => {
    focused = false
    active = null
  }}
>
  <svg width={labelWidth + weeks * (cell + GAP)} {height} aria-hidden="true" {onpointermove} onpointerleave={() => !focused && (active = null)}>
    {#each weekdayNames as weekday (weekday.row)}
      <text class="tick" x="0" y={weekday.row * (cell + GAP) + cell / 2} dy="0.32em">{weekday.name}</text>
    {/each}
    {#each cells as item, index (item.day)}
      <rect
        class="cell"
        class:empty={item.level === 0}
        class:active={active === index}
        x={labelWidth + item.week * (cell + GAP)}
        y={item.weekday * (cell + GAP)}
        width={cell}
        height={cell}
        rx="2"
        fill-opacity={item.level === 0 ? 1 : LEVEL_OPACITY[item.level]}
      />
    {/each}
  </svg>
  {#if current && active !== null}
    <div class="tooltip" class:end={current.week < weeks / 2}>
      <span class="muted">{formatDay(current.day)}</span>
      <strong>{i18n.t('stats.reviewsCount', { count: current.count })}</strong>
    </div>
  {/if}
  <div class="legend" aria-hidden="true">
    <span>{i18n.t('stats.heatmap.less')}</span>
    {#each LEVEL_OPACITY as opacity, level (level)}
      <span class="swatch" class:empty={level === 0} style:opacity={level === 0 ? 1 : opacity}></span>
    {/each}
    <span>{i18n.t('stats.heatmap.more')}</span>
  </div>
  <p id="{ids}-hint" class="visually-hidden">{i18n.t('stats.heatmap.keyboardHint')}</p>
  <p class="visually-hidden" aria-live="polite">{announcement}</p>
</div>

<style>
  .heatmap {
    position: relative;
    display: grid;
    gap: var(--space-2);
    min-width: 0;
    font-size: var(--text-xs);
    border-radius: var(--radius-sm);
  }

  svg {
    display: block;
    touch-action: pan-y;
  }

  .tick {
    fill: var(--color-text-muted);
    font-size: var(--text-xs);
  }

  .cell {
    fill: var(--color-chart-1);
  }

  .cell.empty {
    fill: var(--color-surface-sunken);
  }

  .cell.active {
    stroke: var(--color-text);
    stroke-width: 2;
  }

  .legend {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1);
    color: var(--color-text-muted);
  }

  .swatch {
    width: 0.75rem;
    height: 0.75rem;
    border-radius: 2px;
    background: var(--color-chart-1);
  }

  .swatch.empty {
    background: var(--color-surface-sunken);
    border: 1px solid var(--color-border);
  }

  .tooltip {
    position: absolute;
    top: 0;
    left: 0;
    z-index: 2;
    display: grid;
    width: max-content;
    max-width: min(12rem, 100%);
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    box-shadow: var(--shadow-2);
    pointer-events: none;
  }

  .tooltip.end {
    right: 0;
    left: auto;
  }
</style>
