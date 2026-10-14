import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { Router } from '../src/app/router.svelte'

const flush = () => new Promise((resolve) => setTimeout(resolve, 0))

async function waitForHash(hash: string) {
  for (let attempt = 0; attempt < 50 && window.location.hash !== hash; attempt += 1) await flush()
}

describe('Router', () => {
  let router: Router
  let stop: () => void

  beforeEach(() => {
    window.history.replaceState(null, '', '#/home')
    router = new Router(window)
    stop = router.start()
  })

  afterEach(() => stop())

  it('starts on the current hash and defaults empty hashes to /home', () => {
    expect(router.current.name).toBe('home')
    stop()
    window.history.replaceState(null, '', '#')
    router = new Router(window)
    stop = router.start()
    expect(router.current.name).toBe('home')
    expect(window.location.hash).toBe('#/home')
  })

  it('navigates, replaces and records the navigation kind', async () => {
    await router.navigate({ name: 'deck', params: { id: '5' } })
    expect(router.current).toMatchObject({ name: 'deck', params: { id: '5' } })
    expect(router.kind).toBe('push')
    expect(window.location.hash).toBe('#/deck/5')

    await router.replace({ name: 'stats' })
    expect(router.current.name).toBe('stats')
    expect(router.kind).toBe('replace')
  })

  it('handles anchor-style hash changes and back navigation as pop', async () => {
    await router.navigate({ name: 'browser' })
    window.location.hash = '#/settings'
    await waitForHash('#/settings')
    await flush()
    expect(router.current.name).toBe('settings')
    expect(router.kind).toBe('push')

    window.history.back()
    await waitForHash('#/browser')
    await flush()
    expect(router.current.name).toBe('browser')
    expect(router.kind).toBe('pop')
  })

  it('blocks navigation while a guard refuses and allows it after confirmation', async () => {
    let answer = false
    const remove = router.addGuard({ isBlocking: () => true, confirmLeave: async () => answer })

    expect(await router.navigate({ name: 'stats' })).toBe(false)
    expect(router.current.name).toBe('home')

    answer = true
    expect(await router.navigate({ name: 'stats' })).toBe(true)
    expect(router.current.name).toBe('stats')
    remove()
  })

  it('reverts a browser-driven change refused by a guard', async () => {
    await router.navigate({ name: 'editor' })
    router.addGuard({ isBlocking: () => true, confirmLeave: async () => false })
    window.location.hash = '#/stats'
    await flush()
    await waitForHash('#/editor')
    await flush()
    expect(router.current.name).toBe('editor')
  })

  it('dismisses the topmost open layer on Back instead of navigating', async () => {
    await router.navigate({ name: 'gallery' })
    const dismissed: string[] = []
    const removeLower = router.addLayer({ dismiss: () => dismissed.push('lower') })
    const removeUpper = router.addLayer({ dismiss: () => dismissed.push('upper') })

    window.history.back()
    await waitForHash('#/home')
    await waitForHash('#/gallery')
    await flush()
    expect(router.current.name).toBe('gallery')
    expect(window.location.hash).toBe('#/gallery')
    expect(dismissed).toEqual(['upper'])

    removeUpper()
    removeLower()
    window.history.back()
    await waitForHash('#/home')
    await flush()
    expect(router.current.name).toBe('home')
    expect(router.kind).toBe('pop')
    expect(dismissed).toEqual(['upper'])
  })

  it('lets programmatic navigation pass while a layer is open', async () => {
    let dismissed = false
    const remove = router.addLayer({ dismiss: () => (dismissed = true) })
    expect(await router.navigate({ name: 'stats' })).toBe(true)
    expect(router.current.name).toBe('stats')
    expect(dismissed).toBe(false)
    remove()
  })

  it('does not consult non-blocking guards', async () => {
    let asked = false
    router.addGuard({
      isBlocking: () => false,
      confirmLeave: async () => {
        asked = true
        return false
      },
    })
    expect(await router.navigate({ name: 'stats' })).toBe(true)
    expect(asked).toBe(false)
  })
})
