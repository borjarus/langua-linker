<script lang="ts" module>
  import type { WordLinkDto } from '../api/types'
  import { inlineRuns, type StyledRun } from '../services/markdown'
  import { utf16Slice } from '../services/text'

  export type TextSegment = { kind: 'text'; text: string } | { kind: 'link'; text: string; link: WordLinkDto }

  /**
   * Splits NFC text by UTF-16 half-open ranges (plan §14.1). Links whose surface does not match
   * the text at their range, or that overlap an earlier link, are rendered as plain text.
   */
  export function segmentText(text: string, links: readonly WordLinkDto[]): TextSegment[] {
    const normalized = text.normalize('NFC')
    const segments: TextSegment[] = []
    let cursor = 0
    for (const link of [...links].sort((a, b) => a.startIndex - b.startIndex)) {
      const valid = link.startIndex >= cursor && utf16Slice(normalized, link.startIndex, link.endIndex) === link.surface.normalize('NFC')
      if (!valid) continue
      if (link.startIndex > cursor) segments.push({ kind: 'text', text: normalized.slice(cursor, link.startIndex) })
      segments.push({ kind: 'link', text: link.surface, link })
      cursor = link.endIndex
    }
    if (cursor < normalized.length) segments.push({ kind: 'text', text: normalized.slice(cursor) })
    return segments
  }

  export type RichSegment =
    | { kind: 'run'; run: StyledRun }
    | { kind: 'link'; link: WordLinkDto; bold: boolean; italic: boolean }
    /** A word without a link that the learner may link (`linkableWords`). */
    | { kind: 'word'; run: StyledRun }

  /** A phrase of the text, as source offsets (UTF-16, NFC), offered for a new link. */
  export interface WordDraft {
    startIndex: number
    endIndex: number
    surface: string
  }

  /** Letters (and digits) joined by apostrophes or hyphens: `don't`, `well-known`, `Straße`. */
  const WORD = /[\p{L}\p{M}\p{N}]+(?:['’-][\p{L}\p{M}\p{N}]+)*/gu
  /** A pronunciation in slashes, `/ʃiː ɪz/`: no space right inside either slash, so "Yes. / No." is not one. */
  const IPA = /\/(?!\s)[^/\n]*?[^\s/]\//gu

  /** Source ranges of the IPA transcriptions of `text`; their words are never offered for a link. */
  export function ipaRanges(text: string): [number, number][] {
    return Array.from(text.matchAll(IPA), (match) => [match.index, match.index + match[0].length] as [number, number])
  }

  /** Splits the plain runs into words (`word`) and the rest (`run`), skipping IPA. */
  export function splitWords(segments: RichSegment[], text: string): RichSegment[] {
    const ipa = ipaRanges(text.normalize('NFC'))
    const inIpa = (start: number, end: number) => ipa.some(([from, to]) => start < to && end > from)
    const result: RichSegment[] = []
    for (const segment of segments) {
      if (segment.kind !== 'run') {
        result.push(segment)
        continue
      }
      const { run } = segment
      let cursor = 0
      const piece = (from: number, to: number): StyledRun => ({ ...run, start: run.start + from, end: run.start + to, text: run.text.slice(from, to) })
      for (const match of run.text.matchAll(WORD)) {
        const from = match.index
        const to = from + match[0].length
        if (inIpa(run.start + from, run.start + to)) continue
        if (from > cursor) result.push({ kind: 'run', run: piece(cursor, from) })
        result.push({ kind: 'word', run: piece(from, to) })
        cursor = to
      }
      if (cursor < run.text.length) result.push({ kind: 'run', run: piece(cursor, run.text.length) })
    }
    return result
  }

  /**
   * [`segmentText`] with inline Markdown (`**bold**`, `*italic*`): link ranges are source offsets
   * (markers included), so the styled runs are cut at the link boundaries; a link takes the style
   * of its first character.
   */
  export function richSegments(text: string, links: readonly WordLinkDto[]): RichSegment[] {
    const normalized = text.normalize('NFC')
    const runs = inlineRuns(normalized)
    const result: RichSegment[] = []
    let offset = 0
    for (const segment of segmentText(normalized, links)) {
      const start = segment.kind === 'link' ? segment.link.startIndex : offset
      const end = segment.kind === 'link' ? segment.link.endIndex : offset + segment.text.length
      offset = end
      if (segment.kind === 'link') {
        const first = runs.find((run) => run.end > start && run.start < end)
        result.push({ kind: 'link', link: segment.link, bold: first?.bold ?? false, italic: first?.italic ?? false })
        continue
      }
      for (const run of runs) {
        const from = Math.max(run.start, start)
        const to = Math.min(run.end, end)
        if (from >= to) continue
        result.push({ kind: 'run', run: { ...run, start: from, end: to, text: run.text.slice(from - run.start, to - run.start) } })
      }
    }
    return result
  }
</script>

<script lang="ts">
  import StyledText from './StyledText.svelte'
  import UnlinkedWord from './UnlinkedWord.svelte'
  import WordLink from './WordLink.svelte'

  interface Props {
    text: string
    links: WordLinkDto[]
    lang?: string
    onselect: (link: WordLinkDto) => void
    /** Makes the words without a link selectable too (IPA excluded), to add a link for them. */
    onaddword?: (draft: WordDraft) => void
  }

  let { text, links, lang, onselect, onaddword }: Props = $props()
  const segments = $derived(onaddword ? splitWords(richSegments(text, links), text) : richSegments(text, links))
</script>

<span {lang} dir="auto">
  {#each segments as segment, index (index)}
    {#if segment.kind === 'link'}
      {#if segment.bold}<strong><WordLink link={segment.link} {onselect} /></strong>{:else if segment.italic}<em><WordLink link={segment.link} {onselect} /></em>{:else}<WordLink link={segment.link} {onselect} />{/if}
    {:else if segment.kind === 'word' && onaddword}{@const run = segment.run}<UnlinkedWord {run} onselect={() => onaddword({ startIndex: run.start, endIndex: run.end, surface: run.text })} />{:else}<StyledText run={segment.run} />{/if}
  {/each}
</span>
