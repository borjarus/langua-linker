<script lang="ts">
  import { toErrorDto } from '../../api/errors'
  import type { DeckImportPreviewDto, ErrorDto, FileHandleDto } from '../../api/types'
  import { useServices } from '../../app/context'
  import { href } from '../../app/routes'
  import ErrorState from '../../components/ErrorState.svelte'
  import Icon from '../../components/Icon.svelte'
  import ProgressPanel from '../../components/ProgressPanel.svelte'
  import StatTile from '../../components/StatTile.svelte'
  import { JobTracker } from '../../services/jobTracker.svelte'
  import { newOperationId } from '../../services/operationId'

  const { api, i18n } = useServices()
  const ids = $props.id()
  const job = new JobTracker(api)
  $effect(() => job.listen())

  let file = $state<FileHandleDto | null>(null)
  let picking = $state(false)
  let pickError = $state<ErrorDto | null>(null)
  let previewing = $state(false)
  let previewError = $state<ErrorDto | null>(null)
  let preview = $state<DeckImportPreviewDto | null>(null)
  let operationId = newOperationId()

  const unsupported = $derived(previewError?.code === 'UNSUPPORTED_FORMAT' ? String(previewError.details?.format ?? '') : null)

  async function pick() {
    picking = true
    pickError = null
    try {
      const picked = await api.pickFile({ purpose: 'import' })
      // Closing the picker is a choice, not an error.
      if (!picked) return
      file = picked
      job.reset()
      await refresh()
    } catch (caught) {
      pickError = toErrorDto(caught)
    } finally {
      picking = false
    }
  }

  /** Dry run: reads and checks the package; nothing is written. */
  async function refresh() {
    if (!file) return
    previewing = true
    previewError = null
    try {
      preview = await api.previewDeckImport({ fileToken: file.token })
      operationId = newOperationId()
    } catch (caught) {
      previewError = toErrorDto(caught)
      preview = null
    } finally {
      previewing = false
    }
  }

  async function commit() {
    if (!preview || job.active) return
    const token = preview.previewToken
    const started = await job.start(() => api.commitDeckImport({ operationId, previewToken: token }))
    if (!started) {
      operationId = newOperationId()
      // The collection changed since the preview: show the current numbers before importing.
      if (job.error?.code === 'PREVIEW_STALE') await refresh()
    }
  }

  function startOver() {
    file = null
    preview = null
    previewError = null
    job.reset()
  }
</script>

<section class="surface stack" aria-labelledby="{ids}-title">
  <h2 id="{ids}-title">{i18n.t('transfer.deck.title')}</h2>
  <p class="muted">{i18n.t('transfer.deck.body')}</p>

  {#if job.job?.status === 'succeeded' && job.job.result?.type === 'import'}
    {@const result = job.job.result}
    <div class="done stack" role="status">
      <p class="ok"><Icon name="check" />{i18n.t('transfer.deck.done')}</p>
      <dl class="tiles">
        <StatTile label={i18n.t('transfer.deck.result.created')} value={i18n.formatNumber(result.create)} />
        <StatTile label={i18n.t('transfer.deck.result.updated')} value={i18n.formatNumber(result.update)} />
        <StatTile label={i18n.t('transfer.deck.result.unchanged')} value={i18n.formatNumber(result.skip)} />
      </dl>
      <div class="row">
        <a class="btn btn-primary" href={href({ name: 'deck', params: { id: result.deckId } })}>{i18n.t('transfer.deck.openDeck')}</a>
        <button type="button" class="btn" onclick={startOver}>{i18n.t('transfer.deck.another')}</button>
      </div>
    </div>
  {:else}
    <div class="row">
      <button type="button" class="btn" class:btn-primary={!file} disabled={picking || job.active} aria-busy={picking} onclick={pick}>
        <Icon name="folder" />{i18n.t(file ? 'transfer.deck.pickOther' : 'transfer.deck.pick')}
      </button>
      {#if file}<span class="muted">{file.name} · {i18n.formatNumber(file.sizeBytes / 1000, { style: 'unit', unit: 'kilobyte', maximumFractionDigits: 1 })}</span>{/if}
    </div>
    {#if pickError}<ErrorState error={pickError} compact onretry={pick} />{/if}

    {#if unsupported !== null}
      <div class="instructions" role="alert">
        {#if unsupported === 'apkg'}
          <h3>{i18n.t('transfer.deck.apkg.title')}</h3>
          <p>{i18n.t('transfer.deck.apkg.body')}</p>
          <code class="command">{i18n.t('transfer.deck.apkg.command')}</code>
          <p>{i18n.t('transfer.deck.apkg.after')}</p>
        {:else}
          <h3>{i18n.t('transfer.deck.unsupported.title')}</h3>
          <p>{i18n.t('transfer.deck.unsupported.body')}</p>
        {/if}
      </div>
    {:else if previewError}
      <ErrorState error={previewError} compact onretry={refresh} />
    {/if}

    {#if preview}
      {@const data = preview}
      <div class="stack" aria-busy={previewing}>
        <section class="stack" aria-labelledby="{ids}-deck">
          <h3 id="{ids}-deck">{data.name}</h3>
          <p class="muted">
            {i18n.t(`transfer.deck.languages.${data.targetLanguage}`)} · {i18n.t(`transfer.deck.kinds.${data.contentKind}`)} ·
            {i18n.t(`transfer.deck.noteTypes.${data.noteType}`)} · {i18n.t('transfer.deck.level', { level: data.level })}
          </p>
          {#if data.origin}
            <p class="muted small">{i18n.t('transfer.deck.origin', { file: data.origin.fileName, date: data.origin.convertedOn })}</p>
          {/if}
        </section>
        <dl class="tiles">
          <StatTile label={i18n.t('transfer.deck.counts.notes')} value={i18n.formatNumber(data.notes)} />
          <StatTile label={i18n.t('transfer.deck.counts.cards')} value={i18n.formatNumber(data.cards)} />
          <StatTile label={i18n.t('transfer.deck.counts.scheduled')} value={i18n.formatNumber(data.scheduledCards)} />
          <StatTile label={i18n.t('transfer.deck.counts.suspended')} value={i18n.formatNumber(data.suspendedCards)} />
          <StatTile label={i18n.t('transfer.deck.counts.reviews')} value={i18n.formatNumber(data.reviews)} />
        </dl>
        {#if data.existingDeckId !== null}
          <p class="note" role="note">{i18n.t('transfer.deck.existing', { known: data.existingNotes, added: data.newNotes })}</p>
        {/if}
        {#if data.warnings.length > 0}
          <details class="warnings">
            <summary>{i18n.t('transfer.deck.warnings', { count: data.warnings.length })}</summary>
            <ul>
              {#each data.warnings as warning, index (index)}
                <li>{warning.line === null ? warning.code : i18n.t('transfer.deck.warningLine', { line: warning.line, code: warning.code })}</li>
              {/each}
            </ul>
          </details>
        {/if}

        {#if job.job && job.job.status !== 'succeeded'}
          <ProgressPanel title={i18n.t('transfer.deck.progress')} status={job.job.status} progress={job.job.progress} />
          {#if job.job.status === 'failed' && job.job.error}<ErrorState error={job.job.error} compact />{/if}
        {/if}
        {#if job.error}<ErrorState error={job.error} compact />{/if}
        <button type="button" class="btn btn-primary commit" disabled={job.active || previewing} aria-busy={job.active} onclick={commit}>
          {i18n.t('transfer.deck.commit')}
        </button>
        <p class="muted small">{i18n.t('transfer.deck.keepsProgress')}</p>
      </div>
    {/if}
  {/if}
</section>

<style>
  h2 {
    font-size: var(--text-lg);
  }

  h3 {
    font-size: var(--text-md);
  }

  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 8rem), 1fr));
    gap: var(--space-3);
    margin: 0;
  }

  .small {
    font-size: var(--text-sm);
  }

  .note {
    padding: var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
  }

  .warnings summary {
    min-height: var(--touch-target);
    display: flex;
    align-items: center;
    font-weight: var(--weight-medium);
    cursor: pointer;
  }

  .warnings ul {
    margin: 0;
    padding-inline-start: var(--space-5);
  }

  .instructions {
    display: grid;
    gap: var(--space-2);
    padding: var(--space-3);
    border: 1px solid var(--color-warning);
    border-radius: var(--radius-md);
    background: var(--color-warning-soft);
  }

  .command {
    display: block;
    padding: var(--space-2);
    overflow-wrap: anywhere;
    border-radius: var(--radius-sm);
    background: var(--color-surface);
    font-size: var(--text-sm);
  }

  .ok {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--color-success);
    font-weight: var(--weight-medium);
  }

  .commit {
    justify-self: start;
  }
</style>
