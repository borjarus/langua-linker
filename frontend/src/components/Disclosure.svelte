<script lang="ts">
  import type { Snippet } from 'svelte'
  import { useServices } from '../app/context'
  import Icon from './Icon.svelte'

  interface Props {
    title: string
    /** Sections start collapsed in study so the answer is not spoiled. */
    expanded?: boolean
    count?: number
    children: Snippet
  }

  let { title, expanded = false, count, children }: Props = $props()
  const { i18n } = useServices()
  const regionId = $props.id()
  // Follows the `expanded` prop until the user toggles it.
  let open = $derived(expanded)
</script>

<section class="disclosure">
  <h3>
    <button
      type="button"
      class="toggle"
      aria-expanded={open}
      aria-controls={regionId}
      aria-label={i18n.t(open ? 'disclosure.hide' : 'disclosure.show', { section: title })}
      onclick={() => (open = !open)}
    >
      <span>{title}</span>
      {#if count !== undefined}<span class="count" aria-hidden="true">{count}</span>{/if}
      <Icon name={open ? 'chevronUp' : 'chevronDown'} />
    </button>
  </h3>
  <div id={regionId} hidden={!open} class="content">
    {@render children()}
  </div>
</section>

<style>
  .disclosure {
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface);
  }

  h3 {
    font-size: var(--text-md);
  }

  .toggle {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    min-height: var(--touch-target);
    padding: var(--space-2) var(--space-4);
    border: 0;
    border-radius: var(--radius-md);
    background: transparent;
    font-weight: var(--weight-medium);
    text-align: start;
    cursor: pointer;
  }

  .toggle > span:first-child {
    flex: 1;
  }

  .count {
    min-width: 1.5rem;
    padding: 0 var(--space-2);
    border-radius: var(--radius-pill);
    background: var(--color-surface-sunken);
    font-size: var(--text-sm);
    text-align: center;
  }

  .content {
    padding: 0 var(--space-4) var(--space-4);
  }
</style>
