<script lang="ts">
  import { untrack } from 'svelte'
  import { toErrorDto } from '../../api/errors'
  import type { WordLinkDto } from '../../api/types'
  import { useServices } from '../../app/context'
  import AssociationPanel from '../../components/AssociationPanel.svelte'
  import Dialog from '../../components/Dialog.svelte'
  import EmptyState from '../../components/EmptyState.svelte'
  import ErrorState from '../../components/ErrorState.svelte'
  import GrammarTipList from '../../components/GrammarTipList.svelte'
  import LinkedText from '../../components/LinkedText.svelte'
  import RatingButtonRow from '../../components/RatingButtonRow.svelte'
  import Skeleton from '../../components/Skeleton.svelte'
  import Screen from '../shared/Screen.svelte'
  import { StudyController } from './studyController.svelte'

  interface Props {
    deckId: string
    sessionId: string | null
  }

  let { deckId, sessionId }: Props = $props()
  const { api, i18n, router, toasts } = useServices()
  const study = new StudyController({ api })
  let activeLink = $state<WordLinkDto | null>(null)

  // Loading (a read) follows the session id; ratings only happen in handlers below.
  $effect(() => {
    const id = sessionId
    if (id) untrack(() => void study.resume(id))
  })

  /** Opened without a session (deep link, reload of an old URL): start one explicitly. */
  async function startSession() {
    try {
      const session = await StudyController.start(api, deckId === 'all' ? null : deckId)
      await router.replace({ name: 'study', params: { deckId }, query: { session: session.sessionId } })
    } catch (caught) {
      toasts.show(i18n.tDynamic(toErrorDto(caught).messageKey, 'errors.INTERNAL'), 'error')
    }
  }

  function onKeydown(event: KeyboardEvent) {
    const target = event.target as HTMLElement | null
    if (target?.closest('input, textarea, select, [contenteditable="true"], [role="dialog"]')) return
    if (event.key === ' ' && study.phase === 'question') {
      event.preventDefault()
      void study.reveal()
    } else if (['1', '2', '3', '4'].includes(event.key) && study.canRate) {
      const rating = (['Again', 'Hard', 'Good', 'Easy'] as const)[Number(event.key) - 1]
      if (rating) void study.rate(rating)
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<Screen title={i18n.t('routes.study')} back phase={3}>
  {#if !sessionId}
    <EmptyState title={i18n.t('routes.study')}>
      {#snippet actions()}
        <button type="button" class="btn btn-primary" onclick={startSession}>{i18n.t('deck.study')}</button>
        <a class="btn" href="#/home">{i18n.t('common.goHome')}</a>
      {/snippet}
    </EmptyState>
  {:else if study.phase === 'loading' || study.phase === 'idle'}
    <Skeleton lines={5} />
  {:else if study.phase === 'error' && study.error}
    <ErrorState error={study.error} onretry={() => sessionId && study.resume(sessionId)} />
  {:else if study.phase === 'finished' || !study.card}
    <EmptyState title={i18n.t('study.finished')} description={i18n.t('summary.reviewed', { count: study.reviewedCount })}>
      {#snippet actions()}
        <button type="button" class="btn btn-primary" onclick={() => sessionId && router.replace({ name: 'summary', params: { sessionId } })}>
          {i18n.t('routes.summary')}
        </button>
      {/snippet}
    </EmptyState>
  {:else}
    {@const card = study.card}
    <p class="muted" aria-live="polite">
      {i18n.t('study.remaining', { count: study.counts?.total ?? 0 })} · {i18n.t('summary.reviewed', { count: study.reviewedCount })}
    </p>
    <section class="card surface" aria-label={i18n.t('study.front')}>
      <p class="front" lang={card.direction === 'forward' ? card.targetLanguage : 'pl'}>
        {#if card.capabilities.wordLinks && card.direction === 'forward'}
          <LinkedText text={card.front} links={card.wordLinks} lang={card.targetLanguage} onselect={(link) => (activeLink = link)} />
        {:else}
          {card.front}
        {/if}
      </p>
      {#if study.phase !== 'question'}
        <hr />
        <p class="back" lang={card.direction === 'forward' ? 'pl' : card.targetLanguage}>{card.back}</p>
      {/if}
    </section>

    {#if study.phase === 'question'}
      <button type="button" class="btn btn-primary reveal" aria-keyshortcuts="Space" onclick={() => study.reveal()}>
        {i18n.t('study.showAnswer')}
      </button>
    {:else}
      {#if study.phase === 'save_failed' && study.error}
        <ErrorState error={study.error} compact onretry={() => study.retry()} />
      {/if}
      <RatingButtonRow preview={study.preview} disabled={study.phase === 'saving'} selected={study.selected} onrate={(rating) => study.rate(rating)} />
      {#if study.phase === 'saving'}<p class="muted" role="status">{i18n.t('study.saving')}</p>{/if}
      {#if card.capabilities.associations}<AssociationPanel associations={card.associations} />{/if}
      {#if card.capabilities.grammarTips}<GrammarTipList tips={card.grammarTips} />{/if}
    {/if}
  {/if}
</Screen>

<Dialog open={activeLink !== null} title={activeLink?.surface ?? ''} onclose={() => (activeLink = null)} variant="sheet">
  {#if activeLink}
    <p>{i18n.t(`wordLink.resolution.${activeLink.resolution}`)}</p>
    {#if activeLink.translation}<p><strong>{activeLink.lemma}</strong> — {activeLink.translation}</p>{/if}
    {#if activeLink.candidates.length > 0}
      <ul>
        {#each activeLink.candidates as candidate (candidate.noteId)}
          <li><strong>{candidate.lemma}</strong> — {candidate.translation}</li>
        {/each}
      </ul>
    {/if}
  {/if}
</Dialog>

<style>
  .card {
    display: grid;
    gap: var(--space-4);
    padding: var(--space-6) var(--space-5);
    text-align: center;
  }

  .front {
    font-size: var(--text-2xl);
    font-weight: var(--weight-bold);
  }

  .back {
    font-size: var(--text-xl);
  }

  hr {
    width: 100%;
    border: 0;
    border-top: 1px solid var(--color-border);
  }

  .reveal {
    width: 100%;
  }
</style>
