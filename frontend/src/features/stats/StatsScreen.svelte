<script lang="ts">
  import type { RatioDto } from '../../api/types'
  import { useServices } from '../../app/context'
  import EmptyState from '../../components/EmptyState.svelte'
  import StatTile from '../../components/StatTile.svelte'
  import { useQuery } from '../../services/query.svelte'
  import QueryView from '../shared/QueryView.svelte'
  import Screen from '../shared/Screen.svelte'

  const { api, i18n, queries } = useServices()
  const stats = useQuery(queries, () => ({
    key: ['stats', null, '1m'],
    fetch: () => api.getStats({ deckId: null, range: '1m' }),
    isEmpty: (data) => data.reviewsByDay.length === 0,
  }))

  /** Never renders 0% for missing data: `null` or an empty denominator means "not enough data". */
  const ratio = (value: RatioDto | null) =>
    value && value.total > 0 ? i18n.formatPercent(value.passed / value.total) : i18n.t('stats.notEnoughData')
</script>

<Screen title={i18n.t('routes.stats')} phase={5}>
  <QueryView query={stats}>
    {#snippet empty()}
      <EmptyState title={i18n.t('stats.notEnoughData')} icon="chart" />
    {/snippet}
    {#snippet ready(data)}
      <dl class="tiles">
        <StatTile label={i18n.t('stats.retentionYoung')} value={ratio(data.trueRetention.young)} tone="info" />
        <StatTile label={i18n.t('stats.retentionMature')} value={ratio(data.trueRetention.mature)} tone="success" />
        <StatTile
          label={i18n.t('stats.reviews', { count: data.reviewsByDay.reduce((sum, day) => sum + day.count, 0) })}
          value={i18n.formatNumber(data.reviewsByDay.reduce((sum, day) => sum + day.count, 0))}
        />
      </dl>
    {/snippet}
  </QueryView>
</Screen>

<style>
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 11rem), 1fr));
    gap: var(--space-3);
    margin: 0;
  }
</style>
