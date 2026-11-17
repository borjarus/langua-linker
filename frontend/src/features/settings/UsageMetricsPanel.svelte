<script lang="ts">
  import { toErrorDto } from '../../api/errors'
  import type { MetricsBridge, TargetCheck, UsageMetrics } from '../../api/metrics'
  import type { ErrorDto } from '../../api/types'
  import { useServices } from '../../app/context'
  import ErrorState from '../../components/ErrorState.svelte'
  import Icon from '../../components/Icon.svelte'

  interface Props {
    metrics: MetricsBridge
  }

  let { metrics }: Props = $props()
  const { i18n, toasts } = useServices()
  const ids = $props.id()

  let data = $state<UsageMetrics | null>(null)
  let error = $state<ErrorDto | null>(null)
  let loading = $state(false)

  async function load() {
    loading = true
    error = null
    try {
      data = await metrics.getUsageMetrics()
    } catch (caught) {
      error = toErrorDto(caught)
    } finally {
      loading = false
    }
  }

  void load()

  function value(check: TargetCheck): string {
    if (check.value === null) return '—'
    if (check.metric === 'coldStartMedian' || check.metric === 'generationP90') return i18n.t('privacy.metrics.ms', { value: i18n.formatNumber(check.value) })
    if (check.metric === 'retainedD7') return check.value >= 1 ? i18n.t('privacy.metrics.yes') : i18n.t('privacy.metrics.no')
    return i18n.formatNumber(check.value, { style: 'percent', maximumFractionDigits: 1 })
  }

  /** The learner decides where the summary goes; this version never sends it (D-08). */
  async function copy(value: UsageMetrics) {
    try {
      await navigator.clipboard.writeText(JSON.stringify(value, null, 2))
      toasts.show(i18n.t('privacy.metrics.copied'), 'success')
    } catch {
      toasts.show(i18n.t('privacy.metrics.copyFailed'), 'error')
    }
  }
</script>

<div class="stack metrics" aria-busy={loading}>
  <h3 id="{ids}-title">{i18n.t('privacy.metrics.title')}</h3>
  {#if error}<ErrorState {error} compact onretry={load} />{/if}
  {#if data?.summary}
    {@const summary = data.summary}
    <p class="muted hint">
      {i18n.t('privacy.metrics.since', { date: i18n.formatDate(summary.since), days: summary.daysObserved })}
      {i18n.t('privacy.metrics.pseudonymous', { id: data.installId?.slice(0, 8) ?? '—' })}
    </p>
    <table aria-labelledby="{ids}-title">
      <thead>
        <tr>
          <th scope="col">{i18n.t('privacy.metrics.metric')}</th>
          <th scope="col">{i18n.t('privacy.metrics.measured')}</th>
          <th scope="col">{i18n.t('privacy.metrics.target')}</th>
          <th scope="col">{i18n.t('privacy.metrics.status')}</th>
        </tr>
      </thead>
      <tbody>
        {#each summary.targets as check (check.metric)}
          <tr>
            <th scope="row">{i18n.t(`privacy.metrics.names.${check.metric}`)}</th>
            <td>{value(check)}</td>
            <td>{check.target}</td>
            <td class="status-{check.status}">{i18n.t(`privacy.metrics.statuses.${check.status}`)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
    <p class="muted hint">
      {i18n.t('privacy.metrics.counts', {
        starts: summary.appStarts,
        sessions: summary.sessions,
        ratings: summary.ratings.again + summary.ratings.hard + summary.ratings.good + summary.ratings.easy,
        undos: summary.undos,
        generations: summary.generations,
        crashes: summary.nativeCrashes,
        errors: summary.clientErrors,
      })}
    </p>
    <div class="row">
      <button type="button" class="btn" onclick={() => data && copy(data)}><Icon name="copy" />{i18n.t('privacy.metrics.copy')}</button>
      <button type="button" class="btn btn-ghost" disabled={loading} onclick={load}><Icon name="refresh" />{i18n.t('common.refresh')}</button>
    </div>
  {:else if data && !data.enabled}
    <p class="muted hint">{i18n.t('privacy.metrics.empty')}</p>
  {/if}
</div>

<style>
  h3 {
    font-size: var(--text-md);
  }

  .hint {
    font-size: var(--text-sm);
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--text-sm);
  }

  th,
  td {
    padding: var(--space-1) var(--space-2);
    border-bottom: 1px solid var(--color-border);
    text-align: start;
    vertical-align: top;
  }

  thead th {
    color: var(--color-text-muted);
    font-weight: var(--weight-medium);
  }

  .status-met {
    color: var(--color-success);
  }

  .status-missed {
    color: var(--color-error);
  }
</style>
