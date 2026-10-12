import { describe, expect, it } from 'vitest'
import { ROUTE_PATTERNS, buildPath, deepLinkToPath, href, matchPath, parseHash, screenKey } from '../src/app/routes'

describe('route table', () => {
  it('contains the nine primary routes from the plan', () => {
    for (const path of ['/home', '/deck/:id', '/study/:deckId', '/summary/:sessionId', '/card/:id', '/editor', '/browser', '/stats', '/settings']) {
      expect(Object.values(ROUTE_PATTERNS)).toContain(path)
    }
  })

  it('matches params and query strings', () => {
    const route = matchPath('/deck/12?tab=cards')
    expect(route).toMatchObject({ name: 'deck', params: { id: '12' }, query: { tab: 'cards' } })
    expect(matchPath('/editor?deckId=3&noteId=7')).toMatchObject({ name: 'editor', query: { deckId: '3', noteId: '7' } })
  })

  it('returns notFound for unknown paths and invalid params', () => {
    expect(matchPath('/nope').name).toBe('notFound')
    expect(matchPath('/deck/a%2Fb').name).toBe('notFound')
    expect(matchPath('/deck/<script>').name).toBe('notFound')
    expect(matchPath('/deck/%E0%A4%A').name).toBe('notFound')
  })

  it('round-trips build and match', () => {
    const path = buildPath({ name: 'summary', params: { sessionId: 'session-90002' } })
    expect(path).toBe('/summary/session-90002')
    expect(matchPath(path)).toMatchObject({ name: 'summary', params: { sessionId: 'session-90002' } })
    expect(href({ name: 'browser', query: { q: 'zażółć', deck: null } })).toBe('#/browser?q=za%C5%BC%C3%B3%C5%82%C4%87')
  })

  it('refuses to build paths with unsafe params', () => {
    expect(() => buildPath({ name: 'deck', params: { id: '../etc' } })).toThrow()
  })

  it('treats an empty hash as "use default"', () => {
    expect(parseHash('')).toBeNull()
    expect(parseHash('#/')).toBeNull()
    expect(parseHash('#/stats')?.name).toBe('stats')
  })
})

describe('deep-link adapter', () => {
  it('maps allowed app links to internal paths and drops query parameters', () => {
    expect(deepLinkToPath('langualinker://app/deck/12?evil=1')).toBe('/deck/12')
    expect(deepLinkToPath('langualinker://app/home')).toBe('/home')
  })

  it('rejects foreign schemes, hosts, unknown or non-linkable routes', () => {
    expect(deepLinkToPath('https://evil.example/deck/1')).toBeNull()
    expect(deepLinkToPath('langualinker://other/deck/1')).toBeNull()
    expect(deepLinkToPath('langualinker://app/settings/backup')).toBeNull()
    expect(deepLinkToPath('langualinker://app/gallery')).toBeNull()
    expect(deepLinkToPath('not a url')).toBeNull()
  })
})

describe('screen identity', () => {
  it('ignores the query only on screens that handle it in place', () => {
    expect(screenKey(matchPath('/browser?q=Haus'))).toBe(screenKey(matchPath('/browser')))
    expect(screenKey(matchPath('/study/1?session=s1'))).not.toBe(screenKey(matchPath('/study/1')))
    expect(screenKey(matchPath('/deck/1'))).not.toBe(screenKey(matchPath('/deck/2')))
  })
})
