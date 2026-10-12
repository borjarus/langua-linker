<script lang="ts">
  import { useServices } from '../../app/context'
  import AssociationPanel from '../../components/AssociationPanel.svelte'
  import Chip from '../../components/Chip.svelte'
  import GrammarTipList from '../../components/GrammarTipList.svelte'
  import { useQuery } from '../../services/query.svelte'
  import QueryView from '../shared/QueryView.svelte'
  import Screen from '../shared/Screen.svelte'

  interface Props {
    id: string
  }

  let { id }: Props = $props()
  const { api, i18n, queries } = useServices()
  const details = useQuery(queries, () => ({ key: ['note', id], fetch: () => api.getNoteDetails(id) }))
</script>

<Screen title={i18n.t('routes.card')} back phase={4}>
  <QueryView query={details}>
    {#snippet ready(data)}
      <section class="surface stack">
        <p class="front" dir="auto">{data.note.front}</p>
        <p dir="auto">{data.note.back}</p>
        <div class="row">
          <Chip label={i18n.t('card.noteType', { type: data.note.noteTypeKind })} />
          {#each data.note.tags as tag (tag)}<Chip label={tag} tone="info" />{/each}
        </div>
        <p class="muted">{i18n.t('card.cards', { count: data.cards.length })}</p>
      </section>
      <AssociationPanel associations={data.associations} expanded />
      <GrammarTipList tips={data.grammarTips} expanded />
      <a class="btn" href={`#/editor?noteId=${encodeURIComponent(data.note.id)}`}>{i18n.t('routes.editor')}</a>
    {/snippet}
  </QueryView>
</Screen>

<style>
  .front {
    font-size: var(--text-xl);
    font-weight: var(--weight-bold);
  }
</style>
