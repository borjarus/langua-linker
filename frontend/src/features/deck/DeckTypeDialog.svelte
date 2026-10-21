<script lang="ts">
  import { untrack } from 'svelte'
  import type { DeckOverviewDto, DeckType, ErrorDto, HiddenSectionCountsDto } from '../../api/types'
  import { useServices } from '../../app/context'
  import Dialog from '../../components/Dialog.svelte'
  import ErrorState from '../../components/ErrorState.svelte'

  interface Props {
    open: boolean
    overview: DeckOverviewDto
    onclose: () => void
    onsave: (deckType: DeckType) => Promise<ErrorDto | null>
  }

  let { open, overview, onclose, onsave }: Props = $props()
  const { i18n } = useServices()
  const ids = $props.id()

  let choice = $state<DeckType>('Simple')
  let saving = $state(false)
  let error = $state<ErrorDto | null>(null)

  $effect.pre(() => {
    if (!open) return
    untrack(() => {
      choice = overview.deck.deckType
      error = null
    })
  })

  const SECTIONS = ['associations', 'grammarTips', 'wordLinks'] as const
  const total = (counts: HiddenSectionCountsDto) => counts.associations + counts.grammarTips + counts.wordLinks
  const selected = $derived(overview.typeOptions.find((option) => option.deckType === choice))
  /** Content hidden now that the chosen type shows again. */
  const restored = $derived(
    selected ? SECTIONS.reduce((sum, section) => sum + Math.max(0, overview.hiddenSectionCounts[section] - selected.hiddenCounts[section]), 0) : 0,
  )

  function describe(counts: HiddenSectionCountsDto): string {
    const parts = SECTIONS.filter((section) => counts[section] > 0).map((section) => i18n.t(`deck.typeChange.counts.${section}`, { count: counts[section] }))
    return parts.length === 0 ? i18n.t('deck.typeChange.hidesNothing') : i18n.t('deck.typeChange.hides', { items: parts.join(', ') })
  }

  async function submit(event: SubmitEvent) {
    event.preventDefault()
    if (saving) return
    if (choice === overview.deck.deckType) {
      onclose()
      return
    }
    saving = true
    error = await onsave(choice)
    saving = false
  }
</script>

<Dialog {open} title={i18n.t('deck.typeChange.title')} onclose={() => !saving && onclose()}>
  <form id="{ids}-form" class="stack" onsubmit={submit}>
    <p class="muted">{i18n.t('deck.typeChange.intro')}</p>
    <fieldset class="types">
      <legend class="visually-hidden">{i18n.t('deck.form.type')}</legend>
      {#each overview.typeOptions as option (option.deckType)}
        <label class="type">
          <input type="radio" name="{ids}-type" value={option.deckType} bind:group={choice} />
          <span class="type-text">
            <strong>{i18n.t(`deck.types.${option.deckType}`)}{option.deckType === overview.deck.deckType ? ` — ${i18n.t('deck.typeChange.current')}` : ''}</strong>
            <span class="muted">{describe(option.hiddenCounts)}</span>
          </span>
        </label>
      {/each}
    </fieldset>
    {#if selected && choice !== overview.deck.deckType}
      <p role="status">
        {#if total(selected.hiddenCounts) > 0}{i18n.t('deck.typeChange.keptNote')}{/if}
        {#if restored > 0}{i18n.t('deck.typeChange.restoredNote', { count: restored })}{/if}
      </p>
    {/if}
    {#if error}<ErrorState {error} compact />{/if}
  </form>
  {#snippet actions()}
    <button type="button" class="btn" disabled={saving} onclick={onclose}>{i18n.t('common.cancel')}</button>
    <button type="submit" form="{ids}-form" class="btn btn-primary" disabled={saving} aria-busy={saving}>{i18n.t('deck.typeChange.apply')}</button>
  {/snippet}
</Dialog>

<style>
  .types {
    display: grid;
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    border: 0;
  }

  .type {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    min-height: var(--touch-target);
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    cursor: pointer;
  }

  .type:has(input:checked) {
    border-color: var(--color-primary);
    background: var(--color-primary-soft);
  }

  .type input {
    width: 1.25rem;
    height: 1.25rem;
    margin-top: 0.15rem;
    flex: none;
  }

  .type-text {
    display: grid;
    font-size: var(--text-sm);
  }
</style>
