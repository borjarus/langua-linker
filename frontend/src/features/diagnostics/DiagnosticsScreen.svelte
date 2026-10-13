<script lang="ts">
  import { toErrorDto } from '../../api/errors'
  import type { ErrorDto } from '../../api/types'
  import { useServices } from '../../app/context'
  import ErrorState from '../../components/ErrorState.svelte'
  import { nativeDiagnosticsAvailable, probeSecretStore, type SecretStoreProbe } from '../../api/nativeDiagnostics'
  import Screen from '../shared/Screen.svelte'

  const { i18n } = useServices()
  const available = nativeDiagnosticsAvailable()

  let running = $state(false)
  let report = $state<SecretStoreProbe | null>(null)
  let failure = $state<ErrorDto | null>(null)
  const passed = $derived(report !== null && report.steps.every((step) => step.ok))

  async function runSecretStore() {
    running = true
    failure = null
    try {
      report = await probeSecretStore()
    } catch (caught) {
      report = null
      failure = toErrorDto(caught)
    } finally {
      running = false
    }
  }
</script>

<Screen title={i18n.t('routes.diagnostics')} back>
  <p class="muted">{i18n.t('diagnostics.intro')}</p>

  <section class="surface stack" aria-labelledby="d-secrets">
    <h2 id="d-secrets">{i18n.t('diagnostics.secretStore')}</h2>
    <p class="muted">{i18n.t('diagnostics.secretStoreHint')}</p>
    {#if available}
      <div class="row">
        <button type="button" class="btn btn-primary" disabled={running} onclick={runSecretStore}>{i18n.t('diagnostics.run')}</button>
      </div>
    {:else}
      <p role="status">{i18n.t('diagnostics.nativeOnly')}</p>
    {/if}

    {#if failure}
      <ErrorState error={failure} compact onretry={runSecretStore} />
    {/if}

    {#if report}
      <div role="status" class="stack">
        <p><strong>{passed ? i18n.t('diagnostics.passed') : i18n.t('diagnostics.failed')}</strong></p>
        <dl class="facts">
          <dt>{i18n.t('diagnostics.backend')}</dt>
          <dd><code>{report.backend}</code></dd>
          <dt>{i18n.t('diagnostics.persisted')}</dt>
          <dd>{report.persistedFromPreviousRun ? i18n.t('diagnostics.yes') : i18n.t('diagnostics.noFirstRun')}</dd>
        </dl>
        <ul class="steps">
          {#each report.steps as step (step.name)}
            <li data-ok={step.ok}>
              <span>{step.ok ? '✓' : '✗'}</span>
              <code>{step.name}</code>
              {#if step.error}<span class="muted">{step.error}</span>{/if}
            </li>
          {/each}
        </ul>
      </div>
    {/if}
  </section>
</Screen>

<style>
  .facts {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: var(--space-1) var(--space-3);
    margin: 0;
  }

  .facts dd {
    margin: 0;
    overflow-wrap: anywhere;
  }

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
