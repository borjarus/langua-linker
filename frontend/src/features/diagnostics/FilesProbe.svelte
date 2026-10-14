<script lang="ts">
  import { probeFileOpen, probeFileSave, type FileProbe } from '../../api/nativeDiagnostics'
  import { useServices } from '../../app/context'
  import ProbeSteps from './ProbeSteps.svelte'

  const { i18n } = useServices()
  let busy = $state(false)
  let report = $state<FileProbe | null>(null)
  let failure = $state<string | null>(null)

  async function run(task: () => Promise<FileProbe>) {
    busy = true
    failure = null
    try {
      report = await task()
    } catch (error) {
      report = null
      failure = String(error)
    } finally {
      busy = false
    }
  }
</script>

<div class="row">
  <button type="button" class="btn btn-primary" disabled={busy} onclick={() => run(() => probeFileOpen(true))}>{i18n.t('diagnostics.openFiltered')}</button>
  <button type="button" class="btn" disabled={busy} onclick={() => run(() => probeFileOpen(false))}>{i18n.t('diagnostics.openAny')}</button>
  <button type="button" class="btn" disabled={busy} onclick={() => run(probeFileSave)}>{i18n.t('diagnostics.saveBackup')}</button>
</div>

{#if failure}
  <p class="error" role="alert">{failure}</p>
{/if}

{#if report}
  <div role="status" class="stack">
    {#if report.cancelled}
      <p>{i18n.t('diagnostics.pickerCancelled')}</p>
    {:else}
      <dl class="facts">
        <dt>{i18n.t('diagnostics.location')}</dt>
        <dd><code>{report.location}</code></dd>
        {#if report.bytes !== null}
          <dt>{i18n.t('diagnostics.size')}</dt>
          <dd>{i18n.t('diagnostics.bytes', { count: report.bytes })}</dd>
        {/if}
        {#if report.kind}
          <dt>{i18n.t('diagnostics.kind')}</dt>
          <dd><code>{report.kind}</code></dd>
        {/if}
      </dl>
      <ProbeSteps steps={report.steps} />
    {/if}
  </div>
{/if}

<style>
  .error {
    color: var(--color-error);
    overflow-wrap: anywhere;
  }
</style>
