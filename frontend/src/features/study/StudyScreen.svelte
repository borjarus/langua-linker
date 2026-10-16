<script lang="ts">
  import { tick, untrack } from 'svelte'
  import { toErrorDto } from '../../api/errors'
  import { RATINGS, type CardFlag, type ErrorDto, type Id, type WordLinkDto } from '../../api/types'
  import { useServices } from '../../app/context'
  import type { Route } from '../../app/routes'
  import AssociationPanel from '../../components/AssociationPanel.svelte'
  import Chip from '../../components/Chip.svelte'
  import Dialog from '../../components/Dialog.svelte'
  import EmptyState from '../../components/EmptyState.svelte'
  import ErrorState from '../../components/ErrorState.svelte'
  import GrammarTipList from '../../components/GrammarTipList.svelte'
  import Icon from '../../components/Icon.svelte'
  import LinkedText from '../../components/LinkedText.svelte'
  import RatingButtonRow from '../../components/RatingButtonRow.svelte'
  import Skeleton from '../../components/Skeleton.svelte'
  import Screen from '../shared/Screen.svelte'
  import { AssociationGenerator } from './associationGenerator.svelte'
  import CardActions, { type HidingAction } from './CardActions.svelte'
  import SpeakButton from './SpeakButton.svelte'
  import { StudyController } from './studyController.svelte'
  import WordSheet from './WordSheet.svelte'

  interface Props {
    deckId: string
    sessionId: string | null
  }

  let { deckId, sessionId }: Props = $props()
  const { api, i18n, router, toasts } = useServices()
  const study = new StudyController({ api })
  const generator = new AssociationGenerator({ api, onupdate: (noteId, associations) => study.setAssociations(noteId, associations) })
  let activeLink = $state<WordLinkDto | null>(null)
  let starting = $state(false)
  let questionEl = $state<HTMLElement | null>(null)
  let answerEl = $state<HTMLElement | null>(null)
  const pausedTitleId = $props.id()

  const card = $derived(study.card)
  const title = $derived(study.deckName ?? (deckId === 'all' ? i18n.t('study.titleAll') : i18n.t('routes.study')))
  /** Word links and TTS belong to the target-language side, which is the back of a reverse card. */
  const targetSide = $derived(card ? (card.direction === 'forward' ? 'front' : 'back') : 'front')

  // Loading (a read) follows the session id; ratings only happen in handlers below.
  $effect(() => {
    const id = sessionId
    if (id) untrack(() => void study.resume(id))
  })

  // Job events drive association generation; the subscription ends with the screen.
  $effect(() => api.onJobEvent((event) => generator.handleEvent(event)))

  // Time in the background is not answer time.
  $effect(() => {
    const onVisibility = () => (document.hidden ? study.suspendClock() : study.resumeClock())
    document.addEventListener('visibilitychange', onVisibility)
    return () => document.removeEventListener('visibilitychange', onVisibility)
  })

  // After user input, focus follows the content: the answer after reveal, the next question after a rating.
  $effect(() => {
    if (study.step === 0) return
    void tick().then(() => untrack(() => (study.phase === 'question' ? questionEl : answerEl))?.focus())
  })

  // Leave guard: ratings are saved, but leaving an open session should be a decision.
  let pendingLeave = $state<((leave: boolean) => void) | null>(null)
  /** Set before explicit exits (edit, add word, summary) that need no confirmation. */
  let leaving = false
  $effect(() => {
    const removeGuard = router.addGuard({
      isBlocking: () => !leaving && study.inSession,
      confirmLeave: (_to: Route) => new Promise<boolean>((resolve) => (pendingLeave = resolve)),
      // Ratings are committed as they happen; a reload loses nothing, so no browser "unsaved changes" prompt.
      blocksUnload: false,
    })
    return () => {
      removeGuard()
      pendingLeave?.(false)
    }
  })

  function decideLeave(leave: boolean) {
    pendingLeave?.(leave)
    pendingLeave = null
  }

  const report = (error: ErrorDto | null) => {
    if (error) toasts.show(i18n.tDynamic(error.messageKey, 'errors.INTERNAL'), 'error')
    return error === null
  }

  /** Opened without a session (deep link, reload of an old URL) or after losing one: start explicitly. */
  async function startSession() {
    if (starting) return
    starting = true
    try {
      const session = await StudyController.start(api, deckId === 'all' ? null : deckId)
      await router.replace({ name: 'study', params: { deckId }, query: { session: session.sessionId } })
    } catch (caught) {
      report(toErrorDto(caught))
    } finally {
      starting = false
    }
  }

  async function openSummary() {
    if (!study.sessionId) return
    leaving = true
    await router.replace({ name: 'summary', params: { sessionId: study.sessionId } })
  }

  async function endSession() {
    if (report(await study.end())) await openSummary()
  }

  async function undo() {
    if (report(await study.undo())) toasts.show(i18n.t('study.undone'), 'success')
  }

  async function hide(action: HidingAction) {
    if (report(await study.applyAction(action))) toasts.show(i18n.t(`study.done.${action}`), 'success')
  }

  async function setFlag(flag: CardFlag) {
    if (report(await study.applyAction('flag', flag))) toasts.show(i18n.t('study.done.flag'), 'success')
  }

  function edit() {
    if (!card) return
    leaving = true
    void router.navigate({ name: 'editor', query: { noteId: card.noteId, deckId: card.deckId } })
  }

  async function resolveLink(link: WordLinkDto, wordNoteId: Id): Promise<ErrorDto | null> {
    const error = await study.resolveWordLink(link, wordNoteId)
    // Show the saved choice with its details in the open sheet.
    if (!error) activeLink = study.card?.wordLinks.find((item) => item.id === link.id) ?? null
    return error
  }

  async function generate(noteId: Id) {
    await generator.generate(noteId)
    if (generator.isQueued(noteId)) toasts.show(i18n.t('association.queued'), 'info')
  }

  async function favorite(noteId: Id, associationId: Id) {
    if (report(await generator.favorite(noteId, associationId))) toasts.show(i18n.t('association.favoriteSaved'), 'success')
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.ctrlKey || event.metaKey || event.altKey || event.repeat || study.paused) return
    const target = event.target as HTMLElement | null
    if (target?.closest('input, textarea, select, [contenteditable="true"], [role="dialog"]')) return
    if (event.key === ' ' && study.phase === 'question') {
      // A focused button handles Space itself (e.g. "Show answer", "Pause").
      if (target?.closest('button, a[href]')) return
      event.preventDefault()
      void study.reveal()
    } else if (['1', '2', '3', '4'].includes(event.key) && study.canRate) {
      event.preventDefault()
      const rating = RATINGS[Number(event.key) - 1]
      if (rating) void study.rate(rating)
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<Screen {title} back prototype>
  {#if !sessionId}
    <EmptyState title={i18n.t('study.startTitle')} description={i18n.t('study.startBody')}>
      {#snippet actions()}
        <button type="button" class="btn btn-primary" disabled={starting} onclick={startSession}>{i18n.t('deck.study')}</button>
        <a class="btn" href="#/home">{i18n.t('common.goHome')}</a>
      {/snippet}
    </EmptyState>
  {:else if study.phase === 'loading' || study.phase === 'idle'}
    <Skeleton lines={5} />
  {:else if study.phase === 'lost'}
    <EmptyState title={i18n.t('study.lostTitle')} description={i18n.t('study.lostBody')} icon="alert">
      {#snippet actions()}
        <button type="button" class="btn btn-primary" disabled={starting} onclick={startSession}>{i18n.t('study.startNew')}</button>
        <a class="btn" href="#/home">{i18n.t('common.goHome')}</a>
      {/snippet}
    </EmptyState>
  {:else if study.phase === 'error' && study.error}
    <ErrorState error={study.error} onretry={() => sessionId && study.resume(sessionId)} />
  {:else if study.phase === 'finished' || !card}
    <EmptyState title={i18n.t('study.finished')} description={i18n.t('summary.reviewed', { count: study.reviewedCount })} icon="check">
      {#snippet actions()}
        <button type="button" class="btn btn-primary" onclick={openSummary}>{i18n.t('routes.summary')}</button>
        <a class="btn" href="#/home">{i18n.t('common.goHome')}</a>
      {/snippet}
    </EmptyState>
  {:else}
    <div class="progress">
      <p class="muted" aria-live="polite">
        {i18n.t('summary.reviewed', { count: study.reviewedCount })} · {i18n.t('study.remaining', { count: study.counts?.total ?? 0 })}
      </p>
      <meter min="0" max={Math.max(study.total, 1)} value={study.reviewedCount} aria-label={i18n.t('study.progress')}></meter>
      {#if study.resumedWith > 0}<p class="muted">{i18n.t('study.resumed', { count: study.resumedWith })}</p>{/if}
    </div>

    <div class="row toolbar">
      <button type="button" class="btn" disabled={!study.undoAvailable || study.busy || study.paused} onclick={undo}>
        <Icon name="back" />{i18n.t('study.undo')}
      </button>
      {#if !study.paused}
        <button type="button" class="btn" disabled={study.busy} onclick={() => study.pause().then(report)}>
          <Icon name="clock" />{i18n.t('study.pause')}
        </button>
      {/if}
      <CardActions {card} disabled={study.busy || study.paused} onhide={hide} onflag={setFlag} onedit={edit} />
      <button type="button" class="btn" disabled={study.busy} onclick={endSession}>
        <Icon name="check" />{i18n.t('study.end')}
      </button>
    </div>

    {#if study.paused}
      <section class="paused surface" aria-labelledby={pausedTitleId}>
        <h2 id={pausedTitleId}>{i18n.t('study.pausedTitle')}</h2>
        <p class="muted">{i18n.t('study.pausedBody')}</p>
        <button type="button" class="btn btn-primary" disabled={study.acting} onclick={() => study.continue().then(report)}>
          {i18n.t('study.continue')}
        </button>
      </section>
    {:else}
      <section class="card surface" aria-label={i18n.t('study.front')}>
        <div class="row chips">
          <Chip label={i18n.t(`study.states.${card.state}`)} />
          {#if card.direction === 'reverse'}<Chip label={i18n.t('study.reverse')} icon="refresh" />{/if}
          {#if card.flag !== 'none'}<Chip label={i18n.t('study.flagged', { flag: i18n.t(`study.flags.${card.flag}`) })} icon="flag" tone="warning" />{/if}
        </div>
        <div class="front" tabindex="-1" bind:this={questionEl} lang={card.direction === 'forward' ? card.targetLanguage : 'pl'}>
          {#if card.capabilities.wordLinks && targetSide === 'front'}
            <LinkedText text={card.front} links={card.wordLinks} lang={card.targetLanguage} onselect={(link) => (activeLink = link)} />
          {:else}
            <span dir="auto">{card.front}</span>
          {/if}
        </div>
        {#if card.capabilities.tts && targetSide === 'front'}<SpeakButton text={card.front} lang={card.targetLanguage} />{/if}
        {#if study.phase !== 'question'}
          <hr />
          <div class="back" tabindex="-1" bind:this={answerEl} lang={card.direction === 'forward' ? 'pl' : card.targetLanguage}>
            {#if card.capabilities.wordLinks && targetSide === 'back'}
              <LinkedText text={card.back} links={card.wordLinks} lang={card.targetLanguage} onselect={(link) => (activeLink = link)} />
            {:else}
              <span dir="auto">{card.back}</span>
            {/if}
          </div>
          {#if card.capabilities.tts && targetSide === 'back'}<SpeakButton text={card.back} lang={card.targetLanguage} />{/if}
        {/if}
      </section>

      {#if study.phase === 'question'}
        <button type="button" class="btn btn-primary reveal" aria-keyshortcuts="Space" onclick={() => study.reveal()}>
          {i18n.t('study.showAnswer')}
        </button>
        <p class="muted hint">{i18n.t('study.keyboardHint')}</p>
      {:else}
        {#if study.phase === 'save_failed' && study.error}
          <ErrorState error={study.error} compact onretry={() => study.retry()} />
        {/if}
        <RatingButtonRow preview={study.preview} disabled={!study.canRate} selected={study.selected} onrate={(rating) => study.rate(rating)} />
        {#if study.phase === 'saving'}<p class="muted" role="status">{i18n.t('study.saving')}</p>{/if}

        {#if card.capabilities.associations}
          {@const noteId = card.noteId}
          {@const generationError = generator.errorFor(noteId)}
          <!-- Generation runs in the background; rating stays available. -->
          <AssociationPanel
            associations={card.associations}
            generating={generator.isRunning(noteId)}
            ongenerate={() => generate(noteId)}
            onfavorite={(association) => favorite(noteId, association.id)}
          />
          {#if generator.isRunning(noteId)}<p class="muted" role="status">{i18n.t('association.generating')}</p>{/if}
          {#if generationError}<ErrorState error={generationError} compact onretry={() => generate(noteId)} />{/if}
        {/if}
        {#if card.capabilities.grammarTips}<GrammarTipList tips={card.grammarTips} />{/if}
      {/if}
    {/if}
  {/if}
</Screen>

<WordSheet
  link={activeLink}
  deckId={card?.deckId ?? deckId}
  onclose={() => (activeLink = null)}
  onresolve={resolveLink}
  onleave={() => (leaving = true)}
/>

<Dialog open={pendingLeave !== null} title={i18n.t('study.leaveTitle')} onclose={() => decideLeave(false)}>
  <p>{i18n.t('study.leaveBody')}</p>
  {#snippet actions()}
    <button type="button" class="btn" onclick={() => decideLeave(false)}>{i18n.t('study.stay')}</button>
    <button type="button" class="btn btn-primary" onclick={() => decideLeave(true)}>{i18n.t('study.leave')}</button>
  {/snippet}
</Dialog>

<style>
  .progress {
    display: grid;
    gap: var(--space-1);
    font-size: var(--text-sm);
  }

  meter {
    width: 100%;
  }

  .toolbar {
    font-size: var(--text-sm);
  }

  .card {
    display: grid;
    gap: var(--space-4);
    padding: var(--space-5) var(--space-5) var(--space-6);
    text-align: center;
  }

  .chips {
    justify-content: center;
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

  .hint {
    font-size: var(--text-sm);
    text-align: center;
  }

  /* Touch-only phones: the shortcuts do not apply (a hardware keyboard usually comes with a fine pointer). */
  @media (hover: none) and (pointer: coarse) {
    .hint {
      display: none;
    }
  }

  .paused {
    display: grid;
    justify-items: center;
    gap: var(--space-3);
    text-align: center;
  }

  .paused h2 {
    font-size: var(--text-lg);
  }
</style>
