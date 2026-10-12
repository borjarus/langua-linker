<script lang="ts">
  import type { Snippet } from 'svelte'

  interface Props {
    label: string
    value: string
    hint?: string
    tone?: 'neutral' | 'success' | 'warning' | 'error' | 'info'
    /** Optional extra visual (sparkline, meter). */
    children?: Snippet
  }

  let { label, value, hint, tone = 'neutral', children }: Props = $props()
</script>

<div class="tile tone-{tone}">
  <dt class="label">{label}</dt>
  <dd class="value">{value}</dd>
  {#if hint}<dd class="hint">{hint}</dd>{/if}
  {#if children}<dd class="extra">{@render children()}</dd>{/if}
</div>

<style>
  .tile {
    display: grid;
    gap: var(--space-1);
    min-width: 0;
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--color-border);
    border-left-width: 4px;
    border-radius: var(--radius-md);
    background: var(--color-surface);
  }

  dd {
    margin: 0;
  }

  .label {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  .value {
    font-size: var(--text-xl);
    font-weight: var(--weight-bold);
    font-variant-numeric: tabular-nums;
  }

  .hint {
    color: var(--color-text-muted);
    font-size: var(--text-xs);
  }

  .tone-success {
    border-left-color: var(--color-success);
  }

  .tone-warning {
    border-left-color: var(--color-warning);
  }

  .tone-error {
    border-left-color: var(--color-error);
  }

  .tone-info {
    border-left-color: var(--color-info);
  }
</style>
