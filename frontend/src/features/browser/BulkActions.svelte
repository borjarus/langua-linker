<script lang="ts">
  import { CARD_FLAGS, type BulkAction, type BulkActionType, type BulkPreviewDto, type CardFlag, type EditorDeckOptionDto, type ErrorDto } from '../../api/types'
  import { useServices } from '../../app/context'
  import Dialog from '../../components/Dialog.svelte'
  import ErrorState from '../../components/ErrorState.svelte'
  import Icon, { type IconName } from '../../components/Icon.svelte'
  import Skeleton from '../../components/Skeleton.svelte'
  import { isErrorResult } from './browserController.svelte'

  interface Props {
    count: number
    decks: EditorDeckOptionDto[]
    tags: string[]
    onpreview: (action: BulkAction) => Promise<BulkPreviewDto | ErrorDto>
    /** Resolves to an error, or `null` once the action is applied. */
    onapply: (action: BulkAction) => Promise<ErrorDto | null>
  }

  let { count, decks, tags, onpreview, onapply }: Props = $props()
  const { i18n } = useServices()
  const ids = $props.id()

  const ACTIONS: { type: BulkActionType; icon: IconName }[] = [
    { type: 'changeDeck', icon: 'folder' },
    { type: 'addTag', icon: 'tag' },
    { type: 'removeTag', icon: 'tag' },
    { type: 'flag', icon: 'flag' },
    { type: 'suspend', icon: 'close' },
    { type: 'unsuspend', icon: 'refresh' },
    { type: 'reposition', icon: 'sort' },
    { type: 'delete', icon: 'trash' },
  ]
  const NEEDS_FORM: ReadonlySet<BulkActionType> = new Set(['changeDeck', 'addTag', 'removeTag', 'flag', 'reposition'])

  let step = $state<'closed' | 'menu' | 'form' | 'confirm'>('closed')
  let type = $state<BulkActionType>('suspend')
  let deckId = $state('')
  let tag = $state('')
  let flag = $state<CardFlag>('red')
  let start = $state(1)
  let stepSize = $state(1)
  let preview = $state<BulkPreviewDto | null>(null)
  let error = $state<ErrorDto | null>(null)
  let applying = $state(false)

  const formValid = $derived(
    type === 'changeDeck' ? deckId !== '' : type === 'addTag' || type === 'removeTag' ? tag.trim() !== '' && !/\s/u.test(tag.trim()) : type === 'reposition' ? Number.isInteger(start) && start >= 1 && Number.isInteger(stepSize) && stepSize >= 1 : true,
  )

  function action(): BulkAction {
    switch (type) {
      case 'changeDeck': return { type, deckId }
      case 'addTag':
      case 'removeTag': return { type, tag: tag.trim() }
      case 'flag': return { type, flag }
      case 'reposition': return { type, start, step: stepSize }
      default: return { type } as BulkAction
    }
  }

  function choose(next: BulkActionType) {
    type = next
    error = null
    preview = null
    deckId = decks[0]?.id ?? ''
    if (NEEDS_FORM.has(next)) step = 'form'
    else void loadPreview()
  }

  async function loadPreview() {
    step = 'confirm'
    preview = null
    error = null
    const result = await onpreview(action())
    if (isErrorResult(result)) error = result
    else preview = result
  }

  async function apply() {
    if (applying) return
    applying = true
    error = await onapply(action())
    applying = false
    if (!error) step = 'closed'
  }

  function close() {
    if (!applying) step = 'closed'
  }

  const actionLabel = $derived(i18n.t(`browser.bulk.actions.${type}`))
</script>

<button type="button" class="btn" disabled={count === 0} aria-haspopup="dialog" onclick={() => (step = 'menu')}>
  <Icon name="settings" />{i18n.t('browser.bulk.open', { count })}
</button>

<Dialog open={step === 'menu'} title={i18n.t('browser.bulk.title', { count })} onclose={close} variant="sheet">
  <ul class="menu">
    {#each ACTIONS as item (item.type)}
      <li>
        <button type="button" class="btn btn-ghost item" class:danger={item.type === 'delete'} onclick={() => choose(item.type)}>
          <Icon name={item.icon} />{i18n.t(`browser.bulk.actions.${item.type}`)}
        </button>
      </li>
    {/each}
  </ul>
</Dialog>

<Dialog open={step === 'form'} title={actionLabel} onclose={close}>
  <form id="{ids}-form" class="stack" novalidate onsubmit={(event) => { event.preventDefault(); if (formValid) void loadPreview() }}>
    {#if type === 'changeDeck'}
      <div class="field">
        <label for="{ids}-deck">{i18n.t('browser.bulk.targetDeck')}</label>
        <select id="{ids}-deck" class="input" bind:value={deckId}>
          {#each decks as deck (deck.id)}<option value={deck.id}>{deck.name}</option>{/each}
        </select>
      </div>
      <p class="muted">{i18n.t('browser.bulk.changeDeckHint')}</p>
    {:else if type === 'addTag' || type === 'removeTag'}
      <div class="field">
        <label for="{ids}-tag">{i18n.t('browser.bulk.tag')}</label>
        <input id="{ids}-tag" class="input" list="{ids}-tags" autocomplete="off" bind:value={tag} aria-invalid={tag !== '' && !formValid} aria-describedby="{ids}-tag-hint" />
        <datalist id="{ids}-tags">{#each tags as option (option)}<option value={option}></option>{/each}</datalist>
        <p id="{ids}-tag-hint" class="muted">{i18n.t('browser.bulk.tagHint')}</p>
      </div>
    {:else if type === 'flag'}
      <div class="field">
        <label for="{ids}-flag">{i18n.t('study.actions.flag')}</label>
        <select id="{ids}-flag" class="input" bind:value={flag}>
          {#each CARD_FLAGS as option (option)}<option value={option}>{i18n.t(`study.flags.${option}`)}</option>{/each}
        </select>
      </div>
    {:else if type === 'reposition'}
      <p class="muted">{i18n.t('browser.bulk.repositionHint')}</p>
      <div class="row numbers">
        <div class="field">
          <label for="{ids}-start">{i18n.t('browser.bulk.start')}</label>
          <input id="{ids}-start" class="input" type="number" min="1" step="1" inputmode="numeric" bind:value={start} />
        </div>
        <div class="field">
          <label for="{ids}-step">{i18n.t('browser.bulk.step')}</label>
          <input id="{ids}-step" class="input" type="number" min="1" step="1" inputmode="numeric" bind:value={stepSize} />
        </div>
      </div>
    {/if}
  </form>
  {#snippet actions()}
    <button type="button" class="btn" onclick={close}>{i18n.t('common.cancel')}</button>
    <button type="submit" form="{ids}-form" class="btn btn-primary" disabled={!formValid}>{i18n.t('browser.bulk.review')}</button>
  {/snippet}
</Dialog>

<!-- The confirmation states the effect before anything changes; a stray tap must not apply it. -->
<Dialog open={step === 'confirm'} title={i18n.t('browser.bulk.confirmTitle', { action: actionLabel })} onclose={close} dismissible={false}>
  {#if error}
    <ErrorState {error} compact onretry={loadPreview} />
  {:else if !preview}
    <Skeleton lines={2} />
  {:else}
    <div class="stack" role="status">
      <p><strong>{i18n.t('browser.bulk.affected', { count: preview.affected })}</strong></p>
      {#if preview.unchanged > 0}<p class="muted">{i18n.t('browser.bulk.unchanged', { count: preview.unchanged })}</p>{/if}
      <p class="muted">{i18n.t('browser.bulk.scope', { notes: preview.notes, decks: preview.decks })}</p>
      {#if type === 'delete'}<p class="danger-text">{i18n.t('browser.bulk.deleteWarning')}</p>{/if}
    </div>
  {/if}
  {#snippet actions()}
    <button type="button" class="btn" disabled={applying} onclick={close}>{i18n.t('common.cancel')}</button>
    <button
      type="button"
      class="btn {type === 'delete' ? 'btn-danger' : 'btn-primary'}"
      disabled={applying || !preview || preview.affected === 0}
      aria-busy={applying}
      onclick={apply}
    >
      {i18n.t('browser.bulk.apply', { count: preview?.affected ?? 0 })}
    </button>
  {/snippet}
</Dialog>

<style>
  .menu {
    display: grid;
    gap: var(--space-1);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .item {
    justify-content: flex-start;
    width: 100%;
    text-align: start;
  }

  .danger,
  .danger-text {
    color: var(--color-error);
  }

  /* flex-basis, not min-width: WebKit resolves min(100%, …) to the rem value when sizing. */
  .numbers .field {
    flex: 1 1 8rem;
    min-width: 0;
  }
</style>
