<script lang="ts" module>
  export interface StackedSegment {
    key: string
    label: string
    value: number
    /** CSS colour token for the mark only; text keeps text colours. */
    color: string
  }
</script>

<script lang="ts">
  import { useServices } from '../../app/context'

  interface Props {
    segments: StackedSegment[]
    label: string
  }

  let { segments, label }: Props = $props()
  const { i18n } = useServices()
  const total = $derived(segments.reduce((sum, segment) => sum + segment.value, 0))
  const share = (value: number) => (total > 0 ? value / total : 0)
</script>

<!-- The bar is a picture of the legend below it, which carries every value as text. -->
<div class="stacked">
  <div class="bar" role="img" aria-label={label}>
    {#each segments.filter((segment) => segment.value > 0) as segment (segment.key)}
      <span class="segment" style:flex-grow={segment.value} style:background={segment.color}></span>
    {/each}
  </div>
  <ul class="legend">
    {#each segments as segment (segment.key)}
      <li>
        <span class="swatch" style:background={segment.color}></span>
        <span class="name">{segment.label}</span>
        <strong>{i18n.formatNumber(segment.value)}</strong>
        <span class="muted">{i18n.formatPercent(share(segment.value))}</span>
      </li>
    {/each}
  </ul>
</div>

<style>
  .stacked {
    display: grid;
    gap: var(--space-3);
  }

  .bar {
    display: flex;
    gap: 2px;
    height: 1.25rem;
    overflow: hidden;
    border-radius: 4px;
    background: var(--color-surface-sunken);
  }

  .segment {
    flex-basis: 0;
    min-width: 2px;
  }

  .legend {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 12rem), 1fr));
    gap: var(--space-2) var(--space-4);
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: var(--text-sm);
  }

  li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }

  .swatch {
    flex: none;
    width: 0.75rem;
    height: 0.75rem;
    border-radius: 2px;
  }

  .name {
    flex: 1;
  }

  strong {
    font-variant-numeric: tabular-nums;
  }
</style>
