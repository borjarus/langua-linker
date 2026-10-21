<script lang="ts">
  import { toErrorDto } from '../../api/errors'
  import type { BackupDto, ErrorDto, RestorePreviewDto, RestoreSource } from '../../api/types'
  import { useServices } from '../../app/context'
  import Chip from '../../components/Chip.svelte'
  import EmptyState from '../../components/EmptyState.svelte'
  import ErrorState from '../../components/ErrorState.svelte'
  import Icon from '../../components/Icon.svelte'
  import ProgressPanel from '../../components/ProgressPanel.svelte'
  import Skeleton from '../../components/Skeleton.svelte'
  import { JobTracker } from '../../services/jobTracker.svelte'
  import { newOperationId } from '../../services/operationId'
  import { useQuery } from '../../services/query.svelte'
  import QueryView from '../shared/QueryView.svelte'
  import Screen from '../shared/Screen.svelte'
  import RestoreDialog from './RestoreDialog.svelte'
  import SaveStatus from './SaveStatus.svelte'
  import { SettingsController } from './settingsController.svelte'

  const { api, i18n, queries, router, toasts } = useServices()
  const ids = $props.id()
  const backups = useQuery(queries, () => ({ key: ['backups'], fetch: () => api.listBackups(), isEmpty: (list) => list.length === 0 }))
  const controller = new SettingsController(api)
  void controller.load()
  const job = new JobTracker(api)
  $effect(() => job.listen())
  const RETENTION = [3, 5, 10, 20] as const

  let restoring = $state<RestorePreviewDto | null>(null)
  let validating = $state<string | null>(null)
  let restoreError = $state<ErrorDto | null>(null)
  let sharing = $state<string | null>(null)
  let shareError = $state<ErrorDto | null>(null)
  let folderError = $state<ErrorDto | null>(null)

  const date = (iso: string) => i18n.formatDate(iso, { dateStyle: 'medium', timeStyle: 'short' })
  const size = (bytes: number) => i18n.formatNumber(bytes / 1000, { style: 'unit', unit: 'kilobyte', maximumFractionDigits: 0 })

  function create() {
    void job.start(
      () => api.createBackup({ operationId: newOperationId() }),
      (finished) => {
        if (finished.status !== 'succeeded') return
        toasts.show(i18n.t('backup.created'), 'success')
        void backups.refresh()
      },
    )
  }

  /** Validation reads the file only; the collection changes after the user confirms the preview. */
  async function validate(source: RestoreSource, key: string) {
    validating = key
    restoreError = null
    try {
      restoring = await api.validateRestore({ source })
    } catch (caught) {
      restoreError = toErrorDto(caught)
    } finally {
      validating = null
    }
  }

  async function restoreFromFile() {
    restoreError = null
    try {
      const file = await api.pickFile({ purpose: 'restore' })
      if (file) await validate({ kind: 'file', fileToken: file.token }, 'file')
    } catch (caught) {
      restoreError = toErrorDto(caught)
    }
  }

  async function share(backup: BackupDto) {
    sharing = backup.id
    shareError = null
    try {
      const result = await api.shareFile({ operationId: newOperationId(), target: { kind: 'backup', backupId: backup.id } })
      if (result.shared) toasts.show(i18n.t('backup.shared'), 'success')
    } catch (caught) {
      shareError = toErrorDto(caught)
    } finally {
      sharing = null
    }
  }

  async function chooseFolder() {
    folderError = null
    try {
      const folder = await api.pickFile({ purpose: 'backupFolder' })
      if (folder) await controller.update('target', { backupFolderToken: folder.token })
    } catch (caught) {
      folderError = toErrorDto(caught)
    }
  }

  async function restored() {
    restoring = null
    // The whole collection changed: drop every cached read and reopen from the dashboard.
    queries.clear()
    toasts.show(i18n.t('backup.restore.done'), 'success', 8000)
    await router.replace({ name: 'home' })
  }
</script>

<Screen title={i18n.t('routes.settingsBackup')} back notice={i18n.t('backup.prototype')}>
  <section class="surface stack" aria-labelledby="{ids}-create">
    <h2 id="{ids}-create">{i18n.t('backup.createTitle')}</h2>
    <p class="muted">{i18n.t('backup.body')}</p>
    <button type="button" class="btn btn-primary" disabled={job.active} aria-busy={job.active} onclick={create}><Icon name="folder" />{i18n.t('backup.create')}</button>
    {#if job.job && job.job.status !== 'succeeded'}
      <ProgressPanel title={i18n.t('backup.creating')} status={job.job.status} progress={job.job.progress} oncancel={() => job.cancel()} />
      {#if job.job.status === 'failed' && job.job.error}<ErrorState error={job.job.error} compact />{/if}
    {/if}
    {#if job.error}<ErrorState error={job.error} compact />{/if}
  </section>

  <section class="surface stack" aria-labelledby="{ids}-settings">
    <h2 id="{ids}-settings">{i18n.t('backup.settings')}</h2>
    {#if controller.loadError}
      <ErrorState error={controller.loadError} compact onretry={() => void controller.load()} />
    {:else if !controller.settings}
      <Skeleton lines={3} />
    {:else}
      {@const settings = controller.settings.backup}
      <label class="choice">
        <input type="checkbox" checked={settings.automatic} onchange={(event) => controller.update('automatic', { backupAutomatic: event.currentTarget.checked })} />
        {i18n.t('backup.automatic')}
      </label>
      <SaveStatus {controller} field="automatic" />
      <div class="field">
        <label for="{ids}-retention">{i18n.t('backup.retention')}</label>
        <select id="{ids}-retention" class="input narrow" value={settings.retention} onchange={(event) => controller.update('retention', { backupRetention: Number(event.currentTarget.value) })} aria-describedby="{ids}-retention-hint">
          {#each RETENTION as option (option)}
            <option value={option}>{i18n.t('backup.keep', { count: option })}</option>
          {/each}
          {#if !RETENTION.includes(settings.retention as (typeof RETENTION)[number])}
            <option value={settings.retention}>{i18n.t('backup.keep', { count: settings.retention })}</option>
          {/if}
        </select>
        <p id="{ids}-retention-hint" class="muted hint">{i18n.t('backup.retentionHint')}</p>
        <SaveStatus {controller} field="retention" />
      </div>
      <fieldset class="field">
        <legend>{i18n.t('backup.target')}</legend>
        <label class="choice">
          <input type="radio" name="{ids}-target" checked={settings.target === 'app'} onchange={() => controller.update('target', { backupTarget: 'app' })} />
          {i18n.t('backup.targets.app')}
        </label>
        <label class="choice">
          <input type="radio" name="{ids}-target" checked={settings.target === 'external'} disabled={settings.externalFolder === null} onchange={() => controller.update('target', { backupTarget: 'external' })} />
          {settings.externalFolder ? i18n.t('backup.targets.external', { folder: settings.externalFolder }) : i18n.t('backup.targets.externalNone')}
        </label>
        <button type="button" class="btn btn-ghost" onclick={chooseFolder}><Icon name="folder" />{i18n.t('backup.chooseFolder')}</button>
        <p class="muted hint">{i18n.t('backup.targetHint')}</p>
        {#if folderError}<ErrorState error={folderError} compact />{/if}
        <SaveStatus {controller} field="target" />
      </fieldset>
    {/if}
  </section>

  <section class="stack" aria-labelledby="{ids}-list">
    <div class="heading">
      <h2 id="{ids}-list">{i18n.t('backup.list')}</h2>
      <button type="button" class="btn" disabled={validating !== null} aria-busy={validating === 'file'} onclick={restoreFromFile}><Icon name="refresh" />{i18n.t('backup.restore.fromFile')}</button>
    </div>
    {#if restoreError}<ErrorState error={restoreError} compact />{/if}
    {#if shareError}<ErrorState error={shareError} compact />{/if}
    <QueryView query={backups}>
      {#snippet empty()}
        <EmptyState title={i18n.t('backup.emptyTitle')} description={i18n.t('backup.emptyBody')} icon="folder" />
      {/snippet}
      {#snippet ready(list)}
        <p class="muted">{i18n.t('backup.count', { count: list.length })}</p>
        <ul class="backups">
          {#each list as backup (backup.id)}
            <li class="surface">
              <div class="info">
                <strong>{date(backup.createdAt)}</strong>
                <div class="row">
                  <Chip label={i18n.t(`backup.kinds.${backup.kind}`)} tone={backup.kind === 'safety' ? 'warning' : 'neutral'} />
                  <Chip label={i18n.t(`backup.locations.${backup.location}`)} />
                </div>
                <span class="muted">{i18n.t('deck.cards', { count: backup.cardCount })} · {size(backup.sizeBytes)}</span>
              </div>
              <div class="row">
                <button type="button" class="btn btn-ghost" disabled={sharing === backup.id} aria-busy={sharing === backup.id} onclick={() => share(backup)}>
                  {i18n.t('backup.share')}<span class="visually-hidden"> {date(backup.createdAt)}</span>
                </button>
                <button type="button" class="btn" disabled={validating !== null} aria-busy={validating === backup.id} onclick={() => validate({ kind: 'backup', backupId: backup.id }, backup.id)}>
                  {i18n.t('backup.restore.open')}<span class="visually-hidden"> {date(backup.createdAt)}</span>
                </button>
              </div>
            </li>
          {/each}
        </ul>
      {/snippet}
    </QueryView>
  </section>

  {#if restoring}
    <RestoreDialog preview={restoring} onclose={() => (restoring = null)} onrestored={restored} />
  {/if}
</Screen>

<style>
  h2 {
    font-size: var(--text-lg);
  }

  section > .btn {
    justify-self: start;
  }

  fieldset {
    display: grid;
    margin: 0;
    padding: 0;
    border: 0;
  }

  fieldset .btn {
    justify-self: start;
  }

  legend {
    margin-bottom: var(--space-1);
    font-weight: var(--weight-medium);
  }

  .choice {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    min-height: var(--touch-target);
  }

  .choice input {
    flex: none;
    width: 1.25rem;
    height: 1.25rem;
    accent-color: var(--color-primary);
  }

  .field > .narrow {
    width: min(100%, 14rem);
  }

  .hint {
    font-size: var(--text-sm);
  }

  .heading {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }

  .backups {
    display: grid;
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .backups li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
  }

  .info {
    display: grid;
    gap: var(--space-1);
  }
</style>
