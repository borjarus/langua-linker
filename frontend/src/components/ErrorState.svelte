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

  /* Wraps instead of a fixed icon | text | button grid, so at large text sizes the retry button
     drops below the message rather than squeezing it to a few characters per line. */
  .compact {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    padding: var(--space-2) var(--space-3);
    text-align: start;
  }

  .compact .body {
    flex: 1 1 12rem;
    min-width: 0;
  }

  .compact .btn {
    margin-inline-start: auto;
  }

  .icon {
    color: var(--color-error);
  }

  h2 {
    font-size: var(--text-lg);
  }
</style>
