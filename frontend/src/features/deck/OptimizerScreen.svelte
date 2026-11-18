<script lang="ts">
  import { toErrorDto } from '../../api/errors'
  import type { ErrorDto, OptimizerDto, OptimizerMetricsDto, OptimizerRunDto } from '../../api/types'
  import { useServices } from '../../app/context'
  import Chip, { type ChipTone } from '../../components/Chip.svelte'
  import Disclosure from '../../components/Disclosure.svelte'
  import ErrorState from '../../components/ErrorState.svelte'
  import Icon from '../../components/Icon.svelte'
  import ProgressPanel from '../../components/ProgressPanel.svelte'
  import { JobTracker } from '../../services/jobTracker.svelte'
  import { newOperationId } from '../../services/operationId'
  import { useQuery } from '../../services/query.svelte'
  import QueryView from '../shared/QueryView.svelte'
  import Screen from '../shared/Screen.svelte'

  interface Props {
    /** Preset id. */
    id: string
  }

  let { id }: Props = $props()
  const { api, i18n, queries, toasts } = useServices()
  const ids = $props.id()
  const optimizer = useQuery(queries, () => ({ key: ['optimizer', id], fetch: () => api.getOptimizer({ presetId: id }) }))
  const job = new JobTracker(api)
  $effect(() => job.listen())

  let acting = $state<string | null>(null)
  let actionError = $state<ErrorDto | null>(null)
  /** Shown after a refresh caused by a preset changed elsewhere. */
  let changedElsewhere = $state(false)
  let attached: string | null = null

  // A run started earlier (another visit, before a restart of the screen) is followed again.
  $effect(() => {
    const state = optimizer.state
    if (state.status !== 'ready') return
    const running = state.data.runningJobId
    if (running && running !== attached && !job.active) {
      attached = running
      void api.getJobStatus(running).then((found) => job.track(found, finished), () => {})
    }
  })

  function finished() {
    void optimizer.refresh()
  }

  function start(data: OptimizerDto) {
    if (job.active) return
    actionError = null
    changedElsewhere = false
    void job.start(() => api.startOptimizer({ operationId: newOperationId(), presetId: data.presetId }), finished)
  }

  async function act(data: OptimizerDto, run: OptimizerRunDto, action: 'apply' | 'revert') {
    acting = run.runId
    actionError = null
    changedElsewhere = false
    const input = { operationId: newOperationId(), runId: run.runId, expectedVersion: data.presetVersion }
    try {
      await (action === 'apply' ? api.applyOptimizerRun(input) : api.revertOptimizerRun(input))
      queries.invalidate(['presets'])
      toasts.show(i18n.t(action === 'apply' ? 'optimizer.applied' : 'optimizer.reverted', { name: data.presetName }), 'success')
      await optimizer.refresh()
    } catch (caught) {
      const error = toErrorDto(caught)
      const stale = error.code === 'VERSION_CONFLICT' || (error.code === 'VALIDATION_FAILED' && (error.details?.reason === 'stale' || error.details?.reason === 'status'))
      if (stale) {
        changedElsewhere = true
        await optimizer.refresh()
      } else {
        actionError = error
      }
    } finally {
      acting = null
    }
  }

  const number = (value: number, digits = 0) => i18n.formatNumber(value, { maximumFractionDigits: digits, minimumFractionDigits: digits })
  const percent = (value: number) => i18n.formatNumber(value, { style: 'percent', maximumFractionDigits: 1 })
  const date = (iso: string) => i18n.formatDate(iso, { dateStyle: 'medium', timeStyle: 'short' })
  const STATUS_TONE: Record<OptimizerRunDto['status'], ChipTone> = { proposed: 'info', applied: 'success', reverted: 'neutral' }
  type MetricRow = [string, (metrics: OptimizerMetricsDto) => string, boolean]
  const metricRows = (): MetricRow[] => [
    [i18n.t('optimizer.metrics.logLoss'), (m) => number(m.logLoss, 4), true],
    [i18n.t('optimizer.metrics.rmse'), (m) => percent(m.rmse), true],
    [i18n.t('optimizer.metrics.dailyReviews'), (m) => number(m.dailyReviews, 1), false],
    [i18n.t('optimizer.metrics.medianInterval'), (m) => i18n.t('optimizer.metrics.days', { count: m.medianIntervalDays }), false],
  ]
  const latest = (data: OptimizerDto) => data.runs[0] ?? null
  const revertable = (data: OptimizerDto) => data.runs.find((run) => run.canRevert) ?? null
</script>

{#snippet metrics(run: OptimizerRunDto)}
  <div class="table-wrap">
    <table>
      <caption class="visually-hidden">{i18n.t('optimizer.metrics.caption')}</caption>
      <thead>
        <tr>
          <th scope="col">{i18n.t('optimizer.metrics.measure')}</th>
          <th scope="col">{i18n.t('optimizer.metrics.current')}</th>
          <th scope="col">{i18n.t('optimizer.metrics.proposed')}</th>
        </tr>
      </thead>
      <tbody>
        {#each metricRows() as [label, format, quality] (label)}
          <tr class:quality>
            <th scope="row">{label}</th>
            <td>{format(run.current)}</td>
            <td>{format(run.proposed)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
  <p class="muted hint">{i18n.t('optimizer.metrics.hint', { training: number(run.trainingReviews), validation: number(run.validationReviews) })}</p>
{/snippet}

{#snippet weights(before: number[], proposed: number[])}
  <Disclosure title={i18n.t('optimizer.weights.title')}>
    <div class="table-wrap">
      <table class="weights">
        <thead>
          <tr>
            <th scope="col">{i18n.t('optimizer.weights.index')}</th>
            <th scope="col">{i18n.t('optimizer.weights.before')}</th>
            <th scope="col">{i18n.t('optimizer.weights.proposed')}</th>
          </tr>
        </thead>
        <tbody>
          {#each before as value, index (index)}
            <tr class:changed={value !== proposed[index]}>
              <th scope="row">{`w${index}`}</th>
              <td>{number(value, 4)}</td>
              <td>{number(proposed[index] ?? 0, 4)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  </Disclosure>
{/snippet}

<Screen title={i18n.t('routes.optimizer')} back>
  <QueryView query={optimizer} skeletonLines={8}>
    {#snippet ready(data)}
      {@const dataset = data.dataset}
      {@const enough = dataset.usable >= data.minimumReviews}
      {@const run = latest(data)}
      {@const previous = revertable(data)}
      <section class="surface stack" aria-labelledby="{ids}-data">
        <h2 id="{ids}-data">{data.presetName}</h2>
        <p class="muted">{i18n.t('optimizer.intro')}</p>
        <dl class="facts">
          <div><dt>{i18n.t('optimizer.dataset.usable')}</dt><dd>{number(dataset.usable)}</dd></div>
          <div><dt>{i18n.t('optimizer.dataset.cards')}</dt><dd>{number(dataset.cards)}</dd></div>
          <div><dt>{i18n.t('optimizer.dataset.reviews')}</dt><dd>{number(dataset.reviews)}</dd></div>
        </dl>
        {#if dataset.excludedUndone + dataset.excludedManual + dataset.excludedIncomplete > 0}
          <p class="muted hint">
            {i18n.t('optimizer.dataset.excluded', { undone: number(dataset.excludedUndone), manual: number(dataset.excludedManual), incomplete: number(dataset.excludedIncomplete) })}
          </p>
        {/if}
        <p class="hint"><Icon name="info" /><span>{i18n.t('optimizer.privacy')}</span></p>
        {#if !enough}
          <p id="{ids}-not-enough" class="warning" role="status">{i18n.t('optimizer.notEnough', { minimum: number(data.minimumReviews), usable: number(dataset.usable) })}</p>
        {:else if dataset.usable < data.recommendedReviews}
          <p class="warning">{i18n.t('optimizer.fewReviews', { recommended: number(data.recommendedReviews) })}</p>
        {/if}
        <button
          type="button"
          class="btn btn-primary"
          disabled={!enough || job.active}
          aria-busy={job.active}
          aria-describedby={enough ? undefined : `${ids}-not-enough`}
          onclick={() => start(data)}
        >
          <Icon name="refresh" />{i18n.t(run ? 'optimizer.startAgain' : 'optimizer.start')}
        </button>
        {#if job.job && job.job.status !== 'succeeded'}
          <ProgressPanel title={i18n.t('optimizer.progress')} status={job.job.status} progress={job.job.progress} oncancel={() => job.cancel()} />
          {#if job.job.status === 'failed' && job.job.error}<ErrorState error={job.job.error} compact />{/if}
          {#if job.job.status === 'cancelled'}<p class="muted" role="status">{i18n.t('optimizer.cancelled')}</p>{/if}
        {/if}
        {#if job.error}<ErrorState error={job.error} compact />{/if}
      </section>

      {#if changedElsewhere}
        <p class="warning" role="alert">{i18n.t('optimizer.changedElsewhere')}</p>
      {/if}
      {#if actionError}<ErrorState error={actionError} compact />{/if}

      {#if run}
        <section class="surface stack" aria-labelledby="{ids}-result">
          <div class="row heading">
            <h2 id="{ids}-result">{i18n.t('optimizer.result.title')}</h2>
            <Chip label={i18n.t(`optimizer.status.${run.status}`)} tone={STATUS_TONE[run.status]} />
          </div>
          <p class="muted">{date(run.createdAt)}</p>
          <p class="recommendation" class:good={run.recommended} role="status">
            <Icon name={run.recommended ? 'check' : 'info'} />
            <span>{i18n.t(run.recommended ? 'optimizer.result.recommended' : 'optimizer.result.keep')}</span>
          </p>
          {#if run.fewReviews}<p class="muted hint">{i18n.t('optimizer.result.fewReviews')}</p>{/if}
          {@render metrics(run)}
          {@render weights(run.weightsBefore, run.weightsProposed)}
          <p class="hint"><Icon name="info" /><span>{i18n.t('optimizer.noReschedule')}</span></p>
          {#if run.canApply}
            <button type="button" class="btn" class:btn-primary={run.recommended} disabled={acting !== null || job.active} aria-busy={acting === run.runId} onclick={() => act(data, run, 'apply')}>
              <Icon name="check" />{i18n.t('optimizer.apply')}
            </button>
          {:else if run.status === 'proposed'}
            <p class="muted hint">{i18n.t('optimizer.result.stale')}</p>
          {/if}
        </section>
      {/if}

      {#if previous}
        <section class="surface stack" aria-labelledby="{ids}-revert">
          <h2 id="{ids}-revert">{i18n.t('optimizer.revert.title')}</h2>
          <p class="muted">{i18n.t('optimizer.revert.body', { date: date(previous.appliedAt ?? previous.createdAt) })}</p>
          <button type="button" class="btn" disabled={acting !== null || job.active} aria-busy={acting === previous.runId} onclick={() => act(data, previous, 'revert')}>
            <Icon name="back" />{i18n.t('optimizer.revert.action')}
          </button>
        </section>
      {/if}

      {#if data.runs.length > 1}
        <section class="surface stack" aria-labelledby="{ids}-history">
          <h2 id="{ids}-history">{i18n.t('optimizer.history.title')}</h2>
          <ul class="history">
            {#each data.runs.slice(1) as item (item.runId)}
              <li>
                <span>{date(item.createdAt)}</span>
                <Chip label={i18n.t(`optimizer.status.${item.status}`)} tone={STATUS_TONE[item.status]} />
                <span class="muted">
                  {i18n.t('optimizer.history.metrics', { current: number(item.current.logLoss, 4), proposed: number(item.proposed.logLoss, 4), usable: number(item.dataset.usable) })}
                </span>
              </li>
            {/each}
          </ul>
        </section>
      {/if}
    {/snippet}
  </QueryView>
</Screen>

<style>
  h2 {
    font-size: var(--text-lg);
  }

  .heading {
    justify-content: space-between;
    flex-wrap: wrap;
  }

  .facts {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 9rem), 1fr));
    gap: var(--space-3);
    margin: 0;
  }

  .facts dd {
    margin: 0;
    font-size: var(--text-lg);
    font-weight: 600;
  }

  .hint {
    display: flex;
    gap: var(--space-2);
    align-items: flex-start;
    font-size: var(--text-sm);
  }

  .warning {
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--color-warning);
    border-radius: var(--radius-sm);
    background: var(--color-warning-soft);
  }

  .recommendation {
    display: flex;
    gap: var(--space-2);
    align-items: flex-start;
    font-weight: 600;
  }

  .recommendation.good {
    color: var(--color-success, inherit);
  }

  .table-wrap {
    overflow-x: auto;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-variant-numeric: tabular-nums;
  }

  /* The global `overflow-wrap: anywhere` would let the columns shrink to one character and break
     "Current" or 0.3229 mid-word at phone width; labels wrap at spaces only, values never (the
     wrapper scrolls if the table still does not fit). */
  th,
  td {
    padding: var(--space-1) var(--space-2);
    text-align: end;
    border-bottom: 1px solid var(--color-border);
    overflow-wrap: normal;
  }

  td,
  thead th:not(:first-child) {
    white-space: nowrap;
  }

  th[scope='row'],
  thead th:first-child {
    text-align: start;
  }

  tr.changed td:last-child {
    font-weight: 600;
  }

  .history {
    display: grid;
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .history li {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    align-items: center;
  }
</style>
