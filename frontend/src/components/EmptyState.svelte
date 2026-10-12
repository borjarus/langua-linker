<script lang="ts">
  import type { Snippet } from 'svelte'
  import Icon, { type IconName } from './Icon.svelte'

  interface Props {
    title: string
    description?: string
    icon?: IconName
    /** Call-to-action buttons; an empty state should never be a dead end. */
    actions?: Snippet
  }

  let { title, description, icon = 'cards', actions }: Props = $props()
</script>

<section class="empty" aria-label={title}>
  <span class="icon"><Icon name={icon} size={40} /></span>
  <h2>{title}</h2>
  {#if description}<p class="muted">{description}</p>{/if}
  {#if actions}<div class="row actions">{@render actions()}</div>{/if}
</section>

<style>
  .empty {
    display: grid;
    justify-items: center;
    gap: var(--space-3);
    padding: var(--space-6) var(--space-4);
    text-align: center;
  }

  .icon {
    color: var(--color-text-muted);
  }

  h2 {
    font-size: var(--text-lg);
  }

  .actions {
    justify-content: center;
  }
</style>
