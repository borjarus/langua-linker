<script lang="ts">
  import type { StyledRun } from '../services/markdown'

  let { run }: { run: StyledRun } = $props()
</script>

{#if run.bold && run.italic}<strong><em>{run.text}</em></strong>{:else if run.bold}<strong>{run.text}</strong>{:else if run.italic}<em>{run.text}</em>{:else}{run.text}{/if}
