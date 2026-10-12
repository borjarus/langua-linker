<script lang="ts">
  import { useServices } from '../../app/context'
  import { useQuery } from '../../services/query.svelte'
  import QueryView from '../shared/QueryView.svelte'
  import Screen from '../shared/Screen.svelte'

  const { api, i18n, queries } = useServices()
  const attributions = useQuery(queries, () => ({ key: ['attributions'], fetch: () => api.getAttributions() }))
</script>

<Screen title={i18n.t('routes.settingsAttributions')} back phase={5}>
  <QueryView query={attributions}>
    {#snippet ready(list)}
      <ul class="stack list">
        {#each list as item (item.name)}
          <li class="surface">
            <h2>{item.name}</h2>
            <p class="muted">{i18n.t('attributions.license', { license: item.license })}</p>
            {#if item.url}<a href={item.url} target="_blank" rel="noreferrer noopener">{item.url}</a>{/if}
          </li>
        {/each}
      </ul>
    {/snippet}
  </QueryView>
</Screen>

<style>
  .list {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  h2 {
    font-size: var(--text-md);
  }
</style>
