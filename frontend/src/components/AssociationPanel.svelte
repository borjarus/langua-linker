<script lang="ts">
  import type { AssociationDto } from '../api/types'
  import { useServices } from '../app/context'
  import { reportable, type ReportCard } from '../services/report'
  import Chip from './Chip.svelte'
  import Disclosure from './Disclosure.svelte'
  import Icon from './Icon.svelte'
  import ExtendedAssociation from './ExtendedAssociation.svelte'
  import ReportAssociationDialog from './ReportAssociationDialog.svelte'

  interface Props {
    associations: AssociationDto[]
    expanded?: boolean
    /** Bindable open state of the section (see `Disclosure`). */
    open?: boolean
    /** Id of the section, for jumping to it. */
    id?: string
    /** Generation never blocks rating; the parent decides whether it is offered. */
    ongenerate?: () => void
    generating?: boolean
    onfavorite?: (association: AssociationDto) => void
    /** The card shown with the associations; given → generated and bundled ones can be reported. */
    card?: ReportCard
  }

  let { associations, expanded = false, open = $bindable(), id, ongenerate, generating = false, onfavorite, card }: Props = $props()
  let reporting = $state<AssociationDto | null>(null)
  const { i18n } = useServices()
  const ordered = $derived([...associations].sort((a, b) => Number(b.favorite) - Number(a.favorite)))
</script>

<Disclosure title={i18n.t('association.title')} {expanded} bind:open {id} icon="lightbulb" count={associations.length}>
  {#if ordered.length === 0}
    <p class="muted">{i18n.t('association.empty')}</p>
  {:else}
    <ul class="list">
      {#each ordered as association (association.id)}
        <li class="item status-{association.status}" class:favorite={association.favorite}>
          {#if association.status === 'ready'}
            <p class="text">{association.text}</p>
            {#if association.extended}<ExtendedAssociation text={association.extended} />{/if}
          {:else}
            <p class="state row" aria-busy={association.status === 'pending'}>
              <Icon name={association.status === 'offlineQueued' ? 'offline' : association.status === 'pending' ? 'clock' : 'alert'} />
              {#if association.error}
                <!-- Provider error, unusable output and moderation are different outcomes for the user. -->
                {i18n.tDynamic(association.error.messageKey, `association.status.${association.status}`)}
              {:else}
                {i18n.t(`association.status.${association.status}`)}
              {/if}
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
            {#if card && reportable(association)}
              <button type="button" class="btn btn-ghost" onclick={() => (reporting = association)}>
                <Icon name="alert" />
                {i18n.t('association.report.open')}
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

{#if reporting && card}
  <ReportAssociationDialog association={reporting} {card} onclose={() => (reporting = null)} />
{/if}

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
