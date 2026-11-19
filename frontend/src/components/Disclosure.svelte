<script lang="ts">
  import type { Snippet } from 'svelte'
  import { useServices } from '../app/context'
  import Icon, { type IconName } from './Icon.svelte'

  interface Props {
    title: string
    /** Sections start collapsed in study so the answer is not spoiled. */
    expanded?: boolean
    /**
     * Bind to open the section from outside (study chips above the card). Unbound, the section
     * follows `expanded` until the user toggles it.
     */
    open?: boolean
    count?: number
    icon?: IconName
    /** Id of the section, for jumping to it. */
    id?: string
    children: Snippet
  }

  let { title, expanded = false, open = $bindable(), count, icon, id, children }: Props = $props()
  const { i18n } = useServices()
  const regionId = $props.id()
  const shown = $derived(open ?? expanded)
</script>

<section class="disclosure" {id}>
  <h3>
    <button
      type="button"
      class="toggle"
      aria-expanded={shown}
      aria-controls={regionId}
      aria-label={i18n.t(shown ? 'disclosure.hide' : 'disclosure.show', { section: title })}
      onclick={() => (open = !shown)}
    >
      {#if icon}<Icon name={icon} />{/if}
      <span class="title">{title}</span>
      {#if count !== undefined}<span class="count" aria-hidden="true">{count}</span>{/if}
      <Icon name={shown ? 'chevronUp' : 'chevronDown'} />
    </button>
  </h3>
  <div id={regionId} hidden={!shown} class="content">
    {@render children()}
  </div>
</section>

<style>
  .disclosure {
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    background: var(--color-surface);
    /* Jumping here from a chip leaves room for the sticky header. */
    scroll-margin-top: var(--space-6);
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

  .title {
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
