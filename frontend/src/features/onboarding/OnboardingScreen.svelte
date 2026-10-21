<script lang="ts">
  import { tick } from 'svelte'
  import { ONBOARDING_STEPS, type OnboardingStateDto, type OnboardingStep, type TargetLanguage } from '../../api/types'
  import { useServices } from '../../app/context'
  import Chip from '../../components/Chip.svelte'
  import ErrorState from '../../components/ErrorState.svelte'
  import Icon from '../../components/Icon.svelte'
  import ProgressPanel from '../../components/ProgressPanel.svelte'
  import { JobTracker } from '../../services/jobTracker.svelte'
  import { newOperationId } from '../../services/operationId'
  import { useQuery } from '../../services/query.svelte'
  import QueryView from '../shared/QueryView.svelte'
  import Screen from '../shared/Screen.svelte'

  interface Props {
    /** `settings` when opened again from the settings: the flow returns there. */
    from: string | null
  }

  let { from }: Props = $props()
  const { api, i18n, queries, router, toasts } = useServices()
  const ids = $props.id()
  const onboarding = useQuery(queries, () => ({ key: ['onboarding'], fetch: () => api.getOnboardingState() }))
  const job = new JobTracker(api)
  $effect(() => job.listen())

  const TARGETS: TargetLanguage[] = ['de', 'en']
  const GOALS = [10, 20, 30, 50] as const
  const fromSettings = $derived(from === 'settings')

  let step = $state<OnboardingStep>('nativeLanguage')
  let targetLanguages = $state<TargetLanguage[]>([])
  let dailyGoal = $state(20)
  let selected = $state<string[]>([])
  let initialized = false
  let operationId = newOperationId()
  let heading = $state<HTMLHeadingElement>()

  const index = $derived(ONBOARDING_STEPS.indexOf(step))
  const goalValid = $derived(Number.isInteger(dailyGoal) && dailyGoal >= 1 && dailyGoal <= 500)

  /** The form starts from the stored choices once; later refetches do not overwrite the user's edits. */
  function init(state: OnboardingStateDto) {
    if (initialized) return
    initialized = true
    targetLanguages = [...state.targetLanguages]
    dailyGoal = state.dailyGoal
    selected = state.availableDecks.filter((deck) => targetLanguages.includes(deck.targetLanguage) && !state.installedDeckGuids.includes(deck.guid)).map((deck) => deck.guid)
  }

  $effect(() => {
    if (onboarding.state.status === 'ready') init(onboarding.state.data)
  })

  async function go(next: OnboardingStep) {
    step = next
    // Each step is a new view: move focus to its heading for keyboard and screen-reader users.
    await tick()
    heading?.focus()
  }

  function toggleLanguage(language: TargetLanguage, state: OnboardingStateDto, checked: boolean) {
    targetLanguages = checked ? [...targetLanguages, language] : targetLanguages.filter((item) => item !== language)
    const allowed = new Set(state.availableDecks.filter((deck) => targetLanguages.includes(deck.targetLanguage)).map((deck) => deck.guid))
    selected = checked
      ? [...new Set([...selected, ...state.availableDecks.filter((deck) => deck.targetLanguage === language && !state.installedDeckGuids.includes(deck.guid)).map((deck) => deck.guid)])]
      : selected.filter((guid) => allowed.has(guid))
  }

  function toggleDeck(guid: string, checked: boolean) {
    selected = checked ? [...selected, guid] : selected.filter((item) => item !== guid)
  }

  async function finish() {
    const started = await job.start(
      () => api.completeOnboarding({ operationId, nativeLanguage: 'pl', targetLanguages, dailyGoal, deckGuids: selected }),
      (finished) => {
        if (finished.status !== 'succeeded') return
        queries.clear()
        toasts.show(i18n.t('onboarding.done', { count: selected.length }), 'success')
        void router.replace(fromSettings ? { name: 'settings' } : { name: 'home' })
      },
    )
    if (!started) operationId = newOperationId()
  }
</script>

<Screen title={i18n.t('routes.onboarding')} back={fromSettings}>
  <QueryView query={onboarding}>
    {#snippet ready(state)}
      {@const available = state.availableDecks.filter((deck) => targetLanguages.includes(deck.targetLanguage))}
      <p class="welcome">{i18n.t('onboarding.welcome')}</p>
      <ol class="steps" aria-label={i18n.t('onboarding.progress')}>
        {#each ONBOARDING_STEPS as item, position (item)}
          <li class:done={position < index} class:current={position === index} aria-current={position === index ? 'step' : undefined}>
            <span class="dot" aria-hidden="true">{#if position < index}<Icon name="check" size={14} />{:else}{position + 1}{/if}</span>
            <span class="name">{i18n.t(`onboarding.steps.${item}`)}</span>
          </li>
        {/each}
      </ol>

      <section class="surface stack" aria-labelledby="{ids}-step">
        <p class="muted counter">{i18n.t('onboarding.stepOf', { step: index + 1, total: ONBOARDING_STEPS.length })}</p>
        <h2 id="{ids}-step" tabindex="-1" bind:this={heading}>{i18n.t(`onboarding.titles.${step}`)}</h2>

        {#if step === 'nativeLanguage'}
          <p>{i18n.t('onboarding.native.body')}</p>
          <label class="choice">
            <input type="radio" name="{ids}-native" checked />
            {i18n.t('settings.nativeLanguages.pl')}
          </label>
          <p class="muted hint">{i18n.t('onboarding.native.hint')}</p>
        {:else if step === 'targetLanguage'}
          <fieldset class="group">
            <legend>{i18n.t('onboarding.target.body')}</legend>
            {#each TARGETS as language (language)}
              <label class="choice">
                <input type="checkbox" checked={targetLanguages.includes(language)} onchange={(event) => toggleLanguage(language, state, event.currentTarget.checked)} />
                {i18n.t(`deck.targetLanguages.${language}`)}
              </label>
            {/each}
          </fieldset>
          {#if targetLanguages.length === 0}<p class="invalid" role="alert">{i18n.t('onboarding.target.required')}</p>{/if}
        {:else if step === 'goal'}
          <fieldset class="group">
            <legend>{i18n.t('onboarding.goal.body')}</legend>
            {#each GOALS as option (option)}
              <label class="choice">
                <input type="radio" name="{ids}-goal" checked={dailyGoal === option} onchange={() => (dailyGoal = option)} />
                {i18n.t('onboarding.goal.option', { count: option })}
                <span class="muted">· {i18n.t(`onboarding.goal.hints.g${option}`)}</span>
              </label>
            {/each}
          </fieldset>
          <div class="field">
            <label for="{ids}-custom">{i18n.t('onboarding.goal.custom')}</label>
            <input id="{ids}-custom" class="input narrow" type="number" inputmode="numeric" min="1" max="500" value={dailyGoal} oninput={(event) => (dailyGoal = event.currentTarget.valueAsNumber)} aria-invalid={!goalValid} />
            {#if !goalValid}<p class="invalid" role="alert">{i18n.t('settings.dailyGoalError')}</p>{/if}
          </div>
          <p class="muted hint">{i18n.t('onboarding.goal.later')}</p>
        {:else if step === 'decks'}
          <p>{i18n.t('onboarding.decks.body')}</p>
          <ul class="decks">
            {#each available as deck (deck.guid)}
              {@const installed = state.installedDeckGuids.includes(deck.guid)}
              <li class="surface">
                <label class="deck">
                  <input type="checkbox" checked={installed || selected.includes(deck.guid)} disabled={installed} onchange={(event) => toggleDeck(deck.guid, event.currentTarget.checked)} />
                  <span class="deck-text">
                    <strong>{deck.name}</strong>
                    <span class="muted">{deck.description}</span>
                    <span class="row">
                      <Chip label={i18n.t(`deck.targetLanguages.${deck.targetLanguage}`)} tone="primary" />
                      <Chip label={deck.level} />
                      <Chip label={i18n.t(`deck.types.${deck.deckType}`)} />
                      <span class="muted">{i18n.t('deck.cards', { count: deck.cardCount })}</span>
                    </span>
                    {#if installed}<span class="muted">{i18n.t('onboarding.decks.installed')}</span>{/if}
                  </span>
                </label>
              </li>
            {/each}
          </ul>
          {#if selected.length === 0}<p class="muted hint">{i18n.t('onboarding.decks.none')}</p>{/if}
        {:else}
          <dl class="summary">
            <div><dt>{i18n.t('onboarding.steps.nativeLanguage')}</dt><dd>{i18n.t('settings.nativeLanguages.pl')}</dd></div>
            <div><dt>{i18n.t('onboarding.steps.targetLanguage')}</dt><dd>{targetLanguages.map((language) => i18n.t(`deck.targetLanguages.${language}`)).join(', ')}</dd></div>
            <div><dt>{i18n.t('onboarding.steps.goal')}</dt><dd>{i18n.t('onboarding.goal.option', { count: dailyGoal })}</dd></div>
            <div><dt>{i18n.t('onboarding.steps.decks')}</dt><dd>{i18n.t('onboarding.ready.decks', { count: selected.length })}</dd></div>
          </dl>
          <p class="muted hint">{i18n.t('onboarding.ready.optional')}</p>
          {#if job.job}
            <ProgressPanel title={i18n.t('deck.preparing')} status={job.job.status} progress={job.job.progress} />
            {#if job.job.status === 'failed' && job.job.error}<ErrorState error={job.job.error} compact />{/if}
          {/if}
          {#if job.error}<ErrorState error={job.error} compact />{/if}
        {/if}

        <div class="row nav">
          {#if index > 0}
            <button type="button" class="btn" disabled={job.active} onclick={() => go(ONBOARDING_STEPS[index - 1] ?? 'nativeLanguage')}>{i18n.t('onboarding.previous')}</button>
          {/if}
          {#if step === 'ready'}
            <button type="button" class="btn btn-primary" disabled={job.active || job.job?.status === 'succeeded'} aria-busy={job.active} onclick={finish}>
              {i18n.t(fromSettings ? 'onboarding.apply' : 'onboarding.start')}
            </button>
          {:else}
            <button
              type="button"
              class="btn btn-primary"
              disabled={(step === 'targetLanguage' && targetLanguages.length === 0) || (step === 'goal' && !goalValid)}
              onclick={() => go(ONBOARDING_STEPS[index + 1] ?? 'ready')}
            >
              {i18n.t('onboarding.next')}
            </button>
          {/if}
        </div>
      </section>
    {/snippet}
  </QueryView>
</Screen>

<style>
  .welcome {
    font-size: var(--text-lg);
  }

  .steps {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2) var(--space-3);
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: var(--text-sm);
  }

  .steps li {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    color: var(--color-text-muted);
  }

  .steps li.current {
    color: var(--color-text);
    font-weight: var(--weight-bold);
  }

  .dot {
    display: inline-grid;
    place-items: center;
    width: 1.5rem;
    height: 1.5rem;
    border: 1px solid var(--color-border-strong);
    border-radius: 50%;
    font-size: var(--text-xs);
  }

  .current .dot {
    border-color: var(--color-primary);
    background: var(--color-primary);
    color: var(--color-on-primary);
  }

  .done .dot {
    border-color: var(--color-success);
    color: var(--color-success);
  }

  h2 {
    font-size: var(--text-xl);
  }

  .counter,
  .hint {
    font-size: var(--text-sm);
  }

  .group {
    display: grid;
    margin: 0;
    padding: 0;
    border: 0;
  }

  legend {
    margin-bottom: var(--space-2);
  }

  .choice {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
    min-height: var(--touch-target);
  }

  .choice input,
  .deck input {
    flex: none;
    width: 1.25rem;
    height: 1.25rem;
    accent-color: var(--color-primary);
  }

  .field > .narrow {
    width: min(100%, 10rem);
  }

  .invalid {
    color: var(--color-error);
  }

  .decks {
    display: grid;
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .deck {
    display: flex;
    align-items: start;
    gap: var(--space-3);
    cursor: pointer;
  }

  .deck input {
    margin-top: 0.25rem;
  }

  .deck-text {
    display: grid;
    gap: var(--space-1);
    min-width: 0;
  }

  .summary {
    display: grid;
    gap: var(--space-2);
    margin: 0;
  }

  .summary div {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1) var(--space-2);
  }

  dt {
    color: var(--color-text-muted);
  }

  dd {
    margin: 0;
    font-weight: var(--weight-medium);
  }

  .nav {
    justify-content: flex-end;
  }
</style>
