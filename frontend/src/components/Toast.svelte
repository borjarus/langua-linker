<script lang="ts">
  import { useServices } from '../app/context'
  import Icon from './Icon.svelte'

  /** Renders the toast queue of the current root. Errors are announced assertively. */
  const { toasts, i18n } = useServices()
</script>

<section class="toasts" aria-label={i18n.t('toast.region')}>
  <div role="status" aria-live="polite" class="stack-list">
    {#each toasts.items.filter((item) => item.tone !== 'error') as item (item.id)}
      <div class="toast tone-{item.tone}">
        <Icon name={item.tone === 'success' ? 'check' : 'info'} />
        <p>{item.message}</p>
        <button type="button" class="btn btn-ghost btn-icon" aria-label={i18n.t('toast.dismiss')} onclick={() => toasts.dismiss(item.id)}>
          <Icon name="close" />
        </button>
      </div>
    {/each}
  </div>
  <div role="alert" class="stack-list">
    {#each toasts.items.filter((item) => item.tone === 'error') as item (item.id)}
      <div class="toast tone-error">
        <Icon name="alert" />
        <p>{item.message}</p>
        <button type="button" class="btn btn-ghost btn-icon" aria-label={i18n.t('toast.dismiss')} onclick={() => toasts.dismiss(item.id)}>
          <Icon name="close" />
        </button>
      </div>
    {/each}
  </div>
</section>

<style>
  .toasts {
    position: fixed;
    inset-inline: var(--space-4);
    bottom: calc(var(--space-4) + var(--bottom-nav-space, 0px) + env(safe-area-inset-bottom));
    z-index: 60;
    display: grid;
    justify-items: center;
    pointer-events: none;
  }

  .stack-list {
    display: grid;
    gap: var(--space-2);
    width: min(100%, 28rem);
  }

  .toast {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-2) var(--space-2) var(--space-4);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    box-shadow: var(--shadow-2);
    pointer-events: auto;
  }

  .tone-success {
    border-color: var(--color-success);
  }

  .tone-error {
    border-color: var(--color-error);
    background: var(--color-error-soft);
  }
</style>
