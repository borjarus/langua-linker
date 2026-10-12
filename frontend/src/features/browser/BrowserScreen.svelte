<script lang="ts">
  import { toErrorDto } from '../../api/errors'
  import type { CardRowDto, ErrorDto, SearchInput } from '../../api/types'
  import { useServices } from '../../app/context'
  import ErrorState from '../../components/ErrorState.svelte'
  import SearchInput_ from '../../components/SearchInput.svelte'
  import Skeleton from '../../components/Skeleton.svelte'
  import VirtualList from '../../components/VirtualList.svelte'
  import Screen from '../shared/Screen.svelte'

  interface Props {
    deckId: string | null
    query: string
  }

  let { deckId, query }: Props = $props()
  const { api, i18n, router } = useServices()
  const PAGE_SIZE = 100

  let total = $state<number | null>(null)
  let rows = $state<(CardRowDto | undefined)[]>([])
  let error = $state<ErrorDto | null>(null)
  let generation = 0
  const requestedPages = new Set<number>()

  function request(page: number): SearchInput {
    return {
      query,
      deckId,
      states: [],
      flags: [],
      sort: 'due',
      direction: 'asc',
      cursor: page === 0 ? null : String(page * PAGE_SIZE),
      pageSize: PAGE_SIZE,
    }
  }

  async function loadPage(page: number, run: number) {
    if (requestedPages.has(page)) return
    requestedPages.add(page)
    try {
      const result = await api.searchCards(request(page))
      if (run !== generation) return
      total = result.total
      const next = rows.length === result.total ? rows : Array.from({ length: result.total }, (_, index) => rows[index])
      result.items.forEach((item, offset) => (next[page * PAGE_SIZE + offset] = item))
      rows = next
      error = null
    } catch (caught) {
      requestedPages.delete(page)
      if (run === generation) error = toErrorDto(caught)
    }
  }

  // Re-run the search when the query or deck filter changes (reads only).
  $effect(() => {
    void query
    void deckId
    const run = ++generation
    requestedPages.clear()
    total = null
    rows = []
    error = null
    void loadPage(0, run)
  })

  function onrange(start: number, end: number) {
    const run = generation
    for (let page = Math.floor(start / PAGE_SIZE); page <= Math.floor(Math.max(start, end - 1) / PAGE_SIZE); page += 1) {
      void loadPage(page, run)
    }
  }
</script>

<Screen title={i18n.t('routes.browser')} phase={4}>
  <SearchInput_
    value={query}
    label={i18n.t('browser.search')}
    placeholder={i18n.t('browser.searchPlaceholder')}
    error={error?.code === 'VALIDATION_FAILED' ? i18n.tDynamic(error.messageKey, 'errors.INTERNAL') : null}
    onsearch={(value) => router.replace({ name: 'browser', query: { q: value, deck: deckId } })}
  />
  {#if error && error.code !== 'VALIDATION_FAILED'}
    <ErrorState error={error} onretry={() => loadPage(0, generation)} />
  {:else if total === null}
    <Skeleton lines={6} />
  {:else}
    <p class="muted" role="status">{i18n.t('browser.results', { count: total })}</p>
    <VirtualList itemCount={total} label={i18n.t('browser.resultsLabel')} {onrange}>
      {#snippet row(index)}
        {@const item = rows[index]}
        {#if item}
          <a class="result" href={`#/card/${encodeURIComponent(item.cardId)}`}>
            <span class="front" dir="auto">{item.front}</span>
            <span class="back muted" dir="auto">{item.back}</span>
          </a>
        {:else}
          <span class="result muted" aria-busy="true">{i18n.t('common.loading')}</span>
        {/if}
      {/snippet}
    </VirtualList>
  {/if}
</Screen>

<style>
  .result {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: var(--space-3);
    width: 100%;
    padding: 0 var(--space-4);
    color: inherit;
    text-decoration: none;
  }

  .result span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .front {
    font-weight: var(--weight-medium);
  }
</style>
