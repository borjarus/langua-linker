<script lang="ts">
  import { toErrorDto } from '../../api/errors'
  import {
    CSV_COLUMN_TARGETS,
    CSV_DELIMITERS,
    DUPLICATE_POLICIES,
    type CsvColumnTarget,
    type CsvDelimiter,
    type DuplicatePolicy,
    type ErrorDto,
    type FileHandleDto,
    type Id,
    type ImportPreviewDto,
    type ImportRowErrorDto,
  } from '../../api/types'
  import { useServices } from '../../app/context'
  import { href } from '../../app/routes'
  import ErrorState from '../../components/ErrorState.svelte'
  import Icon from '../../components/Icon.svelte'
  import ProgressPanel from '../../components/ProgressPanel.svelte'
  import StatTile from '../../components/StatTile.svelte'
  import { JobTracker } from '../../services/jobTracker.svelte'
  import { newOperationId } from '../../services/operationId'
  import { useQuery } from '../../services/query.svelte'

  const { api, i18n, queries } = useServices()
  const ids = $props.id()
  const editor = useQuery(queries, () => ({ key: ['editor'], fetch: () => api.getEditorData() }))
  const job = new JobTracker(api)
  $effect(() => job.listen())

  const DELIMITER_KEYS: Record<CsvDelimiter, 'comma' | 'semicolon' | 'tab'> = { ',': 'comma', ';': 'semicolon', '\t': 'tab' }
  const IMPORT_COUNTS = ['create', 'update', 'skip', 'duplicate', 'error'] as const

  let file = $state<FileHandleDto | null>(null)
  let picking = $state(false)
  let pickError = $state<ErrorDto | null>(null)
  let previewing = $state(false)
  let previewError = $state<ErrorDto | null>(null)
  let preview = $state<ImportPreviewDto | null>(null)
  let delimiter = $state<CsvDelimiter | null>(null)
  let hasHeader = $state(true)
  let mapping = $state<CsvColumnTarget[] | null>(null)
  let duplicatePolicy = $state<DuplicatePolicy>('skip')
  let targetDeckId = $state<Id | null>(null)
  let operationId = newOperationId()

  const decks = $derived(editor.state.status === 'ready' ? editor.state.data.decks : [])
  const mappingProblem = $derived.by(() => {
    if (!mapping) return null
    const fronts = mapping.filter((target) => target === 'front').length
    const backs = mapping.filter((target) => target === 'back').length
    return fronts !== 1 || backs !== 1 ? i18n.t('transfer.mapping.needFrontBack') : null
  })
  const importable = $derived(preview ? preview.counts.create + preview.counts.update + preview.counts.duplicate : 0)
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
      delimiter = null
      hasHeader = true
      mapping = null
      targetDeckId = targetDeckId ?? decks[0]?.id ?? null
      await refresh()
    } catch (caught) {
      pickError = toErrorDto(caught)
    } finally {
      picking = false
    }
  }

  /** Dry run: every option change asks the backend again; nothing is written. */
  async function refresh() {
    if (!file || mappingProblem) return
    previewing = true
    previewError = null
    try {
      preview = await api.previewImport({ fileToken: file.token, delimiter, hasHeader, mapping, duplicatePolicy, targetDeckId })
      mapping = preview.mapping
      operationId = newOperationId()
    } catch (caught) {
      previewError = toErrorDto(caught)
      preview = null
    } finally {
      previewing = false
    }
  }

  function changeDelimiter(value: string) {
    delimiter = value === '' ? null : (value as CsvDelimiter)
    mapping = null
    void refresh()
  }

  function changeMapping(index: number, target: CsvColumnTarget) {
    if (!mapping) return
    mapping = mapping.map((current, position) => (position === index ? target : current))
    void refresh()
  }

  async function commit() {
    if (!preview || importable === 0 || job.active) return
    const token = preview.previewToken
    const started = await job.start(() => api.commitImport({ operationId, previewToken: token }))
    if (!started) {
      operationId = newOperationId()
      // A collection change since the preview: show the current numbers before importing.
      if (job.error?.code === 'PREVIEW_STALE') await refresh()
    }
  }

  function startOver() {
    file = null
    preview = null
    previewError = null
    job.reset()
  }

  function rowError(item: ImportRowErrorDto): string {
    const field = String(item.error.details?.field ?? '')
    const reason = String(item.error.details?.reason ?? '')
    const fieldName = field === 'front' || field === 'back' ? i18n.t(`transfer.mapping.targets.${field}`) : field
    const message = reason === 'required' ? i18n.t('transfer.rowErrors.required', { field: fieldName }) : reason === 'tooLong' ? i18n.t('transfer.rowErrors.tooLong', { field: fieldName }) : i18n.tDynamic(item.error.messageKey, 'errors.INTERNAL')
    return i18n.t('transfer.rowErrors.line', { row: item.row, message })
  }
</script>

{#snippet errors(list: ImportRowErrorDto[])}
  {#if list.length > 0}
    <details class="row-errors">
      <summary>{i18n.t('transfer.rowErrors.title', { count: list.length })}</summary>
      <ul>
        {#each list as item (item.row)}<li>{rowError(item)}</li>{/each}
      </ul>
    </details>
  {/if}
{/snippet}

<section class="surface stack" aria-labelledby="{ids}-title">
  <h2 id="{ids}-title">{i18n.t('transfer.import.title')}</h2>
  <p class="muted">{i18n.t('transfer.import.body')}</p>

  {#if job.job?.status === 'succeeded' && job.job.result?.type === 'import'}
    {@const result = job.job.result}
    <div class="done stack" role="status">
      <p class="ok"><Icon name="check" />{i18n.t('transfer.result.done', { count: result.create + result.duplicate + result.update })}</p>
      <dl class="tiles">
        {#each IMPORT_COUNTS as kind (kind)}
          <StatTile label={i18n.t(`transfer.counts.${kind}`)} value={i18n.formatNumber(result[kind])} tone={kind === 'error' && result[kind] > 0 ? 'error' : 'neutral'} />
        {/each}
      </dl>
      {@render errors(result.rowErrors)}
      <div class="row">
        <a class="btn btn-primary" href={href({ name: 'deck', params: { id: result.deckId } })}>{i18n.t('transfer.result.openDeck')}</a>
        <button type="button" class="btn" onclick={startOver}>{i18n.t('transfer.result.another')}</button>
      </div>
    </div>
  {:else}
    <div class="row">
      <button type="button" class="btn" class:btn-primary={!file} disabled={picking || job.active} aria-busy={picking} onclick={pick}>
        <Icon name="folder" />{i18n.t(file ? 'transfer.import.pickOther' : 'transfer.import.pick')}
      </button>
      {#if file}<span class="muted">{file.name} · {i18n.formatNumber(file.sizeBytes / 1000, { style: 'unit', unit: 'kilobyte', maximumFractionDigits: 1 })}</span>{/if}
    </div>
    {#if pickError}<ErrorState error={pickError} compact onretry={pick} />{/if}

    {#if unsupported !== null}
      <div class="instructions" role="alert">
        <h3>{i18n.t(unsupported === 'apkg' ? 'transfer.apkg.title' : 'transfer.unsupported.title')}</h3>
        {#if unsupported === 'apkg'}
          <p>{i18n.t('transfer.apkg.body')}</p>
          <ol>
            <li>{i18n.t('transfer.apkg.step1')}</li>
            <li>{i18n.t('transfer.apkg.step2')}</li>
            <li>{i18n.t('transfer.apkg.step3')}</li>
          </ol>
        {:else}
          <p>{i18n.t('transfer.unsupported.body')}</p>
        {/if}
      </div>
    {:else if previewError}
      <ErrorState error={previewError} compact onretry={refresh} />
    {/if}

    {#if preview}
      {@const data = preview}
      <div class="stack" aria-busy={previewing}>
        <p class="muted">
          {i18n.t('transfer.preview.file', { encoding: data.encoding, rows: data.totalRows })}
          {data.hasBom ? i18n.t('transfer.preview.bom') : ''}
        </p>
        <div class="options">
          <div class="field">
            <label for="{ids}-delimiter">{i18n.t('transfer.options.delimiter')}</label>
            <select id="{ids}-delimiter" class="input" value={delimiter ?? ''} disabled={job.active} onchange={(event) => changeDelimiter(event.currentTarget.value)}>
              <option value="">{i18n.t('transfer.options.detected', { delimiter: i18n.t(`transfer.delimiters.${DELIMITER_KEYS[data.detectedDelimiter ?? data.delimiter]}`) })}</option>
              {#each CSV_DELIMITERS as option (option)}
                <option value={option}>{i18n.t(`transfer.delimiters.${DELIMITER_KEYS[option]}`)}</option>
              {/each}
            </select>
          </div>
          <div class="field">
            <label for="{ids}-deck">{i18n.t('transfer.options.deck')}</label>
            <select id="{ids}-deck" class="input" value={targetDeckId ?? ''} disabled={job.active} onchange={(event) => { targetDeckId = event.currentTarget.value || null; void refresh() }}>
              {#each decks as deck (deck.id)}
                <option value={deck.id}>{deck.name}</option>
              {/each}
              <option value="">{i18n.t('transfer.options.newDeck', { name: data.fileName.replace(/\.[^.]+$/u, '') })}</option>
            </select>
          </div>
        </div>
        <label class="choice">
          <input type="checkbox" checked={hasHeader} disabled={job.active} onchange={(event) => { hasHeader = event.currentTarget.checked; mapping = null; void refresh() }} />
          {i18n.t('transfer.options.header')}
        </label>

        <fieldset class="group">
          <legend>{i18n.t('transfer.mapping.title')}</legend>
          <ul class="mapping">
            {#each data.columns as column, index (index)}
              <li>
                <label for="{ids}-map-{index}">
                  <span class="column">{data.hasHeader ? column : i18n.t('transfer.mapping.column', { number: index + 1 })}</span>
                  <span class="muted sample">{i18n.t('transfer.mapping.sample', { value: data.sampleRows[0]?.[index] ?? '' })}</span>
                </label>
                <select id="{ids}-map-{index}" class="input" value={mapping?.[index] ?? 'ignore'} disabled={job.active} onchange={(event) => changeMapping(index, event.currentTarget.value as CsvColumnTarget)}>
                  {#each CSV_COLUMN_TARGETS as target (target)}
                    <option value={target}>{i18n.t(`transfer.mapping.targets.${target}`)}</option>
                  {/each}
                </select>
              </li>
            {/each}
          </ul>
          {#if mappingProblem}<p class="invalid" role="alert">{mappingProblem}</p>{/if}
        </fieldset>

        <fieldset class="group">
          <legend>{i18n.t('transfer.duplicates.title')}</legend>
          {#each DUPLICATE_POLICIES as policy (policy)}
            <label class="choice policy">
              <input type="radio" name="{ids}-policy" checked={duplicatePolicy === policy} disabled={job.active} onchange={() => { duplicatePolicy = policy; void refresh() }} />
              <span>
                {i18n.t(`transfer.duplicates.${policy}`)}
                <span class="muted block">{i18n.t(`transfer.duplicates.${policy}Hint`)}</span>
              </span>
            </label>
          {/each}
        </fieldset>

        <section class="stack" aria-labelledby="{ids}-dry">
          <h3 id="{ids}-dry">{i18n.t('transfer.preview.title')}</h3>
          <dl class="tiles">
            {#each IMPORT_COUNTS as kind (kind)}
              <StatTile label={i18n.t(`transfer.counts.${kind}`)} value={i18n.formatNumber(data.counts[kind])} tone={kind === 'error' && data.counts[kind] > 0 ? 'error' : 'neutral'} />
            {/each}
          </dl>
          {@render errors(data.rowErrors)}
          {#if mapping && !mappingProblem}
            <ul class="samples" aria-label={i18n.t('transfer.preview.samples')}>
              {#each data.sampleRows.slice(0, 3) as row, index (index)}
                <li>
                  <strong>{row[mapping.indexOf('front')] ?? ''}</strong> → {row[mapping.indexOf('back')] ?? ''}
                  {#if mapping.includes('tags') && row[mapping.indexOf('tags')]}<span class="muted"> · {row[mapping.indexOf('tags')]}</span>{/if}
                </li>
              {/each}
            </ul>
          {/if}
        </section>

        {#if job.job && job.job.status !== 'succeeded'}
          <ProgressPanel title={i18n.t('transfer.import.progress')} status={job.job.status} progress={job.job.progress} oncancel={() => job.cancel()} />
          {#if job.job.status === 'cancelled'}<p class="muted" role="status">{i18n.t('transfer.import.cancelled')}</p>{/if}
          {#if job.job.status === 'failed' && job.job.error}<ErrorState error={job.job.error} compact />{/if}
        {/if}
        {#if job.error}<ErrorState error={job.error} compact />{/if}
        <button type="button" class="btn btn-primary commit" disabled={importable === 0 || job.active || previewing || mappingProblem !== null} aria-busy={job.active} onclick={commit}>
          {i18n.t('transfer.import.commit', { count: importable })}
        </button>
        {#if data.counts.update > 0}<p class="muted hint">{i18n.t('transfer.import.updateWarning', { count: data.counts.update })}</p>{/if}
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

  .options {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 14rem), 1fr));
    gap: var(--space-3);
  }

  .group {
    display: grid;
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    border: 0;
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

  .policy {
    align-items: start;
    padding-block: var(--space-1);
  }

  .choice input {
    flex: none;
    width: 1.25rem;
    height: 1.25rem;
    margin-top: 0.125rem;
    accent-color: var(--color-primary);
  }

  .block {
    display: block;
    font-size: var(--text-sm);
  }

  .mapping {
    display: grid;
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .mapping li {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 12rem), 1fr));
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
  }

  .mapping label {
    display: grid;
  }

  .column {
    font-weight: var(--weight-medium);
  }

  .sample,
  .hint {
    font-size: var(--text-sm);
  }

  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 8rem), 1fr));
    gap: var(--space-3);
    margin: 0;
  }

  .samples {
    display: grid;
    gap: var(--space-1);
    margin: 0;
    padding-inline-start: var(--space-5);
  }

  .row-errors summary {
    min-height: var(--touch-target);
    display: flex;
    align-items: center;
    color: var(--color-error);
    font-weight: var(--weight-medium);
    cursor: pointer;
  }

  .row-errors ul {
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

  .instructions ol {
    margin: 0;
    padding-inline-start: var(--space-5);
  }

  .invalid {
    color: var(--color-error);
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
