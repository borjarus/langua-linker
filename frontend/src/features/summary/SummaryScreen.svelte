<script lang="ts">
  import { toErrorDto } from '../../api/errors'
  import { RATINGS, type Id, type SessionSummaryDto } from '../../api/types'
  import { useServices } from '../../app/context'
  import { href } from '../../app/routes'
  import EmptyState from '../../components/EmptyState.svelte'
  import Icon from '../../components/Icon.svelte'
  import StatTile from '../../components/StatTile.svelte'
  import { useQuery } from '../../services/query.svelte'
  import QueryView from '../shared/QueryView.svelte'
  import Screen from '../shared/Screen.svelte'
  import { StudyController } from '../study/studyController.svelte'
  import ReminderOffer from './ReminderOffer.svelte'

  interface Props {
    sessionId: string
  }

  let { sessionId }: Props = $props()
  const { api, i18n, queries, router, toasts } = useServices()
  const ids = $props.id()
  const summary = useQuery(queries, () => ({
    key: ['summary', sessionId],
    fetch: () => api.getSessionSummary(sessionId),
    isEmpty: (data) => data.reviewedCount === 0,
  }))
  let starting = $state(false)

  function duration(ms: number): string {
    const total = Math.round(ms / 1000)
    const minutes = Math.floor(total / 60)
    const seconds = total % 60
    return minutes > 0
      ? i18n.t('summary.minutesSeconds', { minutes: i18n.formatNumber(minutes), seconds: i18n.formatNumber(seconds) })
      : i18n.t('summary.seconds', { seconds: i18n.formatNumber(seconds) })
  }

  const share = (part: number, whole: number) => (whole > 0 ? part / whole : 0)

  /** Starting a session is a mutation, so it only runs from these buttons. */
  async function startStudy(deckId: Id | null, cardIds?: Id[]) {
    if (starting) return
    starting = true
    try {
      const session = await StudyController.start(api, deckId, undefined, cardIds)
      await router.navigate({ name: 'study', params: { deckId: deckId ?? 'all' }, query: { session: session.sessionId } })
    } catch (caught) {
      toasts.show(i18n.tDynamic(toErrorDto(caught).messageKey, 'errors.INTERNAL'), 'error')
    } finally {
      starting = false
    }
  }

  const resumeTarget = (data: SessionSummaryDto) =>
    href({ name: 'study', params: { deckId: data.deckId ?? 'all' }, query: { session: data.sessionId } })
</script>

{#snippet nextSteps(data: SessionSummaryDto)}
  <section class="stack" aria-labelledby="{ids}-next">
    <h2 id="{ids}-next" class="section-title">{i18n.t('summary.whatNext')}</h2>
    <div class="row">
      {#if data.status !== 'finished'}
        <a class="btn btn-primary" href={resumeTarget(data)}>{i18n.t('dashboard.resume')}</a>
      {:else if data.remainingDue > 0}
        <button type="button" class="btn btn-primary" disabled={starting} onclick={() => startStudy(data.deckId)}>
          {i18n.t('summary.continueDeck', { count: data.remainingDue })}
        </button>
      {/if}
      {#if data.nextDeck}
        {@const next = data.nextDeck}
        <button type="button" class="btn" disabled={starting} onclick={() => startStudy(next.id)}>
          {i18n.t('summary.nextDeck', { name: next.name })}
        </button>
      {/if}
      <a class="btn" href={href({ name: 'home' })}>{i18n.t('common.goHome')}</a>
    </div>
  </section>
{/snippet}

<Screen title={i18n.t('routes.summary')} prototype>
  <QueryView query={summary}>
    {#snippet empty(data)}
      <EmptyState title={i18n.t('summary.emptyTitle')} description={i18n.t('summary.emptyBody')}>
        {#snippet actions()}
          {#if data.status !== 'finished'}
            <a class="btn btn-primary" href={resumeTarget(data)}>{i18n.t('dashboard.resume')}</a>
          {:else}
            <button type="button" class="btn btn-primary" disabled={starting} onclick={() => startStudy(data.deckId)}>{i18n.t('deck.study')}</button>
          {/if}
          <a class="btn" href={href({ name: 'home' })}>{i18n.t('common.goHome')}</a>
        {/snippet}
      </EmptyState>
    {/snippet}
    {#snippet ready(data)}
      {@const answered = data.reviewedCount}
      <p class="lead">
        {data.deckName ? i18n.t('summary.deck', { name: data.deckName }) : i18n.t('summary.allDecks')} ·
        {i18n.t('summary.reviewed', { count: answered })}
      </p>
      {#if data.status !== 'finished'}<p class="muted row"><Icon name="info" />{i18n.t('summary.inProgress')}</p>{/if}

      <dl class="tiles">
        <StatTile label={i18n.t('summary.time')} value={duration(data.activeMs)} hint={`${i18n.t('summary.perCard')}: ${duration(data.activeMs / Math.max(answered, 1))}`} tone="info" />
        <StatTile label={i18n.t('summary.withoutAgain')} value={i18n.formatPercent(share(answered - data.ratings.Again, answered))} hint={i18n.t('summary.withoutAgainNote')} tone="success" />
        <StatTile
          label={i18n.t('summary.nextDue')}
          value={data.nextDueAt ? i18n.formatDate(data.nextDueAt, { dateStyle: 'medium', timeStyle: 'short' }) : i18n.t('summary.noNextDue')}
        />
      </dl>

      <section class="stack" aria-labelledby="{ids}-ratings">
        <h2 id="{ids}-ratings" class="section-title">{i18n.t('summary.distribution')}</h2>
        <dl class="distribution">
          {#each RATINGS as rating (rating)}
            {@const count = data.ratings[rating]}
            <div class="bar-row">
              <dt>{i18n.t(`study.ratings.${rating}`)}</dt>
              <dd>
                <span class="bar" aria-hidden="true"><span class="fill rating-{rating.toLowerCase()}" style:width={`${share(count, answered) * 100}%`}></span></span>
                <span class="value">{i18n.t('summary.ratingShare', { count: i18n.formatNumber(count), percent: i18n.formatPercent(share(count, answered)) })}</span>
              </dd>
            </div>
          {/each}
        </dl>
      </section>

      {#if data.againCards.length > 0}
        <section class="stack" aria-labelledby="{ids}-again">
          <h2 id="{ids}-again" class="section-title">{i18n.t('summary.againTitle')}</h2>
          <ul class="again">
            {#each data.againCards as card (card.cardId)}
              <li class="surface">
                <strong lang={card.direction === 'forward' ? card.targetLanguage : 'pl'} dir="auto">{card.front}</strong>
                <span class="muted" lang={card.direction === 'forward' ? 'pl' : card.targetLanguage} dir="auto">{card.back}</span>
              </li>
            {/each}
          </ul>
          <button
            type="button"
            class="btn btn-primary"
            disabled={starting}
            onclick={() => startStudy(data.deckId, data.againCards.map((card) => card.cardId))}
          >
            {i18n.t('summary.studyAgain', { count: data.againCards.length })}
          </button>
        </section>
      {:else}
        <EmptyState title={i18n.t('summary.noAgainTitle')} description={i18n.t('summary.noAgainBody')} icon="check" />
      {/if}

      {#if data.firstCompleted}<ReminderOffer />{/if}

      {@render nextSteps(data)}
    {/snippet}
  </QueryView>
</Screen>

<style>
  .lead {
    font-size: var(--text-lg);
  }

  .section-title {
    font-size: var(--text-lg);
  }

  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 12rem), 1fr));
    gap: var(--space-3);
    margin: 0;
  }

  .distribution {
    display: grid;
    gap: var(--space-2);
    margin: 0;
  }

  .bar-row {
    display: grid;
    grid-template-columns: minmax(6rem, auto) 1fr;
    align-items: center;
    gap: var(--space-3);
  }

  .bar-row dd {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    margin: 0;
  }

  .bar {
    flex: 1 1 6rem;
    height: 0.75rem;
    overflow: hidden;
    border-radius: var(--radius-pill);
    background: var(--color-surface-sunken);
  }

  .fill {
    display: block;
    height: 100%;
  }

  .value {
    font-variant-numeric: tabular-nums;
  }

  .rating-again {
    background: var(--color-rating-again);
  }

  .rating-hard {
    background: var(--color-rating-hard);
  }

  .rating-good {
    background: var(--color-rating-good);
  }

  .rating-easy {
    background: var(--color-rating-easy);
  }

  .again {
    display: grid;
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .again li {
    display: grid;
    gap: var(--space-1);
    padding: var(--space-3) var(--space-4);
  }

  @media (max-width: 30rem) {
    .bar-row {
      grid-template-columns: 1fr;
      gap: var(--space-1);
    }
  }
</style>
