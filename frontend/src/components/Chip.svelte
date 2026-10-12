<script lang="ts">
  import Icon, { type IconName } from './Icon.svelte'

  export type ChipTone = 'neutral' | 'info' | 'success' | 'warning' | 'error' | 'primary'

  interface Props {
    label: string
    tone?: ChipTone
    icon?: IconName
    /** When set, the chip becomes a toggle button with `aria-pressed`. */
    pressed?: boolean
    ontoggle?: (pressed: boolean) => void
  }

  let { label, tone = 'neutral', icon, pressed, ontoggle }: Props = $props()
</script>

{#if ontoggle}
  <button
    type="button"
    class="chip chip-{tone} toggle"
    aria-pressed={pressed ?? false}
    onclick={() => ontoggle?.(!(pressed ?? false))}
  >
    {#if pressed}<Icon name="check" size={16} />{:else if icon}<Icon name={icon} size={16} />{/if}
    <span>{label}</span>
  </button>
{:else}
  <span class="chip chip-{tone}">
    {#if icon}<Icon name={icon} size={16} />{/if}
    <span>{label}</span>
  </span>
{/if}

<style>
  .chip {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    max-width: 100%;
    padding: var(--space-1) var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-pill);
    background: var(--color-surface-sunken);
    color: var(--color-text);
    font-size: var(--text-sm);
    line-height: var(--leading-tight);
  }

  .toggle {
    min-height: var(--touch-target);
    cursor: pointer;
  }

  .toggle[aria-pressed='true'] {
    border-color: var(--color-primary);
    background: var(--color-primary-soft);
  }

  .chip-info {
    border-color: var(--color-info);
    background: var(--color-info-soft);
  }

  .chip-success {
    border-color: var(--color-success);
    background: var(--color-success-soft);
  }

  .chip-warning {
    border-color: var(--color-warning);
    background: var(--color-warning-soft);
  }

  .chip-error {
    border-color: var(--color-error);
    background: var(--color-error-soft);
  }

  .chip-primary {
    border-color: var(--color-primary);
    background: var(--color-primary-soft);
  }
</style>
