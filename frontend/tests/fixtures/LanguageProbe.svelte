<script lang="ts">
  import { useServices } from '../../src/app/context'

  const { i18n } = useServices()
</script>

<h1>{i18n.t('dashboard.title')}</h1>
<p>{i18n.t('deck.cards', { count: 5 })}</p>
