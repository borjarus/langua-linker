<script lang="ts">
  import { toErrorDto } from '../../api/errors'
  import type { AiSettingsDto, ErrorDto, LocalModelDto } from '../../api/types'
  import { useServices } from '../../app/context'
  import ErrorState from '../../components/ErrorState.svelte'
  import Icon from '../../components/Icon.svelte'
  import { newOperationId } from '../../services/operationId'

  interface Props {
    model: LocalModelDto
    /** The AI settings after a command (the screen shows them). */
    onchange: (ai: AiSettingsDto) => void
  }

  let { model, onchange }: Props = $props()
  const { api, i18n } = useServices()
  const ids = $props.id()
  let busy = $state(false)
  let error = $state<ErrorDto | null>(null)

  const gigabytes = (bytes: number) => i18n.formatNumber(bytes / 1_000_000_000, { maximumFractionDigits: 1, minimumFractionDigits: 1 })
  const percent = $derived(model.bytes > 0 ? Math.floor((model.downloadedBytes / model.bytes) * 100) : 0)
  const resumable = $derived(model.state !== 'ready' && model.state !== 'downloading' && model.downloadedBytes > 0)

  async function run(command: 'downloadAiModel' | 'deleteAiModel') {
    if (busy) return
    busy = true
    error = null
    try {
      onchange(await api[command]({ operationId: newOperationId(), modelId: model.id }))
    } catch (caught) {
      error = toErrorDto(caught)
    } finally {
      busy = false
    }
  }
</script>

<section class="model stack" aria-labelledby="{ids}-title">
  <h3 id="{ids}-title">{model.name}</h3>
  <p class="muted hint">{i18n.t('ai.local.about', { size: gigabytes(model.bytes) })}</p>

  {#if model.state === 'ready'}
    <p class="state" role="status"><Icon name="check" />{i18n.t('ai.local.ready')}</p>
    <button type="button" class="btn btn-ghost danger" disabled={busy} onclick={() => run('deleteAiModel')}><Icon name="trash" />{i18n.t('ai.local.delete')}</button>
  {:else if model.state === 'downloading'}
    <p class="state" role="status">{i18n.t('ai.local.downloading', { percent, done: gigabytes(model.downloadedBytes), size: gigabytes(model.bytes) })}</p>
    <progress max={model.bytes} value={model.downloadedBytes} aria-label={i18n.t('ai.local.progress')}></progress>
    <button type="button" class="btn" disabled={busy} onclick={() => run('deleteAiModel')}><Icon name="close" />{i18n.t('ai.local.cancel')}</button>
  {:else}
    {#if model.state === 'failed'}
      <p class="note warning" role="alert"><Icon name="alert" />{i18n.tDynamic(`ai.local.errors.${model.error ?? 'network'}`, 'ai.local.errors.network')}</p>
    {:else}
      <p class="state">{i18n.t('ai.local.notDownloaded')}</p>
    {/if}
    <p class="muted hint">{i18n.t('ai.local.wifi')}</p>
    <div class="row">
      <button type="button" class="btn btn-primary" disabled={busy} aria-busy={busy} onclick={() => run('downloadAiModel')}>
        <Icon name="download" />{i18n.t(resumable ? 'ai.local.resume' : 'ai.local.download', { size: gigabytes(model.bytes) })}
      </button>
      {#if resumable}
        <button type="button" class="btn btn-ghost danger" disabled={busy} onclick={() => run('deleteAiModel')}><Icon name="trash" />{i18n.t('ai.local.discard')}</button>
      {/if}
    </div>
  {/if}
  {#if error}<ErrorState {error} compact />{/if}
</section>

<style>
  h3 {
    margin: 0;
    font-size: var(--text-md, 1rem);
  }

  .model {
    padding: var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
  }

  .state,
  .note {
    display: flex;
    align-items: start;
    gap: var(--space-2);
    margin: 0;
  }

  .note {
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-sm);
    background: var(--color-warning-soft, var(--color-info-soft));
  }

  .hint {
    font-size: var(--text-sm);
  }

  .danger {
    justify-self: start;
    color: var(--color-error);
  }

  progress {
    width: 100%;
    height: 0.75rem;
  }
</style>
