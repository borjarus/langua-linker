<script lang="ts">
  import type { DemoScenario } from '../demo/scenarios'
  import { useServices } from './context'

  // Type-only demo imports: this component ships in release but renders nothing without `demo`.
  const { demo, i18n, queries, router } = useServices()
  const selectId = $props.id()
  let scenario = $state<DemoScenario>(demo?.scenario ?? 'happy')

  function changeScenario(next: DemoScenario) {
    if (!demo) return
    scenario = next
    demo.setScenario(next)
    queries.clear()
    void router.replace(next === 'newUser' ? { name: 'onboarding' } : { name: 'home' })
  }
</script>

{#if demo}
  <aside class="banner" aria-label={i18n.t('demo.scenario')}>
    <p>{i18n.t('demo.banner')}</p>
    <div class="row">
      <label for={selectId}>{i18n.t('demo.scenario')}</label>
      <select
        id={selectId}
        class="input"
        value={scenario}
        onchange={(event) => changeScenario(event.currentTarget.value as DemoScenario)}
      >
        {#each demo.scenarios as option (option)}
          <option value={option}>{i18n.t(`demo.scenarios.${option}`)}</option>
        {/each}
      </select>
    </div>
  </aside>
{/if}

<style>
  .banner {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2) var(--space-4);
    padding: var(--space-2) var(--space-4);
    padding-top: calc(var(--space-2) + env(safe-area-inset-top));
    background: var(--color-demo);
    color: var(--color-on-demo);
    font-size: var(--text-sm);
  }

  /* A flex item defaults to its min-content width, which is the select's longest option; at 200%
     text that is wider than a phone, so let the row shrink and the select cap at its width. */
  .row {
    min-width: 0;
  }

  label {
    font-weight: var(--weight-medium);
  }

  .input {
    min-height: 2.5rem;
    color: var(--color-text);
  }
</style>
