import { screen, waitFor, within } from '@testing-library/svelte'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'
import type { ErrorDto } from '../src/api/types'
import { href } from '../src/app/routes'
import { MockAppApi } from '../src/demo/mockAppApi'
import { levelLabel } from '../src/features/deck/level'
import GrammarScreen from '../src/features/grammar/GrammarScreen.svelte'
import { createTestServices, renderWith } from './helpers/services'

async function rejection(promise: Promise<unknown>): Promise<ErrorDto> {
  try {
    await promise
  } catch (caught) {
    return (caught as { dto: ErrorDto }).dto
  }
  throw new Error('resolved')
}

describe('MockAppApi grammar', () => {
  it('lists decks with tips and groups the first deck by its chapters', async () => {
    const api = new MockAppApi()
    const grammar = await api.getGrammar({ deckId: null })
    expect(grammar.decks.map((deck) => deck.deckId)).toEqual(['1', '5'])
    expect(grammar.decks[0]).toMatchObject({ level: 'A1', sectionCount: 4 })
    const deck = grammar.deck!
    expect(deck.deckId).toBe('1')
    // `heute Abend` (tag `czas`) belongs to no chapter: it closes the deck untitled.
    expect(deck.sections.map((section) => section.tag)).toEqual(['dom', 'jedzenie', 'miasto', 'czasowniki', null])
    const total = deck.sections.reduce((sum, section) => sum + section.entries.length, 0)
    expect(total).toBe(grammar.decks[0]!.tipCount)
  })

  it('puts tips outside every chapter into an untitled section', async () => {
    const grammar = await new MockAppApi().getGrammar({ deckId: '5' })
    expect(grammar.deck!.sections).toHaveLength(1)
    expect(grammar.deck!.sections[0]).toMatchObject({ tag: null, title: null })
  })

  it('answers NOT_FOUND for a deck without tips', async () => {
    expect((await rejection(new MockAppApi().getGrammar({ deckId: '4' }))).code).toBe('NOT_FOUND')
  })
})

describe('GrammarScreen', () => {
  it('shows numbered chapters of the deck and opens the card of a tip', async () => {
    const services = await createTestServices({ language: 'pl' })
    renderWith(GrammarScreen, services, {})
    const user = userEvent.setup()
    expect(await screen.findByRole('heading', { level: 2, name: 'Niemiecki — słówka A1' })).toBeInTheDocument()
    expect(screen.getByText('Poziom A1')).toBeInTheDocument()
    const chapter = screen.getByRole('button', { name: 'Pokaż: 1. Dom i codzienne przedmioty' })
    await user.click(chapter)
    const region = chapter.closest('section')!
    const link = within(region).getAllByRole('link')[0]!
    expect(link.getAttribute('href')).toMatch(/^#\/card\//)
    expect(within(region).getByText('Rodzaj męski: der Kühlschrank, die Kühlschränke.')).toBeInTheDocument()
  })

  it('filters tips and switches decks', async () => {
    const services = await createTestServices({ language: 'pl' })
    renderWith(GrammarScreen, services, {})
    const user = userEvent.setup()
    await screen.findByRole('heading', { level: 2, name: 'Niemiecki — słówka A1' })
    await user.type(screen.getByRole('searchbox', { name: 'Szukaj we wskazówkach' }), 'Banken')
    expect(await screen.findByText('Pasuje 1 wskazówka')).toBeInTheDocument()
    expect(screen.getByText('Liczba mnoga: die Banken.')).toBeVisible()
    expect(screen.queryByRole('button', { name: /Dom i codzienne/ })).not.toBeInTheDocument()

    await user.selectOptions(screen.getByRole('combobox', { name: 'Talia' }), '5')
    expect(await screen.findByRole('heading', { level: 2, name: 'Złożenia (Komposita)' })).toBeInTheDocument()
    expect(screen.getByText(`Poziom ${levelLabel('A1-A2')}`)).toBeInTheDocument()
  })

  it('opens the deck from the link and falls back when it has no tips', async () => {
    const services = await createTestServices({ language: 'pl' })
    renderWith(GrammarScreen, services, { deckId: '4' })
    expect(await screen.findByRole('heading', { level: 2, name: 'Niemiecki — słówka A1' })).toBeInTheDocument()
    expect(href({ name: 'grammar', query: { deck: '5' } })).toBe('#/grammar?deck=5')
  })

  it('says how to get tips when no deck has any', async () => {
    const services = await createTestServices({ language: 'pl', scenario: 'empty' })
    renderWith(GrammarScreen, services, {})
    await waitFor(() => expect(screen.getByRole('heading', { name: 'Brak wskazówek gramatycznych' })).toBeInTheDocument())
  })
})
