<script lang="ts">
  import { untrack } from 'svelte'
  import { toErrorDto } from '../../api/errors'
  import type { ErrorDto, RestorePreviewDto, RestoreSource } from '../../api/types'
  import { useServices } from '../../app/context'
  import Dialog from '../../components/Dialog.svelte'
  import ErrorState from '../../components/ErrorState.svelte'
  import Icon from '../../components/Icon.svelte'
  import ProgressPanel from '../../components/ProgressPanel.svelte'
  import { JobTracker } from '../../services/jobTracker.svelte'
  import { newOperationId } from '../../services/operationId'

  interface Props {
    preview: RestorePreviewDto
    onclose: () => void
    /** The restore replaced the collection; the caller reopens the app on it. */
    onrestored: () => void
  }

  let { preview: initial, onclose, onrestored }: Props = $props()
  const { api, i18n } = useServices()
  const job = new JobTracker(api)
  $effect(() => job.listen())

  // Starts from the validated file; a recovery switches it to the safety copy.
  let preview = $state<RestorePreviewDto>(untrack(() => initial))
  let operationId = newOperationId()
  let switching = $state(false)
  let switchError = $state<ErrorDto | null>(null)

  const failed = $derived(job.job?.status === 'failed')
  const safetyBackupId = $derived(job.job?.result?.type === 'restore' ? job.job.result.safetyBackupId : null)
  const date = (iso: string) => i18n.formatDate(iso, { dateStyle: 'medium', timeStyle: 'short' })

  async function restore() {
    if (job.active || !preview.compatible) return
    const token = preview.previewToken
    const started = await job.start(
      () => api.restoreBackup({ operationId, previewToken: token }),
      (finished) => {
        if (finished.status === 'succeeded') onrestored()
      },
    )
    if (!started) operationId = newOperationId()
  }

  /** After a failed restore: validate the safety copy so the user can go back to it in one step. */
  async function useSafetyCopy(backupId: string) {
    switching = true
    switchError = null
    try {
      const source: RestoreSource = { kind: 'backup', backupId }
      preview = await api.validateRestore({ source })
      operationId = newOperationId()
      job.reset()
    } catch (caught) {
      switchError = toErrorDto(caught)
    } finally {
      switching = false
    }
  }
</script>

<!-- Destructive: replaces the whole collection, so a stray backdrop tap must not dismiss it mid-way. -->
<Dialog open title={i18n.t('backup.restore.title')} onclose={() => !job.active && onclose()} dismissible={false}>
  <div class="stack">
    <dl class="facts">
      <div><dt>{i18n.t('backup.restore.file')}</dt><dd>{preview.fileName}</dd></div>
      <div><dt>{i18n.t('backup.restore.created')}</dt><dd>{date(preview.createdAt)}</dd></div>
      <div><dt>{i18n.t('backup.restore.schema')}</dt><dd>{preview.schemaVersion}</dd></div>
    </dl>
    <table class="compare">
      <thead>
        <tr>
          <th scope="col"><span class="visually-hidden">{i18n.t('backup.restore.what')}</span></th>
          <th scope="col" class="num">{i18n.t('backup.restore.now')}</th>
          <th scope="col" class="num">{i18n.t('backup.restore.after')}</th>
        </tr>
      </thead>
      <tbody>
        {#each ['decks', 'notes', 'cards', 'reviews'] as const as key (key)}
          <tr>
            <th scope="row">{i18n.t(`backup.restore.counts.${key}`)}</th>
            <td class="num">{i18n.formatNumber(preview.current[key])}</td>
            <td class="num">{i18n.formatNumber(preview.counts[key])}</td>
          </tr>
        {/each}
      </tbody>
    </table>
    {#each preview.issues as issue, index (index)}
      <ErrorState error={issue} compact />
    {/each}
    <p class="warning"><Icon name="alert" /><span>{i18n.t('backup.restore.replaces')}</span></p>
    <p class="muted">{i18n.t('backup.restore.safety')}</p>

    {#if job.job && job.job.status !== 'succeeded'}
      <ProgressPanel title={i18n.t('backup.restore.progress')} status={job.job.status} progress={job.job.progress} />
    {/if}
    {#if job.job?.status === 'succeeded'}
      <p class="ok" role="status"><Icon name="check" />{i18n.t('backup.restore.reopening')}</p>
    {/if}
    {#if failed}
      <div class="recovery" role="alert">
        <p><strong>{i18n.t('backup.restore.failed')}</strong></p>
        {#if job.job?.error}<p>{i18n.tDynamic(job.job.error.messageKey, 'errors.INTERNAL')}</p>{/if}
        {#if safetyBackupId}
          <p>{i18n.t('backup.restore.safetyKept')}</p>
          <button type="button" class="btn" disabled={switching} aria-busy={switching} onclick={() => useSafetyCopy(safetyBackupId)}>{i18n.t('backup.restore.useSafety')}</button>
        {/if}
      </div>
    {/if}
    {#if switchError}<ErrorState error={switchError} compact />{/if}
    {#if job.error}<ErrorState error={job.error} compact />{/if}
  </div>
  {#snippet actions()}
    <button type="button" class="btn" disabled={job.active} onclick={onclose}>{i18n.t(failed ? 'common.close' : 'common.cancel')}</button>
    {#if !failed}
      <button type="button" class="btn btn-danger" disabled={job.active || !preview.compatible || job.job?.status === 'succeeded'} aria-busy={job.active} onclick={restore}>
        {i18n.t('backup.restore.confirm')}
      </button>
    {/if}
  {/snippet}
</Dialog>

<style>
  .facts {
    display: grid;
    gap: var(--space-2);
    margin: 0;
  }

  .facts div {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1) var(--space-2);
  }

  dt {
    color: var(--color-text-muted);
  }

  dd {
    margin: 0;
    font-weight: var(--weight-medium);
  }

  .compare {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--text-sm);
  }

  .compare th,
  .compare td {
    padding: var(--space-1) var(--space-2);
    border-bottom: 1px solid var(--color-border);
    text-align: start;
  }

  .compare .num {
    text-align: end;
    font-variant-numeric: tabular-nums;
  }

  .warning,
  .ok {
    display: flex;
    align-items: start;
    gap: var(--space-2);
  }

  .warning {
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--color-warning);
    border-radius: var(--radius-sm);
    background: var(--color-warning-soft);
  }

  .ok {
    color: var(--color-success);
  }

  .recovery {
    display: grid;
    gap: var(--space-2);
    padding: var(--space-3);
    border: 1px solid var(--color-error);
    border-radius: var(--radius-md);
    background: var(--color-error-soft);
  }

  .recovery .btn {
    justify-self: start;
  }
</style>
