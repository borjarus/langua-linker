<script lang="ts">
  import { toErrorDto } from '../../api/errors'
  import { useServices } from '../../app/context'
  import Chip from '../../components/Chip.svelte'
  import DeckCard from '../../components/DeckCard.svelte'
  import StatTile from '../../components/StatTile.svelte'
  import { useQuery } from '../../services/query.svelte'
  import QueryView from '../shared/QueryView.svelte'
  import Screen from '../shared/Screen.svelte'
  import { StudyController } from '../study/studyController.svelte'

  interface Props {
    id: string
  }

  let { id }: Props = $props()
  const { api, i18n, queries, router, toasts } = useServices()
  const overview = useQuery(queries, () => ({ key: ['deck', id], fetch: () => api.getDeck(id) }))

  async function study(deckId: string) {
    try {
      const session = await StudyController.start(api, deckId)
      await router.navigate({ name: 'study', params: { deckId }, query: { session: session.sessionId } })
    } catch (caught) {
      toasts.show(i18n.tDynamic(toErrorDto(caught).messageKey, 'errors.INTERNAL'), 'error')
    }
  }
</script>

<Screen title={overview.state.status === 'ready' ? overview.state.data.deck.name : i18n.t('routes.deck')} back phase={4}>
  <QueryView query={overview}>
    {#snippet ready(data)}
      <p>{data.description}</p>
      <div class="row">
        <Chip label={i18n.t(`deck.types.${data.deck.deckType}`)} />
        <Chip label={i18n.t('deck.preset', { name: data.presetName })} />
      </div>
      <dl class="tiles">
        <StatTile label={i18n.t('deck.counts.new')} value={i18n.formatNumber(data.deck.counts.new)} tone="info" />
        <StatTile label={i18n.t('deck.counts.learning')} value={i18n.formatNumber(data.deck.counts.learning)} tone="warning" />
        <StatTile label={i18n.t('deck.counts.review')} value={i18n.formatNumber(data.deck.counts.review)} tone="success" />
      </dl>
      <div class="row">
        <button type="button" class="btn btn-primary" onclick={() => study(data.deck.id)}>{i18n.t('deck.study')}</button>
        <a class="btn" href={`#/browser?deck=${encodeURIComponent(data.deck.id)}`}>{i18n.t('nav.browser')}</a>
        <a class="btn" href={`#/editor?deckId=${encodeURIComponent(data.deck.id)}`}>{i18n.t('nav.add')}</a>
      </div>
      {#if data.children.length > 0}
        <h2>{i18n.t('deck.subdecks', { count: data.children.length })}</h2>
        <ul class="children">
          {#each data.children as child (child.id)}
            <li><DeckCard deck={child} onstudy={(selected) => study(selected.id)} /></li>
          {/each}
        </ul>
      {/if}
    {/snippet}
  </QueryView>
</Screen>

<style>
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 9rem), 1fr));
    gap: var(--space-3);
    margin: 0;
  }

  .children {
    display: grid;
    gap: var(--space-3);
    margin: 0;
    padding: 0;
    list-style: none;
  }
</style>
