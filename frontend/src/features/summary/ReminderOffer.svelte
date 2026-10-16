<script lang="ts">
  import { toErrorDto } from '../../api/errors'
  import type { ErrorDto, SettingsDto } from '../../api/types'
  import { useServices } from '../../app/context'
  import Dialog from '../../components/Dialog.svelte'
  import ErrorState from '../../components/ErrorState.svelte'
  import Icon from '../../components/Icon.svelte'
  import { newOperationId } from '../../services/operationId'
  import { useQuery } from '../../services/query.svelte'

  /**
   * Offered after the first completed session. It only stores the reminder time; the native
   * notification adapter asks for the OS permission later (Phase 15), the GUI never fakes it.
   */
  const { api, i18n, queries, toasts } = useServices()
  const ids = $props.id()
  const settings = useQuery(queries, () => ({ key: ['settings'], fetch: () => api.getSettings() }))
  let open = $state(false)
  let dismissed = $state(false)
  let time = $state('19:00')
  let saving = $state(false)
  let error = $state<ErrorDto | null>(null)

  const offered = (current: SettingsDto) => !dismissed && current.reminderTime === null && current.notifications !== 'denied'

  async function save(current: SettingsDto) {
    if (saving) return
    saving = true
    error = null
    try {
      await api.updateSettings({ operationId: newOperationId(), expectedVersion: current.version, patch: { reminderTime: time } })
      open = false
      dismissed = true
      toasts.show(i18n.t('summary.reminderSaved', { time }), 'success')
    } catch (caught) {
      error = toErrorDto(caught)
    } finally {
      saving = false
    }
  }
</script>

{#if settings.state.status === 'ready' && offered(settings.state.data)}
  {@const current = settings.state.data}
  <section class="offer surface" aria-labelledby="{ids}-title">
    <span class="icon"><Icon name="clock" size={28} /></span>
    <div class="text">
      <h2 id="{ids}-title">{i18n.t('summary.reminderTitle')}</h2>
      <p class="muted">{i18n.t('summary.reminderBody')}</p>
    </div>
    <div class="row">
      <button type="button" class="btn btn-primary" onclick={() => (open = true)}>{i18n.t('summary.reminderAction')}</button>
      <button type="button" class="btn btn-ghost" onclick={() => (dismissed = true)}>{i18n.t('summary.reminderLater')}</button>
    </div>
  </section>

  <Dialog {open} title={i18n.t('summary.reminderDialog')} onclose={() => (open = false)}>
    <div class="field">
      <label for="{ids}-time">{i18n.t('summary.reminderTime')}</label>
      <input id="{ids}-time" class="input" type="time" bind:value={time} required />
    </div>
    {#if error}<ErrorState {error} compact />{/if}
    {#snippet actions()}
      <button type="button" class="btn" onclick={() => (open = false)}>{i18n.t('common.cancel')}</button>
      <button type="button" class="btn btn-primary" disabled={saving || !time} onclick={() => save(current)}>{i18n.t('summary.reminderSave')}</button>
    {/snippet}
  </Dialog>
{/if}

<style>
  .offer {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-3);
  }

  .icon {
    color: var(--color-primary);
  }

  .text {
    display: grid;
    flex: 1 1 14rem;
    gap: var(--space-1);
  }

  h2 {
    font-size: var(--text-lg);
  }
</style>
