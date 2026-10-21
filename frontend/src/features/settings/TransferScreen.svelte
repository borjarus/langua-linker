<script lang="ts">
  import { useServices } from '../../app/context'
  import Screen from '../shared/Screen.svelte'
  import ExportPanel from './ExportPanel.svelte'
  import ImportPanel from './ImportPanel.svelte'

  const { i18n } = useServices()
</script>

<Screen title={i18n.t('routes.settingsTransfer')} back notice={i18n.t('transfer.prototype')}>
  <ImportPanel />
  <ExportPanel />
</Screen>
