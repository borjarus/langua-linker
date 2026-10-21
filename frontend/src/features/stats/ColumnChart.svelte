<script lang="ts" module>
  export interface ColumnPoint {
    key: string
    /** Category label, already formatted (date, bucket range). */
    label: string
    value: number
    /** Second measure on the same axis, drawn as a line (e.g. the projected forecast). */
    overlay?: number
  }
</script>

<script lang="ts">
  import { useServices } from '../../app/context'
  import { columnPath, labelIndices, niceMax, stepIndex } from './charts'

  interface Props {
    points: ColumnPoint[]
    /** Accessible name of the chart. */
    label: string
    seriesLabel: string
    overlayLabel?: string
    format: (value: number) => string
    /** Short axis labels (ticks); defaults to `format`. */
    formatTick?: (value: number) => string
  }

  let { points, label, seriesLabel, overlayLabel, format, formatTick = format }: Props = $props()
  const { i18n, theme } = useServices()
  const ids = $props.id()

  const HEIGHT = 176
  let element = $state<HTMLDivElement>()
  let measured = $state(0)
  let fontPx = $state(13)
  let active = $state<number | null>(null)
  let focused = $state(false)
  let announcement = $state('')

  // Axis text follows the user's text size; re-read it when the layout or the scale changes.
  $effect(() => {
    void measured
    void theme.textScale
    if (element) fontPx = Number.parseFloat(getComputedStyle(element).fontSize) || 13
  })

  const width = $derived(measured > 0 ? measured : 320)
  const max = $derived(niceMax(Math.max(0, ...points.map((point) => Math.max(point.value, point.overlay ?? 0)))))
  const ticks = $derived([0, max / 2, max])
  const left = $derived(Math.ceil(Math.max(...ticks.map((tick) => formatTick(tick).length)) * fontPx * 0.62) + 10)
  const right = 8
  const top = $derived(Math.ceil(fontPx * 0.7))
  const bottom = $derived(Math.ceil(fontPx * 1.8))
  const plotWidth = $derived(Math.max(1, width - left - right))
  const baseline = $derived(HEIGHT - bottom)
  const plotHeight = $derived(Math.max(1, baseline - top))
  const slot = $derived(plotWidth / Math.max(points.length, 1))
  const barWidth = $derived(Math.min(24, Math.max(1, slot - 2)))
  const y = (value: number) => baseline - (value / max) * plotHeight
  const centre = (index: number) => left + index * slot + slot / 2
  const labelled = $derived(labelIndices(points.length, plotWidth, Math.max(...points.map((point) => point.label.length), 1) * fontPx * 0.6))
  const overlayPath = $derived(
    overlayLabel === undefined
      ? ''
      : points.map((point, index) => `${index === 0 ? 'M' : 'L'}${centre(index)},${y(point.overlay ?? 0)}`).join(''),
  )
  const current = $derived(active === null ? null : (points[active] ?? null))

  function describe(point: ColumnPoint): string {
    const parts = [`${point.label}: ${seriesLabel} ${format(point.value)}`]
    if (overlayLabel !== undefined && point.overlay !== undefined) parts.push(`${overlayLabel} ${format(point.overlay)}`)
    return parts.join(', ')
  }

  function onpointermove(event: PointerEvent) {
    const box = (event.currentTarget as SVGSVGElement).getBoundingClientRect()
    const x = event.clientX - box.left - left
    active = x < 0 || x > plotWidth ? null : Math.min(points.length - 1, Math.floor(x / slot))
  }

  function onkeydown(event: KeyboardEvent) {
    const next = stepIndex(event.key, active, points.length)
    if (next === null) return
    event.preventDefault()
    active = next
    const point = points[next]
    if (point) announcement = describe(point)
  }
</script>

<!-- A labelled, focusable group: arrow keys read values into the live region (the table view has them all). -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
  class="chart"
  bind:this={element}
  bind:clientWidth={measured}
  role="group"
  tabindex="0"
  aria-label={label}
  aria-describedby="{ids}-hint"
  {onkeydown}
  onfocus={() => {
    focused = true
    active ??= points.length - 1
  }}
  onblur={() => {
    focused = false
    active = null
  }}
>
  <svg width={width} height={HEIGHT} aria-hidden="true" {onpointermove} onpointerleave={() => !focused && (active = null)}>
    {#each ticks as tick (tick)}
      <line class="grid" x1={left} x2={width - right} y1={y(tick)} y2={y(tick)} />
      <text class="tick" x={left - 6} y={y(tick)} dy="0.32em" text-anchor="end">{formatTick(tick)}</text>
    {/each}
    {#if active !== null}
      <rect class="wash" x={left + active * slot} y={top} width={slot} height={plotHeight} />
    {/if}
    {#each points as point, index (point.key)}
      <path class="bar" class:dim={active !== null && active !== index} d={columnPath(centre(index) - barWidth / 2, baseline, barWidth, (point.value / max) * plotHeight)} />
    {/each}
    {#if overlayPath}
      <path class="overlay" d={overlayPath} />
      {#if active !== null && points[active]}
        <circle class="marker" cx={centre(active)} cy={y(points[active]?.overlay ?? 0)} r="4" />
      {/if}
    {/if}
    <line class="axis" x1={left} x2={width - right} y1={baseline} y2={baseline} />
    {#each labelled as index (index)}
      {@const point = points[index]}
      {#if point}
        <text
          class="tick"
          x={index === 0 ? left : index === points.length - 1 ? width - right : centre(index)}
          y={HEIGHT - 4}
          text-anchor={index === 0 ? 'start' : index === points.length - 1 ? 'end' : 'middle'}>{point.label}</text
        >
      {/if}
    {/each}
  </svg>
  {#if current && active !== null}
    <!-- Docked to the opposite half from the active bar, so it never covers it or leaves the chart. -->
    <div class="tooltip" class:end={centre(active) < width / 2}>
      <span class="tip-label">{current.label}</span>
      <span class="tip-row"><span class="key key-bar"></span><strong>{format(current.value)}</strong> {seriesLabel}</span>
      {#if overlayLabel !== undefined && current.overlay !== undefined}
        <span class="tip-row"><span class="key key-line"></span><strong>{format(current.overlay)}</strong> {overlayLabel}</span>
      {/if}
    </div>
  {/if}
  <p id="{ids}-hint" class="visually-hidden">{i18n.t('stats.chart.keyboardHint')}</p>
  <p class="visually-hidden" aria-live="polite">{announcement}</p>
</div>

<style>
  .chart {
    position: relative;
    min-width: 0;
    font-size: var(--text-xs);
    border-radius: var(--radius-sm);
  }

  svg {
    display: block;
    overflow: visible;
    touch-action: pan-y;
  }

  .grid,
  .axis {
    stroke: var(--color-chart-grid);
    stroke-width: 1;
    shape-rendering: crispEdges;
  }

  .tick {
    fill: var(--color-text-muted);
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
  }

  .wash {
    fill: var(--color-surface-sunken);
  }

  .bar {
    fill: var(--color-chart-1);
    transition: opacity var(--motion-fast) var(--motion-ease);
  }

  .bar.dim {
    opacity: 0.55;
  }

  .overlay {
    fill: none;
    stroke: var(--color-chart-2);
    stroke-width: 2;
    stroke-linejoin: round;
    stroke-linecap: round;
  }

  .marker {
    fill: var(--color-chart-2);
    stroke: var(--color-surface);
    stroke-width: 2;
  }

  .tooltip {
    position: absolute;
    top: 0;
    z-index: 2;
    display: grid;
    gap: 0.125rem;
    left: 0;
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

  .tip-label {
    color: var(--color-text-muted);
  }

  .tip-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .key {
    flex: none;
    width: 0.75rem;
  }

  .key-bar {
    height: 0.75rem;
    border-radius: 2px;
    background: var(--color-chart-1);
  }

  .key-line {
    height: 2px;
    background: var(--color-chart-2);
  }
</style>
