<script lang="ts">
  import { toErrorDto } from '../../api/errors'
  import type { ErrorDto, Id, JobDto, ResetPreviewDto, ResetScope } from '../../api/types'
  import { useServices } from '../../app/context'
  import Dialog from '../../components/Dialog.svelte'
  import ErrorState from '../../components/ErrorState.svelte'
  import Icon from '../../components/Icon.svelte'
  import ProgressPanel from '../../components/ProgressPanel.svelte'
  import Skeleton from '../../components/Skeleton.svelte'
  import { JobTracker } from '../../services/jobTracker.svelte'
  import { newOperationId } from '../../services/operationId'

  interface Props {
    scope: ResetScope
    onclose: () => void
    ondone: (job: JobDto) => void
  }

  let { scope, onclose, ondone }: Props = $props()
  const { api, i18n } = useServices()
  const ids = $props.id()
  const backup = new JobTracker(api)
  const reset = new JobTracker(api)
  $effect(() => backup.listen())
  $effect(() => reset.listen())

  let preview = $state<ResetPreviewDto | null>(null)
  let previewError = $state<ErrorDto | null>(null)
  let safetyBackupId = $state<Id | null>(null)
  let phrase = $state('')
  let operationId = newOperationId()

  async function load() {
    previewError = null
    try {
      preview = await api.previewReset({ scope })
    } catch (caught) {
      previewError = toErrorDto(caught)
    }
  }
  void load()

  const busy = $derived(backup.active || reset.active)
  const phraseMatches = $derived(preview !== null && phrase.trim() === preview.confirmationPhrase)
  const canConfirm = $derived(preview !== null && !busy && (safetyBackupId !== null || phraseMatches))

  function makeBackup() {
    void backup.start(
      () => api.createBackup({ operationId: newOperationId() }),
      (job) => {
        if (job.status === 'succeeded' && job.result?.type === 'backup') safetyBackupId = job.result.backupId
      },
    )
  }

  async function confirm() {
    if (!preview || !canConfirm) return
    const current = preview
    const started = await reset.start(
      () => api.resetCollection({ operationId, previewToken: current.previewToken, safetyBackupId, confirmationPhrase: safetyBackupId ? null : phrase.trim() }),
      (job) => {
        if (job.status === 'succeeded') ondone(job)
      },
    )
    // A stale preview is refreshed so the user can confirm again with current counts.
    if (!started) {
      operationId = newOperationId()
      if (reset.error?.code === 'PREVIEW_STALE') await load()
    }
  }
</script>

<Dialog open title={i18n.t(`privacy.reset.${scope}.title`)} onclose={() => !busy && onclose()} dismissible={false}>
  <div class="stack">
    {#if previewError}
      <ErrorState error={previewError} compact onretry={load} />
    {:else if !preview}
      <Skeleton lines={3} />
    {:else}
      <p>{i18n.t(`privacy.reset.${scope}.scope`, preview.counts)}</p>
      <p class="warning"><Icon name="alert" />{i18n.t(`privacy.reset.${scope}.irreversible`)}</p>
      <p class="muted">
        {preview.latestBackupAt
          ? i18n.t('privacy.reset.latestBackup', { time: i18n.formatDate(preview.latestBackupAt, { dateStyle: 'medium', timeStyle: 'short' }) })
          : i18n.t('privacy.reset.noBackup')}
      </p>

      <section class="option stack" aria-labelledby="{ids}-backup">
        <h3 id="{ids}-backup">{i18n.t('privacy.reset.backupTitle')}</h3>
        {#if safetyBackupId}
          <p class="ok" role="status"><Icon name="check" />{i18n.t('privacy.reset.backupDone')}</p>
        {:else}
          <p class="muted">{i18n.t('privacy.reset.backupBody')}</p>
          <button type="button" class="btn" disabled={busy} onclick={makeBackup}><Icon name="folder" />{i18n.t('privacy.reset.backupAction')}</button>
        {/if}
        {#if backup.job && backup.active}
          <ProgressPanel title={i18n.t('backup.creating')} status={backup.job.status} progress={backup.job.progress} />
        {/if}
        {#if backup.job?.status === 'failed' && backup.job.error}<ErrorState error={backup.job.error} compact />{/if}
        {#if backup.error}<ErrorState error={backup.error} compact />{/if}
      </section>

      {#if !safetyBackupId}
        <div class="field">
          <label for="{ids}-phrase">{i18n.t('privacy.reset.phraseLabel', { phrase: preview.confirmationPhrase })}</label>
          <input id="{ids}-phrase" class="input" autocomplete="off" spellcheck="false" bind:value={phrase} aria-describedby="{ids}-phrase-hint" />
          <p id="{ids}-phrase-hint" class="muted hint">{i18n.t('privacy.reset.phraseHint')}</p>
        </div>
      {/if}

      {#if reset.job && reset.job.status !== 'succeeded'}
        <ProgressPanel title={i18n.t(`privacy.reset.${scope}.progress`)} status={reset.job.status} progress={reset.job.progress} />
      {/if}
      {#if reset.job?.status === 'failed' && reset.job.error}<ErrorState error={reset.job.error} compact />{/if}
      {#if reset.error}<ErrorState error={reset.error} compact />{/if}
    {/if}
  </div>
  {#snippet actions()}
    <button type="button" class="btn" disabled={busy} onclick={onclose}>{i18n.t('common.cancel')}</button>
    <button type="button" class="btn btn-danger" disabled={!canConfirm} aria-busy={reset.active} onclick={confirm}>{i18n.t(`privacy.reset.${scope}.confirm`)}</button>
  {/snippet}
</Dialog>

<style>
  h3 {
    font-size: var(--text-md);
  }

  .option {
    gap: var(--space-2);
    padding: var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
  }

  .option .btn {
    justify-self: start;
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

  .hint {
    font-size: var(--text-sm);
  }
</style>
