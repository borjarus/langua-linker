<script lang="ts">
  import type { WordLinkDto } from '../api/types'
  import { useServices } from '../app/context'

  interface Props {
    link: WordLinkDto
    onselect: (link: WordLinkDto) => void
  }

  let { link, onselect }: Props = $props()
  const { i18n } = useServices()
  const state = $derived(i18n.t(`wordLink.resolution.${link.resolution}`))

  function onkeydown(event: KeyboardEvent) {
    if (event.key !== 'Enter' && event.key !== ' ') return
    event.preventDefault()
    onselect(link)
  }
</script>

<!-- An inline span, not a <button>: a button is an atomic box, and the line may break between it
     and the punctuation that follows ("fine" / ", and"). The span keeps the word with its comma. -->
<span
  role="button"
  tabindex="0"
  class="word resolution-{link.resolution}"
  aria-label={i18n.t('wordLink.label', { word: link.surface, state })}
  aria-haspopup="dialog"
  onclick={() => onselect(link)}
  {onkeydown}
>{link.surface}{#if link.resolution === 'ambiguous'}<span class="marker" aria-hidden="true">?</span>{/if}</span>

<style>
  .word {
    display: inline;
    border-bottom: 2px solid var(--color-primary);
    cursor: pointer;
    /* Inline text: a multi-word phrase wraps like the rest of the sentence. */
    overflow-wrap: anywhere;
    -webkit-tap-highlight-color: transparent;
  }

  .word:focus-visible {
    outline: 2px solid var(--color-primary);
    outline-offset: 2px;
    border-radius: 2px;
  }

  .word:hover {
    background: var(--color-primary-soft);
  }

  .resolution-unresolved {
    border-bottom-style: dashed;
    border-bottom-color: var(--color-border-strong);
  }

  .resolution-ambiguous {
    border-bottom-style: dotted;
    border-bottom-color: var(--color-warning);
  }

  .marker {
    margin-inline-start: 0.1em;
    color: var(--color-warning);
    font-size: 0.75em;
    font-weight: var(--weight-bold);
    vertical-align: super;
  }
</style>
