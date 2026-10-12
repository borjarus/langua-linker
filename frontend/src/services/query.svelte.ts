import { untrack } from 'svelte'
import { toErrorDto } from '../api/errors'
import type { ErrorDto } from '../api/types'
import { serializeKey, type QueryClient, type QueryKey } from './queryClient'

export type UiState<T> =
  | { status: 'loading' }
  | { status: 'empty'; data: T }
  | { status: 'ready'; data: T }
  | { status: 'error'; error: ErrorDto }

export interface QueryOptions<T> {
  key: QueryKey
  fetch: () => Promise<T>
  /** Distinguishes "loaded but nothing to show" from "ready". */
  isEmpty?: (data: T) => boolean
}

export interface QueryHandle<T> {
  readonly state: UiState<T>
  /** Showing cached data while a refresh runs or after a failed refresh. */
  readonly stale: boolean
  readonly fetching: boolean
  /** Error of the last refresh when cached data is still shown. */
  readonly refreshError: ErrorDto | null
  refresh(): Promise<void>
}

/**
 * Read-only query bound to the component lifecycle. `$effect` is used only to (re)subscribe when
 * the key changes and to drop late responses after unmount — never to run mutations.
 */
export function useQuery<T>(client: QueryClient, options: () => QueryOptions<T>): QueryHandle<T> {
  let state = $state<UiState<T>>({ status: 'loading' })
  let stale = $state(false)
  let fetching = $state(false)
  let refreshError = $state<ErrorDto | null>(null)
  let current: (QueryOptions<T> & { serialized: string }) | null = null
  let generation = 0

  const settle = (data: T): UiState<T> =>
    current?.isEmpty?.(data) ? { status: 'empty', data } : { status: 'ready', data }

  const load = async (force: boolean): Promise<void> => {
    const active = current
    if (!active) return
    const run = ++generation
    const cached = client.peek<T>(active.serialized)
    if (cached.hasData) {
      state = settle(cached.data as T)
      stale = cached.stale || force
      if (!cached.stale && !force) return
    } else {
      state = { status: 'loading' }
    }
    fetching = true
    try {
      const data = await client.fetch(active.serialized, active.fetch)
      if (run !== generation) return
      state = settle(data)
      stale = false
      refreshError = null
    } catch (caught) {
      if (run !== generation) return
      const error = toErrorDto(caught)
      if (cached.hasData) {
        stale = true
        refreshError = error
      } else {
        state = { status: 'error', error }
      }
    } finally {
      if (run === generation) fetching = false
    }
  }

  $effect(() => {
    const next = options()
    const serialized = serializeKey(next.key)
    untrack(() => {
      current = { ...next, serialized }
      void load(false)
    })
    const unsubscribe = client.subscribe(serialized, () => void load(true))
    return () => {
      unsubscribe()
      generation += 1
      current = null
    }
  })

  return {
    get state() {
      return state
    },
    get stale() {
      return stale
    },
    get fetching() {
      return fetching
    },
    get refreshError() {
      return refreshError
    },
    refresh: () => load(true),
  }
}
