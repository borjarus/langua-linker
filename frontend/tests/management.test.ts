import { fireEvent, screen, waitFor, within } from '@testing-library/svelte'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it } from 'vitest'
import type { AppApi, AppApiOperation } from '../src/api/appApi'
import { OPERATIONS, type ScreenId } from '../src/api/operations'
import type { SearchInput } from '../src/api/types'
import Shell from '../src/app/Shell.svelte'
import { MockAppApi } from '../src/demo/mockAppApi'
import type { DemoScenario } from '../src/demo/scenarios'
import { createTestServices, renderWith } from './helpers/services'

/** Records every operation a screen calls, so each action can be checked against the catalogue. */
function recording(mock: MockAppApi) {
  const calls = new Set<AppApiOperation>()
  const api = new Proxy(mock, {
    get(target, property) {
      const value = Reflect.get(target, property, target) as unknown
      if (typeof value !== 'function') return value
      if (typeof property === 'string' && property in OPERATIONS) calls.add(property as AppApiOperation)
      return (value as (...args: unknown[]) => unknown).bind(target)
    },
  })
  return { api: api as unknown as AppApi, calls }
}

async function open(target: string | ((mock: MockAppApi) => Promise<string>), scenario?: DemoScenario) {
  window.history.replaceState(null, '', '#/home')
  const mock = new MockAppApi({ scenario })
  const { api, calls } = recording(mock)
  const services = await createTestServices({ language: 'en', api })
  const hash = typeof target === 'string' ? target : await target(mock)
  calls.clear()
  window.history.replaceState(null, '', hash)
  const stop = services.router.start()
  renderWith(Shell, services)
  return { services, mock, calls, stop, user: userEvent.setup() }
}

/** Every operation a screen used must name that screen in the catalogue: no action without a future backend contract. */
function expectContract(calls: Set<AppApiOperation>, ...screens: ScreenId[]) {
  for (const operation of calls) {
    const allowed: readonly ScreenId[] = OPERATIONS[operation].screens
    expect(screens.some((item) => allowed.includes(item)) || allowed.includes('shell'), `${operation} → ${screens.join('/')}`).toBe(true)
  }
}

const search = (mock: MockAppApi, overrides: Partial<SearchInput> = {}) =>
  mock.searchCards({ query: '', deckId: null, tag: null, states: [], flags: [], sort: 'created', direction: 'asc', cursor: null, pageSize: 100, ...overrides })

async function cardOf(mock: MockAppApi, query: string, direction: 'forward' | 'reverse' = 'forward') {
  const row = (await search(mock, { query })).items.find((item) => item.direction === direction)
  if (!row) throw new Error(`no ${direction} card for ${query}`)
  return row
}

const region = (name: string | RegExp) => screen.getByRole('region', { name })

beforeEach(() => {
  document.body.innerHTML = ''
})

describe('card details', () => {
  it('a reversed note shows two cards with independent schedules', async () => {
    const { mock, calls, stop, user } = await open(async (api) => `#/card/${(await cardOf(api, '"thank you"', 'reverse')).cardId}`)
    const forward = await screen.findByRole('region', { name: 'Forward (English → Polish)' })
    const reverse = region('Reverse (Polish → English)')
    expect(within(reverse).getByText('Opened card')).toBeInTheDocument()
    expect(within(forward).queryByText('Opened card')).not.toBeInTheDocument()
    expect(screen.getByText('The cards share the note’s content, but each has its own schedule.')).toBeInTheDocument()

    await user.click(within(forward).getByRole('button', { name: 'Suspend' }))
    expect(await screen.findByText('Card suspended. You can unsuspend it in Browse.')).toBeInTheDocument()
    await waitFor(() => expect(within(region('Forward (English → Polish)')).getByText('Suspended')).toBeInTheDocument())
    expect(within(region('Reverse (Polish → English)')).queryByText('Suspended')).not.toBeInTheDocument()
    const details = await mock.getNoteDetails((await cardOf(mock, '"thank you"', 'reverse')).cardId)
    expect(details.cards.map((card) => card.queue)).toEqual(['Suspended', expect.not.stringMatching('Suspended')])
    expectContract(calls, 'card')
    stop()
  })

  it('labels R with its calculation time and lists the history', async () => {
    const { stop, user } = await open(async (api) => `#/card/${(await api.searchCards({ query: '', deckId: '1', tag: null, states: ['Review'], flags: [], sort: 'created', direction: 'asc', cursor: null, pageSize: 1 })).items[0]?.cardId}`)
    expect(await screen.findByText(/^Calculated /)).toBeInTheDocument()
    expect(screen.getByText('Retrievability (R)')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Show Review history' }))
    expect(screen.getByRole('columnheader', { name: 'Rating' })).toBeInTheDocument()
    stop()
  })

  it('adds, reorders and deletes grammar tips; only own associations are editable', async () => {
    const { calls, stop, user } = await open(async (api) => `#/card/${(await cardOf(api, 'Kühlschrank')).cardId}`)
    const tips = await screen.findByRole('region', { name: 'Grammar tips' })
    expect(within(tips).getAllByRole('listitem')[0]).toHaveTextContent('Rodzaj męski')
    await user.click(within(tips).getByRole('button', { name: 'Move tip 2 up' }))
    expect(await screen.findByText('Order of grammar tips saved.')).toBeInTheDocument()
    await waitFor(() => expect(within(region('Grammar tips')).getAllByRole('listitem')[0]).toHaveTextContent('Złożenie'))

    await user.type(within(region('Grammar tips')).getByRole('textbox', { name: 'New grammar tip' }), 'Die Kühlschränke — liczba mnoga.')
    await user.click(within(region('Grammar tips')).getByRole('button', { name: 'Add tip' }))
    expect(await screen.findByText('Grammar tip added.')).toBeInTheDocument()
    await waitFor(() => expect(within(region('Grammar tips')).getAllByRole('listitem')).toHaveLength(3))

    const third = within(region('Grammar tips')).getAllByRole('listitem')[2] as HTMLElement
    await user.click(within(third).getByRole('button', { name: 'Delete' }))
    const confirm = await screen.findByRole('dialog', { name: 'Delete this grammar tip?' })
    await user.click(within(confirm).getByRole('button', { name: 'Delete' }))
    await waitFor(() => expect(within(region('Grammar tips')).getAllByRole('listitem')).toHaveLength(2))

    const associations = region('Associations')
    expect(within(associations).queryByRole('button', { name: 'Edit' })).not.toBeInTheDocument()
    await user.type(within(associations).getByRole('textbox', { name: 'Your own association' }), 'Mein Kühlschrank brummt')
    await user.click(within(associations).getByRole('button', { name: 'Add association' }))
    expect(await screen.findByText('Association added.')).toBeInTheDocument()
    await waitFor(() => expect(within(region('Associations')).getAllByRole('button', { name: 'Edit' })).toHaveLength(1))
    expectContract(calls, 'card')
    stop()
  })

  it('deleting the opened card shows the other card; deleting the note leaves for the browser', async () => {
    const { services, mock, stop, user } = await open(async (api) => `#/card/${(await cardOf(api, '"good morning"')).cardId}`)
    const reverseId = (await cardOf(mock, '"good morning"', 'reverse')).cardId
    const forward = await screen.findByRole('region', { name: 'Forward (English → Polish)' })
    await user.click(within(forward).getByRole('button', { name: 'Delete card' }))
    const dialog = await screen.findByRole('dialog', { name: 'Delete this card?' })
    expect(within(dialog).getByText(/The note and its other card stay/)).toBeInTheDocument()
    await user.click(within(dialog).getByRole('button', { name: 'Delete card' }))
    await waitFor(() => expect(services.router.current).toMatchObject({ name: 'card', params: { id: reverseId } }))
    expect(await screen.findByRole('region', { name: 'Reverse (Polish → English)' })).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Delete note' }))
    const noteDialog = await screen.findByRole('dialog', { name: 'Delete this note?' })
    await user.click(within(noteDialog).getByRole('button', { name: 'Delete note' }))
    await waitFor(() => expect(services.router.current.name).toBe('browser'))
    expect((await search(mock, { query: '"good morning"' })).total).toBe(0)
    stop()
  })
})

describe('editor', () => {
  it('validates fields, warns about duplicates without blocking, and saves and adds the next note', async () => {
    const { services, calls, stop, user } = await open('#/editor?deckId=1')
    const front = await screen.findByRole('textbox', { name: 'Front (German)' })
    await user.click(screen.getByRole('button', { name: 'Save' }))
    expect(screen.getByText('Enter the front.')).toBeInTheDocument()
    expect(front).toHaveAttribute('aria-invalid', 'true')

    await user.type(front, 'der Kühlschrank')
    expect(await screen.findByText('A note with the same front already exists:', {}, { timeout: 2000 })).toBeInTheDocument()
    await user.type(screen.getByRole('textbox', { name: 'Back (Polish)' }), 'chłodziarka')
    await user.type(screen.getByRole('combobox', { name: 'Tags' }), 'dom{Enter}')
    expect(screen.getByRole('button', { name: 'Remove tag dom' })).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Save and add next' }))
    expect(await screen.findByText('Note saved. Add the next one.')).toBeInTheDocument()
    expect(front).toHaveValue('')
    expect(screen.getByRole('combobox', { name: 'Deck' })).toHaveValue('1')
    expect(screen.getByRole('button', { name: 'Remove tag dom' })).toBeInTheDocument()
    expect((await services.api.checkDuplicate({ deckId: '1', front: 'der Kühlschrank', excludeNoteId: null })).duplicates).toHaveLength(2)
    expectContract(calls, 'editor')
    stop()
  })

  it('guards an unsaved draft on navigation', async () => {
    const { services, stop, user } = await open('#/editor?deckId=1')
    await user.type(await screen.findByRole('textbox', { name: 'Front (German)' }), 'das Fenster')
    void services.router.navigate({ name: 'home' })
    const guard = await screen.findByRole('dialog', { name: 'Discard changes?' })
    await user.click(within(guard).getByRole('button', { name: 'Keep editing' }))
    expect(services.router.current.name).toBe('editor')
    expect(screen.getByRole('textbox', { name: 'Front (German)' })).toHaveValue('das Fenster')
    void services.router.navigate({ name: 'home' })
    await user.click(within(await screen.findByRole('dialog', { name: 'Discard changes?' })).getByRole('button', { name: 'Discard changes' }))
    await waitFor(() => expect(services.router.current.name).toBe('home'))
    stop()
  })

  it('a stale version opens the conflict dialog; keeping the draft saves over the latest version', async () => {
    const { services, mock, stop, user } = await open('#/home')
    const card = await cardOf(mock, 'Tee')
    await services.router.navigate({ name: 'editor', query: { cardId: card.cardId } })
    const back = await screen.findByRole('textbox', { name: 'Back (Polish)' })
    await waitFor(() => expect(back).toHaveValue('herbata'))
    // Someone else saves the note meanwhile.
    const stored = await mock.getNoteDetails(card.cardId)
    await mock.saveNote({ operationId: 'elsewhere', noteId: stored.note.id, expectedVersion: stored.note.version, deckId: '1', noteTypeKind: 'Basic', front: 'der Tee', back: 'herbata (napój)', tags: [], manualAssociation: null, grammarTips: [], wordLinks: null })
    await user.clear(back)
    await user.type(back, 'herbata — moja wersja')
    await user.click(screen.getByRole('button', { name: 'Save' }))
    const dialog = await screen.findByRole('dialog', { name: 'This note changed in the meantime' })
    await user.click(within(dialog).getByRole('button', { name: 'Keep my changes' }))
    expect(await screen.findByText('Your changes are kept. Save again to replace the latest version.')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Save' }))
    await waitFor(() => expect(services.router.current.name).toBe('home'))
    expect((await mock.getNoteDetails(card.cardId)).note.back).toBe('herbata — moja wersja')
    stop()
  })

  it('a note-type change previews archiving and restores the card with its history', async () => {
    const { services, mock, stop, user } = await open('#/home')
    const reverse = await cardOf(mock, '"see you later"', 'reverse')
    const forward = await cardOf(mock, '"see you later"')
    const before = (await mock.getNoteDetails(forward.cardId)).cards.find((card) => card.id === reverse.cardId)
    await services.router.navigate({ name: 'editor', query: { cardId: forward.cardId } })
    await user.click(await screen.findByRole('radio', { name: /^Basic One card/ }))
    const preview = region(/^Preview/)
    expect(within(preview).getByText('Will be archived')).toBeInTheDocument()
    expect(within(preview).getByText('Schedule kept')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Save' }))
    await waitFor(() => expect(services.router.current.name).toBe('home'))
    expect((await mock.getNoteDetails(forward.cardId)).archivedOrdinals).toEqual([1])

    await services.router.navigate({ name: 'editor', query: { cardId: forward.cardId } })
    await user.click(await screen.findByRole('radio', { name: /^Basic and reversed/ }))
    expect(within(region(/^Preview/)).getByText('Restored with history')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Save' }))
    await waitFor(() => expect(services.router.current.name).toBe('home'))
    const restored = (await mock.getNoteDetails(forward.cardId)).cards.find((card) => card.ordinal === 1)
    expect(restored).toMatchObject({ id: reverse.cardId, reps: before?.reps, history: before?.history })
    stop()
  })

  it('links selected words of a sentence only to a word the user picks', async () => {
    const { services, mock, calls, stop, user } = await open('#/editor?deckId=2')
    await user.click(await screen.findByRole('radio', { name: /^Sentence(?! and reversed)/ }))
    const front = screen.getByRole('textbox', { name: 'Sentence (German)' }) as HTMLTextAreaElement
    await user.type(front, 'Der Tee ist heiß.')
    await user.type(screen.getByRole('textbox', { name: 'Back (Polish)' }), 'Herbata jest gorąca.')
    const link = screen.getByRole('button', { name: 'Link selected words' })
    expect(link).toBeDisabled()

    front.setSelectionRange(4, 7)
    await fireEvent.select(front)
    await user.click(link)
    const picker = await screen.findByRole('dialog', { name: 'Link “Tee”' })
    const confirm = within(picker).getByRole('button', { name: 'Link to this word' })
    expect(confirm).toBeDisabled()
    await user.click(await within(picker).findByRole('radio', { name: /der Tee/ }))
    await user.click(confirm)
    expect(await screen.findByText('“Tee” linked.')).toBeInTheDocument()

    front.setSelectionRange(12, 16)
    await fireEvent.select(front)
    await user.click(screen.getByRole('button', { name: 'Link selected words' }))
    const second = await screen.findByRole('dialog', { name: 'Link “heiß”' })
    expect(await within(second).findByText(/No word in your decks matches/)).toBeInTheDocument()
    await user.click(within(second).getByRole('button', { name: 'Keep without a word' }))
    expect(await screen.findByText('“heiß” marked without a word.')).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Save' }))
    await waitFor(() => expect(services.router.current.name).toBe('card'))
    const route = services.router.current
    const saved = await mock.getNoteDetails(route.name === 'card' ? route.params.id : '')
    expect(saved.wordLinks.map((item) => [item.surface, item.resolution])).toEqual([['Tee', 'userOverridden'], ['heiß', 'unresolved']])
    expectContract(calls, 'editor', 'card')
    stop()
  })
})

describe('browser', () => {
  it('shows both cards of reversed notes as separate rows', async () => {
    const { stop } = await open('#/browser?deck=4')
    expect(await screen.findByText('8 matching cards')).toBeInTheDocument()
    const meta = [...document.querySelectorAll('.meta')].map((element) => element.textContent ?? '')
    expect(meta.filter((text) => text.includes('Reverse'))).toHaveLength(4)
    stop()
  })

  it('pages 50,000 matches without rendering every row', async () => {
    const { calls, stop } = await open('#/browser', 'large')
    expect(await screen.findByText('50,000 matching cards')).toBeInTheDocument()
    const rows = within(screen.getByRole('list', { name: 'Search results' })).getAllByRole('listitem')
    expect(rows.length).toBeLessThan(60)
    expect(rows[0]).toHaveAttribute('aria-setsize', '50000')
    expectContract(calls, 'browser')
    stop()
  })

  it('reports parser errors and filters by state', async () => {
    const { stop, user } = await open('#/browser')
    await screen.findByText(/matching cards/)
    await user.type(screen.getByRole('searchbox', { name: 'Search cards' }), 'foo:bar')
    expect(await screen.findByText('Unknown search field “foo”. Use deck:, tag: or is:.')).toBeInTheDocument()
    await user.clear(screen.getByRole('searchbox', { name: 'Search cards' }))
    await screen.findByText(/matching cards/)
    await user.click(screen.getByRole('button', { name: 'Show Filters and sorting' }))
    await user.click(screen.getByRole('button', { name: 'New' }))
    await waitFor(() => expect(document.querySelectorAll('.meta')[0]?.textContent).toContain('New'))
    const meta = [...document.querySelectorAll('.meta')].map((element) => element.textContent ?? '')
    expect(meta.every((text) => text.includes('New'))).toBe(true)
    stop()
  })

  it('bulk actions preview their effect and apply only after confirmation', async () => {
    const { mock, calls, stop, user } = await open('#/browser?deck=4')
    await user.click(await screen.findByRole('button', { name: 'Select all 8 matches' }))
    await user.click(screen.getByRole('button', { name: 'Actions (8)' }))
    await user.click(within(await screen.findByRole('dialog', { name: 'Change 8 selected cards' })).getByRole('button', { name: 'Suspend' }))
    const confirm = await screen.findByRole('dialog', { name: 'Suspend — confirm' })
    expect(await within(confirm).findByText('8 cards will change.')).toBeInTheDocument()
    expect((await search(mock, { query: 'is:suspended' })).total).toBe(0)
    await user.click(within(confirm).getByRole('button', { name: 'Apply to 8 cards' }))
    expect(await screen.findByText('8 cards changed.')).toBeInTheDocument()
    expect((await search(mock, { query: 'is:suspended' })).total).toBe(8)

    // The same action again changes nothing, and says so.
    await user.click(await screen.findByRole('button', { name: 'Select all 8 matches' }))
    await user.click(screen.getByRole('button', { name: 'Actions (8)' }))
    await user.click(within(await screen.findByRole('dialog', { name: 'Change 8 selected cards' })).getByRole('button', { name: 'Suspend' }))
    const again = await screen.findByRole('dialog', { name: 'Suspend — confirm' })
    expect(await within(again).findByText('0 cards will change.')).toBeInTheDocument()
    expect(within(again).getByText(/8 selected cards are already in that state/)).toBeInTheDocument()
    expect(within(again).getByRole('button', { name: 'Apply to 0 cards' })).toBeDisabled()
    expectContract(calls, 'browser')
    stop()
  })

  it('keyboard: arrows move between rows and Space ticks a row', async () => {
    const { stop, user } = await open('#/browser?deck=4')
    await screen.findByText('8 matching cards')
    const boxes = () => [...document.querySelectorAll<HTMLInputElement>('[data-index] input[type="checkbox"]')]
    boxes()[0]?.focus()
    await fireEvent.keyDown(boxes()[0] as HTMLInputElement, { key: 'ArrowDown' })
    await waitFor(() => expect(document.activeElement).toBe(boxes()[1]))
    await user.keyboard(' ')
    expect(boxes()[1]).toBeChecked()
    expect(screen.getByText(/1 selected/)).toBeInTheDocument()
    await fireEvent.keyDown(boxes()[1] as HTMLInputElement, { key: 'ArrowUp' })
    await waitFor(() => expect(document.activeElement).toBe(boxes()[0]))
    stop()
  })
})

describe('deck overview', () => {
  it('shows counters and a collapsible subdeck tree', async () => {
    const { calls, stop, user } = await open(async (api) => {
      await api.createDeck({ operationId: 'g', name: 'Złożenia — miasto', parentId: '5', targetLanguage: 'de', deckType: 'Linguistic' })
      return '#/deck/1'
    })
    const tree = await screen.findByRole('region', { name: '2 subdecks' })
    expect(within(tree).getByRole('link', { name: 'Złożenia — miasto' })).toBeInTheDocument()
    await user.click(within(tree).getByRole('button', { name: 'Collapse Złożenia (Komposita)' }))
    expect(within(tree).queryByRole('link', { name: 'Złożenia — miasto' })).not.toBeInTheDocument()
    expect(within(tree).getByRole('button', { name: 'Expand Złożenia (Komposita)' })).toHaveAttribute('aria-expanded', 'false')
    expect(screen.getByText('Overdue')).toBeInTheDocument()
    expectContract(calls, 'deck')
    stop()
  })

  it('renames and moves a deck; its own subdecks are not offered as a parent', async () => {
    const { mock, stop, user } = await open('#/deck/1')
    await user.click(await screen.findByRole('button', { name: 'Rename or move…' }))
    const dialog = await screen.findByRole('dialog', { name: 'Deck name and place' })
    const parent = within(dialog).getByRole('combobox', { name: 'Parent deck' })
    const offered = within(parent).getAllByRole('option').map((option) => option.textContent)
    expect(offered).not.toContain('Złożenia (Komposita)')
    expect(offered).not.toContain('Niemiecki — słówka A1')
    const name = within(dialog).getByRole('textbox', { name: 'Deck name' })
    await user.clear(name)
    await user.click(within(dialog).getByRole('button', { name: 'Save' }))
    expect(within(dialog).getByText('Enter a deck name.')).toBeInTheDocument()
    await user.type(name, 'Niemiecki A1')
    await user.selectOptions(parent, 'Niemiecki — zdania A1')
    expect(within(dialog).getByText(/Moves 2 deck\(s\)/)).toBeInTheDocument()
    await user.click(within(dialog).getByRole('button', { name: 'Save' }))
    expect(await screen.findByText('Deck moved.')).toBeInTheDocument()
    expect((await mock.getDeck('1')).deck).toMatchObject({ name: 'Niemiecki A1', parentId: '2' })
    stop()
  })

  it('a type change hides content without deleting it; switching back restores it', async () => {
    const { mock, stop, user } = await open('#/deck/1')
    await user.click(await screen.findByRole('button', { name: 'Change type…' }))
    const dialog = await screen.findByRole('dialog', { name: 'Change the deck type' })
    await user.click(within(dialog).getByRole('radio', { name: /^Simple/ }))
    expect(within(dialog).getByText('Hides 10 associations, 8 grammar tips.')).toBeInTheDocument()
    await user.click(within(dialog).getByRole('button', { name: 'Change type' }))
    expect(await screen.findByText('Deck type changed. Hidden content is kept.')).toBeInTheDocument()
    expect(await screen.findByText('18 items kept but hidden by this type.')).toBeInTheDocument()
    const kühlschrank = await cardOf(mock, 'Kühlschrank')
    expect((await mock.getNoteDetails(kühlschrank.cardId)).grammarTips).toHaveLength(2)

    await user.click(screen.getByRole('button', { name: 'Change type…' }))
    const back = await screen.findByRole('dialog', { name: 'Change the deck type' })
    await user.click(within(back).getByRole('radio', { name: /^Linguistic/ }))
    expect(within(back).getByText('18 hidden items will be shown again.')).toBeInTheDocument()
    await user.click(within(back).getByRole('button', { name: 'Change type' }))
    expect(await screen.findByText('Deck type changed — 18 hidden items are visible again.')).toBeInTheDocument()
    stop()
  })

  it('delete names the whole scope and returns to the dashboard', async () => {
    const { services, mock, stop, user } = await open('#/deck/1')
    await user.click(await screen.findByRole('button', { name: 'Delete deck…' }))
    const dialog = await screen.findByRole('dialog', { name: 'Delete “Niemiecki — słówka A1”?' })
    expect(within(dialog).getByText(/This deletes 2 deck\(s\).*with 13 card\(s\)/)).toBeInTheDocument()
    await user.click(within(dialog).getByRole('button', { name: 'Delete deck and 13 cards' }))
    await waitFor(() => expect(services.router.current.name).toBe('home'))
    await expect(mock.getDeck('5')).rejects.toMatchObject({ dto: { code: 'NOT_FOUND' } })
    stop()
  })

  it('presets: dependents are shown, the editor validates and warns, and a copy is assigned to the deck', async () => {
    const { mock, calls, stop, user } = await open('#/deck/1')
    await user.click(await screen.findByRole('button', { name: 'Options preset…' }))
    const picker = await screen.findByRole('dialog', { name: 'Choose an options preset' })
    expect(await within(picker).findByText('Used by 4 decks')).toBeInTheDocument()
    await user.click(within(picker).getByRole('button', { name: 'Edit Domyślny (20 nowych/dzień)' }))
    const editor = await screen.findByRole('dialog', { name: 'Edit preset “Domyślny (20 nowych/dzień)”' })
    expect(within(editor).getByText(/This preset is used by 4 decks — saving changes all of them/)).toBeInTheDocument()
    const retention = within(editor).getByRole('spinbutton', { name: 'Desired retention (%)' })
    await user.clear(retention)
    await user.type(retention, '120')
    await user.click(within(editor).getByRole('button', { name: 'Save preset' }))
    expect(within(editor).getByText('Enter a whole number from 70 to 99.')).toBeInTheDocument()
    await user.clear(retention)
    await user.type(retention, '85')
    await user.click(within(editor).getByRole('button', { name: 'Save as a copy for Niemiecki — słówka A1' }))
    expect(await screen.findByText('Copy “Domyślny (20 nowych/dzień) (copy)” saved and used by this deck.')).toBeInTheDocument()
    const deck = await mock.getDeck('1')
    expect(deck).toMatchObject({ presetName: 'Domyślny (20 nowych/dzień) (copy)', presetDependentDecks: 1 })
    expect((await mock.listPresets()).find((preset) => preset.id === deck.presetId)?.config.desiredRetention).toBe(0.85)
    expectContract(calls, 'deck')
    stop()
  })
})
