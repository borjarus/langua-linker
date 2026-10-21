<script lang="ts">
  import { useServices } from '../../app/context'
  import ErrorState from '../../components/ErrorState.svelte'
  import Icon from '../../components/Icon.svelte'
  import type { SettingsController } from './settingsController.svelte'

  interface Props {
    controller: SettingsController
    field: string
  }

  let { controller, field }: Props = $props()
  const { i18n } = useServices()
  const error = $derived(controller.errorFor(field))
</script>

<!-- Settings save on change; this says so next to the control that changed. -->
{#if controller.saving === field}
  <span class="status muted" role="status">{i18n.t('settings.saving')}</span>
{:else if controller.saved === field}
  <span class="status ok" role="status"><Icon name="check" size={16} />{i18n.t('settings.saved')}</span>
{/if}
{#if error}<ErrorState {error} compact />{/if}

<style>
  .status {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    font-size: var(--text-sm);
  }

  .ok {
    color: var(--color-success);
  }
</style>
