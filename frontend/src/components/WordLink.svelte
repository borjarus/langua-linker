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
</script>

<button
  type="button"
  class="word resolution-{link.resolution}"
  aria-label={i18n.t('wordLink.label', { word: link.surface, state })}
  aria-haspopup="dialog"
  onclick={() => onselect(link)}
>
  {link.surface}{#if link.resolution === 'ambiguous'}<span class="marker" aria-hidden="true">?</span>{/if}
</button>

<style>
  .word {
    display: inline;
    min-height: auto;
    margin: 0;
    padding: 0 0.1em;
    border: 0;
    border-bottom: 2px solid var(--color-primary);
    border-radius: 0;
    background: transparent;
    font: inherit;
    cursor: pointer;
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
