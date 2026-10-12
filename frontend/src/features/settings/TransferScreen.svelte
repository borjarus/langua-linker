<script lang="ts">
  import { toErrorDto } from '../../api/errors'
  import type { ErrorDto, ImportPreviewDto } from '../../api/types'
  import { useServices } from '../../app/context'
  import ErrorState from '../../components/ErrorState.svelte'
  import StatTile from '../../components/StatTile.svelte'
  import Screen from '../shared/Screen.svelte'

  const { api, i18n } = useServices()
  let preview = $state<ImportPreviewDto | null>(null)
  let error = $state<ErrorDto | null>(null)
  let busy = $state(false)
  const IMPORT_COUNTS = ['create', 'update', 'skip', 'duplicate', 'error'] as const

  /** Dry run only: preview never mutates the collection (commit arrives with Phase 5). */
  async function pickAndPreview() {
    busy = true
    error = null
    try {
      const file = await api.pickFile({ purpose: 'import' })
      if (!file) return
      preview = await api.previewImport({ fileToken: file.token, delimiter: null, hasHeader: true, mapping: null, duplicatePolicy: 'skip', targetDeckId: null })
    } catch (caught) {
      error = toErrorDto(caught)
    } finally {
      busy = false
    }
  }
</script>

<Screen title={i18n.t('routes.settingsTransfer')} back phase={5}>
  <button type="button" class="btn btn-primary" disabled={busy} onclick={pickAndPreview}>{i18n.t('transfer.pick')}</button>
  {#if error}<ErrorState {error} onretry={pickAndPreview} />{/if}
  {#if preview}
    <p class="muted">{preview.fileName} · {preview.encoding}</p>
    <dl class="tiles">
      {#each IMPORT_COUNTS as kind (kind)}
        <StatTile label={i18n.t(`transfer.counts.${kind}`)} value={i18n.formatNumber(preview.counts[kind])} tone={kind === 'error' && preview.counts[kind] > 0 ? 'error' : 'neutral'} />
      {/each}
    </dl>
  {/if}
</Screen>

<style>
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 8rem), 1fr));
    gap: var(--space-3);
    margin: 0;
  }
</style>
