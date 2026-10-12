<script lang="ts">
  import { toErrorDto } from '../../api/errors'
  import type { DeckSummaryDto, Id } from '../../api/types'
  import { useServices } from '../../app/context'
  import DeckCard from '../../components/DeckCard.svelte'
  import EmptyState from '../../components/EmptyState.svelte'
  import StatTile from '../../components/StatTile.svelte'
  import { useQuery } from '../../services/query.svelte'
  import QueryView from '../shared/QueryView.svelte'
  import Screen from '../shared/Screen.svelte'
  import { StudyController } from '../study/studyController.svelte'

  const { api, i18n, queries, router, toasts } = useServices()
  const dashboard = useQuery(queries, () => ({
    key: ['dashboard'],
    fetch: () => api.getDashboard(),
    isEmpty: (data) => data.decks.length === 0,
  }))
  let starting = $state(false)

  /** Explicit handler: starting a session is a mutation and never runs on mount. */
  async function study(deckId: Id | null) {
    if (starting) return
    starting = true
    try {
      const session = await StudyController.start(api, deckId)
      await router.navigate({ name: 'study', params: { deckId: deckId ?? 'all' }, query: { session: session.sessionId } })
    } catch (caught) {
      toasts.show(i18n.tDynamic(toErrorDto(caught).messageKey, 'errors.INTERNAL'), 'error')
    } finally {
      starting = false
    }
  }

  const roots = (decks: DeckSummaryDto[]) => decks.filter((deck) => deck.parentId === null)
</script>

<Screen title={i18n.t('dashboard.title')} phase={3}>
  <QueryView query={dashboard}>
    {#snippet empty()}
      <EmptyState title={i18n.t('dashboard.emptyTitle')} description={i18n.t('dashboard.emptyBody')}>
        {#snippet actions()}
          <button type="button" class="btn btn-primary" onclick={() => router.navigate({ name: 'onboarding' })}>
            {i18n.t('dashboard.emptyAction')}
          </button>
        {/snippet}
      </EmptyState>
    {/snippet}
    {#snippet ready(data)}
      <dl class="tiles">
        <StatTile label={i18n.t('dashboard.dueToday')} value={i18n.formatNumber(data.dueToday)} tone="info" />
        <StatTile
          label={i18n.t('dashboard.studiedToday')}
          value={i18n.formatNumber(data.studiedToday)}
          hint={`${i18n.t('dashboard.goal')}: ${i18n.formatNumber(data.dailyGoal)}`}
          tone="success"
        />
        <StatTile label={i18n.t('dashboard.streak', { count: data.streakDays })} value={i18n.formatNumber(data.streakDays)} tone="warning" />
      </dl>
      <div class="row">
        {#if data.resumableSessionId}
          <button
            type="button"
            class="btn"
            onclick={() => router.navigate({ name: 'study', params: { deckId: 'all' }, query: { session: data.resumableSessionId } })}
          >
            {i18n.t('dashboard.resume')}
          </button>
        {/if}
        <button type="button" class="btn btn-primary" disabled={starting} onclick={() => study(null)}>
          {i18n.t('dashboard.studyAll')}
        </button>
      </div>
      <ul class="decks">
        {#each roots(data.decks) as deck (deck.id)}
          <li><DeckCard {deck} onstudy={(selected) => study(selected.id)} /></li>
        {/each}
      </ul>
    {/snippet}
  </QueryView>
</Screen>

<style>
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 10rem), 1fr));
    gap: var(--space-3);
    margin: 0;
  }

  .decks {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(min(100%, 20rem), 1fr));
    gap: var(--space-4);
    margin: 0;
    padding: 0;
    list-style: none;
  }
</style>
