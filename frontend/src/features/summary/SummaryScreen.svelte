<script lang="ts">
  import { RATINGS } from '../../api/types'
  import { useServices } from '../../app/context'
  import StatTile from '../../components/StatTile.svelte'
  import { useQuery } from '../../services/query.svelte'
  import QueryView from '../shared/QueryView.svelte'
  import Screen from '../shared/Screen.svelte'

  interface Props {
    sessionId: string
  }

  let { sessionId }: Props = $props()
  const { api, i18n, queries } = useServices()
  const summary = useQuery(queries, () => ({
    key: ['summary', sessionId],
    fetch: () => api.getSessionSummary(sessionId),
    isEmpty: (data) => data.reviewedCount === 0,
  }))
</script>

<Screen title={i18n.t('routes.summary')} phase={3}>
  <QueryView query={summary}>
    {#snippet ready(data)}
      <p>{i18n.t('summary.reviewed', { count: data.reviewedCount })}</p>
      <dl class="tiles">
        {#each RATINGS as rating (rating)}
          <StatTile label={i18n.t(`study.ratings.${rating}`)} value={i18n.formatNumber(data.ratings[rating])} />
        {/each}
      </dl>
      <a class="btn btn-primary" href="#/home">{i18n.t('common.goHome')}</a>
    {/snippet}
  </QueryView>
</Screen>

<style>
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 8rem), 1fr));
    gap: var(--space-3);
    margin: 0;
  }
</style>
