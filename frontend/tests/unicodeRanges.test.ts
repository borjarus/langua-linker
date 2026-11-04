import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import { segmentText } from '../src/components/LinkedText.svelte'
import { utf16Slice } from '../src/services/text'

interface RangeCase {
  name: string
  text: string
  nfc: string
  start: number
  end: number
  slice: string | null
}

// Shared with the Rust domain tests (crates/domain/src/text.rs): one rule for ranges everywhere.
const fixture = JSON.parse(readFileSync(resolve(process.cwd(), '../tests/fixtures/unicode/ranges.json'), 'utf8')) as { cases: RangeCase[] }

describe('UTF-16 ranges over NFC text (shared fixture)', () => {
  it.each(fixture.cases.map((item) => [item.name, item] as const))('%s', (_, item) => {
    expect(item.text.normalize('NFC')).toBe(item.nfc)
    expect(utf16Slice(item.nfc, item.start, item.end)).toBe(item.slice)
  })

  it('renders exactly the valid fixture ranges as links', () => {
    for (const item of fixture.cases) {
      const link = { id: '1', startIndex: item.start, endIndex: item.end, surface: item.slice ?? 'x' } as Parameters<typeof segmentText>[1][number]
      const linked = segmentText(item.text, [link]).filter((segment) => segment.kind === 'link')
      expect(linked.length, item.name).toBe(item.slice === null ? 0 : 1)
    }
  })
})
