<script lang="ts">
  import { toErrorDto } from '../../api/errors'
  import type { AssociationDto, CardDetailsDto, GrammarTipDto, NoteContentChange, NoteDetailsDto, WordLinkDto, WordMatchDto } from '../../api/types'
  import { useServices } from '../../app/context'
  import { href } from '../../app/routes'
  import Chip from '../../components/Chip.svelte'
  import Dialog from '../../components/Dialog.svelte'
  import Icon from '../../components/Icon.svelte'
  import LinkedText from '../../components/LinkedText.svelte'
  import type { TranslationKey } from '../../locales/types'
  import { newOperationId } from '../../services/operationId'
  import { useQuery } from '../../services/query.svelte'
  import QueryView from '../shared/QueryView.svelte'
  import Screen from '../shared/Screen.svelte'
  import WordPicker from '../shared/WordPicker.svelte'
  import { AssociationGenerator } from '../study/associationGenerator.svelte'
  import AssociationEditor from './AssociationEditor.svelte'
  import CardSchedule, { type ScheduleAction } from './CardSchedule.svelte'
  import LinkList from './LinkList.svelte'
  import TipEditor from './TipEditor.svelte'

  interface Props {
    id: string
  }

  let { id }: Props = $props()
  const { api, i18n, queries, router, toasts } = useServices()
  const details = useQuery(queries, () => ({ key: ['note', id], fetch: () => api.getNoteDetails(id) }))
  const generator = new AssociationGenerator({ api, onupdate: () => void details.refresh() })
  const ids = $props.id()

  let busy = $state(false)
  let picking = $state<WordLinkDto | null>(null)
  /** A pending destructive decision; every one of them is confirmed in the same dialog. */
  let confirming = $state<{ title: TranslationKey; body: string; confirm: TranslationKey; run: () => Promise<void> } | null>(null)

  $effect(() => api.onJobEvent((event) => generator.handleEvent(event)))

  const fail = (caught: unknown) => toasts.show(i18n.tDynamic(toErrorDto(caught).messageKey, 'errors.INTERNAL'), 'error')

  /** One explicit mutation at a time; the details are re-read from the backend afterwards. */
  async function mutate(run: () => Promise<unknown>, done: TranslationKey): Promise<boolean> {
    if (busy) return false
    busy = true
    try {
      await run()
      await details.refresh()
      toasts.show(i18n.t(done), 'success')
      return true
    } catch (caught) {
      fail(caught)
      return false
    } finally {
      busy = false
    }
  }

  const DONE: Record<NoteContentChange['type'], TranslationKey> = {
    addTip: 'card.done.tipAdded',
    editTip: 'card.done.tipSaved',
    deleteTip: 'card.done.tipDeleted',
    reorderTips: 'card.done.tipsReordered',
    addAssociation: 'card.done.associationAdded',
    editAssociation: 'card.done.associationSaved',
    deleteAssociation: 'card.done.associationDeleted',
    unlinkWord: 'card.done.unlinked',
  }

  function change(data: NoteDetailsDto, next: NoteContentChange): Promise<boolean> {
    return mutate(() => api.updateNoteContent({ operationId: newOperationId(), noteId: data.note.id, change: next }), DONE[next.type])
  }

  function question(data: NoteDetailsDto, card: CardDetailsDto): string {
    return card.direction === 'forward' ? data.note.front : data.note.back
  }

  function cardAction(data: NoteDetailsDto, card: CardDetailsDto, action: ScheduleAction) {
    const run = () => api.applyCardAction({ operationId: newOperationId(), cardId: card.id, expectedVersion: card.version, action: action.type, flag: action.type === 'flag' ? action.flag : undefined })
    if (action.type !== 'delete') {
      void mutate(run, action.type === 'flag' ? 'study.done.flag' : action.type === 'suspend' ? 'study.done.suspend' : 'card.done.unsuspended')
      return
    }
    confirming = {
      title: 'card.confirm.deleteCardTitle',
      body: i18n.t(data.cards.length > 1 ? 'card.confirm.deleteCardBody' : 'card.confirm.deleteLastCardBody'),
      confirm: 'card.deleteCard',
      run: async () => {
        const other = data.cards.find((candidate) => candidate.id !== card.id)
        if (!(await mutateQuietly(run))) return
        toasts.show(i18n.t('study.done.delete'), 'success')
        // The opened card is gone: show the note through its other card, or leave for the browser.
        if (card.id === id) await router.replace(other ? { name: 'card', params: { id: other.id } } : { name: 'browser' })
        else await details.refresh()
      },
    }
  }

  async function mutateQuietly(run: () => Promise<unknown>): Promise<boolean> {
    busy = true
    try {
      await run()
      return true
    } catch (caught) {
      fail(caught)
      return false
    } finally {
      busy = false
    }
  }

  function deleteNote(data: NoteDetailsDto) {
    confirming = {
      title: 'card.confirm.deleteNoteTitle',
      body: i18n.t('card.confirm.deleteNoteBody', { count: data.cards.length }),
      confirm: 'card.deleteNote',
      run: async () => {
        if (!(await mutateQuietly(() => api.deleteNote({ operationId: newOperationId(), noteId: data.note.id, expectedVersion: data.note.version })))) return
        toasts.show(i18n.t('card.done.noteDeleted'), 'success')
        await router.replace({ name: 'browser' })
      },
    }
  }

  function deleteTip(data: NoteDetailsDto, tip: GrammarTipDto) {
    confirming = { title: 'card.confirm.deleteTipTitle', body: tip.text, confirm: 'card.delete', run: async () => void (await change(data, { type: 'deleteTip', tipId: tip.id })) }
  }

  function deleteAssociation(data: NoteDetailsDto, association: AssociationDto) {
    confirming = {
      title: 'card.confirm.deleteAssociationTitle',
      body: association.text || i18n.t(`association.status.${association.status}`),
      confirm: 'card.delete',
      run: async () => void (await change(data, { type: 'deleteAssociation', associationId: association.id })),
    }
  }

  async function runConfirmed() {
    const pending = confirming
    confirming = null
    await pending?.run()
  }

  async function pick(match: WordMatchDto | null) {
    const link = picking
    picking = null
    if (!link || !match) return
    await mutate(() => api.resolveWordLink({ operationId: newOperationId(), linkId: link.id, wordNoteId: match.noteId }), 'wordSheet.choiceSaved')
  }

  async function favorite(data: NoteDetailsDto, association: AssociationDto) {
    await mutate(() => api.setFavoriteAssociation({ operationId: newOperationId(), noteId: data.note.id, associationId: association.id }), 'association.favoriteSaved')
  }

  async function generate(data: NoteDetailsDto) {
    await generator.generate(data.note.id)
    const error = generator.errorFor(data.note.id)
    if (error) toasts.show(i18n.tDynamic(error.messageKey, 'errors.INTERNAL'), 'error')
    else if (generator.isQueued(data.note.id)) toasts.show(i18n.t('association.queued'), 'info')
  }
</script>

<Screen title={i18n.t('routes.card')} back notice={i18n.t('common.prototypeManagement')}>
  <QueryView query={details}>
    {#snippet ready(data)}
      {@const sentence = data.note.noteTypeKind === 'Sentence'}
      <section class="surface stack note" aria-label={i18n.t('card.note')}>
        <p class="front" lang={data.targetLanguage}>
          {#if sentence && data.capabilities.wordLinks}
            <LinkedText text={data.note.front} links={data.wordLinks} lang={data.targetLanguage} onselect={(link) => (picking = link)} />
          {:else}
            <span dir="auto">{data.note.front}</span>
          {/if}
        </p>
        <p dir="auto" lang="pl">{data.note.back}</p>
        <div class="row">
          <Chip label={i18n.t(`editor.noteTypes.${data.note.noteTypeKind}`)} />
          {#each data.note.tags as tag (tag)}<Chip label={tag} tone="info" icon="tag" />{/each}
        </div>
        <p class="muted meta">
          {i18n.t('card.updated', { date: i18n.formatDate(data.note.updatedAt) })}
        </p>
        <div class="row">
          <a class="btn btn-primary" href={href({ name: 'editor', query: { cardId: id } })}><Icon name="edit" />{i18n.t('card.editNote')}</a>
          <button type="button" class="btn btn-ghost danger" disabled={busy} onclick={() => deleteNote(data)}><Icon name="trash" />{i18n.t('card.deleteNote')}</button>
        </div>
      </section>

      <section class="stack" aria-labelledby="{ids}-cards">
        <h2 id="{ids}-cards">{i18n.t('card.cards', { count: data.cards.length })}</h2>
        {#if data.cards.length > 1}<p class="muted">{i18n.t('card.independent')}</p>{/if}
        {#if data.archivedOrdinals.length > 0}<p class="muted">{i18n.t('card.archived', { count: data.archivedOrdinals.length })}</p>{/if}
        {#each data.cards as card (card.id)}
          <CardSchedule
            {card}
            opened={card.id === id}
            question={question(data, card)}
            targetLanguage={data.targetLanguage}
            {busy}
            onaction={(target, action) => cardAction(data, target, action)}
          />
        {/each}
      </section>

      {#if data.capabilities.associations}
        <AssociationEditor
          associations={data.associations}
          {busy}
          generating={generator.isRunning(data.note.id)}
          onchange={(next) => change(data, next)}
          onfavorite={(association) => favorite(data, association)}
          ondelete={(association) => deleteAssociation(data, association)}
          ongenerate={() => generate(data)}
        />
      {/if}
      {#if data.capabilities.grammarTips}
        <TipEditor tips={data.grammarTips} {busy} onchange={(next) => change(data, next)} ondelete={(tip) => deleteTip(data, tip)} />
      {/if}
      {#if sentence && data.capabilities.wordLinks}
        <LinkList links={data.wordLinks} {busy} onpick={(link) => (picking = link)} onunlink={(link) => void change(data, { type: 'unlinkWord', linkId: link.id })} />
      {/if}

      <WordPicker
        open={picking !== null}
        surface={picking?.surface ?? ''}
        targetLanguage={data.targetLanguage}
        allowUnresolved={false}
        onpick={pick}
        onclose={() => (picking = null)}
      />
    {/snippet}
  </QueryView>
</Screen>

<!-- Destructive: a stray backdrop tap must not decide it. -->
<Dialog open={confirming !== null} title={confirming ? i18n.t(confirming.title) : ''} onclose={() => (confirming = null)} dismissible={false}>
  <p class="confirm-body">{confirming?.body}</p>
  {#snippet actions()}
    <button type="button" class="btn" onclick={() => (confirming = null)}>{i18n.t('common.cancel')}</button>
    <button type="button" class="btn btn-danger" onclick={runConfirmed}>{confirming ? i18n.t(confirming.confirm) : ''}</button>
  {/snippet}
</Dialog>

<style>
  .front {
    font-size: var(--text-xl);
    font-weight: var(--weight-bold);
    overflow-wrap: anywhere;
  }

  .note p {
    overflow-wrap: anywhere;
  }

  .meta {
    font-size: var(--text-sm);
  }

  h2 {
    font-size: var(--text-lg);
  }

  .confirm-body {
    overflow-wrap: anywhere;
  }

  .danger {
    color: var(--color-error);
  }
</style>
