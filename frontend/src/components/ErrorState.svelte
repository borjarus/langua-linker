<script lang="ts">
  import type { ErrorDto } from '../api/types'
  import { useServices } from '../app/context'
  import Icon from './Icon.svelte'

  interface Props {
    error: ErrorDto
    /** Shown only when the error is retryable. */
    onretry?: () => void
    compact?: boolean
  }

  let { error, onretry, compact = false }: Props = $props()
  const { i18n } = useServices()
  const message = $derived(i18n.tDynamic(error.messageKey, 'errors.INTERNAL'))
</script>

<section class="error" class:compact role="alert">
  <span class="icon"><Icon name="alert" size={compact ? 20 : 32} /></span>
  <div class="body">
    {#if !compact}<h2>{i18n.t('errors.title')}</h2>{/if}
    <p>{message}</p>
  </div>
  {#if error.retryable && onretry}
    <button type="button" class="btn" onclick={onretry}>
      <Icon name="refresh" />
      {i18n.t('common.retry')}
    </button>
  {/if}
</section>

<style>
  .error {
    display: grid;
    justify-items: center;
    gap: var(--space-3);
    padding: var(--space-5) var(--space-4);
    border: 1px solid var(--color-error);
    border-radius: var(--radius-md);
    background: var(--color-error-soft);
    text-align: center;
  }

  .compact {
    grid-template-columns: auto 1fr auto;
    align-items: center;
    justify-items: start;
    padding: var(--space-2) var(--space-3);
    text-align: start;
  }

  .icon {
    color: var(--color-error);
  }

  h2 {
    font-size: var(--text-lg);
  }
</style>
