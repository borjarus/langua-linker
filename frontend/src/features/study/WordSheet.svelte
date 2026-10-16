<script lang="ts">
  import type { ErrorDto, Id, WordLinkDto } from '../../api/types'
  import { useServices } from '../../app/context'
  import { href } from '../../app/routes'
  import Chip from '../../components/Chip.svelte'
  import Dialog from '../../components/Dialog.svelte'
  import ErrorState from '../../components/ErrorState.svelte'
  import WordDetails from './WordDetails.svelte'

  interface Props {
    link: WordLinkDto | null
    /** Deck that receives a new word note ("Add this word"). */
    deckId: Id
    onclose: () => void
    /** Saves the user's choice for an ambiguous link; resolves to an error or `null`. */
    onresolve: (link: WordLinkDto, wordNoteId: Id) => Promise<ErrorDto | null>
    /** Called before leaving study for the editor, so the leave guard does not ask. */
    onleave: () => void
  }

  let { link, deckId, onclose, onresolve, onleave }: Props = $props()
  const { i18n } = useServices()
  const groupName = $props.id()

  let choice = $state<Id | null>(null)
  let saving = $state(false)
  let error = $state<ErrorDto | null>(null)
  // A new link resets the pending choice.
  $effect.pre(() => {
    void link?.id
    choice = null
    error = null
  })

  const phrase = $derived(link ? /\s/u.test(link.surface.trim()) : false)

  async function save() {
    if (!link || !choice || saving) return
    saving = true
    error = await onresolve(link, choice)
    saving = false
  }
</script>

<Dialog open={link !== null} title={link?.surface ?? ''} {onclose} variant="sheet">
  {#if link}
    <div class="stack">
      <div class="row">
        <Chip label={i18n.t(phrase ? 'wordSheet.phrase' : 'wordSheet.word')} tone="info" />
        <Chip label={i18n.t(`wordLink.resolution.${link.resolution}`)} tone={link.resolution === 'ambiguous' ? 'warning' : link.resolution === 'unresolved' ? 'neutral' : 'success'} />
      </div>

      {#if link.resolution === 'ambiguous'}
        <fieldset class="choices">
          <legend>{i18n.t('wordSheet.chooseTitle')}</legend>
          {#each link.candidates as candidate (candidate.noteId)}
            <label class="choice">
              <input type="radio" name={groupName} value={candidate.noteId} bind:group={choice} />
              <span><strong>{candidate.lemma}</strong> — {candidate.translation}</span>
            </label>
          {/each}
        </fieldset>
        {#if error}<ErrorState {error} compact onretry={save} />{/if}
        <button type="button" class="btn btn-primary" disabled={!choice || saving} aria-busy={saving} onclick={save}>
          {i18n.t('wordSheet.choose')}
        </button>
      {:else if link.wordNoteId}
        {#if link.resolution === 'userOverridden'}<p class="muted" role="status">{i18n.t('wordSheet.choiceSaved')}</p>{/if}
        <WordDetails noteId={link.wordNoteId} lemma={link.lemma} />
      {:else}
        <p>{i18n.t('wordSheet.notInDecks')}</p>
        {#if link.lemma && link.lemma !== link.surface}<p class="muted">{i18n.t('wordSheet.baseForm', { lemma: link.lemma })}</p>{/if}
        <a class="btn" href={href({ name: 'editor', query: { deckId } })} onclick={onleave}>{i18n.t('wordSheet.addWord')}</a>
      {/if}
    </div>
  {/if}
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
    align-items: center;
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

  input {
    width: 1.25rem;
    height: 1.25rem;
    flex: none;
  }
</style>
