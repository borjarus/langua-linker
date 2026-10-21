<script lang="ts">
  import { STATS_RANGES, type RatioDto, type StatsDto, type StatsRange } from '../../api/types'
  import { useServices } from '../../app/context'
  import { href } from '../../app/routes'
  import Disclosure from '../../components/Disclosure.svelte'
  import EmptyState from '../../components/EmptyState.svelte'
  import ErrorState from '../../components/ErrorState.svelte'
  import Skeleton from '../../components/Skeleton.svelte'
  import StatTile from '../../components/StatTile.svelte'
  import { useQuery } from '../../services/query.svelte'
  import Screen from '../shared/Screen.svelte'
  import ChartFrame from './ChartFrame.svelte'
  import { aggregateReviews, bucketFor, heatmapCells, type ReviewBucket } from './charts'
  import ColumnChart, { type ColumnPoint } from './ColumnChart.svelte'
  import Heatmap from './Heatmap.svelte'
  import ScatterPlot from './ScatterPlot.svelte'
  import StackedBar from './StackedBar.svelte'

  const { api, i18n, queries } = useServices()
  const ids = $props.id()
  let deckId = $state<string | null>(null)
  let range = $state<StatsRange>('1m')
  let heatWeeks = $state(0)

  const editor = useQuery(queries, () => ({ key: ['editor'], fetch: () => api.getEditorData() }))
  const stats = useQuery(queries, () => ({
    key: ['stats', deckId, range],
    fetch: () => api.getStats({ deckId, range }),
    isEmpty: (data) => Object.values(data.cardStates).every((count) => count === 0),
  }))

  // A new filter keeps the previous charts on screen (dimmed) instead of flashing a skeleton.
  let previous = $state<StatsDto | null>(null)
  $effect(() => {
    if (stats.state.status === 'ready' || stats.state.status === 'empty') previous = stats.state.data
  })
  const shown = $derived(stats.state.status === 'ready' || stats.state.status === 'empty' ? stats.state.data : stats.state.status === 'loading' ? previous : null)
  const refetching = $derived(stats.state.status === 'loading' && previous !== null)

  const decks = $derived(editor.state.status === 'ready' ? editor.state.data.decks : [])
  const depth = (id: string | null): number => {
    const deck = decks.find((item) => item.id === id)
    return deck?.parentId ? depth(deck.parentId) + 1 : 0
  }

  const day = (value: string, options: Intl.DateTimeFormatOptions = { day: 'numeric', month: 'short' }) =>
    i18n.formatDate(`${value}T00:00:00Z`, { ...options, timeZone: 'UTC' })
  const longDay = (value: string) => day(value, { dateStyle: 'medium' })
  const minutes = (ms: number) => Math.round(ms / 60_000)
  const formatMinutes = (value: number) => i18n.t('stats.minutes', { count: Math.round(value) })
  const compact = (value: number) => i18n.formatNumber(value, { notation: 'compact', maximumFractionDigits: 1 })
  const count = (value: number) => i18n.formatNumber(value)

  function bucketLabel(bucket: ReviewBucket): string {
    const kind = bucketFor(range)
    if (kind === 'day') return day(bucket.start)
    if (kind === 'month') return day(bucket.start, { month: 'short', year: 'numeric' })
    return i18n.t('stats.weekOf', { date: day(bucket.start) })
  }

  const buckets = $derived(shown ? aggregateReviews(shown.reviewsByDay, bucketFor(range)) : [])
  const reviewPoints = $derived<ColumnPoint[]>(buckets.map((bucket) => ({ key: bucket.start, label: bucketLabel(bucket), value: bucket.count })))
  const timePoints = $derived<ColumnPoint[]>(buckets.map((bucket) => ({ key: bucket.start, label: bucketLabel(bucket), value: minutes(bucket.activeMs) })))
  const totalReviews = $derived(shown ? shown.reviewsByDay.reduce((sum, item) => sum + item.count, 0) : 0)
  const totalMs = $derived(shown ? shown.reviewsByDay.reduce((sum, item) => sum + item.activeMs, 0) : 0)
  const forecastPoints = $derived<ColumnPoint[]>(
    shown ? shown.forecast.map((item, index) => ({ key: item.day, label: index === 0 ? i18n.t('stats.forecast.today') : day(item.day), value: item.stored, overlay: item.projected })) : [],
  )
  const heatDays = $derived(shown && heatWeeks > 0 ? heatmapCells(shown.reviewsByDay, heatWeeks).filter((cell) => cell.count > 0) : [])

  function stabilityLabel(min: number, max: number | null): string {
    return max === null ? i18n.t('stats.stability.over', { min }) : i18n.t('stats.stability.between', { min, max })
  }

  /** Never 0% for missing data: `null` or an empty denominator says "not enough data". */
  const ratio = (value: RatioDto | null) => (value && value.total > 0 ? i18n.formatPercent(value.passed / value.total) : i18n.t('stats.notEnoughData'))
  const ratioHint = (value: RatioDto | null) =>
    value && value.total > 0 ? i18n.t('stats.retention.sample', { passed: value.passed, count: value.total }) : i18n.t('stats.retention.needMore')
</script>

{#snippet reviewsTable(points: ColumnPoint[], valueHeader: string, format: (value: number) => string)}
  <table>
    <thead><tr><th scope="col">{i18n.t('stats.table.period')}</th><th scope="col" class="num">{valueHeader}</th></tr></thead>
    <tbody>
      {#each points as point (point.key)}
        <tr><th scope="row">{point.label}</th><td class="num">{format(point.value)}</td></tr>
      {/each}
    </tbody>
  </table>
{/snippet}

<Screen title={i18n.t('routes.stats')} notice={i18n.t('stats.prototype')}>
  <div class="filters" role="group" aria-label={i18n.t('stats.filters')}>
    <div class="field">
      <label for="{ids}-deck">{i18n.t('stats.deck')}</label>
      <select id="{ids}-deck" class="input" value={deckId ?? ''} onchange={(event) => (deckId = event.currentTarget.value || null)}>
        <option value="">{i18n.t('stats.allDecks')}</option>
        {#each decks as deck (deck.id)}
          <option value={deck.id}>{'— '.repeat(depth(deck.id))}{deck.name}</option>
        {/each}
      </select>
    </div>
    <fieldset class="field ranges">
      <legend>{i18n.t('stats.range')}</legend>
      <div class="segmented">
        {#each STATS_RANGES as option (option)}
          <label class="segment" class:selected={range === option}>
            <input type="radio" name="{ids}-range" value={option} checked={range === option} onchange={() => (range = option)} />
            {i18n.t(`stats.ranges.${option}`)}
          </label>
        {/each}
      </div>
    </fieldset>
  </div>

  {#if stats.state.status === 'error'}
    <ErrorState error={stats.state.error} onretry={() => void stats.refresh()} />
  {:else if !shown}
    <Skeleton lines={8} />
  {:else if Object.values(shown.cardStates).every((value) => value === 0)}
    <EmptyState title={i18n.t('stats.emptyTitle')} description={i18n.t('stats.emptyBody')} icon="chart">
      {#snippet actions()}
        <a class="btn btn-primary" href={href({ name: 'home' })}>{i18n.t('common.goHome')}</a>
        <a class="btn" href={href({ name: 'editor' })}>{i18n.t('nav.add')}</a>
      {/snippet}
    </EmptyState>
  {:else}
    {@const data = shown}
    <div class="content stack" class:refetching aria-busy={refetching}>
      {#if stats.refreshError}<ErrorState error={stats.refreshError} compact onretry={() => void stats.refresh()} />{/if}
      <dl class="tiles">
        <StatTile
          label={i18n.t('stats.streak')}
          value={i18n.t('stats.days', { count: data.streakDays })}
          hint={i18n.t('stats.longestStreak', { count: data.longestStreakDays })}
          tone="success"
        />
        <StatTile label={i18n.t('stats.reviewsInRange')} value={count(totalReviews)} hint={i18n.t(`stats.rangeHints.${range}`)} />
        <StatTile label={i18n.t('stats.timeInRange')} value={formatMinutes(minutes(totalMs))} hint={i18n.t(`stats.rangeHints.${range}`)} />
        <StatTile label={i18n.t('stats.retentionYoung')} value={ratio(data.trueRetention.young)} hint={ratioHint(data.trueRetention.young)} tone="info" />
        <StatTile label={i18n.t('stats.retentionMature')} value={ratio(data.trueRetention.mature)} hint={ratioHint(data.trueRetention.mature)} tone="info" />
      </dl>
      <Disclosure title={i18n.t('stats.retention.whatTitle')}>
        <p class="explain">{i18n.t('stats.retention.what', { days: data.matureThresholdDays })}</p>
      </Disclosure>

      <ChartFrame
        title={i18n.t('stats.heatmap.title')}
        description={i18n.t('stats.heatmap.description', { count: heatWeeks })}
        empty={totalReviews === 0}
      >
        {#snippet chart()}
          <Heatmap days={data.reviewsByDay} label={i18n.t('stats.heatmap.title')} formatDay={longDay} onweeks={(weeks) => (heatWeeks = weeks)} />
        {/snippet}
        {#snippet table()}
          <p class="muted note">{i18n.t('stats.heatmap.tableNote')}</p>
          <table>
            <thead><tr><th scope="col">{i18n.t('stats.table.day')}</th><th scope="col" class="num">{i18n.t('stats.table.reviews')}</th></tr></thead>
            <tbody>
              {#each heatDays as cell (cell.day)}
                <tr><th scope="row">{longDay(cell.day)}</th><td class="num">{count(cell.count)}</td></tr>
              {/each}
            </tbody>
          </table>
        {/snippet}
      </ChartFrame>

      <ChartFrame title={i18n.t('stats.reviews.title')} description={i18n.t(`stats.buckets.${bucketFor(range)}`)} empty={totalReviews === 0}>
        {#snippet chart()}
          <ColumnChart points={reviewPoints} label={i18n.t('stats.reviews.title')} seriesLabel={i18n.t('stats.table.reviews')} format={count} formatTick={compact} />
        {/snippet}
        {#snippet table()}
          {@render reviewsTable(reviewPoints, i18n.t('stats.table.reviews'), count)}
        {/snippet}
      </ChartFrame>

      <ChartFrame title={i18n.t('stats.time.title')} description={i18n.t('stats.time.description')} empty={totalReviews === 0}>
        {#snippet chart()}
          <ColumnChart points={timePoints} label={i18n.t('stats.time.title')} seriesLabel={i18n.t('stats.table.minutes')} format={formatMinutes} formatTick={compact} />
        {/snippet}
        {#snippet table()}
          {@render reviewsTable(timePoints, i18n.t('stats.table.minutes'), count)}
        {/snippet}
      </ChartFrame>

      <ChartFrame
        title={i18n.t('stats.forecast.title')}
        description={i18n.t('stats.forecast.assumptions', { reviews: data.forecastAssumptions.reviewsPerDay, newPerDay: data.forecastAssumptions.newPerDay })}
        empty={data.forecast.every((item) => item.stored === 0)}
        emptyText={i18n.t('stats.forecast.none')}
      >
        {#snippet chart()}
          {#if data.overdue > 0}<p class="overdue">{i18n.t('stats.forecast.overdue', { count: data.overdue })}</p>{/if}
          <ColumnChart
            points={forecastPoints}
            label={i18n.t('stats.forecast.title')}
            seriesLabel={i18n.t('stats.forecast.stored')}
            overlayLabel={i18n.t('stats.forecast.projected')}
            format={count}
            formatTick={compact}
          />
        {/snippet}
        {#snippet legend()}
          <ul class="legend">
            <li><span class="key key-bar"></span>{i18n.t('stats.forecast.stored')}</li>
            <li><span class="key key-line"></span>{i18n.t('stats.forecast.projected')}</li>
          </ul>
        {/snippet}
        {#snippet table()}
          <table>
            <thead>
              <tr>
                <th scope="col">{i18n.t('stats.table.day')}</th>
                <th scope="col" class="num">{i18n.t('stats.forecast.stored')}</th>
                <th scope="col" class="num">{i18n.t('stats.forecast.projected')}</th>
              </tr>
            </thead>
            <tbody>
              {#each forecastPoints as point (point.key)}
                <tr><th scope="row">{point.label}</th><td class="num">{count(point.value)}</td><td class="num">{count(point.overlay ?? 0)}</td></tr>
              {/each}
            </tbody>
          </table>
        {/snippet}
      </ChartFrame>

      <ChartFrame title={i18n.t('stats.states.title')} description={i18n.t('stats.states.maturity', { young: data.maturity.young, mature: data.maturity.mature, days: data.matureThresholdDays })}>
        {#snippet chart()}
          <StackedBar
            label={i18n.t('stats.states.title')}
            segments={[
              { key: 'New', label: i18n.t('study.states.New'), value: data.cardStates.New, color: 'var(--color-state-new)' },
              { key: 'Learning', label: i18n.t('study.states.Learning'), value: data.cardStates.Learning, color: 'var(--color-state-learning)' },
              { key: 'Review', label: i18n.t('study.states.Review'), value: data.cardStates.Review, color: 'var(--color-state-review)' },
              { key: 'Relearning', label: i18n.t('study.states.Relearning'), value: data.cardStates.Relearning, color: 'var(--color-state-relearning)' },
              { key: 'Suspended', label: i18n.t('card.suspended'), value: data.cardStates.Suspended, color: 'var(--color-border-strong)' },
            ]}
          />
        {/snippet}
        {#snippet table()}
          <table>
            <thead><tr><th scope="col">{i18n.t('stats.table.state')}</th><th scope="col" class="num">{i18n.t('stats.table.cards')}</th></tr></thead>
            <tbody>
              {#each ['New', 'Learning', 'Review', 'Relearning'] as const as state (state)}
                <tr><th scope="row">{i18n.t(`study.states.${state}`)}</th><td class="num">{count(data.cardStates[state])}</td></tr>
              {/each}
              <tr><th scope="row">{i18n.t('card.suspended')}</th><td class="num">{count(data.cardStates.Suspended)}</td></tr>
            </tbody>
          </table>
        {/snippet}
      </ChartFrame>

      <ChartFrame title={i18n.t('stats.stability.title')} description={i18n.t('stats.stability.description')} empty={data.stability.every((bucket) => bucket.count === 0)}>
        {#snippet chart()}
          <ColumnChart
            points={data.stability.map((bucket) => ({ key: String(bucket.minDays), label: stabilityLabel(bucket.minDays, bucket.maxDays), value: bucket.count }))}
            label={i18n.t('stats.stability.title')}
            seriesLabel={i18n.t('stats.table.cards')}
            format={count}
            formatTick={compact}
          />
        {/snippet}
        {#snippet table()}
          {@render reviewsTable(
            data.stability.map((bucket) => ({ key: String(bucket.minDays), label: stabilityLabel(bucket.minDays, bucket.maxDays), value: bucket.count })),
            i18n.t('stats.table.cards'),
            count,
          )}
        {/snippet}
      </ChartFrame>

      <Disclosure title={i18n.t('stats.advanced')}>
        <ChartFrame title={i18n.t('stats.scatter.title')} description={i18n.t('stats.scatter.description')} empty={data.difficultyStability.length === 0}>
          {#snippet chart()}
            <ScatterPlot points={data.difficultyStability} label={i18n.t('stats.scatter.title')} />
          {/snippet}
          {#snippet table()}
            <table>
              <thead>
                <tr>
                  <th scope="col">{i18n.t('stats.table.card')}</th>
                  <th scope="col" class="num">{i18n.t('stats.scatter.stability')}</th>
                  <th scope="col" class="num">{i18n.t('stats.scatter.difficulty')}</th>
                </tr>
              </thead>
              <tbody>
                {#each data.difficultyStability as point (point.cardId)}
                  <tr>
                    <th scope="row"><a href={href({ name: 'card', params: { id: point.cardId } })}>#{point.cardId}</a></th>
                    <td class="num">{i18n.formatNumber(point.stability, { maximumFractionDigits: 1 })}</td>
                    <td class="num">{i18n.formatNumber(point.difficulty, { maximumFractionDigits: 1 })}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          {/snippet}
        </ChartFrame>
      </Disclosure>
      <p class="muted note">{i18n.t('stats.generatedAt', { time: i18n.formatDate(data.generatedAt, { dateStyle: 'medium', timeStyle: 'short' }) })}</p>
    </div>
  {/if}
</Screen>

<style>
  .filters {
    display: flex;
    flex-wrap: wrap;
    align-items: end;
    gap: var(--space-3) var(--space-4);
  }

  .filters > .field {
    flex: 1 1 14rem;
    min-width: 0;
  }

  fieldset {
    margin: 0;
    padding: 0;
    border: 0;
  }

  legend {
    margin-bottom: var(--space-1);
    font-weight: var(--weight-medium);
  }

  .segmented {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
  }

  .segment {
    position: relative;
    display: inline-flex;
    align-items: center;
    min-height: var(--touch-target);
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    font-weight: var(--weight-medium);
    cursor: pointer;
  }

  .segment.selected {
    border-color: var(--color-primary);
    background: var(--color-primary-soft);
  }

  .segment:focus-within {
    outline: 3px solid var(--color-focus);
    outline-offset: 2px;
  }

  /* The native radio covers its label: it takes the clicks and keeps the keyboard behaviour. */
  .segment input {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    margin: 0;
    opacity: 0;
    cursor: pointer;
  }

  .content {
    transition: opacity var(--motion-fast) var(--motion-ease);
  }

  .refetching {
    opacity: 0.6;
  }

  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 10rem), 1fr));
    gap: var(--space-3);
    margin: 0;
  }

  .explain,
  .note {
    font-size: var(--text-sm);
  }

  .overdue {
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
  }

  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2) var(--space-4);
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: var(--text-sm);
  }

  .legend li {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
  }

  .key {
    width: 0.875rem;
  }

  .key-bar {
    height: 0.875rem;
    border-radius: 2px;
    background: var(--color-chart-1);
  }

  .key-line {
    height: 2px;
    background: var(--color-chart-2);
  }
</style>
