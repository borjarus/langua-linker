/**
 * Framework-neutral query cache (ADR-02): de-duplicates in-flight reads, keeps the last good
 * result for stale-while-revalidate, and notifies subscribers on invalidation.
 * Errors are never cached. No React bindings, no Svelte imports.
 */

export type QueryKeyPart = string | number | boolean | null
export type QueryKey = readonly QueryKeyPart[]

type Listener = () => void

interface Entry {
  hasData: boolean
  data: unknown
  stale: boolean
  inflight: Promise<unknown> | null
  listeners: Set<Listener>
}

export interface CachedValue<T> {
  hasData: boolean
  data: T | undefined
  stale: boolean
}

export const serializeKey = (key: QueryKey): string => JSON.stringify(key)

export class QueryClient {
  readonly #entries = new Map<string, Entry>()

  #entry(key: string): Entry {
    let entry = this.#entries.get(key)
    if (!entry) {
      entry = { hasData: false, data: undefined, stale: false, inflight: null, listeners: new Set() }
      this.#entries.set(key, entry)
    }
    return entry
  }

  peek<T>(key: string): CachedValue<T> {
    const entry = this.#entries.get(key)
    return entry ? { hasData: entry.hasData, data: entry.data as T | undefined, stale: entry.stale } : { hasData: false, data: undefined, stale: false }
  }

  fetch<T>(key: string, fetcher: () => Promise<T>): Promise<T> {
    const entry = this.#entry(key)
    if (entry.inflight) return entry.inflight as Promise<T>
    const request = fetcher().then(
      (data) => {
        entry.data = data
        entry.hasData = true
        entry.stale = false
        return data
      },
    )
    entry.inflight = request
    const clear = () => {
      if (entry.inflight === request) entry.inflight = null
    }
    request.then(clear, clear)
    return request
  }

  subscribe(key: string, listener: Listener): () => void {
    const entry = this.#entry(key)
    entry.listeners.add(listener)
    return () => entry.listeners.delete(listener)
  }

  /** Marks matching entries stale; mounted queries refetch, others refetch on next mount. */
  invalidate(prefix: QueryKey = []): void {
    const serialized = serializeKey(prefix)
    const open = serialized.slice(0, -1)
    const notify: Listener[] = []
    for (const [key, entry] of this.#entries) {
      const matches = prefix.length === 0 || key === serialized || key.startsWith(`${open},`)
      if (!matches) continue
      entry.stale = true
      notify.push(...entry.listeners)
    }
    for (const listener of notify) listener()
  }

  /** Drops all data, e.g. after switching demo scenario or restoring a backup. */
  clear(): void {
    const notify = [...this.#entries.values()].flatMap((entry) => [...entry.listeners])
    for (const entry of this.#entries.values()) {
      entry.hasData = false
      entry.data = undefined
      entry.stale = false
    }
    for (const listener of notify) listener()
  }
}
