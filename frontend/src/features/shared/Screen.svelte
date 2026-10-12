<script lang="ts">
  import type { Snippet } from 'svelte'
  import { useServices } from '../../app/context'
  import Icon from '../../components/Icon.svelte'

  interface Props {
    title: string
    /** Shows a back button; hardware/browser back works regardless. */
    back?: boolean
    /** Plan phase that replaces this placeholder with the complete screen. */
    phase?: number
    actions?: Snippet
    children: Snippet
  }

  let { title, back = false, phase, actions, children }: Props = $props()
  const { i18n, router } = useServices()
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
  {#if phase !== undefined}
    <p class="placeholder row"><Icon name="info" />{i18n.t('common.phasePlaceholder', { phase })}</p>
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

  .placeholder {
    padding: var(--space-2) var(--space-3);
    border: 1px dashed var(--color-border-strong);
    border-radius: var(--radius-md);
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    flex-wrap: nowrap;
  }
</style>
