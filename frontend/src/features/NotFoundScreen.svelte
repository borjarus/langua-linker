<script lang="ts">
  import { useServices } from '../app/context'
  import EmptyState from '../components/EmptyState.svelte'
  import Screen from './shared/Screen.svelte'

  interface Props {
    path: string
  }

  let { path }: Props = $props()
  const { i18n } = useServices()
</script>

<Screen title={i18n.t('routes.notFound')}>
  <EmptyState title={i18n.t('routes.notFound')} description={i18n.t('notFound.body', { path })} icon="help">
    {#snippet actions()}
      <a class="btn btn-primary" href="#/home">{i18n.t('common.goHome')}</a>
    {/snippet}
  </EmptyState>
</Screen>
