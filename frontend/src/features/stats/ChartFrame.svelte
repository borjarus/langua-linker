<script lang="ts">
  import type { Snippet } from 'svelte'
  import { useServices } from '../../app/context'
  import Icon from '../../components/Icon.svelte'

  interface Props {
    title: string
    /** What the chart shows and assumes; always visible, not hidden in a tooltip. */
    description?: string
    /** "Not enough data": the chart and table are replaced by a sentence, never an empty axis or 0%. */
    empty?: boolean
    emptyText?: string
    legend?: Snippet
    chart: Snippet
    table: Snippet
  }

  let { title, description, empty = false, emptyText, legend, chart, table }: Props = $props()
  const { i18n } = useServices()
  const ids = $props.id()
  // Every chart has a tabular alternative with the same values (plan Phase 5, step 3).
  let asTable = $state(false)
</script>

<section class="frame surface" aria-labelledby="{ids}-title">
  <div class="head">
    <h2 id="{ids}-title">{title}</h2>
    {#if !empty}
      <button type="button" class="btn btn-ghost" aria-pressed={asTable} onclick={() => (asTable = !asTable)}>
        <Icon name={asTable ? 'chart' : 'cards'} />{i18n.t(asTable ? 'stats.chart.showChart' : 'stats.chart.showTable')}
      </button>
    {/if}
  </div>
  {#if description}<p class="muted description">{description}</p>{/if}
  {#if empty}
    <p class="empty">{emptyText ?? i18n.t('stats.notEnoughData')}</p>
  {:else if asTable}
    <div class="table">{@render table()}</div>
  {:else}
    {@render chart()}
    {#if legend}{@render legend()}{/if}
  {/if}
</section>

<style>
  .frame {
    display: grid;
    gap: var(--space-3);
    min-width: 0;
  }

  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }

  h2 {
    font-size: var(--text-lg);
  }

  .description {
    font-size: var(--text-sm);
  }

  .empty {
    padding: var(--space-4);
    border-radius: var(--radius-md);
    background: var(--color-surface-sunken);
    color: var(--color-text-muted);
    text-align: center;
  }

  .table :global(table) {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--text-sm);
  }

  .table :global(th),
  .table :global(td) {
    padding: var(--space-1) var(--space-2);
    border-bottom: 1px solid var(--color-border);
    text-align: start;
  }

  .table :global(td.num),
  .table :global(th.num) {
    text-align: end;
    font-variant-numeric: tabular-nums;
  }
</style>
