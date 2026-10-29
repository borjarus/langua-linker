<script lang="ts">
  import type { Snippet } from 'svelte'
  import { useServices } from '../../app/context'
  import Icon from '../../components/Icon.svelte'

  interface Props {
    title: string
    /** Shows a back button; hardware/browser back works regardless. */
    back?: boolean
    /** Complete screen on demo data: says it is a prototype, not production learning. Demo profile only. */
    prototype?: boolean
    /** Screen-specific prototype notice (e.g. demo drafts), shown instead of the learning one. Demo profile only. */
    notice?: string
    actions?: Snippet
    children: Snippet
  }

  let { title, back = false, prototype = false, notice, actions, children }: Props = $props()
  // The notices describe the mock; with the production adapter (`demo === null`) the same screen shows none.
  const { demo, i18n, router } = useServices()
</script>

<svelte:head>
  <title>{title} · {i18n.t('app.name')}</title>
</svelte:head>

<div class="screen">
  <header class="header">
    {#if back}
      <button type="button" class="btn btn-ghost btn-icon" aria-label={i18n.t('common.back')} onclick={() => router.back()}>
        <Icon name="back" />
      </button>
    {/if}
    <h1 tabindex="-1" data-screen-heading>{title}</h1>
    {#if actions}<div class="row actions">{@render actions()}</div>{/if}
  </header>
  {#if demo && notice}
    <p class="notice row"><Icon name="info" />{notice}</p>
  {:else if demo && prototype}
    <p class="notice row"><Icon name="info" />{i18n.t('common.prototype')}</p>
  {/if}
  <div class="stack">{@render children()}</div>
</div>

<style>
  .screen {
    display: grid;
    gap: var(--space-4);
  }

  .header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2);
  }

  h1 {
    flex: 1;
    min-width: 0;
  }

  .notice {
    padding: var(--space-2) var(--space-3);
    border: 1px dashed var(--color-border-strong);
    border-radius: var(--radius-md);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    flex-wrap: nowrap;
  }
</style>
