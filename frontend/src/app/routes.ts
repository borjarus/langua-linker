/**
 * Typed route table for the hash router (ADR-01). Pure TypeScript: no Svelte, no DOM.
 */

export const ROUTE_PATTERNS = {
  home: '/home',
  deck: '/deck/:id',
  study: '/study/:deckId',
  summary: '/summary/:sessionId',
  card: '/card/:id',
  editor: '/editor',
  browser: '/browser',
  stats: '/stats',
  settings: '/settings',
  settingsTransfer: '/settings/transfer',
  settingsBackup: '/settings/backup',
  settingsAttributions: '/settings/attributions',
  onboarding: '/onboarding',
  gallery: '/gallery',
} as const

export type RouteName = keyof typeof ROUTE_PATTERNS

type ParamNames<Pattern extends string> = Pattern extends `${string}:${infer Param}/${infer Rest}`
  ? Param | ParamNames<`/${Rest}`>
  : Pattern extends `${string}:${infer Param}`
    ? Param
    : never

export type RouteParams<Name extends RouteName> = { [K in ParamNames<(typeof ROUTE_PATTERNS)[Name]>]: string }
export type RouteQuery = Readonly<Record<string, string>>

export type MatchedRoute = {
  [Name in RouteName]: { name: Name; params: RouteParams<Name>; query: RouteQuery; path: string }
}[RouteName]

export type NotFoundRoute = { name: 'notFound'; params: { path: string }; query: RouteQuery; path: string }
export type Route = MatchedRoute | NotFoundRoute

export type RouteTarget = {
  [Name in RouteName]: keyof RouteParams<Name> extends never
    ? { name: Name; params?: RouteParams<Name>; query?: Record<string, string | null | undefined> }
    : { name: Name; params: RouteParams<Name>; query?: Record<string, string | null | undefined> }
}[RouteName]

export const DEFAULT_ROUTE: RouteTarget = { name: 'home' }

/** Allowed characters in a path parameter: ids, GUIDs and slugs only. */
const PARAM_PATTERN = /^[A-Za-z0-9_-]{1,128}$/

const compiled = (Object.entries(ROUTE_PATTERNS) as [RouteName, string][]).map(([name, pattern]) => ({
  name,
  segments: pattern.split('/').filter(Boolean),
}))

function parseQuery(search: string): RouteQuery {
  const query: Record<string, string> = {}
  for (const [key, value] of new URLSearchParams(search)) query[key] = value
  return query
}

/** Parses a path such as `/deck/12?tab=cards` (no leading `#`). */
export function matchPath(rawPath: string): Route {
  const [pathPart = '', search = ''] = rawPath.split('?', 2)
  const path = pathPart.startsWith('/') ? pathPart : `/${pathPart}`
  const query = parseQuery(search)
  const segments = path.split('/').filter(Boolean)
  for (const route of compiled) {
    if (route.segments.length !== segments.length) continue
    const params: Record<string, string> = {}
    let matched = true
    for (const [index, segment] of route.segments.entries()) {
      const actual = segments[index] ?? ''
      if (segment.startsWith(':')) {
        let decoded: string
        try {
          decoded = decodeURIComponent(actual)
        } catch {
          matched = false
          break
        }
        if (!PARAM_PATTERN.test(decoded)) {
          matched = false
          break
        }
        params[segment.slice(1)] = decoded
      } else if (segment !== actual) {
        matched = false
        break
      }
    }
    if (matched) return { name: route.name, params, query, path } as MatchedRoute
  }
  return { name: 'notFound', params: { path }, query, path }
}

export function parseHash(hash: string): Route | null {
  const raw = hash.replace(/^#/, '')
  if (raw === '' || raw === '/') return null
  return matchPath(raw)
}

/** Screens that update their own query (search text, filters) and react to it in place. */
const QUERY_IN_PLACE: ReadonlySet<Route['name']> = new Set(['browser'])

/**
 * Identity of the rendered screen: a new key remounts the screen and moves focus to its heading.
 * Query changes count too, except on screens that handle them in place — remounting those while
 * the user types would drop focus and close the mobile keyboard.
 */
export function screenKey(route: Route): string {
  const query = QUERY_IN_PLACE.has(route.name) ? '' : JSON.stringify(route.query)
  return `${route.name}:${JSON.stringify(route.params)}:${query}`
}

export function buildPath(target: RouteTarget): string {
  const pattern: string = ROUTE_PATTERNS[target.name]
  const params = (target.params ?? {}) as Record<string, string>
  const path = pattern.replace(/:(\w+)/g, (_, name: string) => {
    const value = params[name]
    if (value === undefined || !PARAM_PATTERN.test(value)) throw new Error(`Invalid route parameter "${name}" for ${target.name}`)
    return encodeURIComponent(value)
  })
  const search = new URLSearchParams()
  for (const [key, value] of Object.entries(target.query ?? {})) {
    if (value !== null && value !== undefined && value !== '') search.set(key, value)
  }
  const query = search.toString()
  return query ? `${path}?${query}` : path
}

export function href(target: RouteTarget): string {
  return `#${buildPath(target)}`
}

/** Routes reachable from outside the app (notifications, shared links). Everything else is rejected. */
const DEEP_LINK_ROUTES: ReadonlySet<RouteName> = new Set(['home', 'deck', 'study', 'card', 'stats', 'settings'])
const DEEP_LINK_SCHEME = 'langualinker:'

/**
 * Native deep-link adapter: validates an external URL and maps it to an internal path.
 * Query parameters are dropped; unknown hosts, schemes and routes return `null`.
 */
export function deepLinkToPath(url: string): string | null {
  let parsed: URL
  try {
    parsed = new URL(url)
  } catch {
    return null
  }
  if (parsed.protocol !== DEEP_LINK_SCHEME || parsed.host !== 'app') return null
  const route = matchPath(parsed.pathname)
  if (route.name === 'notFound' || !DEEP_LINK_ROUTES.has(route.name)) return null
  return buildPath({ name: route.name, params: route.params } as RouteTarget)
}
