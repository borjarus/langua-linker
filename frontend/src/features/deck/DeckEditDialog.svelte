<script lang="ts">
  import { untrack } from 'svelte'
  import type { DeckOverviewDto, EditorDeckOptionDto, ErrorDto, UpdateDeckInput } from '../../api/types'
  import { useServices } from '../../app/context'
  import Dialog from '../../components/Dialog.svelte'
  import ErrorState from '../../components/ErrorState.svelte'

  interface Props {
    open: boolean
    overview: DeckOverviewDto
    /** Every deck; the deck itself and its subdecks are not offered as a new parent (no cycles). */
    decks: EditorDeckOptionDto[]
    onclose: () => void
    onsave: (patch: UpdateDeckInput['patch']) => Promise<ErrorDto | null>
  }

  let { open, overview, decks, onclose, onsave }: Props = $props()
  const { i18n } = useServices()
  const ids = $props.id()

  let name = $state('')
  let description = $state('')
  let parentId = $state('')
  let touched = $state(false)
  let saving = $state(false)
  let error = $state<ErrorDto | null>(null)

  // Each opening starts from the stored deck; a background refresh does not reset what is typed.
  $effect.pre(() => {
    if (!open) return
    untrack(() => {
      name = overview.deck.name
      description = overview.description
      parentId = overview.deck.parentId ?? ''
      touched = false
      error = null
    })
  })

  const excluded = $derived(new Set([overview.deck.id, ...overview.subdecks.map((deck) => deck.id)]))
  const parents = $derived(decks.filter((deck) => !excluded.has(deck.id)))
  const nameMissing = $derived(name.trim() === '')
  const moving = $derived((overview.deck.parentId ?? '') !== parentId)

  async function submit(event: SubmitEvent) {
    event.preventDefault()
    touched = true
    if (nameMissing || saving) return
    const patch: UpdateDeckInput['patch'] = {}
    if (name.trim() !== overview.deck.name) patch.name = name.trim()
    if (description.trim() !== overview.description) patch.description = description.trim()
    if (moving) patch.parentId = parentId || null
    if (Object.keys(patch).length === 0) {
      onclose()
      return
    }
    saving = true
    error = await onsave(patch)
    saving = false
  }
</script>

<Dialog {open} title={i18n.t('deck.edit.title')} onclose={() => !saving && onclose()}>
  <form id="{ids}-form" class="stack" novalidate onsubmit={submit}>
    <div class="field">
      <label for="{ids}-name">{i18n.t('deck.form.name')}</label>
      <input
        id="{ids}-name"
        class="input"
        bind:value={name}
        maxlength="120"
        autocomplete="off"
        aria-invalid={touched && nameMissing}
        aria-describedby={touched && nameMissing ? `${ids}-name-error` : undefined}
      />
      {#if touched && nameMissing}<p id="{ids}-name-error" class="field-error">{i18n.t('deck.form.nameRequired')}</p>{/if}
    </div>
    <div class="field">
      <label for="{ids}-description">{i18n.t('deck.edit.description')}</label>
      <textarea id="{ids}-description" class="input" rows="3" maxlength="2000" bind:value={description}></textarea>
    </div>
    <div class="field">
      <label for="{ids}-parent">{i18n.t('deck.edit.parent')}</label>
      <select id="{ids}-parent" class="input" bind:value={parentId}>
        <option value="">{i18n.t('deck.edit.topLevel')}</option>
        {#each parents as deck (deck.id)}<option value={deck.id}>{deck.name}</option>{/each}
      </select>
      {#if moving}
        <p class="muted" role="status">
          {i18n.t('deck.edit.moveScope', { decks: overview.subdecks.length + 1, cards: overview.deck.counts.total })}
        </p>
      {/if}
    </div>
    {#if error}<ErrorState {error} compact />{/if}
  </form>
  {#snippet actions()}
    <button type="button" class="btn" disabled={saving} onclick={onclose}>{i18n.t('common.cancel')}</button>
    <button type="submit" form="{ids}-form" class="btn btn-primary" disabled={saving} aria-busy={saving}>{i18n.t('common.save')}</button>
  {/snippet}
</Dialog>

<style>
  .field-error {
    color: var(--color-error);
    font-size: var(--text-sm);
  }
</style>
