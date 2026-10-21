import type { AppApi } from '../../api/appApi'
import { toErrorDto } from '../../api/errors'
import type {
  BulkAction,
  BulkPreviewDto,
  BulkSelection,
  CardFlag,
  CardRowDto,
  CardState,
  ErrorDto,
  Id,
  SearchInput,
  SortKey,
} from '../../api/types'
import { newOperationId } from '../../services/operationId'

export const PAGE_SIZE = 100

export interface BrowserFilters {
  query: string
  deckId: Id | null
  tag: string | null
  states: CardState[]
  flags: CardFlag[]
  sort: SortKey
  direction: 'asc' | 'desc'
}

export const DEFAULT_FILTERS: BrowserFilters = { query: '', deckId: null, tag: null, states: [], flags: [], sort: 'due', direction: 'asc' }

/**
 * Card browser: pages are fetched for the visible window only (contract paging), so a 50k result
 * never becomes 50k DTOs or DOM rows. The selection is either explicit ids or "all matches of
 * this filter" minus exclusions, bound to the filter snapshot the backend handed out.
 */
export class BrowserController {
  filters = $state<BrowserFilters>({ ...DEFAULT_FILTERS })
  total = $state<number | null>(null)
  /** Sparse by position; replaced as a whole, so it is not deeply proxied. */
  rows = $state.raw<(CardRowDto | undefined)[]>([])
  error = $state<ErrorDto | null>(null)
  filterVersion = $state<string | null>(null)
  /** `ids`: the rows the user ticked; `all`: every match except the unticked ones. */
  selection = $state<{ mode: 'ids'; ids: Id[] } | { mode: 'all'; excluded: Id[] }>({ mode: 'ids', ids: [] })

  readonly #api: AppApi
  readonly #pageSize: number
  #generation = 0
  readonly #requested = new Set<number>()

  constructor({ api, pageSize = PAGE_SIZE }: { api: AppApi; pageSize?: number }) {
    this.#api = api
    this.#pageSize = pageSize
  }

  readonly selectedCount = $derived(
    this.selection.mode === 'ids' ? this.selection.ids.length : Math.max(0, (this.total ?? 0) - this.selection.excluded.length),
  )
  readonly allSelected = $derived(this.selection.mode === 'all')

  /** Starts a new search; earlier responses are dropped. Selection never survives a filter change. */
  search(filters: BrowserFilters): Promise<void> {
    this.filters = { ...filters, states: [...filters.states], flags: [...filters.flags] }
    const run = ++this.#generation
    this.#requested.clear()
    this.total = null
    this.rows = []
    this.error = null
    this.filterVersion = null
    this.clearSelection()
    return this.#loadPage(0, run)
  }

  reload(): Promise<void> {
    return this.search(this.filters)
  }

  /** Visible window changed: fetch the pages it touches. */
  loadRange(start: number, end: number): void {
    const run = this.#generation
    const last = Math.floor(Math.max(start, end - 1) / this.#pageSize)
    for (let page = Math.floor(start / this.#pageSize); page <= last; page += 1) void this.#loadPage(page, run)
  }

  isSelected(cardId: Id): boolean {
    return this.selection.mode === 'ids' ? this.selection.ids.includes(cardId) : !this.selection.excluded.includes(cardId)
  }

  toggle(cardId: Id): void {
    if (this.selection.mode === 'ids') {
      const ids = this.selection.ids
      this.selection = { mode: 'ids', ids: ids.includes(cardId) ? ids.filter((id) => id !== cardId) : [...ids, cardId] }
    } else {
      const excluded = this.selection.excluded
      this.selection = { mode: 'all', excluded: excluded.includes(cardId) ? excluded.filter((id) => id !== cardId) : [...excluded, cardId] }
    }
  }

  /** Ticks the rows loaded so far (the "page"), not every match. */
  selectLoaded(): void {
    const ids = this.rows.flatMap((row) => (row ? [row.cardId] : []))
    this.selection = { mode: 'ids', ids: [...new Set([...(this.selection.mode === 'ids' ? this.selection.ids : []), ...ids])] }
  }

  selectAllMatches(): void {
    this.selection = { mode: 'all', excluded: [] }
  }

  clearSelection(): void {
    this.selection = { mode: 'ids', ids: [] }
  }

  selectionDto(): BulkSelection | null {
    if (this.selection.mode === 'ids') return this.selection.ids.length > 0 ? { kind: 'ids', cardIds: [...this.selection.ids] } : null
    return this.filterVersion ? { kind: 'query', filterVersion: this.filterVersion, excludedCardIds: [...this.selection.excluded] } : null
  }

  async preview(action: BulkAction): Promise<BulkPreviewDto | ErrorDto> {
    const selection = this.selectionDto()
    if (!selection) return toErrorDto(null)
    try {
      return await this.#api.previewBulkAction({ selection, action })
    } catch (caught) {
      return toErrorDto(caught)
    }
  }

  /** Applies a confirmed bulk action, then searches again (the old snapshot is stale by design). */
  async apply(action: BulkAction, operationId: string = newOperationId()): Promise<{ affected: number } | ErrorDto> {
    const selection = this.selectionDto()
    if (!selection) return toErrorDto(null)
    try {
      const result = await this.#api.applyBulkAction({ operationId, selection, action })
      await this.reload()
      return result
    } catch (caught) {
      return toErrorDto(caught)
    }
  }

  #request(page: number): SearchInput {
    return { ...this.filters, cursor: page === 0 ? null : String(page * this.#pageSize), pageSize: this.#pageSize }
  }

  async #loadPage(page: number, run: number): Promise<void> {
    if (this.#requested.has(page)) return
    this.#requested.add(page)
    try {
      const result = await this.#api.searchCards(this.#request(page))
      if (run !== this.#generation) return
      this.total = result.total
      this.filterVersion = result.filterVersion
      const next = this.rows.length === result.total ? [...this.rows] : Array.from({ length: result.total }, (_, index) => this.rows[index])
      result.items.forEach((item, offset) => (next[page * this.#pageSize + offset] = item))
      this.rows = next
      this.error = null
    } catch (caught) {
      this.#requested.delete(page)
      if (run === this.#generation) this.error = toErrorDto(caught)
    }
  }
}

export const isErrorResult = (value: object): value is ErrorDto => 'code' in value && 'messageKey' in value
