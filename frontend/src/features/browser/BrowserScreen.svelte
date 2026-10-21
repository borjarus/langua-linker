<script lang="ts">
  import { tick, untrack } from 'svelte'
  import { CARD_FLAGS, SORT_KEYS, type BulkAction, type CardFlag, type CardState, type ErrorDto } from '../../api/types'
  import { useServices } from '../../app/context'
  import { href } from '../../app/routes'
  import Chip from '../../components/Chip.svelte'
  import Disclosure from '../../components/Disclosure.svelte'
  import EmptyState from '../../components/EmptyState.svelte'
  import ErrorState from '../../components/ErrorState.svelte'
  import Icon from '../../components/Icon.svelte'
  import SearchInput from '../../components/SearchInput.svelte'
  import Skeleton from '../../components/Skeleton.svelte'
  import VirtualList from '../../components/VirtualList.svelte'
  import type { TranslationKey } from '../../locales/types'
  import { useQuery } from '../../services/query.svelte'
  import Screen from '../shared/Screen.svelte'
  import { BrowserController, isErrorResult, type BrowserFilters } from './browserController.svelte'
  import BulkActions from './BulkActions.svelte'

  interface Props {
    deckId: string | null
    query: string
  }

  let { deckId, query }: Props = $props()
  const { api, i18n, queries, router, toasts } = useServices()
  const browser = new BrowserController({ api })
  const options = useQuery(queries, () => ({ key: ['editor'], fetch: () => api.getEditorData() }))
  const ids = $props.id()
  const STATES: CardState[] = ['New', 'Learning', 'Review', 'Relearning']
  const SYNTAX: [string[], TranslationKey][] = [
    [['deck:'], 'browser.help.deck'],
    [['tag:'], 'browser.help.tag'],
    [['is:new', 'is:due', 'is:suspended'], 'browser.help.is'],
    [['"…"'], 'browser.help.phrase'],
  ]
  let list = $state<ReturnType<typeof VirtualList> | null>(null)
  let listEl = $state<HTMLElement | null>(null)

  // The URL owns the text query and the deck; a change re-runs the search (a read).
  $effect(() => {
    const text = query
    const deck = deckId
    untrack(() => void browser.search({ ...browser.filters, query: text, deckId: deck }))
  })

  const decks = $derived(options.state.status === 'ready' ? options.state.data.decks : [])
  const tags = $derived(options.state.status === 'ready' ? options.state.data.tags : [])
  const activeFilters = $derived(
    (browser.filters.tag ? 1 : 0) + browser.filters.states.length + browser.filters.flags.length,
  )
  const parseError = $derived.by(() => {
    const error = browser.error
    if (error?.code !== 'VALIDATION_FAILED') return null
    const reason = error.details?.reason
    const token = String(error.details?.token ?? '')
    return typeof reason === 'string' ? i18n.tDynamic(`browser.parseErrors.${reason}`, 'errors.VALIDATION_FAILED', { token }) : i18n.t('errors.VALIDATION_FAILED')
  })

  function update(patch: Partial<BrowserFilters>) {
    void browser.search({ ...browser.filters, ...patch })
  }

  function toggleState(state: CardState, pressed: boolean) {
    update({ states: pressed ? [...browser.filters.states, state] : browser.filters.states.filter((item) => item !== state) })
  }

  async function apply(action: BulkAction): Promise<ErrorDto | null> {
    const result = await browser.apply(action)
    if (isErrorResult(result)) return result
    toasts.show(i18n.t('browser.bulk.done', { count: result.affected }), 'success')
    return null
  }

  /** Arrow keys move between rows; only rendered rows exist, so the list scrolls first. */
  async function onListKeydown(event: KeyboardEvent) {
    if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return
    const target = event.target as HTMLElement
    const row = target.closest<HTMLElement>('[data-index]')
    if (!row || browser.total === null) return
    event.preventDefault()
    const next = Math.max(0, Math.min(browser.total - 1, Number(row.dataset.index) + (event.key === 'ArrowDown' ? 1 : -1)))
    const kind = target.matches('input') ? 'input' : 'a'
    list?.scrollToIndex(next)
    await tick()
    listEl?.querySelector<HTMLElement>(`[data-index="${next}"] ${kind}`)?.focus()
  }
</script>

<Screen title={i18n.t('routes.browser')} notice={i18n.t('common.prototypeManagement')}>
  <SearchInput
    value={query}
    label={i18n.t('browser.search')}
    placeholder={i18n.t('browser.searchPlaceholder')}
    error={parseError}
    onsearch={(value) => router.replace({ name: 'browser', query: { q: value, deck: deckId } })}
  />
  <Disclosure title={i18n.t('browser.help.title')}>
    <!-- Search operators are syntax, not prose: they stay untranslated. -->
    <dl class="help">
      {#each SYNTAX as [operators, key] (key)}
        <div>
          <dt>{#each operators as operator, index (operator)}{#if index > 0} · {/if}<code>{operator}</code>{/each}</dt>
          <dd>{i18n.t(key)}</dd>
        </div>
      {/each}
    </dl>
  </Disclosure>

  <Disclosure title={i18n.t('browser.filters.title')} count={activeFilters}>
    <div class="filters">
      <div class="field">
        <label for="{ids}-deck">{i18n.t('browser.filters.deck')}</label>
        <select id="{ids}-deck" class="input" value={deckId ?? ''} onchange={(event) => router.replace({ name: 'browser', query: { q: query, deck: event.currentTarget.value || null } })}>
          <option value="">{i18n.t('browser.filters.allDecks')}</option>
          {#each decks as deck (deck.id)}<option value={deck.id}>{deck.name}</option>{/each}
        </select>
      </div>
      <div class="field">
        <label for="{ids}-tag">{i18n.t('browser.filters.tag')}</label>
        <select id="{ids}-tag" class="input" value={browser.filters.tag ?? ''} onchange={(event) => update({ tag: event.currentTarget.value || null })}>
          <option value="">{i18n.t('browser.filters.anyTag')}</option>
          {#each tags as tag (tag)}<option value={tag}>{tag}</option>{/each}
        </select>
      </div>
      <div class="field">
        <label for="{ids}-flag">{i18n.t('browser.filters.flag')}</label>
        <select
          id="{ids}-flag"
          class="input"
          value={browser.filters.flags[0] ?? ''}
          onchange={(event) => update({ flags: event.currentTarget.value ? [event.currentTarget.value as CardFlag] : [] })}
        >
          <option value="">{i18n.t('browser.filters.anyFlag')}</option>
          {#each CARD_FLAGS as flag (flag)}<option value={flag}>{i18n.t(`study.flags.${flag}`)}</option>{/each}
        </select>
      </div>
      <div class="field">
        <label for="{ids}-sort">{i18n.t('browser.filters.sort')}</label>
        <select id="{ids}-sort" class="input" value={browser.filters.sort} onchange={(event) => update({ sort: event.currentTarget.value as BrowserFilters['sort'] })}>
          {#each SORT_KEYS as key (key)}<option value={key}>{i18n.t(`browser.sort.${key}`)}</option>{/each}
        </select>
        <button
          type="button"
          class="btn direction"
          aria-pressed={browser.filters.direction === 'desc'}
          onclick={() => update({ direction: browser.filters.direction === 'asc' ? 'desc' : 'asc' })}
        >
          <Icon name="sort" />{i18n.t(browser.filters.direction === 'asc' ? 'browser.sort.asc' : 'browser.sort.desc')}
        </button>
      </div>
      <fieldset class="states">
        <legend>{i18n.t('browser.filters.states')}</legend>
        <div class="row">
          {#each STATES as state (state)}
            <Chip label={i18n.t(`study.states.${state}`)} pressed={browser.filters.states.includes(state)} ontoggle={(pressed) => toggleState(state, pressed)} />
          {/each}
        </div>
      </fieldset>
    </div>
  </Disclosure>

  {#if browser.error && !parseError}
    <ErrorState error={browser.error} onretry={() => browser.reload()} />
  {:else if parseError}
    <p class="muted">{i18n.t('browser.fixQuery')}</p>
  {:else if browser.total === null}
    <Skeleton lines={6} />
  {:else if browser.total === 0}
    <EmptyState title={i18n.t('browser.emptyTitle')} description={i18n.t('browser.emptyBody')} icon="search">
      {#snippet actions()}
        <a class="btn btn-primary" href={href({ name: 'editor', query: { deckId } })}>{i18n.t('deck.addCards')}</a>
      {/snippet}
    </EmptyState>
  {:else}
    <div class="row toolbar">
      <p class="muted" role="status">
        {i18n.t('browser.results', { count: browser.total })}{#if browser.selectedCount > 0} · {i18n.t('browser.selected', { count: browser.selectedCount })}{/if}
      </p>
      <div class="row">
        {#if browser.allSelected}
          <button type="button" class="btn btn-ghost" onclick={() => browser.clearSelection()}>{i18n.t('browser.clearSelection')}</button>
        {:else}
          <button type="button" class="btn btn-ghost" onclick={() => browser.selectLoaded()}>{i18n.t('browser.selectLoaded')}</button>
          <button type="button" class="btn btn-ghost" onclick={() => browser.selectAllMatches()}>{i18n.t('browser.selectAll', { count: browser.total })}</button>
          {#if browser.selectedCount > 0}
            <button type="button" class="btn btn-ghost" onclick={() => browser.clearSelection()}>{i18n.t('browser.clearSelection')}</button>
          {/if}
        {/if}
        <BulkActions count={browser.selectedCount} {decks} {tags} onpreview={(action) => browser.preview(action)} onapply={apply} />
      </div>
    </div>
    {#if browser.allSelected}<p class="muted" role="status">{i18n.t('browser.allSelectedHint')}</p>{/if}
    <p class="muted hint">{i18n.t('browser.keyboardHint')}</p>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div bind:this={listEl} onkeydown={onListKeydown}>
      <VirtualList bind:this={list} itemCount={browser.total} rowHeightRem={4.5} label={i18n.t('browser.resultsLabel')} onrange={(start, end) => browser.loadRange(start, end)}>
        {#snippet row(index)}
          {@const item = browser.rows[index]}
          {#if item}
            <div class="result">
              <input
                type="checkbox"
                checked={browser.isSelected(item.cardId)}
                aria-label={i18n.t('browser.selectRow', { front: item.front })}
                onchange={() => browser.toggle(item.cardId)}
              />
              <a class="text" href={href({ name: 'card', params: { id: item.cardId } })}>
                <span class="front" dir="auto">
                  {#if item.flag !== 'none'}<span class="dot flag-{item.flag}" role="img" aria-label={i18n.t('study.flagged', { flag: i18n.t(`study.flags.${item.flag}`) })}></span>{/if}
                  {item.front}
                </span>
                <span class="meta muted">
                  <span dir="auto">{item.back}</span>
                  · {i18n.t(`study.states.${item.state}`)}{#if item.queue === 'Suspended'} · {i18n.t('card.suspended')}{/if}{#if item.direction === 'reverse'} · {i18n.t('study.reverse')}{/if}{#if item.position !== null} · {i18n.t('browser.position', { position: item.position })}{/if}
                  · {item.deckName}
                </span>
              </a>
            </div>
          {:else}
            <span class="result muted" aria-busy="true">{i18n.t('common.loading')}</span>
          {/if}
        {/snippet}
      </VirtualList>
    </div>
  {/if}
</Screen>

<style>
  .help {
    display: grid;
    gap: var(--space-2);
    margin: 0;
  }

  .help div {
    display: grid;
    gap: var(--space-1);
  }

  .help dd {
    margin: 0;
    color: var(--color-text-muted);
  }

  .filters {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 12rem), 1fr));
    gap: var(--space-3);
  }

  .direction {
    justify-self: start;
  }

  .states {
    grid-column: 1 / -1;
    margin: 0;
    padding: 0;
    border: 0;
  }

  legend {
    margin-bottom: var(--space-2);
    font-weight: var(--weight-medium);
  }

  .toolbar {
    justify-content: space-between;
  }

  .hint {
    font-size: var(--text-sm);
  }

  @media (hover: none) and (pointer: coarse) {
    .hint {
      display: none;
    }
  }

  .result {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    align-items: center;
    gap: var(--space-3);
    width: 100%;
    height: 100%;
    padding: 0 var(--space-3);
  }

  .result input {
    width: 1.25rem;
    height: 1.25rem;
  }

  .text {
    display: grid;
    min-width: 0;
    color: inherit;
    text-decoration: none;
  }

  .text span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .front {
    font-weight: var(--weight-medium);
  }

  .meta {
    font-size: var(--text-sm);
  }

  .dot {
    display: inline-block;
    width: 0.75rem;
    height: 0.75rem;
    margin-inline-end: var(--space-1);
    border-radius: 50%;
    vertical-align: middle;
  }

  .flag-red { background: var(--color-flag-red); }
  .flag-orange { background: var(--color-flag-orange); }
  .flag-green { background: var(--color-flag-green); }
  .flag-blue { background: var(--color-flag-blue); }
  .flag-pink { background: var(--color-flag-pink); }
  .flag-turquoise { background: var(--color-flag-turquoise); }
  .flag-purple { background: var(--color-flag-purple); }
</style>
