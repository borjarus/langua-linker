<script lang="ts" module>
  import type { WordLinkDto } from '../api/types'

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
      const valid =
        link.startIndex >= cursor &&
        link.endIndex <= normalized.length &&
        link.startIndex < link.endIndex &&
        normalized.slice(link.startIndex, link.endIndex) === link.surface.normalize('NFC')
      if (!valid) continue
      if (link.startIndex > cursor) segments.push({ kind: 'text', text: normalized.slice(cursor, link.startIndex) })
      segments.push({ kind: 'link', text: link.surface, link })
      cursor = link.endIndex
    }
    if (cursor < normalized.length) segments.push({ kind: 'text', text: normalized.slice(cursor) })
    return segments
  }
</script>

<script lang="ts">
  import WordLink from './WordLink.svelte'

  interface Props {
    text: string
    links: WordLinkDto[]
    lang?: string
    onselect: (link: WordLinkDto) => void
  }

  let { text, links, lang, onselect }: Props = $props()
  const segments = $derived(segmentText(text, links))
</script>

<span {lang} dir="auto">
  {#each segments as segment, index (index)}
    {#if segment.kind === 'link'}<WordLink link={segment.link} {onselect} />{:else}{segment.text}{/if}
  {/each}
</span>
