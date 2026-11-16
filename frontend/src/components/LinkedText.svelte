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

  export type RichSegment = { kind: 'run'; run: StyledRun } | { kind: 'link'; link: WordLinkDto; bold: boolean; italic: boolean }

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
  import WordLink from './WordLink.svelte'

  interface Props {
    text: string
    links: WordLinkDto[]
    lang?: string
    onselect: (link: WordLinkDto) => void
  }

  let { text, links, lang, onselect }: Props = $props()
  const segments = $derived(richSegments(text, links))
</script>

<span {lang} dir="auto">
  {#each segments as segment, index (index)}
    {#if segment.kind === 'link'}
      {#if segment.bold}<strong><WordLink link={segment.link} {onselect} /></strong>{:else if segment.italic}<em><WordLink link={segment.link} {onselect} /></em>{:else}<WordLink link={segment.link} {onselect} />{/if}
    {:else}<StyledText run={segment.run} />{/if}
  {/each}
</span>
