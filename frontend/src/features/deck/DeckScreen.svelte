<script lang="ts">
  import { toErrorDto } from '../../api/errors'
  import type { DeckOverviewDto, DeckSummaryDto, DeckType, ErrorDto, Id, PresetDto, UpdateDeckInput } from '../../api/types'
  import { useServices } from '../../app/context'
  import { href } from '../../app/routes'
  import Chip from '../../components/Chip.svelte'
  import Dialog from '../../components/Dialog.svelte'
  import Icon from '../../components/Icon.svelte'
  import StatTile from '../../components/StatTile.svelte'
  import { newOperationId } from '../../services/operationId'
  import { useQuery } from '../../services/query.svelte'
  import QueryView from '../shared/QueryView.svelte'
  import Screen from '../shared/Screen.svelte'
  import { StudyController } from '../study/studyController.svelte'
  import DeckEditDialog from './DeckEditDialog.svelte'
  import DeckTree from './DeckTree.svelte'
  import DeckTypeDialog from './DeckTypeDialog.svelte'
  import PresetEditor from './PresetEditor.svelte'
  import PresetPicker from './PresetPicker.svelte'

  interface Props {
    id: string
  }

  let { id }: Props = $props()
  const { api, i18n, queries, router, toasts } = useServices()
  const overview = useQuery(queries, () => ({ key: ['deck', id], fetch: () => api.getDeck(id) }))
  const editorData = useQuery(queries, () => ({ key: ['editor'], fetch: () => api.getEditorData() }))
  const ids = $props.id()

  let dialog = $state<'edit' | 'type' | 'delete' | 'presets' | null>(null)
  let editingPreset = $state<PresetDto | null>(null)
  let collapsed = $state<Id[]>([])
  let deleting = $state(false)
  let deleteError = $state<ErrorDto | null>(null)

  const SECTIONS = ['associations', 'grammarTips', 'wordLinks'] as const

  async function study(deckId: string) {
    try {
      const session = await StudyController.start(api, deckId)
      await router.navigate({ name: 'study', params: { deckId }, query: { session: session.sessionId } })
    } catch (caught) {
      toasts.show(i18n.tDynamic(toErrorDto(caught).messageKey, 'errors.INTERNAL'), 'error')
    }
  }

  /** One update intent; a stale version is reported, never silently overwritten. */
  async function update(data: DeckOverviewDto, patch: UpdateDeckInput['patch']): Promise<ErrorDto | null> {
    try {
      await api.updateDeck({ operationId: newOperationId(), deckId: data.deck.id, expectedVersion: data.deck.version, patch })
      await overview.refresh()
      return null
    } catch (caught) {
      const error = toErrorDto(caught)
      if (error.code === 'VERSION_CONFLICT') await overview.refresh()
      return error
    }
  }

  async function saveEdit(data: DeckOverviewDto, patch: UpdateDeckInput['patch']) {
    const error = await update(data, patch)
    if (!error) {
      dialog = null
      toasts.show(i18n.t(patch.parentId !== undefined ? 'deck.done.moved' : 'deck.done.saved'), 'success')
    }
    return error
  }

  async function saveType(data: DeckOverviewDto, deckType: DeckType) {
    const before = data.hiddenSectionCounts
    const target = data.typeOptions.find((option) => option.deckType === deckType)?.hiddenCounts
    const error = await update(data, { deckType })
    if (!error) {
      dialog = null
      const restored = target ? SECTIONS.reduce((sum, section) => sum + Math.max(0, before[section] - target[section]), 0) : 0
      toasts.show(restored > 0 ? i18n.t('deck.done.typeRestored', { count: restored }) : i18n.t('deck.done.typeChanged'), 'success')
    }
    return error
  }

  async function pickPreset(data: DeckOverviewDto, preset: PresetDto) {
    const error = await update(data, { presetId: preset.id })
    if (!error) {
      dialog = null
      toasts.show(i18n.t('deck.done.presetChanged', { name: preset.name }), 'success')
    }
    return error
  }

  async function presetSaved(data: DeckOverviewDto, preset: PresetDto, asNew: boolean) {
    editingPreset = null
    queries.invalidate(['presets'])
    if (asNew) {
      const error = await update(data, { presetId: preset.id })
      if (error) {
        toasts.show(i18n.tDynamic(error.messageKey, 'errors.INTERNAL'), 'error')
        return
      }
      dialog = null
      toasts.show(i18n.t('deck.done.presetCloned', { name: preset.name }), 'success')
    } else {
      await overview.refresh()
      toasts.show(i18n.t('deck.done.presetSaved', { name: preset.name }), 'success')
    }
  }

  async function remove(data: DeckOverviewDto) {
    if (deleting) return
    deleting = true
    deleteError = null
    try {
      const result = await api.deleteDeck({ operationId: newOperationId(), deckId: data.deck.id, expectedVersion: data.deck.version })
      dialog = null
      toasts.show(i18n.t('deck.done.deleted', { name: data.deck.name, cards: result.deletedCards }), 'success')
      await router.replace({ name: 'home' })
    } catch (caught) {
      deleteError = toErrorDto(caught)
    } finally {
      deleting = false
    }
  }

  function toggle(deckId: Id) {
    collapsed = collapsed.includes(deckId) ? collapsed.filter((item) => item !== deckId) : [...collapsed, deckId]
  }

  const due = (deck: DeckSummaryDto) => deck.counts.new + deck.counts.learning + deck.counts.review
  const hiddenTotal = (data: DeckOverviewDto) => SECTIONS.reduce((sum, section) => sum + data.hiddenSectionCounts[section], 0)
</script>

<Screen title={overview.state.status === 'ready' ? overview.state.data.deck.name : i18n.t('routes.deck')} back notice={i18n.t('common.prototypeManagement')}>
  <QueryView query={overview}>
    {#snippet ready(data)}
      {#if data.description}<p class="description">{data.description}</p>{/if}
      <div class="row">
        <Chip label={i18n.t(`deck.types.${data.deck.deckType}`)} />
        <Chip label={i18n.t(`deck.targetLanguages.${data.deck.targetLanguage}`)} />
        {#if data.deck.bundled}<Chip label={i18n.t('deck.bundled')} tone="info" />{/if}
        <Chip label={i18n.t('deck.cards', { count: data.deck.counts.total })} />
      </div>

      <dl class="tiles">
        <StatTile label={i18n.t('deck.counts.new')} value={i18n.formatNumber(data.deck.counts.new)} tone="info" />
        <StatTile label={i18n.t('deck.counts.learning')} value={i18n.formatNumber(data.deck.counts.learning)} tone="warning" />
        <StatTile label={i18n.t('deck.counts.review')} value={i18n.formatNumber(data.deck.counts.review)} tone="success" />
        <StatTile label={i18n.t('deck.counts.overdue')} value={i18n.formatNumber(data.deck.counts.overdue)} tone="error" />
      </dl>

      <div class="row">
        {#if data.deck.contentState === 'preparing'}
          <p class="muted" role="status">{i18n.t('deck.preparing')}</p>
        {:else if due(data.deck) > 0}
          <button type="button" class="btn btn-primary" onclick={() => study(data.deck.id)}>{i18n.t('deck.study')}</button>
        {:else}
          <p class="muted">{i18n.t(data.deck.counts.total === 0 ? 'deck.noCards' : 'deck.noDue')}</p>
        {/if}
        <a class="btn" href={href({ name: 'browser', query: { deck: data.deck.id } })}><Icon name="search" />{i18n.t('nav.browser')}</a>
        <a class="btn" href={href({ name: 'editor', query: { deckId: data.deck.id } })}><Icon name="plus" />{i18n.t('deck.addCards')}</a>
      </div>

      <section class="stack" aria-labelledby="{ids}-subdecks">
        <div class="row heading">
          <h2 id="{ids}-subdecks">{i18n.t('deck.subdecks', { count: data.subdecks.length })}</h2>
          {#if data.subdecks.length > 0}
            <button type="button" class="btn btn-ghost" onclick={() => (collapsed = collapsed.length > 0 ? [] : data.subdecks.map((deck) => deck.id))}>
              {i18n.t(collapsed.length > 0 ? 'deck.tree.expandAll' : 'deck.tree.collapseAll')}
            </button>
          {/if}
        </div>
        {#if data.subdecks.length === 0}
          <p class="muted">{i18n.t('deck.tree.none')}</p>
        {:else}
          <DeckTree decks={data.subdecks} parentId={data.deck.id} {collapsed} ontoggle={toggle} onstudy={(deck) => study(deck.id)} />
        {/if}
      </section>

      <section class="surface stack" aria-labelledby="{ids}-settings">
        <h2 id="{ids}-settings">{i18n.t('deck.settings')}</h2>
        <dl class="settings">
          <div>
            <dt>{i18n.t('deck.form.type')}</dt>
            <dd>
              {i18n.t(`deck.types.${data.deck.deckType}`)}
              {#if hiddenTotal(data) > 0}
                <span class="muted block">{i18n.t('deck.typeChange.hiddenNow', { count: hiddenTotal(data) })}</span>
              {/if}
            </dd>
          </div>
          <div>
            <dt>{i18n.t('deck.preset.label')}</dt>
            <dd>
              {data.presetName}
              <span class="muted block">{i18n.t('deck.preset.usedBy', { count: data.presetDependentDecks })}</span>
            </dd>
          </div>
        </dl>
        <div class="row">
          <button type="button" class="btn" onclick={() => (dialog = 'edit')}><Icon name="edit" />{i18n.t('deck.edit.open')}</button>
          <button type="button" class="btn" onclick={() => (dialog = 'type')}><Icon name="cards" />{i18n.t('deck.typeChange.open')}</button>
          <button type="button" class="btn" onclick={() => (dialog = 'presets')}><Icon name="settings" />{i18n.t('deck.preset.open')}</button>
          <button type="button" class="btn btn-ghost danger" onclick={() => { deleteError = null; dialog = 'delete' }}><Icon name="trash" />{i18n.t('deck.delete.open')}</button>
        </div>
      </section>

      <DeckEditDialog
        open={dialog === 'edit'}
        overview={data}
        decks={editorData.state.status === 'ready' ? editorData.state.data.decks : []}
        onclose={() => (dialog = null)}
        onsave={(patch) => saveEdit(data, patch)}
      />
      <DeckTypeDialog open={dialog === 'type'} overview={data} onclose={() => (dialog = null)} onsave={(deckType) => saveType(data, deckType)} />
      {#if dialog === 'presets' && !editingPreset}
        <PresetPicker currentId={data.presetId} onclose={() => (dialog = null)} onpick={(preset) => pickPreset(data, preset)} onedit={(preset) => (editingPreset = preset)} />
      {/if}
      {#if editingPreset}
        <PresetEditor preset={editingPreset} deckName={data.deck.name} onclose={() => (editingPreset = null)} onsaved={(preset, asNew) => presetSaved(data, preset, asNew)} />
      {/if}

      <!-- Destructive: names the whole scope; a stray backdrop tap must not decide it. -->
      <Dialog open={dialog === 'delete'} title={i18n.t('deck.delete.title', { name: data.deck.name })} onclose={() => !deleting && (dialog = null)} dismissible={false}>
        <div class="stack">
          <p>{i18n.t('deck.delete.scope', { decks: data.subdecks.length + 1, cards: data.deck.counts.total })}</p>
          <p class="muted">{i18n.t('deck.delete.irreversible')}</p>
          {#if deleteError}<p class="danger" role="alert">{i18n.tDynamic(deleteError.messageKey, 'errors.INTERNAL')}</p>{/if}
        </div>
        {#snippet actions()}
          <button type="button" class="btn" disabled={deleting} onclick={() => (dialog = null)}>{i18n.t('common.cancel')}</button>
          <button type="button" class="btn btn-danger" disabled={deleting} aria-busy={deleting} onclick={() => remove(data)}>
            {i18n.t('deck.delete.confirm', { count: data.deck.counts.total })}
          </button>
        {/snippet}
      </Dialog>
    {/snippet}
  </QueryView>
</Screen>

<style>
  .description {
    overflow-wrap: anywhere;
  }

  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 9rem), 1fr));
    gap: var(--space-3);
    margin: 0;
  }

  .heading {
    justify-content: space-between;
  }

  h2 {
    font-size: var(--text-lg);
  }

  .settings {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 14rem), 1fr));
    gap: var(--space-3);
    margin: 0;
  }

  .settings div {
    display: grid;
    gap: var(--space-1);
  }

  dt {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  dd {
    margin: 0;
    font-weight: var(--weight-medium);
    overflow-wrap: anywhere;
  }

  .block {
    display: block;
    font-size: var(--text-sm);
    font-weight: normal;
  }

  .danger {
    color: var(--color-error);
  }
</style>
