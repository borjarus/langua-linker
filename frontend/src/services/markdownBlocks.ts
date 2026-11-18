/**
 * Block Markdown of extended associations: `#`–`######` headings, paragraphs, `-`/`*` lists,
 * `>` quotes (nested blocks), `---` rules. Lines inside a paragraph or quote keep their breaks.
 * Inline text goes through `inlineRuns` (bold/italic); nothing becomes HTML, so the result is
 * rendered with Svelte text nodes only.
 */

export type MarkdownBlock =
  | { kind: 'heading'; level: number; text: string }
  | { kind: 'paragraph'; lines: string[] }
  | { kind: 'list'; items: string[] }
  | { kind: 'quote'; blocks: MarkdownBlock[] }
  | { kind: 'rule' }

const HEADING = /^(#{1,6})\s+(.*)$/u
const ITEM = /^[-*+]\s+(.*)$/u
const RULE = /^(?:-{3,}|\*{3,}|_{3,})$/u
const QUOTE = /^>\s?(.*)$/u

export function parseBlocks(source: string): MarkdownBlock[] {
  const lines = source.normalize('NFC').replace(/\r\n?/gu, '\n').split('\n')
  const blocks: MarkdownBlock[] = []
  let index = 0
  while (index < lines.length) {
    const line = (lines[index] ?? '').trim()
    if (line === '') {
      index += 1
      continue
    }
    const heading = HEADING.exec(line)
    if (heading) {
      blocks.push({ kind: 'heading', level: heading[1]!.length, text: heading[2]!.trim() })
      index += 1
      continue
    }
    if (RULE.test(line)) {
      blocks.push({ kind: 'rule' })
      index += 1
      continue
    }
    if (QUOTE.test(line)) {
      const inner: string[] = []
      while (index < lines.length) {
        const match = QUOTE.exec((lines[index] ?? '').trim())
        if (!match) break
        inner.push(match[1] ?? '')
        index += 1
      }
      blocks.push({ kind: 'quote', blocks: parseBlocks(inner.join('\n')) })
      continue
    }
    if (ITEM.test(line)) {
      const items: string[] = []
      while (index < lines.length) {
        const current = (lines[index] ?? '').trim()
        const match = ITEM.exec(current)
        if (match && !RULE.test(current)) {
          items.push(match[1]!.trim())
        } else if (current !== '' && items.length > 0 && /^\s+/u.test(lines[index] ?? '') && !HEADING.test(current)) {
          // An indented line continues the previous item.
          items[items.length - 1] += ` ${current}`
        } else {
          break
        }
        index += 1
      }
      blocks.push({ kind: 'list', items })
      continue
    }
    const paragraph: string[] = []
    while (index < lines.length) {
      const current = (lines[index] ?? '').trim()
      if (current === '' || HEADING.test(current) || RULE.test(current) || QUOTE.test(current) || ITEM.test(current)) break
      paragraph.push(current)
      index += 1
    }
    blocks.push({ kind: 'paragraph', lines: paragraph })
  }
  return blocks
}
