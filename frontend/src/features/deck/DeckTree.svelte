<script lang="ts">
  import type { DeckSummaryDto, Id } from '../../api/types'
  import { useServices } from '../../app/context'
  import { href } from '../../app/routes'
  import Icon from '../../components/Icon.svelte'
  import Self from './DeckTree.svelte'

  interface Props {
    /** All subdecks of the overview deck (any depth). */
    decks: DeckSummaryDto[]
    parentId: Id
    collapsed: Id[]
    ontoggle: (deckId: Id) => void
    onstudy: (deck: DeckSummaryDto) => void
    level?: number
  }

  let { decks, parentId, collapsed, ontoggle, onstudy, level = 1 }: Props = $props()
  const { i18n } = useServices()
  const children = $derived(decks.filter((deck) => deck.parentId === parentId))
  const hasChildren = (deckId: Id) => decks.some((deck) => deck.parentId === deckId)
  const due = (deck: DeckSummaryDto) => deck.counts.new + deck.counts.learning + deck.counts.review
</script>

<ul class="tree" class:nested={level > 1}>
  {#each children as deck (deck.id)}
    {@const open = !collapsed.includes(deck.id)}
    <li>
      <div class="node row">
        {#if hasChildren(deck.id)}
          <button
            type="button"
            class="btn btn-ghost btn-icon"
            aria-expanded={open}
            aria-label={i18n.t(open ? 'deck.tree.collapse' : 'deck.tree.expand', { name: deck.name })}
            onclick={() => ontoggle(deck.id)}
          >
            <Icon name={open ? 'chevronDown' : 'chevronRight'} />
          </button>
        {:else}
          <span class="spacer" aria-hidden="true"></span>
        {/if}
        <a class="name" href={href({ name: 'deck', params: { id: deck.id } })}>{deck.name}</a>
        <span class="muted counts">
          {i18n.t('deck.tree.counts', { due: due(deck), total: deck.counts.total })}
        </span>
        {#if due(deck) > 0 && deck.contentState === 'ready'}
          <button type="button" class="btn" onclick={() => onstudy(deck)} aria-label={i18n.t('deck.studyDeck', { name: deck.name })}>
            {i18n.t('deck.study')}
          </button>
        {/if}
      </div>
      {#if open && hasChildren(deck.id)}
        <Self {decks} parentId={deck.id} {collapsed} {ontoggle} {onstudy} level={level + 1} />
      {/if}
    </li>
  {/each}
</ul>

<style>
  .tree {
    display: grid;
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .nested {
    margin-top: var(--space-2);
    padding-inline-start: var(--space-5);
    border-inline-start: 1px solid var(--color-border);
  }

  .node {
    gap: var(--space-2);
    padding: var(--space-2);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
    background: var(--color-surface);
  }

  .spacer {
    width: var(--touch-target);
    flex: none;
  }

  /* flex-basis, not min-width: WebKit resolves min(100%, …) to the rem value when sizing. */
  .name {
    flex: 1 1 8rem;
    min-width: 0;
    font-weight: var(--weight-medium);
    overflow-wrap: anywhere;
  }

  .counts {
    font-size: var(--text-sm);
  }
</style>
