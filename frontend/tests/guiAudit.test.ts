import { readdirSync, readFileSync } from 'node:fs'
import { join, relative, resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import { ROUTE_PATTERNS, type RouteName } from '../src/app/routes'

// Phase 6 gate, steps 1 and 9: every route is reachable from the interface, and the active
// frontend has no React dependency and no pre-runes Svelte patterns.

const ROOT = process.cwd()
const SRC = resolve(ROOT, 'src')
/** Debug screens of the dev profile; release bundles exclude them (`verify:release`). */
const DEV_ONLY: ReadonlySet<RouteName> = new Set(['gallery', 'diagnostics'])

function files(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) =>
    entry.isDirectory() ? files(join(dir, entry.name)) : /\.(svelte|ts)$/.test(entry.name) ? [join(dir, entry.name)] : [],
  )
}

const sources = files(SRC).map((file) => ({ file: relative(ROOT, file).replaceAll('\\', '/'), text: readFileSync(file, 'utf8') }))

describe('routes', () => {
  it.each(Object.keys(ROUTE_PATTERNS).filter((name) => !DEV_ONLY.has(name as RouteName)))('%s has an entry point in the UI', (name) => {
    // A typed target (`{ name: 'x' }` for navigate/href) anywhere except the route table itself.
    const entry = sources.filter(({ file, text }) => file !== 'src/app/routes.ts' && text.includes(`name: '${name}'`))
    expect(entry.map(({ file }) => file)).not.toEqual([])
  })
})

describe('frontend hygiene', () => {
  it('has no React packages', () => {
    const pkg = JSON.parse(readFileSync(resolve(ROOT, 'package.json'), 'utf8')) as Record<string, Record<string, string> | undefined>
    const names = [...Object.keys(pkg.dependencies ?? {}), ...Object.keys(pkg.devDependencies ?? {})]
    expect(names.filter((name) => /(^|[/-])react($|[-/])/.test(name))).toEqual([])
  })

  it.each([
    ['legacy reactive statement `$:`', /^\s*\$:/m],
    ['legacy prop `export let`', /<script[^>]*>[\s\S]*?\bexport let\b/],
    ['legacy event directive `on:`', /<[a-zA-Z][^>]*\son:[a-z]+[=|>\s]/],
    ['legacy `<slot>`', /<slot[\s/>]/],
    ['createEventDispatcher', /createEventDispatcher/],
    ['a React import', /from ['"]react(-dom)?['"]/],
  ])('no %s in src', (_, pattern) => {
    expect(sources.filter(({ text }) => pattern.test(text)).map(({ file }) => file)).toEqual([])
  })

  it('screens never call Tauri directly', () => {
    const direct = sources.filter(({ file, text }) => !file.startsWith('src/api/') && !file.startsWith('src/services/') && /@tauri-apps\/api/.test(text))
    expect(direct.map(({ file }) => file)).toEqual([])
  })
})
