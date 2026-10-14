import { fireEvent, screen, waitFor, within } from '@testing-library/svelte'
import userEvent from '@testing-library/user-event'
import { flushSync } from 'svelte'
import { describe, expect, it, vi } from 'vitest'
import { errorDto } from '../src/api/errors'
import type { DeckSummaryDto, RatingPreviewDto, WordLinkDto } from '../src/api/types'
import Chip from '../src/components/Chip.svelte'
import DeckCard from '../src/components/DeckCard.svelte'
import ErrorState from '../src/components/ErrorState.svelte'
import { segmentText } from '../src/components/LinkedText.svelte'
import RatingButtonRow from '../src/components/RatingButtonRow.svelte'
import DialogHarness from './fixtures/DialogHarness.svelte'
import VirtualListHarness from './fixtures/VirtualListHarness.svelte'
import { createTestServices, renderWith } from './helpers/services'

const preview: RatingPreviewDto = {
  cardId: '1',
  cardVersion: 1,
  previewToken: 't',
  intervals: {
    Again: { intervalSeconds: 60, dueAt: '2026-01-01T00:01:00.000Z' },
    Hard: { intervalSeconds: 600, dueAt: '2026-01-01T00:10:00.000Z' },
    Good: { intervalSeconds: 86_400, dueAt: '2026-01-02T00:00:00.000Z' },
    Easy: { intervalSeconds: 4 * 86_400, dueAt: '2026-01-05T00:00:00.000Z' },
  },
}

describe('RatingButtonRow', () => {
  it('labels buttons with rating and interval and reports the choice', async () => {
    const services = await createTestServices({ language: 'en' })
    const onrate = vi.fn()
    renderWith(RatingButtonRow, services, { preview, onrate })
    const group = screen.getByRole('group', { name: 'How well did you remember?' })
    const good = within(group).getByRole('button', { name: 'Good — next review in 1 day' })
    await fireEvent.click(good)
    expect(onrate).toHaveBeenCalledWith('Good')
    expect(within(group).getAllByRole('button')).toHaveLength(4)
  })

  it('is disabled while saving', async () => {
    const services = await createTestServices({ language: 'pl' })
    const onrate = vi.fn()
    renderWith(RatingButtonRow, services, { preview: null, disabled: true, onrate })
    const again = screen.getByRole('button', { name: 'Jeszcze raz' })
    expect(again).toBeDisabled()
    await userEvent.click(again)
    expect(onrate).not.toHaveBeenCalled()
  })
})

describe('Dialog', () => {
  it('moves focus inside, traps Tab, closes on Escape and restores focus', async () => {
    const services = await createTestServices({ language: 'en' })
    const user = userEvent.setup()
    renderWith(DialogHarness, services)
    const opener = screen.getByRole('button', { name: 'open' })
    await user.click(opener)
    const dialog = screen.getByRole('dialog', { name: 'Example dialog' })
    expect(dialog).toHaveAttribute('aria-modal', 'true')
    expect(dialog.contains(document.activeElement)).toBe(true)

    for (let step = 0; step < 5; step += 1) {
      await user.tab()
      expect(dialog.contains(document.activeElement)).toBe(true)
    }
    await user.keyboard('{Escape}')
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
    expect(document.activeElement).toBe(opener)
  })

  it('closes on history Back instead of leaving the screen', async () => {
    window.history.replaceState(null, '', '#/home')
    const services = await createTestServices({ language: 'en' })
    const stop = services.router.start()
    await services.router.navigate({ name: 'gallery' })
    const user = userEvent.setup()
    renderWith(DialogHarness, services)
    await user.click(screen.getByRole('button', { name: 'open' }))
    expect(screen.getByRole('dialog')).toBeInTheDocument()

    window.history.back()
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    await waitFor(() => expect(window.location.hash).toBe('#/gallery'))
    expect(services.router.current.name).toBe('gallery')
    stop()
  })
})

describe('ErrorState', () => {
  it('offers retry only for retryable errors and translates messageKey', async () => {
    const services = await createTestServices({ language: 'en' })
    const onretry = vi.fn()
    const { unmount } = renderWith(ErrorState, services, { error: errorDto('DB_BUSY'), onretry })
    expect(screen.getByRole('alert')).toHaveTextContent('The collection is busy')
    await fireEvent.click(screen.getByRole('button', { name: 'Try again' }))
    expect(onretry).toHaveBeenCalledOnce()
    unmount()

    renderWith(ErrorState, services, { error: errorDto('BACKUP_CORRUPT'), onretry })
    expect(screen.queryByRole('button', { name: 'Try again' })).not.toBeInTheDocument()
  })
})

describe('Chip', () => {
  it('acts as a toggle button with aria-pressed when interactive', async () => {
    const services = await createTestServices()
    const ontoggle = vi.fn()
    renderWith(Chip, services, { label: 'Due only', pressed: false, ontoggle })
    const chip = screen.getByRole('button', { name: 'Due only' })
    expect(chip).toHaveAttribute('aria-pressed', 'false')
    await fireEvent.click(chip)
    expect(ontoggle).toHaveBeenCalledWith(true)
  })
})

describe('DeckCard', () => {
  const deck: DeckSummaryDto = {
    id: '1', guid: 'g', name: 'Niemiecki — słówka', parentId: null, targetLanguage: 'de', nativeLanguage: 'pl', deckType: 'Linguistic',
    capabilities: { associations: true, grammarTips: true, wordLinks: true, tts: true },
    counts: { new: 2, learning: 0, review: 3, overdue: 1, total: 22 }, progress: 0.5, bundled: true, contentState: 'ready', version: 1,
  }

  it('shows counts with text labels and a study action', async () => {
    const services = await createTestServices({ language: 'pl' })
    const onstudy = vi.fn()
    renderWith(DeckCard, services, { deck, onstudy })
    expect(screen.getByRole('link', { name: 'Otwórz Niemiecki — słówka' })).toHaveAttribute('href', '#/deck/1')
    expect(screen.getByText('22 karty')).toBeInTheDocument()
    expect(screen.getByText('Zaległe')).toBeInTheDocument()
    await fireEvent.click(screen.getByRole('button', { name: 'Ucz się: Niemiecki — słówka' }))
    expect(onstudy).toHaveBeenCalledWith(deck)
  })

  it('distinguishes "nothing due" from "preparing content"', async () => {
    const services = await createTestServices({ language: 'en' })
    const { unmount } = renderWith(DeckCard, services, { deck: { ...deck, counts: { ...deck.counts, new: 0, review: 0, overdue: 0 } }, onstudy: vi.fn() })
    expect(screen.getByText('Nothing due today')).toBeInTheDocument()
    unmount()
    renderWith(DeckCard, services, { deck: { ...deck, contentState: 'preparing' }, onstudy: vi.fn() })
    expect(screen.getByText('Preparing content…')).toBeInTheDocument()
    expect(screen.queryByRole('button')).not.toBeInTheDocument()
  })
})

describe('VirtualList', () => {
  it('keeps the DOM bounded and exposes list semantics', async () => {
    const services = await createTestServices()
    const onrange = vi.fn()
    renderWith(VirtualListHarness, services, { count: 50_000, onrange })
    flushSync()
    const list = screen.getByRole('list', { name: 'rows' })
    const items = within(list).getAllByRole('listitem')
    expect(items.length).toBeLessThan(60)
    expect(items[0]).toHaveAttribute('aria-setsize', '50000')
    expect(items[0]).toHaveAttribute('aria-posinset', '1')
    expect(onrange).toHaveBeenCalled()

    list.scrollTop = 56 * 1000
    await fireEvent.scroll(list)
    flushSync()
    const shifted = within(list).getAllByRole('listitem')
    expect(Number(shifted[0]?.getAttribute('aria-posinset'))).toBeGreaterThan(900)
    expect(shifted.length).toBeLessThan(60)
  })
})

describe('segmentText (UTF-16 half-open ranges)', () => {
  const link = (text: string, surface: string, offset = 0): WordLinkDto => {
    const startIndex = text.indexOf(surface, offset)
    return { id: surface, sentenceNoteId: 's', surface, startIndex, endIndex: startIndex + surface.length, lemma: null, resolution: 'resolved', wordNoteId: null, translation: null, candidates: [] }
  }

  it('splits around multi-word units, emoji and diacritics', () => {
    const text = 'Ich 👩‍💻 trinke heute Abend Tee — zażółć'
    const segments = segmentText(text, [link(text, 'heute Abend'), link(text, 'zażółć'), link(text, 'trinke')])
    expect(segments.map((segment) => segment.text).join('')).toBe(text)
    expect(segments.filter((segment) => segment.kind === 'link').map((segment) => segment.text)).toEqual(['trinke', 'heute Abend', 'zażółć'])
  })

  it('ignores ranges that do not match the text or overlap', () => {
    const text = 'Die Bank ist geschlossen.'
    const wrong = { ...link(text, 'Bank'), startIndex: 0, endIndex: 4 }
    const overlapping = { ...link(text, 'ist'), startIndex: 6 }
    const segments = segmentText(text, [wrong, link(text, 'Bank'), overlapping])
    expect(segments.filter((segment) => segment.kind === 'link')).toHaveLength(1)
    expect(segments.map((segment) => segment.text).join('')).toBe(text)
  })

  it('normalizes decomposed input to NFC before slicing', () => {
    const decomposed = 'Kühlschrank'
    const composed = decomposed.normalize('NFC')
    const segments = segmentText(decomposed, [{ ...link(composed, composed), startIndex: 0, endIndex: composed.length }])
    expect(segments).toEqual([expect.objectContaining({ kind: 'link', text: composed })])
  })
})
