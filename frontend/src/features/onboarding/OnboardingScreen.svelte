<script lang="ts">
  import { toErrorDto } from '../../api/errors'
  import type { JobDto } from '../../api/types'
  import { useServices } from '../../app/context'
  import Chip from '../../components/Chip.svelte'
  import ProgressPanel from '../../components/ProgressPanel.svelte'
  import { newOperationId } from '../../services/operationId'
  import { useQuery } from '../../services/query.svelte'
  import QueryView from '../shared/QueryView.svelte'
  import Screen from '../shared/Screen.svelte'

  const { api, i18n, queries, router, toasts } = useServices()
  const onboarding = useQuery(queries, () => ({ key: ['onboarding'], fetch: () => api.getOnboardingState() }))
  let job = $state<JobDto | null>(null)

  $effect(() =>
    api.onJobEvent((event) => {
      if (!job || event.jobId !== job.jobId || event.sequence <= job.sequence) return
      job = { ...job, ...event }
      if (event.status === 'succeeded') void router.replace({ name: 'home' })
    }),
  )

  async function start(deckGuids: string[]) {
    try {
      job = await api.completeOnboarding({ operationId: newOperationId(), targetLanguages: ['de', 'en'], dailyGoal: 20, deckGuids })
    } catch (caught) {
      toasts.show(i18n.tDynamic(toErrorDto(caught).messageKey, 'errors.INTERNAL'), 'error')
    }
  }
</script>

<Screen title={i18n.t('routes.onboarding')} phase={5}>
  <p>{i18n.t('onboarding.welcome')}</p>
  <QueryView query={onboarding}>
    {#snippet ready(state)}
      <h2>{i18n.t('onboarding.decks')}</h2>
      <ul class="stack decks">
        {#each state.availableDecks as deck (deck.guid)}
          <li class="surface row">
            <strong>{deck.name}</strong>
            <Chip label={i18n.t(`deck.targetLanguages.${deck.targetLanguage}`)} tone="primary" />
            <Chip label={deck.level} />
            <span class="muted">{i18n.t('deck.cards', { count: deck.cardCount })}</span>
          </li>
        {/each}
      </ul>
      {#if job}
        <ProgressPanel title={i18n.t('deck.preparing')} status={job.status} progress={job.progress} />
      {:else}
        <button type="button" class="btn btn-primary" onclick={() => start(state.availableDecks.map((deck) => deck.guid))}>
          {i18n.t('onboarding.start')}
        </button>
      {/if}
    {/snippet}
  </QueryView>
</Screen>

<style>
  .decks {
    margin: 0;
    padding: 0;
    list-style: none;
  }
</style>
