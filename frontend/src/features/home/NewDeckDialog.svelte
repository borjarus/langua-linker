<script lang="ts">
  import { toErrorDto } from '../../api/errors'
  import { DECK_TYPES, type DeckOverviewDto, type DeckType, type ErrorDto, type TargetLanguage } from '../../api/types'
  import { useServices } from '../../app/context'
  import Dialog from '../../components/Dialog.svelte'
  import ErrorState from '../../components/ErrorState.svelte'
  import { newOperationId } from '../../services/operationId'

  interface Props {
    open: boolean
    onclose: () => void
    oncreated: (deck: DeckOverviewDto) => void
  }

  let { open, onclose, oncreated }: Props = $props()
  const { api, i18n } = useServices()
  const ids = $props.id()

  let name = $state('')
  let targetLanguage = $state<TargetLanguage>('de')
  let deckType = $state<DeckType>('Linguistic')
  let touched = $state(false)
  let saving = $state(false)
  let error = $state<ErrorDto | null>(null)
  /** One intent = one operationId, so a retry after a timeout cannot create two decks. */
  let operationId = newOperationId()

  const nameMissing = $derived(name.trim() === '')

  function reset() {
    name = ''
    targetLanguage = 'de'
    deckType = 'Linguistic'
    touched = false
    error = null
    operationId = newOperationId()
  }

  function close() {
    if (saving) return
    reset()
    onclose()
  }

  async function submit(event: SubmitEvent) {
    event.preventDefault()
    touched = true
    if (nameMissing || saving) return
    saving = true
    error = null
    try {
      const deck = await api.createDeck({ operationId, name: name.trim(), parentId: null, targetLanguage, deckType })
      reset()
      oncreated(deck)
    } catch (caught) {
      error = toErrorDto(caught)
    } finally {
      saving = false
    }
  }
</script>

<Dialog {open} title={i18n.t('deck.newDeck')} onclose={close}>
  <form id="{ids}-form" class="stack" novalidate onsubmit={submit}>
    <div class="field">
      <label for="{ids}-name">{i18n.t('deck.form.name')}</label>
      <input
        id="{ids}-name"
        class="input"
        bind:value={name}
        required
        maxlength="120"
        autocomplete="off"
        aria-invalid={touched && nameMissing}
        aria-describedby={touched && nameMissing ? `${ids}-name-error` : undefined}
      />
      {#if touched && nameMissing}<p id="{ids}-name-error" class="field-error">{i18n.t('deck.form.nameRequired')}</p>{/if}
    </div>
    <div class="field">
      <label for="{ids}-language">{i18n.t('deck.form.targetLanguage')}</label>
      <select id="{ids}-language" class="input" bind:value={targetLanguage}>
        <option value="de">{i18n.t('deck.targetLanguages.de')}</option>
        <option value="en">{i18n.t('deck.targetLanguages.en')}</option>
      </select>
    </div>
    <fieldset class="types">
      <legend>{i18n.t('deck.form.type')}</legend>
      {#each DECK_TYPES as type (type)}
        <label class="type">
          <input type="radio" name="{ids}-type" value={type} bind:group={deckType} />
          <span class="type-text">
            <strong>{i18n.t(`deck.types.${type}`)}</strong>
            <span class="muted">{i18n.t(`deck.form.typeHints.${type}`)}</span>
          </span>
        </label>
      {/each}
    </fieldset>
    {#if error}<ErrorState {error} compact />{/if}
  </form>
  {#snippet actions()}
    <button type="button" class="btn" disabled={saving} onclick={close}>{i18n.t('common.cancel')}</button>
    <button type="submit" form="{ids}-form" class="btn btn-primary" disabled={saving} aria-busy={saving}>{i18n.t('deck.create')}</button>
  {/snippet}
</Dialog>

<style>
  .field-error {
    color: var(--color-error);
    font-size: var(--text-sm);
  }

  .types {
    display: grid;
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    border: 0;
  }

  legend {
    margin-bottom: var(--space-2);
    font-weight: var(--weight-medium);
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
