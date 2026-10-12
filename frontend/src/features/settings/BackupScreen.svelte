<script lang="ts">
  import { toErrorDto } from '../../api/errors'
  import type { JobDto } from '../../api/types'
  import { useServices } from '../../app/context'
  import ProgressPanel from '../../components/ProgressPanel.svelte'
  import { newOperationId } from '../../services/operationId'
  import { useQuery } from '../../services/query.svelte'
  import QueryView from '../shared/QueryView.svelte'
  import Screen from '../shared/Screen.svelte'

  const { api, i18n, queries, toasts } = useServices()
  const backups = useQuery(queries, () => ({ key: ['backups'], fetch: () => api.listBackups() }))
  let job = $state<JobDto | null>(null)

  // Job events are read signals; drop out-of-order ones by sequence. Unsubscribed on unmount.
  $effect(() =>
    api.onJobEvent((event) => {
      if (job && event.jobId === job.jobId && event.sequence > job.sequence) job = { ...job, ...event }
    }),
  )

  async function create() {
    try {
      job = await api.createBackup({ operationId: newOperationId() })
    } catch (caught) {
      toasts.show(i18n.tDynamic(toErrorDto(caught).messageKey, 'errors.INTERNAL'), 'error')
    }
  }
</script>

<Screen title={i18n.t('routes.settingsBackup')} back phase={5}>
  <button type="button" class="btn btn-primary" disabled={job?.status === 'running' || job?.status === 'queued'} onclick={create}>
    {i18n.t('backup.create')}
  </button>
  {#if job}
    <ProgressPanel title={i18n.t('backup.create')} status={job.status} progress={job.progress} oncancel={() => job && api.cancelJob(job.jobId)} />
  {/if}
  <QueryView query={backups}>
    {#snippet ready(list)}
      <p>{i18n.t('backup.count', { count: list.length })}</p>
      <ul>
        {#each list as backup (backup.id)}
          <li>{i18n.formatDate(backup.createdAt, { dateStyle: 'medium', timeStyle: 'short' })} · {i18n.formatNumber(backup.sizeBytes / 1000, { style: 'unit', unit: 'kilobyte', maximumFractionDigits: 0 })}</li>
        {/each}
      </ul>
    {/snippet}
  </QueryView>
</Screen>
