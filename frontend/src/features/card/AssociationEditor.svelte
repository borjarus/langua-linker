<script lang="ts">
  import type { AssociationDto, Id, NoteContentChange } from '../../api/types'
  import { useServices } from '../../app/context'
  import Chip from '../../components/Chip.svelte'
  import Icon from '../../components/Icon.svelte'

  interface Props {
    associations: AssociationDto[]
    busy: boolean
    generating: boolean
    onchange: (change: NoteContentChange) => Promise<boolean>
    onfavorite: (association: AssociationDto) => void
    ondelete: (association: AssociationDto) => void
    ongenerate: () => void
  }

  let { associations, busy, generating, onchange, onfavorite, ondelete, ongenerate }: Props = $props()
  const { i18n } = useServices()
  const ids = $props.id()

  const ordered = $derived([...associations].sort((a, b) => Number(b.favorite) - Number(a.favorite)))
  let draft = $state('')
  let editing = $state<{ id: Id; text: string } | null>(null)

  async function add(event: SubmitEvent) {
    event.preventDefault()
    if (!draft.trim()) return
    if (await onchange({ type: 'addAssociation', text: draft })) draft = ''
  }

  async function saveEdit() {
    if (!editing || !editing.text.trim()) return
    if (await onchange({ type: 'editAssociation', associationId: editing.id, text: editing.text })) editing = null
  }
</script>

<section class="stack" aria-labelledby="{ids}-title">
  <div class="row heading">
    <h2 id="{ids}-title">{i18n.t('association.title')}</h2>
    <button type="button" class="btn" disabled={generating} aria-busy={generating} onclick={ongenerate}>
      <Icon name="sparkles" />{i18n.t('association.generate')}
    </button>
  </div>
  {#if generating}<p class="muted" role="status">{i18n.t('card.associations.generating')}</p>{/if}
  {#if ordered.length === 0}
    <p class="muted">{i18n.t('association.empty')}</p>
  {:else}
    <ul class="list">
      {#each ordered as association (association.id)}
        <li class="item" class:favorite={association.favorite}>
          {#if editing?.id === association.id}
            <div class="field">
              <label for="{ids}-edit">{i18n.t('card.associations.editLabel')}</label>
              <textarea id="{ids}-edit" class="input" rows="2" maxlength="500" bind:value={editing.text}></textarea>
            </div>
            <div class="row">
              <button type="button" class="btn btn-primary" disabled={busy || !editing.text.trim()} onclick={saveEdit}>{i18n.t('common.save')}</button>
              <button type="button" class="btn" onclick={() => (editing = null)}>{i18n.t('common.cancel')}</button>
            </div>
          {:else}
            {#if association.status === 'ready'}
              <p class="text">{association.text}</p>
            {:else}
              <p class="muted row" aria-busy={association.status === 'pending'}>
                <Icon name={association.status === 'offlineQueued' ? 'offline' : association.status === 'pending' ? 'clock' : 'alert'} />
                {#if association.error}
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
              {:else if association.status === 'ready'}
                <button type="button" class="btn btn-ghost" disabled={busy} onclick={() => onfavorite(association)}>
                  <Icon name="star" />{i18n.t('association.setFavorite')}
                </button>
              {/if}
              {#if association.provenance === 'manual'}
                <button type="button" class="btn btn-ghost" disabled={busy} onclick={() => (editing = { id: association.id, text: association.text })}>
                  <Icon name="edit" />{i18n.t('card.edit')}
                </button>
              {/if}
              <button type="button" class="btn btn-ghost danger" disabled={busy} onclick={() => ondelete(association)}>
                <Icon name="trash" />{i18n.t('card.delete')}
              </button>
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
  <form class="stack add" onsubmit={add}>
    <div class="field">
      <label for="{ids}-new">{i18n.t('card.associations.newLabel')}</label>
      <textarea id="{ids}-new" class="input" rows="2" maxlength="500" bind:value={draft}></textarea>
    </div>
    <button type="submit" class="btn" disabled={busy || !draft.trim()}><Icon name="plus" />{i18n.t('card.associations.add')}</button>
  </form>
</section>

<style>
  .heading {
    justify-content: space-between;
  }

  h2 {
    font-size: var(--text-lg);
  }

  .list {
    display: grid;
    gap: var(--space-3);
    margin: 0;
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

  .text {
    overflow-wrap: anywhere;
  }

  .meta {
    gap: var(--space-1);
  }

  .add {
    justify-items: start;
  }

  .add .field {
    width: 100%;
  }

  .danger {
    color: var(--color-error);
  }
</style>
