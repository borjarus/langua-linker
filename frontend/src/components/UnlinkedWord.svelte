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

  function onkeydown(event: KeyboardEvent) {
    if (event.key !== 'Enter' && event.key !== ' ') return
    event.preventDefault()
    onselect()
  }
</script>

<!-- A word the content left without a link: tapping it offers to link it to a word note. An inline
     span like WordLink, so the punctuation after it never wraps onto the next line. -->
<span role="button" tabindex="0" class="word" aria-label={i18n.t('wordLink.addLabel', { word: run.text })} aria-haspopup="dialog" onclick={onselect} {onkeydown}
  >{#if run.bold && run.italic}<strong><em>{run.text}</em></strong>{:else if run.bold}<strong>{run.text}</strong>{:else if run.italic}<em>{run.text}</em>{:else}{run.text}{/if}</span
>

<style>
  .word {
    display: inline;
    border-bottom: 2px dotted var(--color-border);
    cursor: pointer;
    overflow-wrap: anywhere;
    -webkit-tap-highlight-color: transparent;
  }

  .word:focus-visible {
    outline: 2px solid var(--color-primary);
    outline-offset: 2px;
    border-radius: 2px;
  }

  .word:hover {
    border-bottom-color: var(--color-border-strong);
    background: var(--color-primary-soft);
  }
</style>
