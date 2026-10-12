<script lang="ts">
  import type { GrammarTipDto } from '../api/types'
  import { useServices } from '../app/context'
  import Disclosure from './Disclosure.svelte'

  interface Props {
    tips: GrammarTipDto[]
    expanded?: boolean
  }

  let { tips, expanded = false }: Props = $props()
  const { i18n } = useServices()
  const ordered = $derived([...tips].sort((a, b) => a.order - b.order))
</script>

<Disclosure title={i18n.t('grammar.title')} {expanded} count={tips.length}>
  {#if ordered.length === 0}
    <p class="muted">{i18n.t('grammar.empty')}</p>
  {:else}
    <ol class="tips">
      {#each ordered as tip (tip.id)}
        <li>{tip.text}</li>
      {/each}
    </ol>
  {/if}
</Disclosure>

<style>
  .tips {
    display: grid;
    gap: var(--space-2);
    margin: 0;
    padding-inline-start: var(--space-5);
  }
</style>
