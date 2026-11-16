<script lang="ts">
  import { inlineRuns } from '../services/markdown'
  import StyledText from './StyledText.svelte'

  /** Card text with inline Markdown (`**bold**`, `*italic*`) rendered as styled text, never HTML. */
  let { text, lang }: { text: string; lang?: string } = $props()
  const runs = $derived(inlineRuns(text.normalize('NFC')))
</script>

<span {lang} dir="auto">{#each runs as run, index (index)}<StyledText {run} />{/each}</span>
