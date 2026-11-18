import { describe, expect, it } from 'vitest'
import type { WordLinkDto } from '../src/api/types'
import { richSegments } from '../src/components/LinkedText.svelte'
import { inlineRuns, speechText, stripInlineMarkdown } from '../src/services/markdown'

const styled = (text: string) => inlineRuns(text).map((run) => [run.text, run.bold, run.italic])

describe('inline Markdown of card text', () => {
  it('renders bold and italic without the markers', () => {
    expect(styled('**Danke.** /ˈdaŋkə/')).toEqual([
      ['Danke.', true, false],
      [' /ˈdaŋkə/', false, false],
    ])
    expect(styled('ein *kleiner* Hund')).toEqual([
      ['ein ', false, false],
      ['kleiner', false, true],
      [' Hund', false, false],
    ])
    expect(styled('__fett__')).toEqual([['fett', true, false]])
    expect(styled('**sehr *gut***')).toEqual([
      ['sehr ', true, false],
      ['gut', true, true],
    ])
  })

  it('leaves unmatched, spaced or escaped markers as text', () => {
    expect(stripInlineMarkdown('5 * 3 * 2')).toBe('5 * 3 * 2')
    expect(stripInlineMarkdown('**open')).toBe('**open')
    expect(stripInlineMarkdown('snake_case_word')).toBe('snake_case_word')
    expect(stripInlineMarkdown('\\*not italic\\*')).toBe('*not italic*')
    expect(stripInlineMarkdown('<b>x</b> & y')).toBe('<b>x</b> & y')
  })

  it('keeps word links on source ranges and gives them the surrounding style', () => {
    const text = '**Danke.** /ˈdaŋkə/'
    const link = { id: '1', startIndex: 2, endIndex: 7, surface: 'Danke', resolution: 'resolved' } as WordLinkDto
    const segments = richSegments(text, [link])
    expect(segments[0]).toMatchObject({ kind: 'link', bold: true })
    expect(segments.slice(1).map((segment) => (segment.kind === 'run' ? [segment.run.text, segment.run.bold] : null))).toEqual([
      ['.', true],
      [' /ˈdaŋkə/', false],
    ])
  })

  it('reads aloud only the bold part and never the IPA between slashes', () => {
    expect(speechText("**he's** /hiːz/")).toBe("he's")
    expect(speechText('**Danke.** /ˈdaŋkə/')).toBe('Danke.')
    expect(speechText('**ein** kleiner **Hund**')).toBe('ein Hund')
    expect(speechText('**ein *kleiner* Hund**')).toBe('ein kleiner Hund')
    expect(speechText('Danke /ˈdaŋkə/ schön')).toBe('Danke schön')
    expect(speechText('und/oder')).toBe('und/oder')
    expect(speechText('ein *kleiner* Hund')).toBe('ein kleiner Hund')
  })
})
