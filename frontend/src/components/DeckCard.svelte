<script lang="ts">
  import type { DeckSummaryDto } from '../api/types'
  import { useServices } from '../app/context'
  import { href } from '../app/routes'
  import Chip from './Chip.svelte'
  import Icon from './Icon.svelte'

  interface Props {
    deck: DeckSummaryDto
    onstudy: (deck: DeckSummaryDto) => void
  }

  let { deck, onstudy }: Props = $props()
  const { i18n } = useServices()
  const headingId = $props.id()
  const due = $derived(deck.counts.new + deck.counts.learning + deck.counts.review)
  const preparing = $derived(deck.contentState === 'preparing')
</script>

<article class="deck surface" aria-labelledby={headingId}>
  <header class="header">
    <h3 id={headingId}>
      <a href={href({ name: 'deck', params: { id: deck.id } })} aria-label={i18n.t('deck.open', { name: deck.name })}>{deck.name}</a>
    </h3>
    <div class="row">
      <Chip label={i18n.t(`deck.targetLanguages.${deck.targetLanguage}`)} tone="primary" />
      <Chip label={i18n.t(`deck.types.${deck.deckType}`)} />
      {#if deck.bundled}<Chip label={i18n.t('deck.bundled')} icon="check" />{/if}
    </div>
  </header>

  {#if preparing}
    <p class="muted row"><Icon name="clock" />{i18n.t('deck.preparing')}</p>
  {:else}
    <dl class="counts">
      <div class="count new"><dt>{i18n.t('deck.counts.new')}</dt><dd>{i18n.formatNumber(deck.counts.new)}</dd></div>
      <div class="count learning"><dt>{i18n.t('deck.counts.learning')}</dt><dd>{i18n.formatNumber(deck.counts.learning)}</dd></div>
      <div class="count review"><dt>{i18n.t('deck.counts.review')}</dt><dd>{i18n.formatNumber(deck.counts.review)}</dd></div>
      {#if deck.counts.overdue > 0}
        <div class="count overdue"><dt>{i18n.t('deck.counts.overdue')}</dt><dd>{i18n.formatNumber(deck.counts.overdue)}</dd></div>
      {/if}
    </dl>
    <div class="progress">
      <span class="muted">{i18n.t('deck.progress')}: {i18n.formatPercent(deck.progress)}</span>
      <meter min="0" max="1" value={deck.progress} aria-label={i18n.t('deck.progress')}></meter>
    </div>
  {/if}

  <footer class="row footer">
    <span class="muted">{i18n.t('deck.cards', { count: deck.counts.total })}</span>
    {#if due > 0 && !preparing}
      <button type="button" class="btn btn-primary" aria-label={i18n.t('deck.studyDeck', { name: deck.name })} onclick={() => onstudy(deck)}>
        {i18n.t('deck.study')}
      </button>
    {:else if !preparing}
      <span class="muted">{i18n.t('deck.noDue')}</span>
    {/if}
  </footer>
</article>

<style>
  .deck {
    display: grid;
    gap: var(--space-3);
  }

  .header {
    display: grid;
    gap: var(--space-2);
  }

  h3 a {
    color: inherit;
    text-decoration: none;
  }

  h3 a:hover {
    text-decoration: underline;
  }

  .counts {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2) var(--space-4);
    margin: 0;
  }

  .count {
    display: grid;
    padding-inline-start: var(--space-2);
    border-inline-start: 4px solid var(--color-border);
  }

  .count dt {
    color: var(--color-text-muted);
    font-size: var(--text-xs);
  }

  .count dd {
    margin: 0;
    font-weight: var(--weight-bold);
    font-variant-numeric: tabular-nums;
  }

  .new {
    border-color: var(--color-state-new);
  }

  .learning {
    border-color: var(--color-state-learning);
  }

  .review {
    border-color: var(--color-state-review);
  }

  .overdue {
    border-color: var(--color-state-relearning);
  }

  .progress {
    display: grid;
    gap: var(--space-1);
    font-size: var(--text-sm);
  }

  meter {
    width: 100%;
  }

  .footer {
    justify-content: space-between;
  }
</style>
