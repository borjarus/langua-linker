<script lang="ts">
  import { errorDto } from '../../api/errors'
  import type { AssociationDto, DeckSummaryDto, GrammarTipDto, Rating, RatingPreviewDto, WordLinkDto } from '../../api/types'
  import { useServices } from '../../app/context'
  import AssociationPanel from '../../components/AssociationPanel.svelte'
  import Chip from '../../components/Chip.svelte'
  import DeckCard from '../../components/DeckCard.svelte'
  import Dialog from '../../components/Dialog.svelte'
  import EmptyState from '../../components/EmptyState.svelte'
  import ErrorState from '../../components/ErrorState.svelte'
  import GrammarTipList from '../../components/GrammarTipList.svelte'
  import LinkedText from '../../components/LinkedText.svelte'
  import ProgressPanel from '../../components/ProgressPanel.svelte'
  import RatingButtonRow from '../../components/RatingButtonRow.svelte'
  import SearchInput from '../../components/SearchInput.svelte'
  import Sheet from '../../components/Sheet.svelte'
  import Skeleton from '../../components/Skeleton.svelte'
  import StatTile from '../../components/StatTile.svelte'
  import VirtualList from '../../components/VirtualList.svelte'
  import Screen from '../shared/Screen.svelte'

  const { i18n, toasts } = useServices()

  const capabilities = { associations: true, grammarTips: true, wordLinks: true, tts: true }
  const deck: DeckSummaryDto = {
    id: 'g1', guid: 'gallery-deck', name: 'Niemiecki — Straßenbahnhaltestelle & zażółć gęślą jaźń', parentId: null,
    targetLanguage: 'de', nativeLanguage: 'pl', deckType: 'Linguistic', capabilities,
    counts: { new: 12, learning: 3, review: 41, overdue: 5, total: 1204 }, progress: 0.37, bundled: true, contentState: 'ready', version: 1,
  }
  const preparingDeck: DeckSummaryDto = { ...deck, id: 'g2', name: 'English 👩‍💻 — שלום', targetLanguage: 'en', deckType: 'Simple', contentState: 'preparing', bundled: false }
  const preview: RatingPreviewDto = {
    cardId: 'c1', cardVersion: 1, previewToken: 'gallery',
    intervals: {
      Again: { intervalSeconds: 60, dueAt: '2026-03-02T08:01:00.000Z' },
      Hard: { intervalSeconds: 360, dueAt: '2026-03-02T08:06:00.000Z' },
      Good: { intervalSeconds: 86_400, dueAt: '2026-03-03T08:00:00.000Z' },
      Easy: { intervalSeconds: 4 * 86_400, dueAt: '2026-03-06T08:00:00.000Z' },
    },
  }
  const association = (id: string, status: AssociationDto['status'], provenance: AssociationDto['provenance'], favorite = false): AssociationDto => ({
    id, noteId: 'n1', text: '„Kul się, szrank!” — chłód z lodówki każe się skulić ❄️', provenance, favorite, status, model: null, error: null,
  })
  const associations = [
    association('a1', 'ready', 'bundled', true),
    association('a2', 'ready', 'generated'),
    association('a3', 'pending', 'generated'),
    association('a4', 'offlineQueued', 'generated'),
    association('a5', 'blocked', 'generated'),
  ]
  const tips: GrammarTipDto[] = [
    { id: 't1', noteId: 'n1', text: 'Rodzaj męski: der Kühlschrank, die Kühlschränke.', order: 0 },
    { id: 't2', noteId: 'n1', text: 'Złożenie: kühl + der Schrank.', order: 1 },
  ]
  const sentence = 'Ich trinke heute Abend Tee 🍵 mit der Bank.'
  const link = (surface: string, resolution: WordLinkDto['resolution']): WordLinkDto => {
    const startIndex = sentence.indexOf(surface)
    return { id: surface, sentenceNoteId: 's1', surface, startIndex, endIndex: startIndex + surface.length, lemma: surface, resolution, wordNoteId: null, translation: null, candidates: [] }
  }
  const links = [link('trinke', 'resolved'), link('heute Abend', 'userOverridden'), link('Tee', 'unresolved'), link('Bank', 'ambiguous')]

  let dialogOpen = $state(false)
  let sheetOpen = $state(false)
  let chipPressed = $state(false)
  let search = $state('')
  let lastRating = $state<Rating | null>(null)
</script>

<Screen title={i18n.t('routes.gallery')} back>
  <p class="muted">{i18n.t('gallery.intro')}</p>

  <section class="stack" aria-labelledby="g-buttons">
    <h2 id="g-buttons">{i18n.t('gallery.buttons')}</h2>
    <div class="row">
      <button type="button" class="btn btn-primary">{i18n.t('gallery.primary')}</button>
      <button type="button" class="btn">{i18n.t('gallery.secondary')}</button>
      <button type="button" class="btn btn-danger">{i18n.t('gallery.danger')}</button>
      <button type="button" class="btn" disabled>{i18n.t('common.save')}</button>
    </div>
  </section>

  <section class="stack" aria-labelledby="g-chips">
    <h2 id="g-chips">{i18n.t('gallery.chips')}</h2>
    <div class="row">
      <Chip label={i18n.t('deck.targetLanguages.de')} tone="primary" />
      <Chip label={i18n.t('deck.counts.overdue')} tone="error" icon="alert" />
      <Chip label={i18n.t('association.provenance.generated')} tone="info" icon="sparkles" />
      <Chip label={i18n.t('gallery.chipFilter')} pressed={chipPressed} ontoggle={(next) => (chipPressed = next)} />
    </div>
    <dl class="tiles">
      <StatTile label={i18n.t('dashboard.dueToday')} value={i18n.formatNumber(56)} tone="info" />
      <StatTile label={i18n.t('stats.retentionMature')} value={i18n.formatPercent(0.907)} hint={i18n.t('stats.reviewsCount', { count: 97 })} tone="success" />
      <StatTile label={i18n.t('stats.retentionYoung')} value={i18n.t('stats.notEnoughData')} />
    </dl>
  </section>

  <section class="stack" aria-labelledby="g-decks">
    <h2 id="g-decks">{i18n.t('routes.home')}</h2>
    <div class="decks">
      <DeckCard {deck} onstudy={() => toasts.show(i18n.t('gallery.toastMessage'), 'success')} />
      <DeckCard deck={preparingDeck} onstudy={() => {}} />
    </div>
  </section>

  <section class="stack" aria-labelledby="g-content">
    <h2 id="g-content">{i18n.t('gallery.content')}</h2>
    <p class="sentence"><LinkedText text={sentence} {links} lang="de" onselect={(selected) => toasts.show(selected.surface)} /></p>
    <RatingButtonRow {preview} selected={null} onrate={(rating) => (lastRating = rating)} />
    {#if lastRating}<p class="muted" role="status">{i18n.t(`study.ratings.${lastRating}`)}</p>{/if}
    <RatingButtonRow preview={null} disabled onrate={() => {}} />
    <AssociationPanel {associations} expanded ongenerate={() => {}} onfavorite={() => {}} />
    <GrammarTipList {tips} />
  </section>

  <section class="stack" aria-labelledby="g-states">
    <h2 id="g-states">{i18n.t('gallery.states')}</h2>
    <Skeleton lines={3} />
    <EmptyState title={i18n.t('dashboard.emptyTitle')} description={i18n.t('dashboard.emptyBody')}>
      {#snippet actions()}<button type="button" class="btn btn-primary">{i18n.t('dashboard.emptyAction')}</button>{/snippet}
    </EmptyState>
    <ErrorState error={errorDto('DB_BUSY')} onretry={() => {}} />
    <ErrorState error={errorDto('NETWORK_UNAVAILABLE')} compact onretry={() => {}} />
    <ErrorState error={errorDto('BACKUP_CORRUPT')} />
    <SearchInput value={search} label={i18n.t('browser.search')} placeholder={i18n.t('browser.searchPlaceholder')} onsearch={(value) => (search = value)} />
    <SearchInput value='is:"' label={i18n.t('browser.search')} error={i18n.t('errors.VALIDATION_FAILED')} onsearch={() => {}} />
  </section>

  <section class="stack" aria-labelledby="g-progress">
    <h2 id="g-progress">{i18n.t('gallery.progress')}</h2>
    <ProgressPanel title={i18n.t('routes.settingsTransfer')} status="running" progress={{ done: 37, total: 120 }} oncancel={() => {}} />
    <ProgressPanel title={i18n.t('backup.create')} status="succeeded" progress={{ done: 5, total: 5 }} />
    <ProgressPanel title={i18n.t('deck.preparing')} status="queued" progress={null} />
  </section>

  <section class="stack" aria-labelledby="g-feedback">
    <h2 id="g-feedback">{i18n.t('gallery.feedback')}</h2>
    <div class="row">
      <button type="button" class="btn" onclick={() => (dialogOpen = true)}>{i18n.t('gallery.openDialog')}</button>
      <button type="button" class="btn" onclick={() => (sheetOpen = true)}>{i18n.t('gallery.openSheet')}</button>
      <button type="button" class="btn" onclick={() => toasts.show(i18n.t('gallery.toastMessage'), 'success')}>{i18n.t('gallery.showToast')}</button>
      <button type="button" class="btn" onclick={() => toasts.show(i18n.t('errors.DB_BUSY'), 'error')}>{i18n.t('errors.title')}</button>
    </div>
  </section>

  <section class="stack" aria-labelledby="g-virtual">
    <h2 id="g-virtual">{i18n.t('gallery.virtualList')}</h2>
    <VirtualList itemCount={50_000} label={i18n.t('gallery.virtualList')}>
      {#snippet row(index)}
        <span class="virtual-row">{i18n.t('gallery.virtualRow', { index: i18n.formatNumber(index + 1) })}</span>
      {/snippet}
    </VirtualList>
  </section>
</Screen>

<Dialog open={dialogOpen} title={i18n.t('gallery.dialogTitle')} onclose={() => (dialogOpen = false)}>
  <p>{i18n.t('gallery.dialogBody')}</p>
  {#snippet actions()}
    <button type="button" class="btn" onclick={() => (dialogOpen = false)}>{i18n.t('common.cancel')}</button>
    <button type="button" class="btn btn-primary" onclick={() => (dialogOpen = false)}>{i18n.t('common.confirm')}</button>
  {/snippet}
</Dialog>

<Sheet open={sheetOpen} title={i18n.t('gallery.sheetTitle')} onclose={() => (sheetOpen = false)}>
  <p>{i18n.t('gallery.sheetBody')}</p>
</Sheet>

<style>
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 10rem), 1fr));
    gap: var(--space-3);
    margin: 0;
  }

  .decks {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(min(100%, 20rem), 1fr));
    gap: var(--space-4);
  }

  .sentence {
    font-size: var(--text-xl);
  }

  .virtual-row {
    padding: 0 var(--space-4);
  }
</style>
