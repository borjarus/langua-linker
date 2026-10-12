<script lang="ts">
  import { useServices } from '../app/context'

  interface Props {
    lines?: number
    /** Overrides the default accessible label. */
    label?: string
  }

  let { lines = 3, label }: Props = $props()
  const { i18n } = useServices()
</script>

<div class="skeleton" role="status" aria-live="polite" aria-busy="true">
  <span class="visually-hidden">{label ?? i18n.t('state.loadingLabel')}</span>
  {#each { length: lines }, index (index)}
    <span class="line" style:width="{index === lines - 1 ? 60 : 100}%" aria-hidden="true"></span>
  {/each}
</div>

<style>
  .skeleton {
    display: grid;
    gap: var(--space-2);
  }

  .line {
    display: block;
    height: 1em;
    border-radius: var(--radius-sm);
    background: linear-gradient(90deg, var(--color-surface-sunken), var(--color-border), var(--color-surface-sunken));
    background-size: 200% 100%;
    animation: shimmer 1.4s linear infinite;
  }

  @keyframes shimmer {
    to {
      background-position: -200% 0;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .line {
      animation: none;
    }
  }
</style>
