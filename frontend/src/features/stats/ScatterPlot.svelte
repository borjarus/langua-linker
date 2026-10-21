<script lang="ts">
  import type { DifficultyStabilityPointDto } from '../../api/types'
  import { useServices } from '../../app/context'
  import { stepIndex } from './charts'

  interface Props {
    points: DifficultyStabilityPointDto[]
    label: string
  }

  let { points, label }: Props = $props()
  const { i18n, theme } = useServices()
  const ids = $props.id()
  const HEIGHT = 220

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

  // Keyboard order: by stability, so ←/→ walk the x axis.
  const sorted = $derived([...points].sort((a, b) => a.stability - b.stability))
  const width = $derived(measured > 0 ? measured : 320)
  const left = $derived(Math.ceil(fontPx * 2.2))
  const bottom = $derived(Math.ceil(fontPx * 1.8))
  const top = 8
  const right = 12
  // Stability spans days to years: a log axis keeps both ends readable.
  const minS = $derived(Math.min(1, ...sorted.map((point) => point.stability)))
  const maxS = $derived(Math.max(10, ...sorted.map((point) => point.stability)))
  const logMin = $derived(Math.log10(Math.max(minS, 0.1)))
  const logMax = $derived(Math.log10(maxS))
  const x = (stability: number) => left + ((Math.log10(Math.max(stability, 0.1)) - logMin) / Math.max(logMax - logMin, 0.001)) * (width - left - right)
  const y = (difficulty: number) => top + ((10 - difficulty) / 9) * (HEIGHT - top - bottom)
  const xTicks = $derived([1, 10, 100, 1000].filter((tick) => tick >= minS && tick <= maxS))
  const current = $derived(active === null ? null : (sorted[active] ?? null))

  const describe = (point: DifficultyStabilityPointDto) =>
    i18n.t('stats.scatter.point', { stability: i18n.formatNumber(point.stability, { maximumFractionDigits: 1 }), difficulty: i18n.formatNumber(point.difficulty, { maximumFractionDigits: 1 }) })

  function onkeydown(event: KeyboardEvent) {
    const next = stepIndex(event.key, active, sorted.length, 10)
    if (next === null) return
    event.preventDefault()
    active = next
    const point = sorted[next]
    if (point) announcement = describe(point)
  }

  /** Nearest point within 24 px: the hit target is bigger than the 8 px dot. */
  function onpointermove(event: PointerEvent) {
    const box = (event.currentTarget as SVGSVGElement).getBoundingClientRect()
    const px = event.clientX - box.left
    const py = event.clientY - box.top
    let best: number | null = null
    let bestDistance = 24
    sorted.forEach((point, index) => {
      const distance = Math.hypot(x(point.stability) - px, y(point.difficulty) - py)
      if (distance <= bestDistance) {
        best = index
        bestDistance = distance
      }
    })
    active = best
  }
</script>

<!-- A labelled, focusable group: arrow keys read values into the live region (the table view has them all). -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
  class="scatter"
  bind:this={element}
  bind:clientWidth={measured}
  role="group"
  tabindex="0"
  aria-label={label}
  aria-describedby="{ids}-hint"
  {onkeydown}
  onfocus={() => {
    focused = true
    active ??= 0
  }}
  onblur={() => {
    focused = false
    active = null
  }}
>
  <svg {width} height={HEIGHT} aria-hidden="true" {onpointermove} onpointerleave={() => !focused && (active = null)}>
    {#each [1, 5, 10] as tick (tick)}
      <line class="grid" x1={left} x2={width - right} y1={y(tick)} y2={y(tick)} />
      <text class="tick" x={left - 6} y={y(tick)} dy="0.32em" text-anchor="end">{tick}</text>
    {/each}
    {#each xTicks as tick (tick)}
      <line class="grid" x1={x(tick)} x2={x(tick)} y1={top} y2={HEIGHT - bottom} />
      <text class="tick" x={x(tick)} y={HEIGHT - 4} text-anchor="middle">{i18n.formatNumber(tick)}</text>
    {/each}
    {#each sorted as point, index (point.cardId)}
      <circle class="dot" class:active={active === index} cx={x(point.stability)} cy={y(point.difficulty)} r={active === index ? 6 : 4} />
    {/each}
  </svg>
  <div class="axes muted">
    <span>{i18n.t('stats.scatter.xAxis')}</span>
    <span>{i18n.t('stats.scatter.yAxis')}</span>
  </div>
  {#if current}
    <div class="tooltip" class:end={x(current.stability) < width / 2}>{describe(current)}</div>
  {/if}
  <p id="{ids}-hint" class="visually-hidden">{i18n.t('stats.chart.keyboardHint')}</p>
  <p class="visually-hidden" aria-live="polite">{announcement}</p>
</div>

<style>
  .scatter {
    position: relative;
    display: grid;
    gap: var(--space-1);
    min-width: 0;
    font-size: var(--text-xs);
    border-radius: var(--radius-sm);
  }

  svg {
    display: block;
    overflow: visible;
    touch-action: pan-y;
  }

  .grid {
    stroke: var(--color-chart-grid);
    stroke-width: 1;
    shape-rendering: crispEdges;
  }

  .tick {
    fill: var(--color-text-muted);
    font-size: var(--text-xs);
  }

  .dot {
    fill: var(--color-chart-1);
    fill-opacity: 0.85;
    stroke: var(--color-surface);
    stroke-width: 2;
  }

  .dot.active {
    fill-opacity: 1;
    stroke: var(--color-text);
  }

  .axes {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    gap: var(--space-2);
  }

  .tooltip {
    position: absolute;
    top: 0;
    left: 0;
    z-index: 2;
    width: max-content;
    max-width: min(14rem, 100%);
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
