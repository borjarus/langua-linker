import { fireEvent, screen, waitFor, within } from '@testing-library/svelte'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { Id } from '../src/api/types'
import Shell from '../src/app/Shell.svelte'
import type { SpeechService } from '../src/services/speech'
import { createTestServices, renderWith, type TestServicesOptions } from './helpers/services'

interface OpenOptions extends TestServicesOptions {
  cardIds?: Id[]
  /** Runs before the shell renders, e.g. to install spies. */
  before?: (services: Awaited<ReturnType<typeof createTestServices>>) => void
}

async function openStudy(deckId: string, { cardIds, before, ...options }: OpenOptions = {}) {
  window.history.replaceState(null, '', '#/home')
  const services = await createTestServices({ language: 'en', ...options })
  const session = await services.api.startStudy(cardIds ? { operationId: 'start', deckId, cardIds } : { operationId: 'start', deckId })
  window.history.replaceState(null, '', `#/study/${deckId}?session=${session.sessionId}`)
  before?.(services)
  const stop = services.router.start()
  renderWith(Shell, services)
  await screen.findByRole('button', { name: 'Show answer' })
  return { services, session, stop, user: userEvent.setup() }
}

const heading = () => document.querySelector('[data-screen-heading]')
const ratingButton = (name: string) => within(screen.getByRole('group', { name: 'How well did you remember?' })).getByRole('button', { name: new RegExp(`^${name}`) })
const front = () => document.querySelector('.front')?.textContent?.trim()

async function reveal() {
  await fireEvent.click(screen.getByRole('button', { name: 'Show answer' }))
  await screen.findByRole('group', { name: 'How well did you remember?' })
}

beforeEach(() => {
  document.body.innerHTML = ''
})

describe('study sections follow DeckCapabilities', () => {
  it('Simple: no associations, grammar tips or word links', async () => {
    const { stop } = await openStudy('4')
    await reveal()
    expect(screen.queryByRole('button', { name: 'Show Associations' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Show Grammar tips' })).not.toBeInTheDocument()
    expect(document.querySelector('.word')).toBeNull()
    stop()
  })

  it('TextWithAssociations: associations only, collapsed at first', async () => {
    const { stop } = await openStudy('3')
    await reveal()
    expect(screen.getByRole('button', { name: 'Show Associations' })).toHaveAttribute('aria-expanded', 'false')
    expect(screen.queryByRole('button', { name: 'Show Grammar tips' })).not.toBeInTheDocument()
    stop()
  })

  it('Linguistic: associations, grammar tips and word links', async () => {
    const { stop } = await openStudy('2')
    expect(document.querySelectorAll('.word').length).toBeGreaterThan(0)
    await reveal()
    expect(screen.getByRole('button', { name: 'Show Associations' })).toHaveAttribute('aria-expanded', 'false')
    expect(screen.getByRole('button', { name: 'Show Grammar tips' })).toHaveAttribute('aria-expanded', 'false')
    stop()
  })
})

describe('study interaction', () => {
  it('Space reveals and 1–4 rate; keys typed in inputs are ignored', async () => {
    const { services, stop, user } = await openStudy('1')
    const rateSpy = vi.spyOn(services.api, 'rateCard')
    const input = document.createElement('input')
    document.body.append(input)
    input.focus()
    await user.keyboard(' ')
    expect(screen.getByRole('button', { name: 'Show answer' })).toBeInTheDocument()

    input.blur()
    await user.keyboard(' ')
    await screen.findByRole('group', { name: 'How well did you remember?' })
    await waitFor(() => expect(document.activeElement).toHaveClass('back'))
    await user.keyboard('3')
    await waitFor(() => expect(rateSpy).toHaveBeenCalledWith(expect.objectContaining({ rating: 'Good' })))
    await waitFor(() => expect(document.activeElement).toHaveClass('front'))
    stop()
  })

  it('a double tap commits one rating', async () => {
    const { services, stop } = await openStudy('1')
    const rateSpy = vi.spyOn(services.api, 'rateCard')
    await reveal()
    const good = ratingButton('Good')
    await fireEvent.click(good)
    await fireEvent.click(good)
    await screen.findByRole('button', { name: 'Show answer' })
    expect(rateSpy).toHaveBeenCalledTimes(1)
    stop()
  })

  it('a failed save keeps the card and retries with the same operation', async () => {
    const { services, stop } = await openStudy('1')
    const rateSpy = vi.spyOn(services.api, 'rateCard')
    const card = front()
    await reveal()
    services.mock.failNext('rateCard', 'DB_BUSY')
    await fireEvent.click(ratingButton('Hard'))
    expect(await screen.findByText(/The collection is busy/)).toBeInTheDocument()
    expect(front()).toBe(card)
    await fireEvent.click(screen.getByRole('button', { name: 'Try again' }))
    await screen.findByRole('button', { name: 'Show answer' })
    expect(rateSpy.mock.calls.map(([input]) => input.operationId)).toEqual([rateSpy.mock.calls[0]?.[0].operationId, rateSpy.mock.calls[0]?.[0].operationId])
    stop()
  })

  it('no next card ends in a finished state that leads to the summary', async () => {
    const services = await createTestServices({ language: 'en' })
    const all = await services.api.startStudy({ operationId: 'probe', deckId: '1' })
    const { stop } = await openStudy('1', { cardIds: [all.current?.cardId ?? ''] })
    await reveal()
    await fireEvent.click(ratingButton('Good'))
    expect(await screen.findByText('No more cards in this session.')).toBeInTheDocument()
    await fireEvent.click(screen.getByRole('button', { name: 'Session summary' }))
    await waitFor(() => expect(heading()).toHaveTextContent('Session summary'))
    stop()
  })

  it('undo brings back the previous card', async () => {
    const { stop } = await openStudy('1')
    const first = front()
    await reveal()
    await fireEvent.click(ratingButton('Good'))
    await screen.findByRole('button', { name: 'Show answer' })
    await fireEvent.click(screen.getByRole('button', { name: 'Undo last rating' }))
    expect(await screen.findByText('Last rating undone.')).toBeInTheDocument()
    expect(front()).toBe(first)
    stop()
  })

  it('pause hides the card and ignores shortcuts until the session continues', async () => {
    const { stop, user } = await openStudy('1')
    await fireEvent.click(screen.getByRole('button', { name: 'Pause' }))
    expect(await screen.findByRole('heading', { name: 'Session paused' })).toBeInTheDocument()
    await user.keyboard(' ')
    expect(screen.queryByRole('group', { name: 'How well did you remember?' })).not.toBeInTheDocument()
    await fireEvent.click(screen.getByRole('button', { name: 'Continue' }))
    await screen.findByRole('button', { name: 'Show answer' })
    stop()
  })
})

describe('leaving, resuming and recovery', () => {
  it('asks before leaving an open session; returning does not duplicate subscriptions or loads', async () => {
    let listeners = 0
    let resumes = 0
    const { services, session, stop } = await openStudy('1', {
      before: (graph) => {
        const subscribe = graph.api.onJobEvent.bind(graph.api)
        vi.spyOn(graph.api, 'onJobEvent').mockImplementation((listener) => {
          listeners += 1
          const unsubscribe = subscribe(listener)
          return () => {
            listeners -= 1
            unsubscribe()
          }
        })
        const resume = graph.api.resumeStudy.bind(graph.api)
        vi.spyOn(graph.api, 'resumeStudy').mockImplementation((id) => {
          resumes += 1
          return resume(id)
        })
      },
    })
    expect(listeners).toBe(1)
    expect(resumes).toBe(1)

    await fireEvent.click(screen.getByRole('button', { name: 'Back' }))
    const dialog = await screen.findByRole('dialog', { name: 'Leave the session?' })
    await fireEvent.click(within(dialog).getByRole('button', { name: 'Keep studying' }))
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument())
    expect(heading()).toHaveTextContent('Niemiecki — słówka A1')

    await fireEvent.click(screen.getByRole('button', { name: 'Back' }))
    await fireEvent.click(within(await screen.findByRole('dialog', { name: 'Leave the session?' })).getByRole('button', { name: 'Leave session' }))
    await waitFor(() => expect(heading()).toHaveTextContent('Your decks'))
    expect(listeners).toBe(0)
    expect(await screen.findByRole('button', { name: 'Resume session' })).toBeInTheDocument()

    await services.router.navigate({ name: 'study', params: { deckId: '1' }, query: { session: session.sessionId } })
    await screen.findByRole('button', { name: 'Show answer' })
    expect(listeners).toBe(1)
    expect(resumes).toBe(2)
    stop()
  })

  it('a lost session offers a new one instead of an error', async () => {
    window.history.replaceState(null, '', '#/study/1?session=session-404')
    const services = await createTestServices({ language: 'en' })
    const stop = services.router.start()
    renderWith(Shell, services)
    expect(await screen.findByText('This session is no longer available')).toBeInTheDocument()
    await fireEvent.click(screen.getByRole('button', { name: 'Start a new session' }))
    await screen.findByRole('button', { name: 'Show answer' })
    stop()
  })
})

describe('card actions', () => {
  it('suspend moves on with a message; delete asks first; flag shows on the card', async () => {
    const { stop } = await openStudy('1')
    const first = front()
    await fireEvent.click(screen.getByRole('button', { name: 'More card actions' }))
    await fireEvent.click(within(await screen.findByRole('dialog', { name: 'More card actions' })).getByRole('button', { name: 'Suspend card' }))
    expect(await screen.findByText(/Card suspended/)).toBeInTheDocument()
    await waitFor(() => expect(front()).not.toBe(first))

    await fireEvent.click(screen.getByRole('button', { name: 'More card actions' }))
    await fireEvent.click(within(await screen.findByRole('dialog', { name: 'More card actions' })).getByRole('button', { name: 'Flag…' }))
    const flagDialog = await screen.findByRole('dialog', { name: 'Flag this card' })
    await fireEvent.click(within(flagDialog).getByRole('radio', { name: 'Red' }))
    await fireEvent.click(within(flagDialog).getByRole('button', { name: 'Set flag' }))
    expect(await screen.findByText('Flag: Red')).toBeInTheDocument()

    const second = front()
    await fireEvent.click(screen.getByRole('button', { name: 'More card actions' }))
    await fireEvent.click(within(await screen.findByRole('dialog', { name: 'More card actions' })).getByRole('button', { name: 'Delete card…' }))
    const confirm = await screen.findByRole('dialog', { name: 'Delete this card?' })
    await fireEvent.click(within(confirm).getByRole('button', { name: 'Delete card' }))
    expect(await screen.findByText('Card deleted.')).toBeInTheDocument()
    await waitFor(() => expect(front()).not.toBe(second))
    stop()
  })
})

describe('text to speech', () => {
  const speech = (voices: Record<string, boolean>) =>
    ({
      availability: (lang) => (lang in voices ? (voices[lang] ? 'available' : 'noVoice') : 'unsupported'),
      speak: vi.fn<SpeechService['speak']>(),
      stop: () => {},
      onChange: () => () => {},
    }) satisfies SpeechService

  it('speaks when a voice exists', async () => {
    const fake = speech({ de: true })
    const { stop } = await openStudy('1', { speech: fake })
    await fireEvent.click(screen.getByRole('button', { name: 'Listen (German)' }))
    expect(fake.speak).toHaveBeenCalledWith(front(), 'de')
    stop()
  })

  it('distinguishes a missing voice from missing speech support', async () => {
    const noVoice = await openStudy('3', { speech: speech({ en: false }) })
    expect(screen.getByRole('button', { name: 'No English voice installed' })).toHaveAttribute('aria-disabled', 'true')
    noVoice.stop()
    document.body.innerHTML = ''
    const unsupported = await openStudy('3')
    expect(screen.getByRole('button', { name: 'Speech is not available on this device' })).toHaveAttribute('aria-disabled', 'true')
    unsupported.stop()
  })
})

describe('associations and word links', () => {
  it('generation runs in the background and never blocks rating', async () => {
    const { stop } = await openStudy('3', { jobStepMs: 30 })
    await reveal()
    await fireEvent.click(screen.getByRole('button', { name: 'Show Associations' }))
    const count = document.querySelectorAll('.disclosure .item').length
    await fireEvent.click(screen.getByRole('button', { name: 'Generate new association' }))
    expect(await screen.findByText('Generating a new association — you can keep studying.')).toBeInTheDocument()
    expect(ratingButton('Good')).toBeEnabled()
    await waitFor(() => expect(screen.queryByText('Generating a new association — you can keep studying.')).not.toBeInTheDocument())
    await waitFor(() => expect(document.querySelectorAll('.disclosure .item').length).toBe(count + 1))
    expect(await screen.findByText(/\(demo\)/)).toBeInTheDocument()
    stop()
  })

  it('quota errors are shown inline and the card can still be rated', async () => {
    const { stop } = await openStudy('3', { scenario: 'quota' })
    await reveal()
    await fireEvent.click(screen.getByRole('button', { name: 'Show Associations' }))
    await fireEvent.click(screen.getByRole('button', { name: 'Generate new association' }))
    expect(await screen.findByText('The AI generation limit has been reached.')).toBeInTheDocument()
    expect(ratingButton('Good')).toBeEnabled()
    stop()
  })

  it('a resolved word opens a sheet with its translation', async () => {
    const { stop } = await openStudy('2')
    const linked = screen.getAllByRole('button', { name: /: linked word$/ })[0]
    if (!linked) throw new Error('expected a resolved link')
    await fireEvent.click(linked)
    const sheet = await screen.findByRole('dialog', { name: linked.textContent?.trim() ?? '' })
    await waitFor(() => expect(sheet.querySelector('.translation')).not.toBeNull())
    stop()
  })

  it('an ambiguous word asks for a choice and saves it', async () => {
    const { stop } = await openStudy('2', { scenario: 'ambiguous' })
    const ambiguous = screen.getAllByRole('button', { name: /several possible words/ })[0]
    if (!ambiguous) throw new Error('expected an ambiguous link')
    await fireEvent.click(ambiguous)
    const sheet = await screen.findByRole('dialog')
    const use = within(sheet).getByRole('button', { name: 'Use this word' })
    expect(use).toBeDisabled()
    await fireEvent.click(within(sheet).getAllByRole('radio')[0] as HTMLElement)
    await fireEvent.click(use)
    expect(await within(sheet).findByText('Your choice is saved for this sentence.')).toBeInTheDocument()
    await waitFor(() => expect(sheet.querySelector('.translation')).not.toBeNull())
    stop()
  })
})
