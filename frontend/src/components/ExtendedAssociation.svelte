<script lang="ts">
  import { useServices } from '../app/context'
  import { parseBlocks } from '../services/markdownBlocks'
  import Icon from './Icon.svelte'
  import MarkdownBlocks from './MarkdownBlocks.svelte'

  /** The long Markdown version of an association, collapsed until the learner opens it. */
  let { text }: { text: string } = $props()
  const { i18n } = useServices()
  const regionId = $props.id()
  let open = $state(false)
</script>

<button type="button" class="btn btn-ghost extended-toggle" aria-expanded={open} aria-controls={regionId} onclick={() => (open = !open)}>
  <Icon name={open ? 'chevronUp' : 'chevronDown'} />
  {i18n.t(open ? 'association.extended.hide' : 'association.extended.show')}
</button>
<div id={regionId} class="extended" hidden={!open}>
  {#if open}<MarkdownBlocks blocks={parseBlocks(text)} />{/if}
</div>

<style>
  .extended-toggle {
    justify-self: start;
  }

  .extended {
    padding-top: var(--space-2);
    border-top: 1px solid var(--color-border);
    overflow-wrap: anywhere;
  }
</style>
