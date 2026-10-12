import { flushSync } from 'svelte'
import { describe, expect, it, vi } from 'vitest'
import { AppError } from '../src/api/errors'
import { useQuery, type QueryHandle } from '../src/services/query.svelte'
import { QueryClient, serializeKey } from '../src/services/queryClient'

const flush = () => new Promise((resolve) => setTimeout(resolve, 0))

describe('QueryClient', () => {
  it('de-duplicates concurrent fetches for the same key', async () => {
    const client = new QueryClient()
    const fetcher = vi.fn(async () => 42)
    const [a, b] = await Promise.all([client.fetch('k', fetcher), client.fetch('k', fetcher)])
    expect(a).toBe(42)
    expect(b).toBe(42)
    expect(fetcher).toHaveBeenCalledTimes(1)
  })

  it('never caches errors', async () => {
    const client = new QueryClient()
    await expect(client.fetch('k', async () => Promise.reject(AppError.of('DB_BUSY')))).rejects.toBeInstanceOf(AppError)
    expect(client.peek('k').hasData).toBe(false)
    expect(await client.fetch('k', async () => 'ok')).toBe('ok')
  })

  it('invalidates by key prefix without matching longer siblings', async () => {
    const client = new QueryClient()
    await client.fetch(serializeKey(['deck', '1']), async () => 1)
    await client.fetch(serializeKey(['decks']), async () => 2)
    const deckListener = vi.fn()
    const decksListener = vi.fn()
    client.subscribe(serializeKey(['deck', '1']), deckListener)
    client.subscribe(serializeKey(['decks']), decksListener)

    client.invalidate(['deck'])
    expect(deckListener).toHaveBeenCalledTimes(1)
    expect(decksListener).not.toHaveBeenCalled()
    expect(client.peek(serializeKey(['deck', '1'])).stale).toBe(true)
  })
})

describe('useQuery', () => {
  function mount<T>(client: QueryClient, options: () => { key: readonly string[]; fetch: () => Promise<T>; isEmpty?: (data: T) => boolean }) {
    let handle!: QueryHandle<T>
    const dispose = $effect.root(() => {
      handle = useQuery(client, options)
    })
    flushSync()
    return { handle: () => handle, dispose }
  }

  it('moves loading → ready and loading → empty', async () => {
    const client = new QueryClient()
    const ready = mount(client, () => ({ key: ['a'], fetch: async () => [1, 2] }))
    expect(ready.handle().state.status).toBe('loading')
    await flush()
    expect(ready.handle().state).toEqual({ status: 'ready', data: [1, 2] })

    const empty = mount(client, () => ({ key: ['b'], fetch: async () => [] as number[], isEmpty: (data) => data.length === 0 }))
    await flush()
    expect(empty.handle().state.status).toBe('empty')
    ready.dispose()
    empty.dispose()
  })

  it('maps rejections to ErrorDto and keeps cached data on a failed refresh', async () => {
    const client = new QueryClient()
    let fail = true
    const query = mount(client, () => ({
      key: ['c'],
      fetch: async () => {
        if (fail) throw AppError.of('DB_BUSY')
        return 'data'
      },
    }))
    await flush()
    expect(query.handle().state).toMatchObject({ status: 'error', error: { code: 'DB_BUSY', retryable: true } })

    fail = false
    await query.handle().refresh()
    expect(query.handle().state).toEqual({ status: 'ready', data: 'data' })

    fail = true
    await query.handle().refresh()
    expect(query.handle().state).toEqual({ status: 'ready', data: 'data' })
    expect(query.handle().stale).toBe(true)
    expect(query.handle().refreshError?.code).toBe('DB_BUSY')
    query.dispose()
  })

  it('refetches mounted queries on invalidation and ignores responses after unmount', async () => {
    const client = new QueryClient()
    const fetcher = vi.fn(async () => 'v')
    const query = mount(client, () => ({ key: ['d'], fetch: fetcher }))
    await flush()
    client.invalidate()
    await flush()
    expect(fetcher).toHaveBeenCalledTimes(2)

    query.dispose()
    client.invalidate()
    await flush()
    expect(fetcher).toHaveBeenCalledTimes(2)
  })

  it('drops a slow response for an outdated key (race protection)', async () => {
    const client = new QueryClient()
    let key = $state('slow')
    const resolvers: Record<string, (value: string) => void> = {}
    const query = mount(client, () => ({
      key: [key],
      fetch: () => new Promise<string>((resolve) => (resolvers[key] = resolve)),
    }))
    key = 'fast'
    flushSync()
    resolvers.fast?.('fast')
    await flush()
    resolvers.slow?.('slow')
    await flush()
    expect(query.handle().state).toEqual({ status: 'ready', data: 'fast' })
    query.dispose()
  })
})
