<script lang="ts">
  import type { AttributionDto } from '../../api/types'
  import { useServices } from '../../app/context'
  import { useQuery } from '../../services/query.svelte'
  import QueryView from '../shared/QueryView.svelte'
  import Screen from '../shared/Screen.svelte'

  const { api, i18n, queries } = useServices()
  const attributions = useQuery(queries, () => ({ key: ['attributions'], fetch: () => api.getAttributions() }))
  const SCOPES: AttributionDto['scope'][] = ['content', 'dependency', 'font', 'icon']
</script>

<Screen title={i18n.t('routes.settingsAttributions')} back>
  <p class="muted">{i18n.t('attributions.intro')}</p>
  <QueryView query={attributions}>
    {#snippet ready(list)}
      {#each SCOPES.filter((scope) => list.some((item) => item.scope === scope)) as scope (scope)}
        <section class="stack">
          <h2>{i18n.t(`attributions.scopes.${scope}`)}</h2>
          <ul class="stack list">
            {#each list.filter((item) => item.scope === scope) as item (item.name)}
              <li class="surface">
                <h3>{item.name}</h3>
                <p class="muted">{i18n.t('attributions.license', { license: item.license })}</p>
                {#if item.url}<a href={item.url} target="_blank" rel="noreferrer noopener">{item.url}</a>{/if}
              </li>
            {/each}
          </ul>
        </section>
      {/each}
    {/snippet}
  </QueryView>
</Screen>

<style>
  .list {
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  h2 {
    font-size: var(--text-lg);
  }

  h3 {
    font-size: var(--text-md);
  }
</style>
