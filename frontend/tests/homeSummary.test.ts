import { fireEvent, screen, waitFor, within } from '@testing-library/svelte'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { Rating } from '../src/api/types'
import Shell from '../src/app/Shell.svelte'
import type { DemoScenario } from '../src/demo/scenarios'
import type { MockAppApi } from '../src/demo/mockAppApi'
import { createTestServices, renderWith } from './helpers/services'

type Services = Awaited<ReturnType<typeof createTestServices>>

async function render(hash: string, scenario?: DemoScenario, prepare?: (services: Services) => Promise<string | void>) {
  window.history.replaceState(null, '', '#/home')
  const services = await createTestServices({ language: 'en', scenario })
  const target = (await prepare?.(services)) ?? hash
  window.history.replaceState(null, '', target)
  const stop = services.router.start()
  renderWith(Shell, services)
  return { services, stop }
}

async function rate(api: MockAppApi, sessionId: string, ratings: Rating[]) {
  for (const [index, rating] of ratings.entries()) {
    const session = await api.resumeStudy(sessionId)
    const card = session.current
    if (!card) throw new Error('expected a current card')
    const preview = await api.getRatingPreview({ sessionId, cardId: card.cardId, cardVersion: card.version })
    await api.rateCard({ operationId: `r-${index}`, sessionId, cardId: card.cardId, cardVersion: card.version, previewToken: preview.previewToken, rating, answerMs: 6_000 })
  }
}

const heading = () => document.querySelector('[data-screen-heading]')

beforeEach(() => {
  document.body.innerHTML = ''
})

describe('dashboard', () => {
  it('shows today, the deck list and a prototype notice', async () => {
    const { stop } = await render('#/home')
    expect(await screen.findByRole('button', { name: 'Study everything' })).toBeInTheDocument()
    expect(screen.getByText(/Prototype — ratings use fixture intervals/)).toBeInTheDocument()
    expect(screen.getByRole('heading', { name: 'Today' })).toBeInTheDocument()
    expect(screen.getByRole('meter', { name: 'Daily goal progress' })).toBeInTheDocument()
    expect(screen.getAllByRole('article').length).toBe(4)
    stop()
  })

  it('"no cards" offers adding or importing cards', async () => {
    const { stop } = await render('#/home', 'noCards')
    expect(await screen.findByText('Your decks have no cards yet')).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Import cards' })).toHaveAttribute('href', '#/settings/transfer')
    expect(screen.getByRole('link', { name: 'Add cards to Niemiecki — słówka A1' })).toHaveAttribute('href', '#/editor?deckId=1')
    expect(screen.queryByRole('button', { name: 'Study everything' })).not.toBeInTheDocument()
    stop()
  })

  it('"nothing due today" is distinct and says when the next review is', async () => {
    const { stop } = await render('#/home', 'caughtUp')
    expect(await screen.findByText('All caught up for today')).toBeInTheDocument()
    expect(screen.getByText(/Next review:/)).toBeInTheDocument()
    expect(within(screen.getByRole('main')).getByRole('link', { name: 'Browse' })).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Study everything' })).not.toBeInTheDocument()
    expect(screen.getAllByText('Nothing due today').length).toBeGreaterThan(0)
    stop()
  })

  it('a deck still being prepared says so and offers no study action', async () => {
    const { stop } = await render('#/home', 'preparing')
    const deck = await screen.findByRole('article', { name: /English — words A1/ })
    expect(within(deck).getByText('Preparing content…')).toBeInTheDocument()
    expect(within(deck).queryByRole('button')).not.toBeInTheDocument()
    stop()
  })

  it('refresh re-reads local data without leaving the screen', async () => {
    const { services, stop } = await render('#/home')
    await screen.findByRole('button', { name: 'Study everything' })
    const spy = vi.spyOn(services.api, 'getDashboard')
    await fireEvent.click(screen.getByRole('button', { name: 'Refresh' }))
    await waitFor(() => expect(spy).toHaveBeenCalledTimes(1))
    expect(heading()).toHaveTextContent('Your decks')
    stop()
  })

  it('offers to resume an unfinished session with its progress', async () => {
    const { services, stop } = await render('#/home', undefined, async ({ api, mock }) => {
      const session = await api.startStudy({ operationId: 's', deckId: '3' })
      await rate(mock, session.sessionId, ['Good', 'Good'])
    })
    const resume = await screen.findByRole('region', { name: 'Unfinished session' })
    expect(within(resume).getByText('English — words A1')).toBeInTheDocument()
    expect(within(resume).getByText(/2 reviews/)).toBeInTheDocument()
    await fireEvent.click(within(resume).getByRole('button', { name: 'Resume session' }))
    await screen.findByRole('button', { name: 'Show answer' })
    expect(services.router.current.name).toBe('study')
    expect(screen.getByText('Session resumed — 2 cards already rated.')).toBeInTheDocument()
    stop()
  })

  it('creates a deck with validation and a single operation', async () => {
    const { services, stop } = await render('#/home')
    const user = userEvent.setup()
    const create = vi.spyOn(services.api, 'createDeck')
    await user.click(await screen.findByRole('button', { name: 'New deck' }))
    const dialog = await screen.findByRole('dialog', { name: 'New deck' })
    await user.click(within(dialog).getByRole('button', { name: 'Create deck' }))
    expect(within(dialog).getByText('Enter a deck name.')).toBeInTheDocument()
    expect(within(dialog).getByRole('textbox', { name: 'Deck name' })).toHaveAttribute('aria-invalid', 'true')
    expect(create).not.toHaveBeenCalled()

    await user.type(within(dialog).getByRole('textbox', { name: 'Deck name' }), 'Moje zwroty')
    await user.click(within(dialog).getByRole('radio', { name: /Simple/ }))
    await user.click(within(dialog).getByRole('button', { name: 'Create deck' }))
    expect(await screen.findByText('Deck “Moje zwroty” created.')).toBeInTheDocument()
    expect(create).toHaveBeenCalledTimes(1)
    expect(create.mock.calls[0]?.[0]).toMatchObject({ name: 'Moje zwroty', deckType: 'Simple', targetLanguage: 'de', parentId: null })
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
    stop()
  })
})

describe('session summary', () => {
  const finished = (ratings: Rating[], deckId = '3') => async ({ api, mock }: Services) => {
    const session = await api.startStudy({ operationId: 's', deckId })
    await rate(mock, session.sessionId, ratings)
    await api.updateStudySession({ operationId: 'end', sessionId: session.sessionId, action: 'end' })
    return `#/summary/${session.sessionId}`
  }

  it('shows time, distribution and the share without Again (not called retention)', async () => {
    const { stop } = await render('', undefined, finished(['Again', 'Good', 'Good', 'Easy']))
    expect(await screen.findByText(/4 reviews/)).toBeInTheDocument()
    expect(screen.getByText('Answered without “Again”')).toBeInTheDocument()
    expect(screen.getByText('75%')).toBeInTheDocument()
    expect(screen.getByText('Share of answers in this session — not true retention.')).toBeInTheDocument()
    expect(screen.getByText('24 s')).toBeInTheDocument()
    expect(screen.getByText('2 (50%)')).toBeInTheDocument()
    stop()
  })

  it('lists Again cards and studies exactly them again', async () => {
    const { services, stop } = await render('', undefined, finished(['Again', 'Good']))
    const again = await screen.findByRole('region', { name: 'Cards to repeat' })
    expect(within(again).getAllByRole('listitem')).toHaveLength(1)
    const start = vi.spyOn(services.api, 'startStudy')
    await fireEvent.click(within(again).getByRole('button', { name: 'Study this card again' }))
    await screen.findByRole('button', { name: 'Show answer' })
    expect(start.mock.calls[0]?.[0].cardIds).toHaveLength(1)
    expect(screen.getByText(/1 card left/)).toBeInTheDocument()
    stop()
  })

  it('has a separate no-Again variant and next steps', async () => {
    const { stop } = await render('', undefined, finished(['Good']))
    expect(await screen.findByText('No forgotten cards')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /^Continue studying/ })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /^Next deck:/ })).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Go to decks' })).toHaveAttribute('href', '#/home')
    stop()
  })

  it('offers a daily reminder after the first completed session only', async () => {
    const { services, stop } = await render('', undefined, finished(['Good']))
    const user = userEvent.setup()
    const offer = await screen.findByRole('region', { name: 'Keep the habit' })
    await user.click(within(offer).getByRole('button', { name: 'Set a daily reminder' }))
    const dialog = await screen.findByRole('dialog', { name: 'Daily reminder' })
    await user.click(within(dialog).getByRole('button', { name: 'Save reminder' }))
    expect(await screen.findByText(/Reminder set for 19:00/)).toBeInTheDocument()
    expect((await services.api.getSettings()).reminderTime).toBe('19:00')
    expect(screen.queryByRole('region', { name: 'Keep the habit' })).not.toBeInTheDocument()
    stop()
  })

  it('a still-open session links back to it', async () => {
    const { stop } = await render('', undefined, async ({ api, mock }) => {
      const session = await api.startStudy({ operationId: 's', deckId: '3' })
      await rate(mock, session.sessionId, ['Good'])
      return `#/summary/${session.sessionId}`
    })
    expect(await screen.findByText('This session is still open.')).toBeInTheDocument()
    expect(screen.getAllByRole('link', { name: 'Resume session' }).length).toBeGreaterThan(0)
    expect(screen.queryByRole('region', { name: 'Keep the habit' })).not.toBeInTheDocument()
    stop()
  })
})
