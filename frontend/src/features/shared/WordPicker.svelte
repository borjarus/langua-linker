<script lang="ts">
  import { untrack } from 'svelte'
  import { toErrorDto } from '../../api/errors'
  import type { ErrorDto, Id, TargetLanguage, WordMatchDto } from '../../api/types'
  import { useServices } from '../../app/context'
  import Chip from '../../components/Chip.svelte'
  import Dialog from '../../components/Dialog.svelte'
  import ErrorState from '../../components/ErrorState.svelte'
  import SearchInput from '../../components/SearchInput.svelte'
  import Skeleton from '../../components/Skeleton.svelte'

  interface Props {
    open: boolean
    /** The selected phrase; it pre-fills the search. */
    surface: string
    targetLanguage: TargetLanguage
    /** Offers "keep without a word" (a marked but unresolved phrase). */
    allowUnresolved?: boolean
    /** `null` = keep the phrase without a word. A word is only ever linked by the user's choice. */
    onpick: (match: WordMatchDto | null) => void
    onclose: () => void
  }

  let { open, surface, targetLanguage, allowUnresolved = true, onpick, onclose }: Props = $props()
  const { api, i18n } = useServices()
  const groupName = $props.id()

  let text = $state('')
  let results = $state<WordMatchDto[] | null>(null)
  let error = $state<ErrorDto | null>(null)
  let choice = $state<Id | null>(null)
  let run = 0

  // Opening the picker searches for the selected phrase (a read, never a link).
  $effect.pre(() => {
    if (!open) return
    const initial = surface
    untrack(() => void search(initial))
  })

  async function search(value: string) {
    const current = ++run
    text = value
    results = null
    error = null
    choice = null
    if (!value.trim()) {
      results = []
      return
    }
    try {
      const found = await api.searchWords({ targetLanguage, text: value, limit: 20 })
      if (current === run) results = found
    } catch (caught) {
      if (current === run) error = toErrorDto(caught)
    }
  }

  function confirm() {
    const match = results?.find((item) => item.noteId === choice)
    if (match) onpick(match)
  }
</script>

<Dialog {open} title={i18n.t('wordPicker.title', { surface })} {onclose} variant="sheet">
  <div class="stack">
    <p class="muted">{i18n.t('wordPicker.hint')}</p>
    <SearchInput value={text} label={i18n.t('wordPicker.search')} onsearch={search} />
    {#if error}
      <ErrorState {error} compact onretry={() => search(text)} />
    {:else if results === null}
      <Skeleton lines={2} />
    {:else if results.length === 0}
      <p role="status">{i18n.t('wordPicker.noMatch', { text })}</p>
    {:else}
      <fieldset class="choices">
        <legend>{i18n.t('wordPicker.choose')}</legend>
        {#each results as match (match.noteId)}
          <label class="choice">
            <input type="radio" name={groupName} value={match.noteId} bind:group={choice} />
            <span class="choice-text">
              <span><strong dir="auto">{match.lemma}</strong> — <span dir="auto">{match.translation}</span></span>
              <span class="row">
                <Chip label={match.deckName} />
                <Chip label={i18n.t(`wordPicker.match.${match.match}`)} tone={match.match === 'exact' ? 'success' : 'info'} />
              </span>
            </span>
          </label>
        {/each}
      </fieldset>
    {/if}
  </div>
  {#snippet actions()}
    <button type="button" class="btn" onclick={onclose}>{i18n.t('common.cancel')}</button>
    {#if allowUnresolved}
      <button type="button" class="btn" onclick={() => onpick(null)}>{i18n.t('wordPicker.keepUnlinked')}</button>
    {/if}
    <button type="button" class="btn btn-primary" disabled={!choice} onclick={confirm}>{i18n.t('wordPicker.link')}</button>
  {/snippet}
</Dialog>

<style>
  .choices {
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

  .choice {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    min-height: var(--touch-target);
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    cursor: pointer;
  }

  .choice:has(input:checked) {
    border-color: var(--color-primary);
    background: var(--color-primary-soft);
  }

  .choice-text {
    display: grid;
    gap: var(--space-1);
    min-width: 0;
    overflow-wrap: anywhere;
  }

  input {
    width: 1.25rem;
    height: 1.25rem;
    margin-top: 0.15rem;
    flex: none;
  }
</style>
