<script lang="ts">
  import type { AssociationDto } from '../api/types'
  import { useServices } from '../app/context'
  import Chip from './Chip.svelte'
  import Disclosure from './Disclosure.svelte'
  import Icon from './Icon.svelte'

  interface Props {
    associations: AssociationDto[]
    expanded?: boolean
    /** Generation never blocks rating; the parent decides whether it is offered. */
    ongenerate?: () => void
    generating?: boolean
    onfavorite?: (association: AssociationDto) => void
  }

  let { associations, expanded = false, ongenerate, generating = false, onfavorite }: Props = $props()
  const { i18n } = useServices()
  const ordered = $derived([...associations].sort((a, b) => Number(b.favorite) - Number(a.favorite)))
</script>

<Disclosure title={i18n.t('association.title')} {expanded} count={associations.length}>
  {#if ordered.length === 0}
    <p class="muted">{i18n.t('association.empty')}</p>
  {:else}
    <ul class="list">
      {#each ordered as association (association.id)}
        <li class="item status-{association.status}" class:favorite={association.favorite}>
          {#if association.status === 'ready'}
            <p class="text">{association.text}</p>
          {:else}
            <p class="state row">
              <Icon name={association.status === 'offlineQueued' ? 'offline' : association.status === 'pending' ? 'clock' : 'alert'} />
              {i18n.t(`association.status.${association.status}`)}
            </p>
          {/if}
          <div class="row meta">
            <Chip
              label={i18n.t(`association.provenance.${association.provenance}`)}
              tone={association.provenance === 'generated' ? 'info' : 'neutral'}
              icon={association.provenance === 'generated' ? 'sparkles' : undefined}
            />
            {#if association.favorite}
              <Chip label={i18n.t('association.favorite')} icon="star" tone="primary" />
            {:else if onfavorite && association.status === 'ready'}
              <button type="button" class="btn btn-ghost" onclick={() => onfavorite?.(association)}>
                <Icon name="star" />
                {i18n.t('association.setFavorite')}
              </button>
            {/if}
          </div>
        </li>
      {/each}
    </ul>
  {/if}
  {#if ongenerate}
    <button type="button" class="btn" disabled={generating} aria-busy={generating} onclick={ongenerate}>
      <Icon name="sparkles" />
      {i18n.t('association.generate')}
    </button>
  {/if}
</Disclosure>

<style>
  .list {
    display: grid;
    gap: var(--space-3);
    margin: 0 0 var(--space-3);
    padding: 0;
    list-style: none;
  }

  .item {
    display: grid;
    gap: var(--space-2);
    padding: var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
  }

  .favorite {
    border-color: var(--color-primary);
  }

  .status-error,
  .status-blocked {
    border-color: var(--color-error);
  }

  .state {
    color: var(--color-text-muted);
  }
</style>
