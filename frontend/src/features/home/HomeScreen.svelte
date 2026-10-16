<script lang="ts">
  import { toErrorDto } from '../../api/errors'
  import type { DashboardDto, DeckSummaryDto, Id, ResumableSessionDto } from '../../api/types'
  import { useServices } from '../../app/context'
  import { href } from '../../app/routes'
  import Chip from '../../components/Chip.svelte'
  import DeckCard from '../../components/DeckCard.svelte'
  import EmptyState from '../../components/EmptyState.svelte'
  import Icon from '../../components/Icon.svelte'
  import StatTile from '../../components/StatTile.svelte'
  import { useQuery } from '../../services/query.svelte'
  import QueryView from '../shared/QueryView.svelte'
  import Screen from '../shared/Screen.svelte'
  import { StudyController } from '../study/studyController.svelte'
  import NewDeckDialog from './NewDeckDialog.svelte'

  const { api, i18n, queries, router, toasts } = useServices()
  const dashboard = useQuery(queries, () => ({
    key: ['dashboard'],
    fetch: () => api.getDashboard(),
    isEmpty: (data) => data.decks.length === 0,
  }))
  const ids = $props.id()
  let starting = $state(false)
  let creating = $state(false)

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

  function resume(session: ResumableSessionDto) {
    void router.navigate({ name: 'study', params: { deckId: session.deckId ?? 'all' }, query: { session: session.sessionId } })
  }

  const roots = (decks: DeckSummaryDto[]) => decks.filter((deck) => deck.parentId === null)
  const dueOf = (deck: DeckSummaryDto) => deck.counts.new + deck.counts.learning + deck.counts.review

  /**
   * "No cards" (add or import) is not "nothing due today" (come back later), and neither is
   * "content still being prepared" (wait; the deck cards say so).
   */
  function overall(data: DashboardDto): 'due' | 'noCards' | 'caughtUp' | 'preparing' {
    const top = roots(data.decks)
    const ready = top.filter((deck) => deck.contentState === 'ready')
    if (ready.some((deck) => dueOf(deck) > 0)) return 'due'
    if (ready.length < top.length) return 'preparing'
    return top.every((deck) => deck.counts.total === 0) ? 'noCards' : 'caughtUp'
  }
</script>

<Screen title={i18n.t('dashboard.title')} prototype>
  {#snippet actions()}
    <!-- Local data: refreshing re-reads the collection and never needs the network. -->
    <button type="button" class="btn" disabled={dashboard.fetching} aria-busy={dashboard.fetching} onclick={() => dashboard.refresh()}>
      <Icon name="refresh" />{i18n.t('common.refresh')}
    </button>
    <button type="button" class="btn" onclick={() => (creating = true)}>
      <Icon name="plus" />{i18n.t('deck.newDeck')}
    </button>
  {/snippet}

  <QueryView query={dashboard}>
    {#snippet empty()}
      <EmptyState title={i18n.t('dashboard.emptyTitle')} description={i18n.t('dashboard.emptyBody')}>
        {#snippet actions()}
          <button type="button" class="btn btn-primary" onclick={() => router.navigate({ name: 'onboarding' })}>
            {i18n.t('dashboard.emptyAction')}
          </button>
          <button type="button" class="btn" onclick={() => (creating = true)}>{i18n.t('deck.newDeck')}</button>
        {/snippet}
      </EmptyState>
    {/snippet}
    {#snippet ready(data)}
      {@const state = overall(data)}
      {#if data.resumable}
        {@const open = data.resumable}
        <section class="resume surface" aria-labelledby="{ids}-resume">
          <div class="resume-text">
            <h2 id="{ids}-resume">{i18n.t('dashboard.resumeTitle')}</h2>
            <p>{open.deckName ?? i18n.t('dashboard.allDecks')}</p>
            <p class="muted">
              {i18n.t('summary.reviewed', { count: open.reviewedCount })} · {i18n.t('study.remaining', { count: open.remaining })}
            </p>
            {#if open.status === 'paused'}<Chip label={i18n.t('dashboard.paused')} icon="clock" tone="warning" />{/if}
          </div>
          <button type="button" class="btn btn-primary" onclick={() => resume(open)}>{i18n.t('dashboard.resume')}</button>
        </section>
      {/if}

      <section class="stack" aria-labelledby="{ids}-today">
        <h2 id="{ids}-today" class="section-title">{i18n.t('dashboard.today')}</h2>
        <dl class="tiles">
          <StatTile label={i18n.t('dashboard.dueToday')} value={i18n.formatNumber(data.dueToday)} tone="info" />
          <StatTile
            label={i18n.t('dashboard.studiedToday')}
            value={i18n.formatNumber(data.studiedToday)}
            hint={`${i18n.t('dashboard.goal')}: ${i18n.formatNumber(data.dailyGoal)}`}
            tone="success"
          >
            <meter min="0" max={Math.max(data.dailyGoal, 1)} value={Math.min(data.studiedToday, data.dailyGoal)} aria-label={i18n.t('dashboard.goalProgress')}></meter>
          </StatTile>
          <StatTile label={i18n.t('dashboard.streak', { count: data.streakDays })} value={i18n.formatNumber(data.streakDays)} tone="warning" />
        </dl>

        {#if state === 'due'}
          <button type="button" class="btn btn-primary study-all" disabled={starting} aria-busy={starting} onclick={() => study(null)}>
            {i18n.t('dashboard.studyAll')}
          </button>
        {:else if state === 'noCards'}
          <EmptyState title={i18n.t('dashboard.noCardsTitle')} description={i18n.t('dashboard.noCardsBody')}>
            {#snippet actions()}
              <a class="btn btn-primary" href={href({ name: 'editor' })}>{i18n.t('deck.addCards')}</a>
              <a class="btn" href={href({ name: 'settingsTransfer' })}>{i18n.t('dashboard.importCards')}</a>
            {/snippet}
          </EmptyState>
        {:else if state === 'caughtUp'}
          <EmptyState
            title={i18n.t('dashboard.caughtUpTitle')}
            description={data.nextDueAt
              ? `${i18n.t('dashboard.caughtUpBody')} ${i18n.t('dashboard.nextReview', { date: i18n.formatDate(data.nextDueAt, { dateStyle: 'full' }) })}`
              : i18n.t('dashboard.caughtUpBody')}
            icon="check"
          >
            {#snippet actions()}
              <a class="btn" href={href({ name: 'browser' })}>{i18n.t('nav.browser')}</a>
              <a class="btn" href={href({ name: 'stats' })}>{i18n.t('nav.stats')}</a>
              <a class="btn" href={href({ name: 'editor' })}>{i18n.t('deck.addCards')}</a>
            {/snippet}
          </EmptyState>
        {/if}
      </section>

      <section class="stack" aria-labelledby="{ids}-decks">
        <h2 id="{ids}-decks" class="section-title">{i18n.t('dashboard.decks')}</h2>
        <ul class="decks">
          {#each roots(data.decks) as deck (deck.id)}
            <li><DeckCard {deck} onstudy={(selected) => study(selected.id)} /></li>
          {/each}
        </ul>
      </section>
    {/snippet}
  </QueryView>
</Screen>

<NewDeckDialog
  open={creating}
  onclose={() => (creating = false)}
  oncreated={(created) => {
    creating = false
    toasts.show(i18n.t('deck.created', { name: created.deck.name }), 'success')
  }}
/>

<style>
  .resume {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    border-color: var(--color-primary);
  }

  .resume-text {
    display: grid;
    gap: var(--space-1);
    min-width: 0;
  }

  .resume h2,
  .section-title {
    font-size: var(--text-lg);
  }

  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 10rem), 1fr));
    gap: var(--space-3);
    margin: 0;
  }

  .tiles meter {
    width: 100%;
  }

  .study-all {
    width: 100%;
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
