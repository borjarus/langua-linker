<script lang="ts" module>
  /**
   * What the screen showed when it was left: Back from a card restores the scroll position, so
   * the deck, the search and the open chapters have to come back too.
   */
  const memory = $state<{ deckId: string | null; search: string; open: Record<string, boolean> }>({ deckId: null, search: '', open: {} })

  /** Starts the next visit from the first deck with everything collapsed (tests). */
  export function forgetGrammarScreen() {
    memory.deckId = null
    memory.search = ''
    memory.open = {}
  }
</script>

<script lang="ts">
  import type { GrammarDto, GrammarEntryDto, GrammarSectionDto } from '../../api/types'
  import { useServices } from '../../app/context'
  import { href } from '../../app/routes'
  import Chip from '../../components/Chip.svelte'
  import Disclosure from '../../components/Disclosure.svelte'
  import EmptyState from '../../components/EmptyState.svelte'
  import ErrorState from '../../components/ErrorState.svelte'
  import RichText from '../../components/RichText.svelte'
  import SearchInput from '../../components/SearchInput.svelte'
  import Skeleton from '../../components/Skeleton.svelte'
  import { stripInlineMarkdown } from '../../services/markdown'
  import { useQuery } from '../../services/query.svelte'
  import { levelLabel } from '../deck/level'
  import Screen from '../shared/Screen.svelte'

  /** Deck to open first (`/grammar?deck=…`); `null` opens the first deck with tips. */
  let { deckId: initialDeckId = null }: { deckId?: string | null } = $props()
  const { api, i18n, queries } = useServices()
  const ids = $props.id()
  // Initial value only: the picker switches decks in place.
  // svelte-ignore state_referenced_locally
  let deckId = $state<string | null>(initialDeckId ?? memory.deckId)
  // A link to a deck starts a fresh search; otherwise the last one comes back.
  // svelte-ignore state_referenced_locally
  let search = $state(initialDeckId === null ? memory.search : '')
  $effect(() => {
    memory.deckId = deckId
    memory.search = search
  })

  const grammar = useQuery(queries, () => ({
    key: ['grammar', deckId],
    fetch: () => api.getGrammar({ deckId }),
    isEmpty: (data: GrammarDto) => data.decks.length === 0,
  }))

  // A deck from the link that was deleted or lost its tips: open the first deck instead.
  $effect(() => {
    if (grammar.state.status === 'error' && grammar.state.error.code === 'NOT_FOUND' && deckId !== null) deckId = null
  })

  // Switching decks keeps the previous chapters on screen (dimmed) instead of flashing a skeleton.
  let previous = $state<GrammarDto | null>(null)
  $effect(() => {
    if (grammar.state.status === 'ready' || grammar.state.status === 'empty') previous = grammar.state.data
  })
  const shown = $derived(
    grammar.state.status === 'ready' || grammar.state.status === 'empty' ? grammar.state.data : grammar.state.status === 'loading' ? previous : null,
  )
  const refetching = $derived(grammar.state.status === 'loading' && previous !== null)

  const normalize = (text: string) => stripInlineMarkdown(text).normalize('NFC').toLocaleLowerCase()
  const needle = $derived(normalize(search.trim()))
  const matches = (entry: GrammarEntryDto) =>
    needle === '' || [i18n.content(entry.text, entry.translations), entry.text, entry.front, entry.back].some((text) => normalize(text).includes(needle))

  /** Chapters with the entries the search keeps; numbered by their place in the whole deck. */
  const sections = $derived.by(() => {
    const deck = shown?.deck
    if (!deck) return []
    return deck.sections
      .map((section, index) => ({ section, number: index + 1, entries: section.entries.filter(matches) }))
      .filter((item) => item.entries.length > 0)
  })
  const found = $derived(sections.reduce((sum, item) => sum + item.entries.length, 0))
  const summary = $derived(shown?.deck ? shown.decks.find((item) => item.deckId === shown.deck?.deckId) ?? null : null)

  function sectionTitle(section: GrammarSectionDto, number: number): string {
    return section.title === null
      ? i18n.t('grammarScreen.otherSection')
      : i18n.t('grammarScreen.sectionTitle', { number, title: i18n.content(section.title, section.titleTranslations) })
  }
</script>

<Screen title={i18n.t('routes.grammar')}>
  {#if grammar.state.status === 'error' && !shown}
    <ErrorState error={grammar.state.error} onretry={() => void grammar.refresh()} />
  {:else if !shown}
    <Skeleton lines={8} />
  {:else if shown.decks.length === 0 || !shown.deck}
    <EmptyState title={i18n.t('grammarScreen.emptyTitle')} description={i18n.t('grammarScreen.emptyBody')} icon="book">
      {#snippet actions()}
        <a class="btn btn-primary" href={href({ name: 'home' })}>{i18n.t('common.goHome')}</a>
      {/snippet}
    </EmptyState>
  {:else}
    {@const deck = shown.deck}
    <div class="stack content" class:refetching aria-busy={refetching}>
      {#if grammar.refreshError}<ErrorState error={grammar.refreshError} compact onretry={() => void grammar.refresh()} />{/if}
      {#if grammar.state.status === 'error'}<ErrorState error={grammar.state.error} compact onretry={() => void grammar.refresh()} />{/if}

      <div class="filters" role="group" aria-label={i18n.t('grammarScreen.filters')}>
        {#if shown.decks.length > 1}
          <div class="field">
            <label for="{ids}-deck">{i18n.t('grammarScreen.deck')}</label>
            <select id="{ids}-deck" class="input" value={deck.deckId} onchange={(event) => (deckId = event.currentTarget.value)}>
              {#each shown.decks as option (option.deckId)}
                <option value={option.deckId}>
                  {option.level ? `${option.name} · ${levelLabel(option.level)}` : option.name}
                </option>
              {/each}
            </select>
          </div>
        {/if}
        <div class="field search">
          <SearchInput value={search} label={i18n.t('grammarScreen.search')} placeholder={i18n.t('grammarScreen.searchPlaceholder')} onsearch={(value) => (search = value)} />
        </div>
      </div>

      <header class="deck-header">
        <h2>{deck.name}</h2>
        <div class="row chips">
          <Chip label={i18n.t(`deck.targetLanguages.${deck.targetLanguage}`)} tone="primary" />
          {#if deck.level}<Chip label={i18n.t('deck.level', { level: levelLabel(deck.level) })} tone="info" />{/if}
          {#if summary}
            <Chip label={i18n.t('grammarScreen.tips', { count: summary.tipCount })} />
            {#if summary.sectionCount > 0}<Chip label={i18n.t('grammarScreen.sections', { count: summary.sectionCount })} />{/if}
          {/if}
        </div>
      </header>

      {#if needle !== ''}
        <p class="muted" role="status">{i18n.t('grammarScreen.found', { count: found })}</p>
      {/if}

      {#if sections.length === 0}
        <p class="muted">{i18n.t('grammarScreen.noMatches')}</p>
      {:else}
        <div class="sections">
          {#each sections as item (`${deck.deckId}:${item.section.tag ?? '-'}`)}
            <Disclosure
              title={sectionTitle(item.section, item.number)}
              count={item.entries.length}
              expanded={needle !== ''}
              bind:open={memory.open[`${deck.deckId}:${item.section.tag ?? '-'}`]}
            >
              <ol class="entries">
                {#each item.entries as entry (entry.tipId)}
                  <li class="entry">
                    <p class="tip">{i18n.content(entry.text, entry.translations)}</p>
                    <a class="example" href={href({ name: 'card', params: { id: entry.cardId } })} aria-label={i18n.t('grammarScreen.openCard', { sentence: stripInlineMarkdown(entry.front) })}>
                      <span class="front"><RichText text={entry.front} lang={deck.targetLanguage} /></span>
                      <span class="back muted"><RichText text={entry.back} lang="pl" /></span>
                    </a>
                  </li>
                {/each}
              </ol>
            </Disclosure>
          {/each}
        </div>
      {/if}
    </div>
  {/if}
</Screen>

<style>
  .content.refetching {
    opacity: 0.6;
    transition: opacity var(--duration-fast, 150ms);
  }

  .filters {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-3);
    align-items: end;
  }

  .field {
    display: grid;
    gap: var(--space-1);
    min-width: min(100%, 16rem);
    flex: 1 1 16rem;
  }

  .deck-header {
    display: grid;
    gap: var(--space-2);
  }

  .deck-header h2 {
    margin: 0;
  }

  .chips {
    flex-wrap: wrap;
  }

  .sections {
    display: grid;
    gap: var(--space-2);
  }

  .entries {
    display: grid;
    gap: var(--space-3);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .entry {
    display: grid;
    gap: var(--space-2);
    padding-block-end: var(--space-3);
    border-block-end: 1px solid var(--color-border);
  }

  .entry:last-child {
    padding-block-end: 0;
    border-block-end: 0;
  }

  .tip {
    margin: 0;
    overflow-wrap: anywhere;
  }

  .example {
    display: grid;
    gap: var(--space-1);
    min-height: var(--touch-target);
    padding: var(--space-2) var(--space-3);
    border-inline-start: 3px solid var(--color-primary);
    border-radius: var(--radius-sm, 4px);
    background: var(--color-surface-sunken);
    color: inherit;
    text-decoration: none;
    overflow-wrap: anywhere;
  }

  .example:hover .front,
  .example:focus-visible .front {
    text-decoration: underline;
  }

  .back {
    font-size: var(--text-sm);
  }
</style>
