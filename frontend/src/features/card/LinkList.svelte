<script lang="ts">
  import type { WordLinkDto } from '../../api/types'
  import { useServices } from '../../app/context'
  import Chip from '../../components/Chip.svelte'
  import Icon from '../../components/Icon.svelte'

  interface Props {
    links: WordLinkDto[]
    busy: boolean
    /** Opens the word picker for this link (choose for ambiguous, change for the rest). */
    onpick: (link: WordLinkDto) => void
    onunlink: (link: WordLinkDto) => void
  }

  let { links, busy, onpick, onunlink }: Props = $props()
  const { i18n } = useServices()
  const ids = $props.id()
  const ordered = $derived([...links].sort((a, b) => a.startIndex - b.startIndex))
  const tone = (link: WordLinkDto) => (link.resolution === 'ambiguous' ? 'warning' : link.resolution === 'unresolved' ? 'neutral' : 'success')
</script>

<section class="stack" aria-labelledby="{ids}-title">
  <h2 id="{ids}-title">{i18n.t('card.links.title')}</h2>
  <p class="muted">{i18n.t('card.links.hint')}</p>
  {#if ordered.length === 0}
    <p class="muted">{i18n.t('card.links.empty')}</p>
  {:else}
    <ul class="list">
      {#each ordered as link (link.id)}
        <li class="item">
          <p class="row word">
            <Icon name="link" />
            <strong dir="auto">{link.surface}</strong>
            {#if link.wordNoteId && link.lemma}<span dir="auto">→ {link.lemma}{link.translation ? ` — ${link.translation}` : ''}</span>{/if}
          </p>
          <div class="row">
            <Chip label={i18n.t(`wordLink.resolution.${link.resolution}`)} tone={tone(link)} />
            <button type="button" class="btn btn-ghost" disabled={busy} onclick={() => onpick(link)}>
              <Icon name="search" />{i18n.t(link.resolution === 'ambiguous' ? 'card.links.choose' : link.wordNoteId ? 'card.links.change' : 'card.links.find')}
            </button>
            <button type="button" class="btn btn-ghost danger" disabled={busy} onclick={() => onunlink(link)}>
              <Icon name="close" />{i18n.t('card.links.unlink')}
            </button>
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  h2 {
    font-size: var(--text-lg);
  }

  .list {
    display: grid;
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .item {
    display: grid;
    gap: var(--space-1);
    padding: var(--space-2) 0;
    border-bottom: 1px solid var(--color-border);
  }

  .word {
    overflow-wrap: anywhere;
  }

  .danger {
    color: var(--color-error);
  }
</style>
