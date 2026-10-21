<script lang="ts">
  import type { JobDto, ResetScope } from '../../api/types'
  import { useServices } from '../../app/context'
  import ErrorState from '../../components/ErrorState.svelte'
  import Icon from '../../components/Icon.svelte'
  import Skeleton from '../../components/Skeleton.svelte'
  import Screen from '../shared/Screen.svelte'
  import ResetDialog from './ResetDialog.svelte'
  import SaveStatus from './SaveStatus.svelte'
  import { SettingsController } from './settingsController.svelte'

  const { api, i18n, queries, router, toasts } = useServices()
  const ids = $props.id()
  const controller = new SettingsController(api)
  void controller.load()
  let resetting = $state<ResetScope | null>(null)

  async function done(scope: ResetScope, job: JobDto) {
    resetting = null
    queries.clear()
    const safety = job.result?.type === 'reset' && job.result.safetyBackupId !== null
    toasts.show(i18n.t(`privacy.reset.${scope}.done`) + (safety ? ` ${i18n.t('privacy.reset.safetyKept')}` : ''), 'success', 8000)
    // After deleting everything the app starts over, like a fresh install.
    await router.replace(scope === 'everything' ? { name: 'onboarding' } : { name: 'home' })
  }
</script>

<Screen title={i18n.t('routes.settingsPrivacy')} back notice={i18n.t('settings.prototype')}>
  {#if controller.loadError}
    <ErrorState error={controller.loadError} onretry={() => void controller.load()} />
  {:else if !controller.settings}
    <Skeleton lines={4} />
  {:else}
    {@const settings = controller.settings}
    <section class="surface stack" aria-labelledby="{ids}-telemetry">
      <h2 id="{ids}-telemetry">{i18n.t('privacy.telemetry.title')}</h2>
      <p>{i18n.t('privacy.telemetry.body')}</p>
      <label class="choice">
        <input type="checkbox" checked={settings.telemetryOptIn} onchange={(event) => controller.update('telemetry', { telemetryOptIn: event.currentTarget.checked })} />
        {i18n.t('privacy.telemetry.optIn')}
      </label>
      <p class="muted hint">{i18n.t('privacy.telemetry.never')}</p>
      <SaveStatus {controller} field="telemetry" />
    </section>
  {/if}

  <section class="surface stack" aria-labelledby="{ids}-local">
    <h2 id="{ids}-local">{i18n.t('privacy.local.title')}</h2>
    <p>{i18n.t('privacy.local.body')}</p>
    <p class="muted hint">{i18n.t('privacy.local.logs')}</p>
  </section>

  <section class="surface stack danger-zone" aria-labelledby="{ids}-reset">
    <h2 id="{ids}-reset">{i18n.t('privacy.reset.title')}</h2>
    <div class="action">
      <div>
        <h3>{i18n.t('privacy.reset.progress.title')}</h3>
        <p class="muted hint">{i18n.t('privacy.reset.progress.summary')}</p>
      </div>
      <button type="button" class="btn btn-danger" onclick={() => (resetting = 'progress')}><Icon name="refresh" />{i18n.t('privacy.reset.progress.open')}</button>
    </div>
    <div class="action">
      <div>
        <h3>{i18n.t('privacy.reset.everything.title')}</h3>
        <p class="muted hint">{i18n.t('privacy.reset.everything.summary')}</p>
      </div>
      <button type="button" class="btn btn-danger" onclick={() => (resetting = 'everything')}><Icon name="trash" />{i18n.t('privacy.reset.everything.open')}</button>
    </div>
  </section>

  {#if resetting}
    {@const scope = resetting}
    <ResetDialog {scope} onclose={() => (resetting = null)} ondone={(job) => done(scope, job)} />
  {/if}
</Screen>

<style>
  h2 {
    font-size: var(--text-lg);
  }

  h3 {
    font-size: var(--text-md);
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

  .hint {
    font-size: var(--text-sm);
  }

  .danger-zone {
    border-color: var(--color-error);
  }

  .action {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
  }

  .action > div {
    flex: 1 1 14rem;
    display: grid;
    gap: var(--space-1);
  }
</style>
