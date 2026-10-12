<script lang="ts">
  import type { Snippet } from 'svelte'
  import { useServices } from '../app/context'
  import { focusTrap } from './focusTrap'
  import Icon from './Icon.svelte'

  interface Props {
    open: boolean
    title: string
    onclose: () => void
    /** `sheet` docks to the bottom edge on narrow screens. */
    variant?: 'dialog' | 'sheet'
    /** Destructive confirmations should not close on a stray backdrop tap. */
    dismissible?: boolean
    children: Snippet
    actions?: Snippet
  }

  let { open, title, onclose, variant = 'dialog', dismissible = true, children, actions }: Props = $props()
  const { i18n } = useServices()
  const titleId = $props.id()

  function onBackdrop(event: MouseEvent) {
    if (dismissible && event.target === event.currentTarget) onclose()
  }
</script>

{#if open}
  <!-- Backdrop click is a pointer convenience; keyboard users close with Escape or the close button. -->
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="backdrop backdrop-{variant}" onclick={onBackdrop}>
    <div
      class="panel panel-{variant}"
      role="dialog"
      aria-modal="true"
      aria-labelledby={titleId}
      tabindex="-1"
      {@attach focusTrap(() => {
        if (dismissible) onclose()
      })}
    >
      <header class="header">
        <h2 id={titleId}>{title}</h2>
        <button type="button" class="btn btn-ghost btn-icon" aria-label={i18n.t('dialog.close')} onclick={onclose}>
          <Icon name="close" />
        </button>
      </header>
      <div class="content">{@render children()}</div>
      {#if actions}<footer class="row actions">{@render actions()}</footer>{/if}
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: grid;
    place-items: center;
    padding: var(--space-4);
    background: var(--color-surface-overlay);
  }

  .panel {
    display: grid;
    gap: var(--space-4);
    width: min(100%, 32rem);
    max-height: calc(100dvh - 2 * var(--space-4));
    overflow: auto;
    padding: var(--space-5);
    border-radius: var(--radius-lg);
    background: var(--color-surface);
    box-shadow: var(--shadow-2);
    animation: rise var(--motion-medium) var(--motion-ease);
  }

  .header {
    display: flex;
    align-items: start;
    justify-content: space-between;
    gap: var(--space-3);
  }

  .header h2 {
    font-size: var(--text-lg);
    padding-top: var(--space-2);
  }

  .actions {
    justify-content: flex-end;
  }

  @media (max-width: 52rem) {
    .backdrop-sheet {
      place-items: end stretch;
      padding: 0;
    }

    .panel-sheet {
      width: 100%;
      border-radius: var(--radius-lg) var(--radius-lg) 0 0;
      padding-bottom: calc(var(--space-5) + env(safe-area-inset-bottom));
    }
  }

  @keyframes rise {
    from {
      transform: translateY(1rem);
      opacity: 0;
    }
  }
</style>
