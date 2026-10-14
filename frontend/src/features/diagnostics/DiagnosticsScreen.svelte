<script lang="ts">
  import { toErrorDto } from '../../api/errors'
  import type { ErrorDto } from '../../api/types'
  import { useServices } from '../../app/context'
  import ErrorState from '../../components/ErrorState.svelte'
  import { nativeDiagnosticsAvailable, probeSecretStore, type SecretStoreProbe } from '../../api/nativeDiagnostics'
  import Screen from '../shared/Screen.svelte'
  import FilesProbe from './FilesProbe.svelte'
  import NotificationsProbe from './NotificationsProbe.svelte'
  import ProbeSteps from './ProbeSteps.svelte'
  import ShareProbe from './ShareProbe.svelte'
  import SpeechProbe from './SpeechProbe.svelte'

  const { i18n } = useServices()
  const available = nativeDiagnosticsAvailable()

  let running = $state(false)
  let report = $state<SecretStoreProbe | null>(null)
  let failure = $state<ErrorDto | null>(null)

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
  <div class="diagnostics stack">
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
          <dl class="facts">
            <dt>{i18n.t('diagnostics.backend')}</dt>
            <dd><code>{report.backend}</code></dd>
            <dt>{i18n.t('diagnostics.persisted')}</dt>
            <dd>{report.persistedFromPreviousRun ? i18n.t('diagnostics.yes') : i18n.t('diagnostics.noFirstRun')}</dd>
          </dl>
          <ProbeSteps steps={report.steps} />
        </div>
      {/if}
    </section>

    <section class="surface stack" aria-labelledby="d-notifications">
      <h2 id="d-notifications">{i18n.t('diagnostics.notifications')}</h2>
      <p class="muted">{i18n.t('diagnostics.notificationsHint')}</p>
      {#if available}
        <NotificationsProbe />
      {:else}
        <p role="status">{i18n.t('diagnostics.nativeOnly')}</p>
      {/if}
    </section>

    <section class="surface stack" aria-labelledby="d-files">
      <h2 id="d-files">{i18n.t('diagnostics.files')}</h2>
      <p class="muted">{i18n.t('diagnostics.filesHint')}</p>
      {#if available}
        <FilesProbe />
      {:else}
        <p role="status">{i18n.t('diagnostics.nativeOnly')}</p>
      {/if}
    </section>

    <!-- TTS and sharing use WebView APIs, so the browser demo can run them too. -->
    <section class="surface stack" aria-labelledby="d-speech">
      <h2 id="d-speech">{i18n.t('diagnostics.speech')}</h2>
      <p class="muted">{i18n.t('diagnostics.speechHint')}</p>
      <SpeechProbe />
    </section>

    <section class="surface stack" aria-labelledby="d-share">
      <h2 id="d-share">{i18n.t('diagnostics.share')}</h2>
      <p class="muted">{i18n.t('diagnostics.shareHint')}</p>
      <ShareProbe />
    </section>
  </div>
</Screen>

<style>
  .diagnostics :global(.facts) {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: var(--space-1) var(--space-3);
    margin: 0;
  }

  .diagnostics :global(.facts dd) {
    margin: 0;
    overflow-wrap: anywhere;
  }
</style>
