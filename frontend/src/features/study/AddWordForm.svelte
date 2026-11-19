<script lang="ts" module>
  /** The deck chosen last for a new word; the next word from a sentence goes there too. */
  let lastDeckId: string | null = null
</script>

<script lang="ts">
  import { toErrorDto } from '../../api/errors'
  import type { EditorDeckOptionDto, ErrorDto, Id, TargetLanguage, WordLinkDto, WordMatchDto } from '../../api/types'
  import { useServices } from '../../app/context'
  import ErrorState from '../../components/ErrorState.svelte'
  import Skeleton from '../../components/Skeleton.svelte'
  import { newOperationId } from '../../services/operationId'

  interface Props {
    /** The unresolved phrase of the sentence; its base form pre-fills the new word. */
    link: WordLinkDto
    targetLanguage: TargetLanguage
    /** Deck of the sentence; the fallback when no word deck of the language exists. */
    sentenceDeckId: Id
    /** Links the phrase to the chosen or new word note; resolves to an error or `null`. */
    onlink: (wordNoteId: Id) => Promise<ErrorDto | null>
  }

  let { link, targetLanguage, sentenceDeckId, onlink }: Props = $props()
  const { api, i18n } = useServices()
  const ids = $props.id()

  let decks = $state<EditorDeckOptionDto[] | null>(null)
  let maxLength = $state(10_000)
  let matches = $state<WordMatchDto[]>([])
  let loadError = $state<ErrorDto | null>(null)
  let front = $state('')
  let back = $state('')
  let deckId = $state<Id>('')
  let saving = $state(false)
  let error = $state<ErrorDto | null>(null)
  /** Operation id of the pending save, kept for a retry so a lost response does not add the word twice. */
  let operationId: string | null = null

  /** Preference: the last deck chosen, a word deck (no word links) of the language, the sentence's deck. */
  function defaultDeck(options: EditorDeckOptionDto[]): Id {
    if (lastDeckId && options.some((deck) => deck.id === lastDeckId)) return lastDeckId
    return options.find((deck) => !deck.capabilities.wordLinks)?.id ?? options.find((deck) => deck.id === sentenceDeckId)?.id ?? options[0]?.id ?? ''
  }

  async function load() {
    loadError = null
    decks = null
    front = link.lemma ?? link.surface
    back = ''
    error = null
    operationId = null
    try {
      const [data, found] = await Promise.all([api.getEditorData(), api.searchWords({ targetLanguage, text: front, limit: 5 })])
      const options = data.decks.filter((deck) => deck.targetLanguage === targetLanguage)
      maxLength = data.maxFieldLength
      matches = found
      deckId = defaultDeck(options)
      decks = options
    } catch (caught) {
      loadError = toErrorDto(caught)
    }
  }

  // A new phrase starts a new form.
  $effect.pre(() => {
    void link.id
    void load()
  })

  const valid = $derived(front.trim().length > 0 && back.trim().length > 0 && deckId !== '')

  async function use(noteId: Id) {
    if (saving) return
    saving = true
    error = await onlink(noteId)
    saving = false
  }

  async function create(event: SubmitEvent) {
    event.preventDefault()
    if (!valid || saving) return
    saving = true
    error = null
    operationId ??= newOperationId()
    try {
      const details = await api.saveNote({
        operationId,
        noteId: null,
        expectedVersion: null,
        deckId,
        noteTypeKind: 'Basic',
        front: front.normalize('NFC').trim(),
        back: back.normalize('NFC').trim(),
        tags: [],
        manualAssociation: null,
        grammarTips: [],
        wordLinks: null,
      })
      lastDeckId = deckId
      error = await onlink(details.note.id)
    } catch (caught) {
      error = toErrorDto(caught)
    } finally {
      saving = false
    }
  }
</script>

{#if loadError}
  <ErrorState error={loadError} compact onretry={load} />
{:else if decks === null}
  <Skeleton lines={3} />
{:else}
  <div class="stack">
    {#if matches.length > 0}
      <section class="stack" aria-labelledby="{ids}-matches">
        <h3 id="{ids}-matches">{i18n.t('wordSheet.existing')}</h3>
        {#each matches as match (match.noteId)}
          <button type="button" class="btn match" disabled={saving} onclick={() => use(match.noteId)}>
            <span><strong lang={targetLanguage}>{match.lemma}</strong> — <span lang="pl">{match.translation}</span></span>
            <span class="muted">{match.deckName}</span>
          </button>
        {/each}
      </section>
    {/if}

    <form class="stack" aria-labelledby="{ids}-title" onsubmit={create}>
      <h3 id="{ids}-title">{i18n.t('wordSheet.createTitle')}</h3>
      <div class="field">
        <label for="{ids}-front">{i18n.t('wordSheet.wordField')}</label>
        <input id="{ids}-front" class="input" lang={targetLanguage} autocomplete="off" required maxlength={maxLength} bind:value={front} />
      </div>
      <div class="field">
        <label for="{ids}-back">{i18n.t('wordSheet.translationField')}</label>
        <input id="{ids}-back" class="input" lang="pl" autocomplete="off" required maxlength={maxLength} bind:value={back} />
      </div>
      <div class="field">
        <label for="{ids}-deck">{i18n.t('wordSheet.deckField')}</label>
        <select id="{ids}-deck" class="input" bind:value={deckId} required>
          {#each decks as deck (deck.id)}<option value={deck.id}>{deck.name}</option>{/each}
        </select>
      </div>
      {#if error}<ErrorState {error} compact />{/if}
      <button type="submit" class="btn btn-primary" disabled={!valid || saving} aria-busy={saving}>{i18n.t('wordSheet.create')}</button>
    </form>
  </div>
{/if}

<style>
  h3 {
    font-size: var(--text-md);
  }

  .match {
    flex-direction: column;
    align-items: flex-start;
    gap: 0;
    text-align: start;
  }

  .match .muted {
    font-size: var(--text-sm);
  }
</style>
