<script lang="ts">
  import { useServices } from '../app/context'
  import type { StyledRun } from '../services/markdown'

  interface Props {
    /** The word as shown, with its source range and style. */
    run: StyledRun
    onselect: () => void
  }

  let { run, onselect }: Props = $props()
  const { i18n } = useServices()
</script>

<!-- A word the content left without a link: tapping it offers to link it to a word note. -->
<button type="button" class="word" aria-label={i18n.t('wordLink.addLabel', { word: run.text })} aria-haspopup="dialog" onclick={onselect}
  >{#if run.bold && run.italic}<strong><em>{run.text}</em></strong>{:else if run.bold}<strong>{run.text}</strong>{:else if run.italic}<em>{run.text}</em>{:else}{run.text}{/if}</button
>

<style>
  .word {
    display: inline;
    min-height: auto;
    margin: 0;
    padding: 0 0.1em;
    border: 0;
    border-bottom: 2px dotted var(--color-border);
    border-radius: 0;
    background: transparent;
    font: inherit;
    white-space: normal;
    overflow-wrap: anywhere;
    text-align: inherit;
    cursor: pointer;
  }

  .word:hover {
    border-bottom-color: var(--color-border-strong);
    background: var(--color-primary-soft);
  }
</style>
