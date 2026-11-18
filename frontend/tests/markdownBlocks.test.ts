import { describe, expect, it } from 'vitest'
import { parseBlocks } from '../src/services/markdownBlocks'

describe('parseBlocks', () => {
  it('reads headings, paragraphs with line breaks, lists, quotes and rules', () => {
    const source = [
      '## 1. Rozkład dźwiękowy',
      '',
      '**Gebirge** wymawia się [ɡəˈbɪʁɡə]',
      'druga linia',
      '',
      '- **Ge** – przedrostek',
      '- **birge** – Berg',
      '',
      '---',
      '',
      '> **„Wir wandern im Gebirge.”**',
      '> (Wędrujemy w górach.)',
    ].join('\n')
    expect(parseBlocks(source)).toEqual([
      { kind: 'heading', level: 2, text: '1. Rozkład dźwiękowy' },
      { kind: 'paragraph', lines: ['**Gebirge** wymawia się [ɡəˈbɪʁɡə]', 'druga linia'] },
      { kind: 'list', items: ['**Ge** – przedrostek', '**birge** – Berg'] },
      { kind: 'rule' },
      { kind: 'quote', blocks: [{ kind: 'paragraph', lines: ['**„Wir wandern im Gebirge.”**', '(Wędrujemy w górach.)'] }] },
    ])
  })

  it('keeps HTML-looking text as plain text and tolerates CRLF', () => {
    expect(parseBlocks('<b>x</b>\r\n\r\n#no heading')).toEqual([
      { kind: 'paragraph', lines: ['<b>x</b>'] },
      { kind: 'paragraph', lines: ['#no heading'] },
    ])
  })
})
