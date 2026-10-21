<script lang="ts">
  import { tick, untrack } from 'svelte'
  import { toErrorDto } from '../../api/errors'
  import type { EditorDeckOptionDto, ErrorDto, NoteDetailsDto, WordMatchDto } from '../../api/types'
  import { useServices } from '../../app/context'
  import { href, type Route } from '../../app/routes'
  import Chip from '../../components/Chip.svelte'
  import Dialog from '../../components/Dialog.svelte'
  import ErrorState from '../../components/ErrorState.svelte'
  import Icon from '../../components/Icon.svelte'
  import Skeleton from '../../components/Skeleton.svelte'
  import { newOperationId } from '../../services/operationId'
  import Screen from '../shared/Screen.svelte'
  import WordPicker from '../shared/WordPicker.svelte'
  import { EditorController } from './editorController.svelte'

  interface Props {
    deckId: string | null
    /** Edits the note of this card; absent = a new note. */
    cardId: string | null
  }

  let { deckId, cardId }: Props = $props()
  const { api, i18n, router, toasts } = useServices()
  const editor = new EditorController({ api })
  const ids = $props.id()

  let loadError = $state<ErrorDto | null>(null)
  let loaded = $state(false)
  let frontEl = $state<HTMLTextAreaElement | null>(null)
  let selection = $state<{ start: number; end: number } | null>(null)
  let picking = $state<{ start: number; end: number; surface: string } | null>(null)
  let linkMessage = $state<string | null>(null)
  let tagDraft = $state('')
  let tipDraft = $state('')
  let resolving = $state(false)
  /** Set right before leaving after a save, so the draft guard does not ask. */
  let leaving = false

  async function load() {
    loadError = null
    loaded = false
    try {
      const [data, details] = await Promise.all([api.getEditorData(), cardId ? api.getNoteDetails(cardId) : Promise.resolve(null)])
      editor.init(data, details, deckId)
      loaded = true
    } catch (caught) {
      loadError = toErrorDto(caught)
    }
  }

  // Loading is a read bound to the route parameters.
  $effect(() => {
    void cardId
    void deckId
    untrack(() => void load())
  })

  // Duplicate warning: a debounced read after the front or the deck changes.
  $effect(() => {
    void editor.front
    void editor.deckId
    if (!loaded) return
    const timer = setTimeout(() => void editor.checkDuplicates(), 300)
    return () => clearTimeout(timer)
  })

  // Draft guard on back, navigation and closing the window (demo drafts live in memory only).
  let pendingLeave = $state<((leave: boolean) => void) | null>(null)
  $effect(() => {
    const removeGuard = router.addGuard({
      isBlocking: () => !leaving && editor.dirty,
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

  const depth = (deck: EditorDeckOptionDto, decks: EditorDeckOptionDto[]): number => {
    let level = 0
    let parent = deck.parentId
    while (parent && level < 8) {
      level += 1
      parent = decks.find((candidate) => candidate.id === parent)?.parentId ?? null
    }
    return level
  }

  function trackSelection() {
    if (!frontEl) return
    const { selectionStart: start, selectionEnd: end } = frontEl
    selection = end > start ? { start, end } : null
  }

  function openPicker() {
    if (!selection) return
    const surface = editor.front.slice(selection.start, selection.end).trim()
    if (!surface) return
    linkMessage = null
    picking = { ...selection, surface }
  }

  function pick(match: WordMatchDto | null) {
    const range = picking
    picking = null
    if (!range) return
    const added = editor.addLink(range.start, range.end, match)
    linkMessage = added ? i18n.t(match ? 'editor.links.linked' : 'editor.links.markedUnlinked', { surface: range.surface }) : i18n.t('editor.links.overlap')
    selection = null
  }

  function addTag(event?: Event) {
    event?.preventDefault()
    if (editor.addTag(tagDraft)) tagDraft = ''
  }

  function addTip() {
    if (!tipDraft.trim()) return
    editor.newTips = [...editor.newTips, tipDraft.trim()]
    tipDraft = ''
  }

  async function afterSave(saved: NoteDetailsDto) {
    if (editor.generateAfterSave && editor.capabilities.associations) {
      try {
        await api.generateAssociation({ operationId: newOperationId(), noteId: saved.note.id, variants: 1 })
        toasts.show(i18n.t('editor.generationStarted'), 'info')
      } catch (caught) {
        toasts.show(i18n.tDynamic(toErrorDto(caught).messageKey, 'errors.INTERNAL'), 'error')
      }
    }
  }

  async function save(mode: 'close' | 'next') {
    const wasNew = editor.isNew
    const saved = await editor.save()
    if (!saved) {
      await tick()
      document.querySelector<HTMLElement>('[aria-invalid="true"]')?.focus()
      return
    }
    await afterSave(saved)
    if (mode === 'next') {
      toasts.show(i18n.t('editor.savedNext'), 'success')
      editor.resetForNext()
      selection = null
      linkMessage = null
      await tick()
      frontEl?.focus()
      return
    }
    toasts.show(i18n.t('editor.saved'), 'success')
    leaving = true
    const first = saved.cards[0]
    // Editing returns to where it was opened (study, details); a new note opens its details.
    if (!wasNew) router.back()
    else if (first) await router.replace({ name: 'card', params: { id: first.id } })
    else await router.replace({ name: 'browser' })
  }

  async function resolve(keep: boolean) {
    resolving = true
    const error = await editor.resolveConflict(keep)
    resolving = false
    if (error) toasts.show(i18n.tDynamic(error.messageKey, 'errors.INTERNAL'), 'error')
    else toasts.show(i18n.t(keep ? 'editor.conflict.kept' : 'editor.conflict.reloaded'), 'info')
  }

  const problemText = (problem: 'required' | 'tooLong' | null, field: 'front' | 'back') =>
    problem === 'required' ? i18n.t(`editor.errors.${field}Required`) : problem === 'tooLong' ? i18n.t('editor.errors.tooLong', { max: editor.maxLength }) : null
  const frontProblem = $derived(editor.touched ? problemText(editor.problems.front, 'front') : null)
  const backProblem = $derived(editor.touched ? problemText(editor.problems.back, 'back') : null)
  const language = $derived(editor.deck ? i18n.t(`deck.targetLanguages.${editor.deck.targetLanguage}`) : '')
</script>

<Screen title={cardId ? i18n.t('editor.editNote') : i18n.t('editor.newNote')} back notice={i18n.t('editor.demoDraft')}>
  {#if loadError}
    <ErrorState error={loadError} onretry={load} />
  {:else if !loaded || !editor.data}
    <Skeleton lines={6} />
  {:else if editor.data.decks.length === 0}
    <p>{i18n.t('editor.noDecks')}</p>
    <a class="btn btn-primary" href="#/home">{i18n.t('common.goHome')}</a>
  {:else}
    {@const data = editor.data}
    <form id="{ids}-form" class="stack" novalidate onsubmit={(event) => { event.preventDefault(); void save('close') }}>
      <section class="surface stack" aria-labelledby="{ids}-where">
        <h2 id="{ids}-where">{i18n.t('editor.where')}</h2>
        <div class="field">
          <label for="{ids}-deck">{i18n.t('editor.deck')}</label>
          <select id="{ids}-deck" class="input" bind:value={editor.deckId}>
            {#each data.decks as deck (deck.id)}
              <option value={deck.id}>{'· '.repeat(depth(deck, data.decks))}{deck.name}</option>
            {/each}
          </select>
          {#if editor.deck}
            <p class="hint muted">{i18n.t(`deck.form.typeHints.${editor.deck.deckType}`)}</p>
          {/if}
        </div>
        <fieldset class="types">
          <legend>{i18n.t('editor.noteType')}</legend>
          {#each data.noteTypes as type (type.kind)}
            <label class="type">
              <input type="radio" name="{ids}-type" value={type.kind} bind:group={editor.noteTypeKind} />
              <span class="type-text">
                <strong>{i18n.t(`editor.noteTypes.${type.kind}`)}</strong>
                <span class="muted">{i18n.t(`editor.noteTypeHints.${type.kind}`)}</span>
              </span>
            </label>
          {/each}
        </fieldset>
      </section>

      <section class="surface stack" aria-labelledby="{ids}-content">
        <h2 id="{ids}-content">{i18n.t('editor.content')}</h2>
        <div class="field">
          <label for="{ids}-front">{i18n.t(editor.noteTypeKind === 'Sentence' ? 'editor.sentence' : 'editor.front', { language })}</label>
          <textarea
            id="{ids}-front"
            class="input"
            rows="2"
            lang={editor.deck?.targetLanguage}
            bind:this={frontEl}
            bind:value={editor.front}
            onselect={trackSelection}
            onkeyup={trackSelection}
            onpointerup={trackSelection}
            aria-invalid={frontProblem !== null}
            aria-describedby={frontProblem ? `${ids}-front-error` : editor.duplicates.length > 0 ? `${ids}-duplicates` : undefined}
          ></textarea>
          {#if frontProblem}<p id="{ids}-front-error" class="field-error">{frontProblem}</p>{/if}
          {#if editor.duplicates.length > 0}
            <div id="{ids}-duplicates" class="warning" role="status">
              <p class="row"><Icon name="alert" />{i18n.t('editor.duplicates', { count: editor.duplicates.length })}</p>
              <ul>
                {#each editor.duplicates as duplicate (duplicate.noteId)}
                  <li>
                    <a href={href({ name: 'browser', query: { q: `"${duplicate.front}"` } })} dir="auto">{duplicate.front}</a>
                    <span class="muted">— {duplicate.deckName}</span>
                  </li>
                {/each}
              </ul>
              <p class="muted">{i18n.t('editor.duplicatesHint')}</p>
            </div>
          {/if}
        </div>

        {#if editor.linksEnabled}
          <div class="links stack" role="group" aria-labelledby="{ids}-links">
            <h3 id="{ids}-links">{i18n.t('editor.links.title')}</h3>
            <p class="muted">{i18n.t('editor.links.hint')}</p>
            <button type="button" class="btn" disabled={!selection} onclick={openPicker}>
              <Icon name="link" />{i18n.t('editor.links.linkSelection')}
            </button>
            {#if linkMessage}<p role="status" class="muted">{linkMessage}</p>{/if}
            {#if editor.linkStates.length > 0}
              <ul class="link-list">
                {#each editor.linkStates as state (state.link.key)}
                  <li class="row link" class:lost={state.status === 'lost'}>
                    <span class="link-text">
                      <strong dir="auto">{state.link.surface}</strong>
                      {#if state.link.lemma}<span dir="auto">→ {state.link.lemma}</span>{/if}
                    </span>
                    <Chip
                      label={i18n.t(state.status === 'lost' ? 'editor.links.lost' : state.link.wordNoteId ? 'editor.links.withWord' : 'editor.links.withoutWord')}
                      tone={state.status === 'lost' ? 'error' : state.link.wordNoteId ? 'success' : 'neutral'}
                    />
                    {#if state.status === 'moved'}<Chip label={i18n.t('editor.links.moved')} tone="info" />{/if}
                    <button type="button" class="btn btn-ghost" onclick={() => editor.removeLink(state.link.key)}>
                      <Icon name="close" />{i18n.t('editor.links.remove', { surface: state.link.surface })}
                    </button>
                  </li>
                {/each}
              </ul>
              {#if editor.linkStates.some((state) => state.status === 'lost')}
                <p class="field-error" role="status">{i18n.t('editor.links.lostWarning')}</p>
              {/if}
            {/if}
          </div>
        {/if}

        <div class="field">
          <label for="{ids}-back">{i18n.t('editor.back')}</label>
          <textarea
            id="{ids}-back"
            class="input"
            rows="2"
            lang="pl"
            bind:value={editor.back}
            aria-invalid={backProblem !== null}
            aria-describedby={backProblem ? `${ids}-back-error` : undefined}
          ></textarea>
          {#if backProblem}<p id="{ids}-back-error" class="field-error">{backProblem}</p>{/if}
        </div>

        <div class="field">
          <label for="{ids}-tag">{i18n.t('editor.tags')}</label>
          <div class="row tag-entry">
            <input
              id="{ids}-tag"
              class="input"
              list="{ids}-tag-list"
              autocomplete="off"
              bind:value={tagDraft}
              onkeydown={(event) => {
                if (event.key === 'Enter') addTag(event)
              }}
            />
            <button type="button" class="btn" disabled={!tagDraft.trim()} onclick={() => addTag()}><Icon name="plus" />{i18n.t('editor.addTag')}</button>
          </div>
          <datalist id="{ids}-tag-list">
            {#each data.tags as tag (tag)}<option value={tag}></option>{/each}
          </datalist>
          {#if editor.tags.length > 0}
            <ul class="row tags" aria-label={i18n.t('editor.tagsList')}>
              {#each editor.tags as tag (tag)}
                <li>
                  <button type="button" class="chip-button" aria-label={i18n.t('editor.removeTag', { tag })} onclick={() => editor.removeTag(tag)}>
                    <Icon name="tag" size={16} /><span>{tag}</span><Icon name="close" size={16} />
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        </div>
      </section>

      {#if editor.capabilities.associations}
        <section class="surface stack" aria-labelledby="{ids}-associations">
          <h2 id="{ids}-associations">{i18n.t('association.title')}</h2>
          {#if editor.details && editor.details.associations.length > 0}
            <p class="muted">{i18n.t('editor.existingAssociations', { count: editor.details.associations.length })}</p>
          {/if}
          <div class="field">
            <label for="{ids}-association">{i18n.t('editor.manualAssociation')}</label>
            <textarea id="{ids}-association" class="input" rows="2" maxlength="500" bind:value={editor.manualAssociation}></textarea>
          </div>
          <label class="check">
            <input type="checkbox" bind:checked={editor.generateAfterSave} />
            <span>{i18n.t('editor.generateAfterSave')}</span>
          </label>
          {#if editor.details && editor.details.cards[0]}
            <a class="btn btn-ghost" href={href({ name: 'card', params: { id: editor.details.cards[0].id } })}>{i18n.t('editor.manageOnDetails')}</a>
          {/if}
        </section>
      {/if}

      {#if editor.capabilities.grammarTips}
        <section class="surface stack" aria-labelledby="{ids}-tips">
          <h2 id="{ids}-tips">{i18n.t('grammar.title')}</h2>
          {#if editor.details && editor.details.grammarTips.length > 0}
            <p class="muted">{i18n.t('editor.existingTips', { count: editor.details.grammarTips.length })}</p>
          {/if}
          {#if editor.newTips.length > 0}
            <ol class="new-tips">
              {#each editor.newTips as tip, index (index)}
                <li class="row">
                  <span class="tip-text">{tip}</span>
                  <button type="button" class="btn btn-ghost" onclick={() => (editor.newTips = editor.newTips.filter((_, other) => other !== index))}>
                    <Icon name="close" />{i18n.t('editor.removeTip', { position: index + 1 })}
                  </button>
                </li>
              {/each}
            </ol>
          {/if}
          <div class="field">
            <label for="{ids}-tip">{i18n.t('editor.newTip')}</label>
            <textarea id="{ids}-tip" class="input" rows="2" maxlength="500" bind:value={tipDraft}></textarea>
          </div>
          <button type="button" class="btn add-tip" disabled={!tipDraft.trim()} onclick={addTip}><Icon name="plus" />{i18n.t('editor.addTip')}</button>
        </section>
      {/if}

      <section class="surface stack" aria-labelledby="{ids}-preview">
        <h2 id="{ids}-preview">{i18n.t('editor.preview.title', { count: editor.templates.filter((item) => item.status !== 'archived').length })}</h2>
        <ul class="previews">
          {#each editor.templates as template (template.ordinal)}
            <li class="preview" class:archived={template.status === 'archived'}>
              <div class="row">
                <strong>{i18n.t(`card.direction.${template.direction}`, { language })}</strong>
                <Chip label={i18n.t(`editor.preview.status.${template.status}`)} tone={template.status === 'archived' ? 'warning' : template.status === 'kept' ? 'neutral' : 'success'} />
              </div>
              <p class="qa"><span class="muted">{i18n.t('study.front')}:</span> <span dir="auto">{template.question || i18n.t('editor.preview.empty')}</span></p>
              <p class="qa"><span class="muted">{i18n.t('study.back')}:</span> <span dir="auto">{template.answer || i18n.t('editor.preview.empty')}</span></p>
            </li>
          {/each}
        </ul>
        {#if editor.templates.some((item) => item.status === 'archived')}
          <p class="muted" role="status">{i18n.t('editor.preview.archivedNote')}</p>
        {/if}
      </section>

      {#if editor.error}<ErrorState error={editor.error} compact onretry={() => save('close')} />{/if}
      {#if editor.touched && !editor.valid}<p class="field-error" role="alert">{i18n.t('editor.errors.summary')}</p>{/if}

      <div class="row form-actions">
        <button type="submit" class="btn btn-primary" disabled={editor.saving} aria-busy={editor.saving}>
          <Icon name="check" />{i18n.t('common.save')}
        </button>
        <button type="button" class="btn" disabled={editor.saving} onclick={() => save('next')}>
          <Icon name="plus" />{i18n.t('editor.saveAndNext')}
        </button>
        <button type="button" class="btn btn-ghost" disabled={editor.saving} onclick={() => router.back()}>{i18n.t('common.cancel')}</button>
      </div>
    </form>

    <WordPicker
      open={picking !== null}
      surface={picking?.surface ?? ''}
      targetLanguage={editor.deck?.targetLanguage ?? 'de'}
      onpick={pick}
      onclose={() => (picking = null)}
    />
  {/if}
</Screen>

<!-- Closing keeps the draft; the next save reports the conflict again. -->
<Dialog open={editor.conflict !== null} title={i18n.t('editor.conflict.title')} onclose={() => (editor.conflict = null)} dismissible={false}>
  <p>{i18n.t('editor.conflict.body')}</p>
  {#snippet actions()}
    <button type="button" class="btn" disabled={resolving} onclick={() => resolve(false)}>{i18n.t('editor.conflict.reload')}</button>
    <button type="button" class="btn btn-primary" disabled={resolving} onclick={() => resolve(true)}>{i18n.t('editor.conflict.keep')}</button>
  {/snippet}
</Dialog>

<Dialog open={pendingLeave !== null} title={i18n.t('guard.title')} onclose={() => decide(false)} dismissible={false}>
  <p>{i18n.t('guard.body')}</p>
  {#snippet actions()}
    <button type="button" class="btn" data-autofocus onclick={() => decide(false)}>{i18n.t('guard.stay')}</button>
    <button type="button" class="btn btn-danger" onclick={() => decide(true)}>{i18n.t('guard.leave')}</button>
  {/snippet}
</Dialog>

<style>
  h2 {
    font-size: var(--text-lg);
  }

  h3 {
    font-size: var(--text-base);
  }

  .hint {
    font-size: var(--text-sm);
  }

  .field-error {
    color: var(--color-error);
    font-size: var(--text-sm);
  }

  .warning {
    display: grid;
    gap: var(--space-1);
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--color-warning);
    border-radius: var(--radius-sm);
    background: var(--color-warning-soft);
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }

  .warning ul {
    margin: 0;
    padding-inline-start: var(--space-5);
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

  .type input,
  .check input {
    width: 1.25rem;
    height: 1.25rem;
    margin-top: 0.15rem;
    flex: none;
  }

  .type-text {
    display: grid;
    font-size: var(--text-sm);
  }

  .check {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    min-height: var(--touch-target);
    cursor: pointer;
  }

  .links {
    padding: var(--space-3);
    border: 1px dashed var(--color-border-strong);
    border-radius: var(--radius-sm);
  }

  .link-list,
  .previews,
  .tags {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .link-list,
  .previews {
    display: grid;
    gap: var(--space-2);
  }

  .link {
    gap: var(--space-2);
  }

  .link-text {
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .lost .link-text {
    text-decoration: line-through;
  }

  /* flex-basis, not min-width: WebKit resolves min(100%, …) to the rem value when sizing. */
  .tag-entry .input {
    flex: 1 1 10rem;
    min-width: 0;
  }

  .tags {
    gap: var(--space-2);
  }

  .chip-button {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    min-height: var(--touch-target);
    padding: var(--space-1) var(--space-3);
    border: 1px solid var(--color-info);
    border-radius: var(--radius-pill);
    background: var(--color-info-soft);
    color: var(--color-text);
    font: inherit;
    font-size: var(--text-sm);
    cursor: pointer;
    overflow-wrap: anywhere;
  }

  .new-tips {
    display: grid;
    gap: var(--space-2);
    margin: 0;
    padding-inline-start: var(--space-5);
  }

  .tip-text {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .add-tip {
    justify-self: start;
  }

  .preview {
    display: grid;
    gap: var(--space-1);
    padding: var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
  }

  .preview.archived {
    border-style: dashed;
    opacity: 0.85;
  }

  .qa {
    overflow-wrap: anywhere;
  }
</style>
