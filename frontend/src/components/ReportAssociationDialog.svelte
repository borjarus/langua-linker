<script lang="ts">
  import type { AssociationDto } from '../api/types'
  import { useServices } from '../app/context'
  import { useQuery } from '../services/query.svelte'
  import { MAX_COMMENT, REPORT_EMAIL, REPORT_REASONS, reportBody, reportMailto, type ReportCard, type ReportReason } from '../services/report'
  import Dialog from './Dialog.svelte'
  import Icon from './Icon.svelte'

  interface Props {
    association: AssociationDto
    card: ReportCard
    onclose: () => void
  }

  let { association, card, onclose }: Props = $props()
  const { api, i18n, queries, toasts } = useServices()
  const ids = $props.id()
  const info = useQuery(queries, () => ({ key: ['appInfo'], fetch: () => api.getAppInfo() }))

  let reason = $state<ReportReason>('offensive')
  let comment = $state('')
  const report = $derived({
    reason,
    comment,
    association: { text: association.text, provenance: association.provenance, model: association.model },
    card,
    appVersion: info.state.status === 'ready' ? info.state.data.appVersion : null,
  })

  /** The mail app opens with everything shown here; nothing is sent by the app itself. */
  function opened() {
    toasts.show(i18n.t('association.report.thanks'), 'success')
    onclose()
  }
</script>

<Dialog open title={i18n.t('association.report.title')} {onclose}>
  <div class="stack">
    <p>{i18n.t('association.report.intro', { email: REPORT_EMAIL })}</p>
    <fieldset class="reasons">
      <legend>{i18n.t('association.report.reason')}</legend>
      {#each REPORT_REASONS as value (value)}
        <label class="choice">
          <input type="radio" name="{ids}-reason" {value} bind:group={reason} />
          {i18n.t(`association.report.reasons.${value}`)}
        </label>
      {/each}
    </fieldset>
    <label class="stack" for="{ids}-comment">
      <span>{i18n.t('association.report.comment')}</span>
      <textarea id="{ids}-comment" class="input" rows="2" maxlength={MAX_COMMENT} bind:value={comment}></textarea>
    </label>
    <section class="stack" aria-labelledby="{ids}-preview">
      <h3 id="{ids}-preview">{i18n.t('association.report.preview')}</h3>
      <pre class="preview">{reportBody(report)}</pre>
    </section>
  </div>
  {#snippet actions()}
    <button type="button" class="btn btn-ghost" onclick={onclose}>{i18n.t('common.cancel')}</button>
    <a class="btn btn-primary" href={reportMailto(report)} onclick={opened}><Icon name="alert" />{i18n.t('association.report.send')}</a>
  {/snippet}
</Dialog>

<style>
  .reasons {
    display: grid;
    gap: var(--space-1);
    margin: 0;
    padding: 0;
    border: 0;
  }

  legend {
    margin-bottom: var(--space-1);
  }

  .choice {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    min-height: var(--touch-target);
  }

  h3 {
    font-size: var(--text-md);
  }

  .preview {
    margin: 0;
    padding: var(--space-3);
    overflow-wrap: anywhere;
    white-space: pre-wrap;
    border-radius: var(--radius-sm);
    background: var(--color-surface-sunken);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }
</style>
