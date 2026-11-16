<script lang="ts">
  import { toErrorDto } from '../../api/errors'
  import { EXPORT_FORMATS, type ErrorDto, type ExportFormat } from '../../api/types'
  import { useServices } from '../../app/context'
  import { href } from '../../app/routes'
  import ErrorState from '../../components/ErrorState.svelte'
  import Icon from '../../components/Icon.svelte'
  import ProgressPanel from '../../components/ProgressPanel.svelte'
  import { JobTracker } from '../../services/jobTracker.svelte'
  import { newOperationId } from '../../services/operationId'
  import { useQuery } from '../../services/query.svelte'

  const { api, i18n, queries, toasts } = useServices()
  const ids = $props.id()
  const editor = useQuery(queries, () => ({ key: ['editor'], fetch: () => api.getEditorData() }))
  const job = new JobTracker(api)
  $effect(() => job.listen())

  let deckId = $state('')
  let format = $state<ExportFormat>('csv')
  let includeScheduling = $state(false)
  let formulaGuard = $state(false)
  let sharing = $state(false)
  let shareError = $state<ErrorDto | null>(null)

  const decks = $derived(editor.state.status === 'ready' ? editor.state.data.decks : [])
  const selected = $derived(deckId || decks[0]?.id || '')

  function exportDeck() {
    if (!selected || job.active) return
    shareError = null
    void job.start(() =>
      api.exportDeck({ operationId: newOperationId(), deckId: selected, format, includeScheduling: format === 'apkg' || includeScheduling, formulaGuard: format === 'csv' && formulaGuard }),
    )
  }

  async function share(jobId: string) {
    sharing = true
    shareError = null
    try {
      const result = await api.shareFile({ operationId: newOperationId(), target: { kind: 'export', jobId } })
      if (result.shared) toasts.show(i18n.t('transfer.export.shared'), 'success')
    } catch (caught) {
      shareError = toErrorDto(caught)
    } finally {
      sharing = false
    }
  }
</script>

<section class="surface stack" aria-labelledby="{ids}-title">
  <h2 id="{ids}-title">{i18n.t('transfer.export.title')}</h2>
  <p class="muted">{i18n.t('transfer.export.body')}</p>
  <p class="hint"><Icon name="info" /><span>{i18n.t('transfer.export.notBackup')} <a href={href({ name: 'settingsBackup' })}>{i18n.t('routes.settingsBackup')}</a></span></p>
  {#if editor.state.status === 'error'}
    <ErrorState error={editor.state.error} compact onretry={() => void editor.refresh()} />
  {:else if editor.state.status === 'ready' && decks.length === 0}
    <p class="muted">{i18n.t('transfer.export.noDecks')}</p>
  {:else}
    <div class="field">
      <label for="{ids}-deck">{i18n.t('transfer.export.deck')}</label>
      <select id="{ids}-deck" class="input" value={selected} disabled={job.active} onchange={(event) => (deckId = event.currentTarget.value)}>
        {#each decks as deck (deck.id)}
          <option value={deck.id}>{deck.name}</option>
        {/each}
      </select>
    </div>
    <fieldset class="group">
      <legend>{i18n.t('transfer.export.format')}</legend>
      {#each EXPORT_FORMATS as option (option)}
        <label class="choice">
          <input type="radio" name="{ids}-format" checked={format === option} disabled={job.active} onchange={() => (format = option)} />
          {i18n.t(`transfer.export.formats.${option}`)}
        </label>
      {/each}
    </fieldset>
    {#if format === 'csv'}
      <label class="choice">
        <input type="checkbox" bind:checked={includeScheduling} disabled={job.active} />
        {i18n.t('transfer.export.scheduling')}
      </label>
      <label class="choice">
        <input type="checkbox" bind:checked={formulaGuard} disabled={job.active} aria-describedby="{ids}-guard" />
        {i18n.t('transfer.export.formulaGuard')}
      </label>
      <p id="{ids}-guard" class="muted hint">{i18n.t('transfer.export.formulaGuardHint')}</p>
    {:else}
      <p class="muted hint">{i18n.t('transfer.export.apkgHint')}</p>
    {/if}
    <button type="button" class="btn" disabled={!selected || job.active} aria-busy={job.active} onclick={exportDeck}><Icon name="copy" />{i18n.t('transfer.export.action')}</button>
    {#if job.job && job.job.status !== 'succeeded'}
      <ProgressPanel title={i18n.t('transfer.export.progress')} status={job.job.status} progress={job.job.progress} oncancel={() => job.cancel()} />
      {#if job.job.status === 'failed' && job.job.error}<ErrorState error={job.job.error} compact />{/if}
    {/if}
    {#if job.error}<ErrorState error={job.error} compact />{/if}
    {#if job.job?.status === 'succeeded' && job.job.result?.type === 'export'}
      {@const result = job.job.result}
      {@const jobId = job.job.jobId}
      <div class="done" role="status">
        <p><Icon name="check" />{i18n.t('transfer.export.done', { file: result.fileName, count: result.cardCount })}</p>
        {#if result.losses.length > 0}
          <details class="losses">
            <summary>{i18n.t('transfer.export.losses.title')}</summary>
            <ul>
              {#each result.losses as loss (loss.code)}
                <li>{i18n.tDynamic(`transfer.export.losses.codes.${loss.code}`, 'transfer.export.losses.title')}: {i18n.formatNumber(loss.count)}</li>
              {/each}
            </ul>
          </details>
        {/if}
        <button type="button" class="btn btn-primary" disabled={sharing} aria-busy={sharing} onclick={() => share(jobId)}>{i18n.t('transfer.export.share')}</button>
      </div>
      {#if shareError}<ErrorState error={shareError} compact />{/if}
    {/if}
  {/if}
</section>

<style>
  h2 {
    font-size: var(--text-lg);
  }

  .hint,
  .done p {
    display: flex;
    align-items: start;
    gap: var(--space-2);
  }

  .hint {
    font-size: var(--text-sm);
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

  .group {
    display: grid;
    gap: var(--space-1);
    margin: 0;
    padding: 0;
    border: 0;
  }

  legend {
    margin-bottom: var(--space-1);
    font-weight: var(--weight-medium);
  }

  .losses ul {
    margin: 0;
    padding-inline-start: var(--space-5);
  }

  .losses summary {
    min-height: var(--touch-target);
    display: flex;
    align-items: center;
    cursor: pointer;
  }

  section > .btn {
    justify-self: start;
  }

  .done {
    display: grid;
    gap: var(--space-2);
    justify-items: start;
  }
</style>
