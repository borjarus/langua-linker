<script lang="ts">
  import type { JobProgressDto, JobStatus } from '../api/types'
  import { useServices } from '../app/context'

  interface Props {
    title: string
    status: JobStatus
    progress: JobProgressDto | null
    /** Shown only while the job can still be cancelled. */
    oncancel?: () => void
  }

  let { title, status, progress, oncancel }: Props = $props()
  const { i18n } = useServices()
  const titleId = $props.id()
  const active = $derived(status === 'queued' || status === 'running')
</script>

<section class="panel surface" aria-labelledby={titleId} aria-busy={active}>
  <div class="row header">
    <h3 id={titleId}>{title}</h3>
    <span class="status status-{status}">{i18n.t(`job.status.${status}`)}</span>
  </div>
  {#if progress}
    <progress max={progress.total} value={progress.done} aria-labelledby={titleId}></progress>
    <p class="muted" aria-live="polite">{i18n.t('job.progress', { done: progress.done, total: progress.total })}</p>
  {:else if active}
    <progress aria-labelledby={titleId}></progress>
  {/if}
  {#if active && oncancel}
    <button type="button" class="btn" onclick={oncancel}>{i18n.t('job.cancel')}</button>
  {/if}
</section>

<style>
  .panel {
    display: grid;
    gap: var(--space-2);
  }

  .header {
    justify-content: space-between;
  }

  h3 {
    font-size: var(--text-md);
  }

  progress {
    width: 100%;
    height: 0.75rem;
    accent-color: var(--color-primary);
  }

  .status {
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
  }

  .status-failed {
    color: var(--color-error);
  }

  .status-succeeded {
    color: var(--color-success);
  }

  .panel > .btn {
    justify-self: start;
  }
</style>
