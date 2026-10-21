<script lang="ts">
  import { toErrorDto } from '../../api/errors'
  import type { DiagnosticsReportDto, ErrorDto } from '../../api/types'
  import { useServices } from '../../app/context'
  import { href } from '../../app/routes'
  import ErrorState from '../../components/ErrorState.svelte'
  import Icon from '../../components/Icon.svelte'
  import { useQuery } from '../../services/query.svelte'
  import QueryView from '../shared/QueryView.svelte'
  import Screen from '../shared/Screen.svelte'

  const { api, i18n, queries, toasts } = useServices()
  const ids = $props.id()
  const info = useQuery(queries, () => ({ key: ['appInfo'], fetch: () => api.getAppInfo() }))
  const about = useQuery(queries, () => ({ key: ['about'], fetch: () => api.getAboutInfo() }))

  let report = $state<DiagnosticsReportDto | null>(null)
  let reportError = $state<ErrorDto | null>(null)
  let loadingReport = $state(false)

  async function loadReport() {
    loadingReport = true
    reportError = null
    try {
      report = await api.getDiagnosticsReport()
    } catch (caught) {
      reportError = toErrorDto(caught)
    } finally {
      loadingReport = false
    }
  }

  const reportText = (value: DiagnosticsReportDto) =>
    [`# ${value.generatedAt}`, ...value.lines.map((line) => `${line.at} ${line.level.toUpperCase()} ${line.target}: ${line.message}`)].join('\n')

  /** Copies only the redacted lines the backend returned; the user decides where to paste them. */
  async function copy(value: DiagnosticsReportDto) {
    try {
      await navigator.clipboard.writeText(reportText(value))
      toasts.show(i18n.t('about.diagnostics.copied'), 'success')
    } catch {
      toasts.show(i18n.t('about.diagnostics.copyFailed'), 'error')
    }
  }
</script>

<Screen title={i18n.t('routes.settingsAbout')} back>
  <section class="surface stack" aria-labelledby="{ids}-versions">
    <h2 id="{ids}-versions">{i18n.t('about.versions')}</h2>
    <QueryView query={about} skeletonLines={5}>
      {#snippet ready(data)}
        <dl class="facts">
          <div><dt>{i18n.t('about.app')}</dt><dd>{info.state.status === 'ready' ? `${info.state.data.appName} ${data.appVersion}` : data.appVersion}</dd></div>
          <div><dt>{i18n.t('about.mode')}</dt><dd>{info.state.status === 'ready' ? i18n.t(`about.modes.${info.state.data.mode}`) : '—'}</dd></div>
          <div><dt>{i18n.t('about.contract')}</dt><dd>{data.apiVersion}</dd></div>
          <div><dt>{i18n.t('about.schema')}</dt><dd>{data.schemaVersion ?? i18n.t('about.noDatabase')}</dd></div>
          <div><dt>{i18n.t('about.scheduler')}</dt><dd>{data.fsrsVersion}</dd></div>
          <div><dt>{i18n.t('about.content')}</dt><dd>{data.contentVersion}</dd></div>
          <div><dt>{i18n.t('about.platform')}</dt><dd>{data.platform}</dd></div>
        </dl>
      {/snippet}
    </QueryView>
    <a class="btn" href={href({ name: 'settingsAttributions' })}><Icon name="link" />{i18n.t('routes.settingsAttributions')}</a>
  </section>

  <section class="surface stack" aria-labelledby="{ids}-diagnostics">
    <h2 id="{ids}-diagnostics">{i18n.t('about.diagnostics.title')}</h2>
    <p>{i18n.t('about.diagnostics.body')}</p>
    {#if !report}
      <button type="button" class="btn" disabled={loadingReport} aria-busy={loadingReport} onclick={loadReport}>{i18n.t('about.diagnostics.show')}</button>
    {/if}
    {#if reportError}<ErrorState error={reportError} compact onretry={loadReport} />{/if}
    {#if report}
      {@const value = report}
      <p class="muted hint">{i18n.t('about.diagnostics.redacted', { count: value.redactions })}</p>
      <ol class="log" aria-label={i18n.t('about.diagnostics.title')}>
        {#each value.lines as line, index (index)}
          <li class="level-{line.level}">
            <span class="meta">{i18n.formatDate(line.at, { timeStyle: 'medium' })} · {line.level.toUpperCase()} · {line.target}</span>
            <span class="message">{line.message}</span>
          </li>
        {/each}
      </ol>
      <div class="row">
        <button type="button" class="btn" onclick={() => copy(value)}><Icon name="copy" />{i18n.t('about.diagnostics.copy')}</button>
        <button type="button" class="btn btn-ghost" onclick={loadReport}><Icon name="refresh" />{i18n.t('common.refresh')}</button>
      </div>
    {/if}
  </section>
</Screen>

<style>
  h2 {
    font-size: var(--text-lg);
  }

  .facts {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 12rem), 1fr));
    gap: var(--space-3);
    margin: 0;
  }

  .facts div {
    display: grid;
    gap: 0.125rem;
  }

  dt {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
  }

  dd {
    margin: 0;
    font-weight: var(--weight-medium);
  }

  section > .btn {
    justify-self: start;
  }

  .hint {
    font-size: var(--text-sm);
  }

  .log {
    display: grid;
    gap: var(--space-2);
    margin: 0;
    padding: var(--space-3);
    border-radius: var(--radius-sm);
    background: var(--color-surface-sunken);
    list-style: none;
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }

  .log li {
    display: grid;
  }

  .meta {
    color: var(--color-text-muted);
  }

  .level-error .meta {
    color: var(--color-error);
  }

  .level-warn .meta {
    color: var(--color-warning);
  }
</style>
