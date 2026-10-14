<script lang="ts">
  import { probeShareFile, shareProbe, shareSupport } from '../../api/nativeDiagnostics'
  import { useServices } from '../../app/context'
  import ProbeLog, { logEntry, type LogEntry } from './ProbeLog.svelte'

  const { i18n } = useServices()
  const support = shareSupport()
  let busy = $state(false)
  let entries = $state<LogEntry[]>([])

  const yesNo = (value: boolean) => (value ? i18n.t('diagnostics.yes') : i18n.t('diagnostics.no'))

  async function share(what: string, data: ShareData) {
    busy = true
    try {
      const result = await shareProbe(data)
      entries = [logEntry(`${what}: ${result}`, result !== 'shared' && result !== 'AbortError'), ...entries]
    } finally {
      busy = false
    }
  }
</script>

<dl class="facts">
  <dt>{i18n.t('diagnostics.shareText')}</dt>
  <dd>{yesNo(support.text)}</dd>
  <dt>{i18n.t('diagnostics.shareFile')}</dt>
  <dd>{yesNo(support.files)}</dd>
</dl>
{#if support.text}
  <div class="row">
    <button type="button" class="btn btn-primary" disabled={busy} onclick={() => share('text', { title: 'LanguaLinker', text: 'Haus — house' })}>
      {i18n.t('diagnostics.shareText')}
    </button>
    <button type="button" class="btn" disabled={busy || !support.files} onclick={() => share('file', { files: [probeShareFile()] })}>
      {i18n.t('diagnostics.shareFile')}
    </button>
  </div>
  <ProbeLog {entries} />
{:else}
  <p role="status">{i18n.t('diagnostics.shareUnavailable')}</p>
{/if}
