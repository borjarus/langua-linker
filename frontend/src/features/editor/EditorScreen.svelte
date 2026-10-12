<script lang="ts">
  import { useServices } from '../../app/context'
  import Dialog from '../../components/Dialog.svelte'
  import type { Route } from '../../app/routes'
  import { useQuery } from '../../services/query.svelte'
  import QueryView from '../shared/QueryView.svelte'
  import Screen from '../shared/Screen.svelte'

  interface Props {
    deckId: string | null
    noteId: string | null
  }

  let { deckId, noteId }: Props = $props()
  const { api, i18n, queries, router } = useServices()
  const editor = useQuery(queries, () => ({ key: ['editor'], fetch: () => api.getEditorData() }))
  const frontId = $props.id()

  let front = $state('')
  let back = $state('')
  const dirty = $derived(front.trim() !== '' || back.trim() !== '')

  // Draft guard: the router asks before leaving; the dialog resolves the pending decision.
  let pendingLeave = $state<((leave: boolean) => void) | null>(null)
  $effect(() => {
    const removeGuard = router.addGuard({
      isBlocking: () => dirty,
      confirmLeave: (_to: Route) => new Promise<boolean>((resolve) => (pendingLeave = resolve)),
    })
    return () => {
      removeGuard()
      pendingLeave?.(false)
    }
  })

  function decide(leave: boolean) {
    pendingLeave?.(leave)
    pendingLeave = null
  }
</script>

<Screen title={noteId ? i18n.t('editor.editNote') : i18n.t('editor.newNote')} back phase={4}>
  <QueryView query={editor}>
    {#snippet ready(data)}
      <form class="stack surface" onsubmit={(event) => event.preventDefault()}>
        <div class="field">
          <label for="{frontId}-deck">{i18n.t('routes.deck')}</label>
          <select id="{frontId}-deck" class="input" value={deckId ?? data.decks[0]?.id}>
            {#each data.decks as deck (deck.id)}<option value={deck.id}>{deck.name}</option>{/each}
          </select>
        </div>
        <div class="field">
          <label for="{frontId}-front">{i18n.t('study.front')}</label>
          <textarea id="{frontId}-front" class="input" rows="2" maxlength={data.maxFieldLength} bind:value={front}></textarea>
        </div>
        <div class="field">
          <label for="{frontId}-back">{i18n.t('study.back')}</label>
          <textarea id="{frontId}-back" class="input" rows="2" maxlength={data.maxFieldLength} bind:value={back}></textarea>
        </div>
      </form>
    {/snippet}
  </QueryView>
</Screen>

<Dialog open={pendingLeave !== null} title={i18n.t('guard.title')} onclose={() => decide(false)} dismissible={false}>
  <p>{i18n.t('guard.body')}</p>
  {#snippet actions()}
    <button type="button" class="btn" data-autofocus onclick={() => decide(false)}>{i18n.t('guard.stay')}</button>
    <button type="button" class="btn btn-danger" onclick={() => decide(true)}>{i18n.t('guard.leave')}</button>
  {/snippet}
</Dialog>
