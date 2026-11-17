import { describe, expect, it } from 'vitest'
import { deepLinkToPath, matchPath, parseHash } from '../src/app/routes'
import { inlineRuns, stripInlineMarkdown } from '../src/services/markdown'

/**
 * Plan Phase 16 step 10: fuzzing of the renderer and the external-input parsers of the WebView.
 * A seeded generator keeps every run reproducible; a failure prints the seed and the input.
 */
function generator(seed: number) {
  let state = seed >>> 0
  const next = () => {
    // mulberry32
    state = (state + 0x6d2b79f5) >>> 0
    let t = state
    t = Math.imul(t ^ (t >>> 15), t | 1)
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61)
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
  const pick = <T>(items: readonly T[]): T => items[Math.floor(next() * items.length)] as T
  const text = (alphabet: readonly string[], max: number) => Array.from({ length: Math.floor(next() * (max + 1)) }, () => pick(alphabet)).join('')
  return { next, pick, text }
}

const RUNS = 3_000
const MARKDOWN_ALPHABET = ['*', '*', '_', '_', '\\', 'a', 'ż', ' ', '\n', '😀', '<', '>', '"', 'é']
const MARKERS = new Set(['*', '_', '\\'])

describe('fuzz: inline Markdown renderer', () => {
  it('never throws, keeps runs ordered inside the source and only drops marker characters', () => {
    const random = generator(0x5eed16)
    for (let run = 0; run < RUNS; run += 1) {
      const source = random.text(MARKDOWN_ALPHABET, 24)
      const context = JSON.stringify(source)
      const runs = inlineRuns(source)
      let previousEnd = 0
      for (const styled of runs) {
        expect(styled.start, context).toBeGreaterThanOrEqual(previousEnd)
        expect(styled.end, context).toBeGreaterThan(styled.start)
        expect(styled.end, context).toBeLessThanOrEqual(source.length)
        previousEnd = styled.end
      }
      // Removing markers is the only change: the visible text is a subsequence of the source
      // and every removed character is a marker.
      const visible = stripInlineMarkdown(source)
      let cursor = 0
      for (const char of source) {
        if (visible.startsWith(char, cursor)) cursor += char.length
        else expect(MARKERS.has(char), `${context} dropped ${JSON.stringify(char)}`).toBe(true)
      }
      expect(cursor, context).toBe(visible.length)
      expect(runs.map((styled) => styled.text).join(''), context).toBe(visible)
    }
  })
})

const URL_PARTS = ['langualinker:', '//', 'app', 'evil.com', '/', 'deck', 'card', 'study', 'settings', 'diagnostics', 'gallery', '12', '%2F', '%00', '..', '?', 'x=1', '#', 'javascript:', 'https:', '@', ':', 'ż', ' ']

describe('fuzz: deep links and hash routes', () => {
  it('maps only langualinker://app links of allowed routes and never yields a foreign target', () => {
    const random = generator(0xdee9)
    let accepted = 0
    for (let run = 0; run < RUNS; run += 1) {
      // Half the inputs start like a real link, so the accepting branch is exercised too.
      const url = (run % 2 === 0 ? 'langualinker://app/' : '') + random.text(URL_PARTS, run % 2 === 0 ? 3 : 8)
      const path = deepLinkToPath(url)
      if (path === null) continue
      accepted += 1
      const context = `${JSON.stringify(url)} -> ${path}`
      expect(url.startsWith('langualinker:'), context).toBe(true)
      expect(path.startsWith('/'), context).toBe(true)
      expect(path, context).not.toMatch(/[?#]|\.\.|:\/\//)
      const route = matchPath(path)
      expect(['home', 'deck', 'study', 'card', 'stats', 'settings'], context).toContain(route.name)
    }
    expect(accepted).toBeGreaterThan(0)
  })

  it('parses any hash without throwing and keeps route parameters to the allowed pattern', () => {
    const random = generator(0x4a54)
    for (let run = 0; run < RUNS; run += 1) {
      const hash = `#${random.text(URL_PARTS, 6)}`
      const route = parseHash(hash)
      if (!route || route.name === 'notFound') continue
      for (const value of Object.values(route.params as Record<string, string>)) {
        expect(value, hash).not.toMatch(/[/?#\s]/)
      }
    }
  })
})
