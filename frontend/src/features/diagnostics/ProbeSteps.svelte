<script lang="ts">
  import type { ProbeStep } from '../../api/nativeDiagnostics'
  import { useServices } from '../../app/context'

  let { steps }: { steps: ProbeStep[] } = $props()
  const { i18n } = useServices()
  const passed = $derived(steps.every((step) => step.ok))
</script>

<p><strong>{passed ? i18n.t('diagnostics.passed') : i18n.t('diagnostics.failed')}</strong></p>
<ul class="steps">
  {#each steps as step (step.name)}
    <li data-ok={step.ok}>
      <span>{step.ok ? '✓' : '✗'}</span>
      <code>{step.name}</code>
      {#if step.error}<span class="muted">{step.error}</span>{/if}
    </li>
  {/each}
</ul>

<style>
  .steps {
    display: grid;
    gap: var(--space-1);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .steps li {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    overflow-wrap: anywhere;
  }

  .steps li[data-ok='false'] {
    color: var(--color-error);
  }
</style>
