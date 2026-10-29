import { screen, waitFor, within } from '@testing-library/svelte'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it } from 'vitest'
import type { AppApi, AppApiOperation } from '../src/api/appApi'
import { OPERATIONS, type ScreenId } from '../src/api/operations'
import Shell from '../src/app/Shell.svelte'
import { MockAppApi } from '../src/demo/mockAppApi'
import type { DemoScenario } from '../src/demo/scenarios'
import { createTestServices, renderWith } from './helpers/services'

// Phase 5: statistics, settings, onboarding, transfer and backup on the mock, through the Shell.

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

async function open(hash: string, scenario?: DemoScenario) {
  window.history.replaceState(null, '', '#/home')
  const mock = new MockAppApi({ scenario })
  const { api, calls } = recording(mock)
  const services = await createTestServices({ language: 'en', api })
  // The demo controls of the test services drive the recorded mock, like the demo banner does.
  services.demo = { ...services.demo!, pickedFile: (purpose) => mock.pickedFile(purpose), setPickedFile: (purpose, token) => mock.setPickedFile(purpose, token) }
  window.history.replaceState(null, '', hash)
  const stop = services.router.start()
  renderWith(Shell, services)
  return { services, mock, calls, stop, user: userEvent.setup() }
}

/** No action without a future backend contract: every call must list one of the screens. */
function expectContract(calls: Set<AppApiOperation>, ...screens: ScreenId[]) {
  for (const operation of calls) {
    const allowed: readonly ScreenId[] = OPERATIONS[operation].screens
    expect(screens.some((item) => allowed.includes(item)) || allowed.includes('shell'), `${operation} → ${screens.join('/')}`).toBe(true)
  }
}

const region = (name: string | RegExp) => screen.getByRole('region', { name })

beforeEach(() => {
  document.body.innerHTML = ''
})

describe('statistics', () => {
  it('shows tiles and charts, each with a table, and refetches for a new period', async () => {
    const { calls, services, stop, user } = await open('#/stats')
    expect(await screen.findByText('True retention — mature cards')).toBeInTheDocument()
    for (const title of ['Activity', 'Reviews', 'Study time', 'Due forecast — next 30 days', 'Card states', 'Memory stability']) {
      expect(screen.getByRole('heading', { level: 2, name: title })).toBeInTheDocument()
    }
    expect(screen.getByText(/Line: the reviews expected with the limit of 200 reviews a day/)).toBeInTheDocument()

    const forecast = region('Due forecast — next 30 days')
    await user.click(within(forecast).getByRole('button', { name: 'Show as table' }))
    expect(within(forecast).getByRole('columnheader', { name: 'Expected with limits' })).toBeInTheDocument()
    expect(within(forecast).getAllByRole('row')).toHaveLength(31)

    const chart = within(region('Reviews')).getByRole('group', { name: 'Reviews' })
    chart.focus()
    await user.keyboard('{Home}')
    expect(within(region('Reviews')).getByText(/: Reviews \d+$/)).toBeInTheDocument()

    await user.click(screen.getByRole('radio', { name: '3 months' }))
    await waitFor(() => expect(screen.getByText('One column per week (Monday to Sunday).')).toBeInTheDocument())
    await user.selectOptions(screen.getByRole('combobox', { name: 'Deck' }), '5')
    await waitFor(() => expect(screen.getAllByText(/Calculated/)).toHaveLength(1))

    // Language changes reach chart titles and the table headers, not only the root.
    await services.i18n.setLanguage('pl')
    expect(await screen.findByRole('heading', { level: 2, name: 'Aktywność' })).toBeInTheDocument()
    expectContract(calls, 'stats')
    stop()
  })

  it('offers a way out of an empty collection instead of empty charts', async () => {
    for (const scenario of ['empty', 'noCards'] as const) {
      document.body.innerHTML = ''
      const { stop } = await open('#/stats', scenario)
      expect(await screen.findByText('No cards to analyse yet')).toBeInTheDocument()
      expect(screen.getByRole('link', { name: 'Go to decks' })).toBeInTheDocument()
      expect(screen.queryByText('0%')).not.toBeInTheDocument()
      stop()
    }
  })

  it('reports a failed load with a retry', async () => {
    const { stop } = await open('#/stats', 'error')
    expect(await screen.findByText('The collection is busy. Your work is kept — try again in a moment.')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument()
    stop()
  })
})

describe('settings', () => {
  it('saves each change with the current version and keeps declined notifications harmless', async () => {
    const { calls, mock, stop, user } = await open('#/settings')
    const goal = await screen.findByRole('spinbutton', { name: 'Daily goal (cards)' })
    await user.clear(goal)
    await user.type(goal, '35')
    await user.tab()
    expect(await screen.findByText('Saved')).toBeInTheDocument()
    await user.selectOptions(screen.getByRole('combobox', { name: 'A new study day starts at' }), '5')
    await waitFor(async () => expect(await mock.getSettings()).toMatchObject({ dailyGoal: 35, dayCutoffHour: 5 }))
    expect(screen.getByText(/Reviews done before 05:00 count towards the previous day/)).toBeInTheDocument()

    mock.setNotificationAnswer('denied')
    await user.click(screen.getByRole('button', { name: 'Allow notifications' }))
    expect(await screen.findByText(/Notifications are blocked. Learning works without them/)).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Allow notifications' })).not.toBeInTheDocument()

    await user.click(screen.getByRole('checkbox', { name: 'Show the listen button on cards' }))
    await waitFor(async () => expect((await mock.getSettings()).ttsEnabled).toBe(false))
    expect(screen.getByRole('slider', { name: /Speech rate/ })).toBeDisabled()
    expectContract(calls, 'settings')
    stop()
  })

  it('changes the language of the whole app, including the navigation', async () => {
    const { mock, stop, user } = await open('#/settings')
    await user.selectOptions(await screen.findByRole('combobox', { name: 'App language' }), 'pl')
    expect(await screen.findByRole('heading', { level: 1, name: 'Ustawienia' })).toBeInTheDocument()
    expect(screen.getByRole('navigation', { name: 'Nawigacja główna' })).toBeInTheDocument()
    await waitFor(async () => expect((await mock.getSettings()).uiLanguage).toBe('pl'))
    stop()
  })

  it('AI can be declined; a saved key is never shown again', async () => {
    const { calls, mock, stop, user } = await open('#/settings/ai')
    await user.click(await screen.findByRole('radio', { name: 'Do not use AI' }))
    expect(await screen.findByText(/AI is off. Learning works fully/)).toBeInTheDocument()
    expect(screen.queryByRole('heading', { name: 'Provider' })).not.toBeInTheDocument()

    await user.click(screen.getByRole('radio', { name: 'Allow generating associations with AI' }))
    await user.selectOptions(await screen.findByRole('combobox', { name: 'AI provider' }), 'openaiCompatible')
    const endpoint = await screen.findByRole('textbox', { name: 'Server address' })
    await user.type(endpoint, 'http://local')
    expect(screen.getByText('Use an address starting with https://.')).toBeInTheDocument()
    await user.clear(endpoint)
    await user.type(endpoint, 'https://llm.example.com/v1')
    await user.click(within(endpoint.closest('.field') as HTMLElement).getByRole('button', { name: 'Save' }))
    await waitFor(async () => expect((await mock.getSettings()).ai.endpoint).toBe('https://llm.example.com/v1'))

    const key = screen.getByLabelText('Paste your API key')
    await user.type(key, 'sk-secret-123456')
    await user.click(screen.getByRole('button', { name: 'Save key' }))
    expect(await screen.findByText('API key saved.')).toBeInTheDocument()
    expect(screen.getByText(/A key is saved in the device’s secure storage/)).toBeInTheDocument()
    expect(screen.getByLabelText('Paste a new key to replace the saved one')).toHaveValue('')
    expect(document.body.innerHTML).not.toContain('sk-secret')
    await user.click(screen.getByRole('button', { name: 'Remove key' }))
    expect(await screen.findByText('No key saved.')).toBeInTheDocument()
    expectContract(calls, 'settingsAi')
    stop()
  })

  it('declining AI, telemetry and notifications does not block learning', async () => {
    const { mock, services, stop, user } = await open('#/settings/ai')
    await user.click(await screen.findByRole('radio', { name: 'Do not use AI' }))
    await waitFor(async () => expect((await mock.getSettings()).ai.consent).toBe('declined'))
    mock.setNotificationAnswer('denied')
    await mock.requestNotificationPermission({ operationId: 'deny' })
    expect((await mock.getSettings()).telemetryOptIn).toBe(false)

    await services.router.navigate({ name: 'home' })
    await user.click(await screen.findByRole('button', { name: 'Study everything' }))
    await user.click(await screen.findByRole('button', { name: 'Show answer' }))
    await user.click(within(screen.getByRole('group', { name: 'How well did you remember?' })).getByRole('button', { name: /^Good/ }))
    expect(await screen.findByText(/^1 review ·/)).toBeInTheDocument()
    stop()
  })
})

describe('privacy, reset and about', () => {
  it('resetting progress needs the phrase or a fresh backup and names its scope', async () => {
    const { calls, services, mock, stop, user } = await open('#/settings/privacy')
    await user.click(await screen.findByRole('button', { name: 'Reset progress…' }))
    const dialog = await screen.findByRole('dialog', { name: 'Reset learning progress' })
    expect(await within(dialog).findByText(/This removes the review history/)).toBeInTheDocument()
    expect(within(dialog).getByText(/cannot be undone in the app/)).toBeInTheDocument()
    const confirm = within(dialog).getByRole('button', { name: 'Reset progress' })
    expect(confirm).toBeDisabled()
    await user.type(within(dialog).getByRole('textbox', { name: /Or type “RESETUJ POSTĘP”/ }), 'RESETUJ POSTĘP')
    expect(confirm).toBeEnabled()
    await user.click(confirm)
    expect(await screen.findByText('Learning progress reset.')).toBeInTheDocument()
    await waitFor(() => expect(services.router.current.name).toBe('home'))
    const stats = await mock.getStats({ deckId: null, range: '1m' })
    expect(stats.cardStates.Review).toBe(0)
    expectContract(calls, 'privacy', 'home')
    stop()
  })

  it('deleting everything with a safety backup starts over in onboarding', async () => {
    const { services, mock, stop, user } = await open('#/settings/privacy')
    await user.click(await screen.findByRole('button', { name: 'Delete everything…' }))
    const dialog = await screen.findByRole('dialog', { name: 'Delete everything' })
    await user.click(await within(dialog).findByRole('button', { name: 'Create a safety backup' }))
    expect(await within(dialog).findByText('Safety backup created. You can confirm now.')).toBeInTheDocument()
    await user.click(within(dialog).getByRole('button', { name: 'Delete everything' }))
    await waitFor(() => expect(services.router.current.name).toBe('onboarding'))
    expect(await screen.findByText(/The safety backup is in Backup & restore/)).toBeInTheDocument()
    expect((await mock.getDashboard()).decks).toHaveLength(0)
    expect(await mock.listBackups()).toHaveLength(4)
    stop()
  })

  it('about lists versions and a redacted diagnostic log', async () => {
    const { calls, stop, user } = await open('#/settings/about')
    expect(await screen.findByText('Database schema')).toBeInTheDocument()
    expect(screen.getByText('Database schema').nextElementSibling).toHaveTextContent(/^none$/)
    expect(screen.getByText('FSRS-6')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Show diagnostic log' }))
    expect(await screen.findByText('3 values were hidden before the log left the app core.')).toBeInTheDocument()
    expect(screen.getByRole('list', { name: 'Diagnostic log' }).textContent).toContain('[redacted]')
    expectContract(calls, 'about')
    stop()
  })
})

describe('import and export', () => {
  it('previews a CSV, changes options, imports and reports the result', async () => {
    const { calls, mock, stop, user } = await open('#/settings/transfer')
    const before = (await mock.getDeck('1')).deck.counts.total
    await user.click(await screen.findByRole('button', { name: 'Choose a CSV file' }))
    const dry = await screen.findByRole('region', { name: 'Dry run — what would happen' })
    expect(within(dry).getByText('New notes').nextElementSibling).toHaveTextContent('12')
    expect(screen.getByText(/with byte order mark/)).toBeInTheDocument()
    await user.click(within(dry).getByText('1 row cannot be imported'))
    expect(within(dry).getByText('Line 9: Front is empty.')).toBeInTheDocument()

    await user.click(screen.getByRole('radio', { name: /Update them/ }))
    await waitFor(() => expect(within(region('Dry run — what would happen')).getByText('Updated').nextElementSibling).toHaveTextContent('2'))
    expect(screen.getByText(/2 existing notes will be overwritten/)).toBeInTheDocument()
    expect((await mock.getDeck('1')).deck.counts.total).toBe(before)

    await user.click(screen.getByRole('button', { name: 'Import 14 notes' }))
    expect(await screen.findByText('Import finished: 14 notes.')).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Open the deck' })).toHaveAttribute('href', '#/deck/1')
    expect((await mock.getDeck('1')).deck.counts.total).toBe(before + 12)
    expectContract(calls, 'transfer')
    stop()
  })

  it('explains APKG, treats a closed picker as no choice, and validates the mapping', async () => {
    const { mock, stop, user } = await open('#/settings/transfer')
    mock.setPickedFile('import', 'demo-import-apkg')
    await user.click(await screen.findByRole('button', { name: 'Choose a CSV file' }))
    expect(await screen.findByRole('heading', { name: 'Anki packages (.apkg) are not supported yet' })).toBeInTheDocument()
    expect(screen.getByText(/Notes in Plain Text/)).toBeInTheDocument()

    mock.setPickedFile('import', null)
    await user.click(screen.getByRole('button', { name: 'Choose another file' }))
    expect(screen.queryByRole('alert', { name: /error/i })).not.toBeInTheDocument()

    mock.setPickedFile('import', 'demo-import-comma')
    await user.click(screen.getByRole('button', { name: 'Choose another file' }))
    await screen.findByRole('region', { name: 'Dry run — what would happen' })
    await user.click(screen.getByRole('checkbox', { name: 'The first row contains column names' }))
    await waitFor(() => expect(screen.getByText('Column 1')).toBeInTheDocument())
    await user.selectOptions(screen.getByRole('combobox', { name: /Column 2/ }), 'front')
    expect(screen.getByText('Choose exactly one Front and one Back column.')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /^Import/ })).toBeDisabled()
    stop()
  })

  it('a cancelled import writes nothing', async () => {
    window.history.replaceState(null, '', '#/home')
    const mock = new MockAppApi({ jobStepMs: 40 })
    const services = await createTestServices({ language: 'en', api: mock })
    window.history.replaceState(null, '', '#/settings/transfer')
    const stop = services.router.start()
    renderWith(Shell, services)
    const user = userEvent.setup()
    const decks = (await mock.getDashboard()).decks.length
    await user.click(await screen.findByRole('button', { name: 'Choose a CSV file' }))
    await user.selectOptions(await screen.findByRole('combobox', { name: 'Import into' }), '')
    await waitFor(() => expect(screen.getByRole('button', { name: 'Import 14 notes' })).toBeEnabled())
    await user.click(screen.getByRole('button', { name: 'Import 14 notes' }))
    await user.click(await screen.findByRole('button', { name: 'Cancel' }))
    expect(await screen.findByText('Import cancelled — nothing was imported.')).toBeInTheDocument()
    expect((await mock.getDashboard()).decks).toHaveLength(decks)
    mock.dispose()
    stop()
  })

  it('exports a deck and shares the file; export is not a backup', async () => {
    const { calls, stop, user } = await open('#/settings/transfer')
    const exportRegion = await screen.findByRole('region', { name: 'Export a deck' })
    expect(within(exportRegion).getByRole('link', { name: 'Backup & restore' })).toBeInTheDocument()
    await user.click(within(exportRegion).getByRole('button', { name: 'Export deck' }))
    expect(await within(exportRegion).findByText(/is ready \(\d+ cards\)/)).toBeInTheDocument()
    await user.click(within(exportRegion).getByRole('button', { name: 'Share or save the file' }))
    expect(await screen.findByText('File shared.')).toBeInTheDocument()
    expectContract(calls, 'transfer')
    stop()
  })
})

describe('backup and restore', () => {
  it('creates a backup and changes where and how many are kept', async () => {
    const { calls, mock, stop, user } = await open('#/settings/backup')
    expect(await screen.findByText('3 backups')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Create backup' }))
    expect(await screen.findByText('Backup created.')).toBeInTheDocument()
    expect(await screen.findByText('4 backups')).toBeInTheDocument()
    await user.selectOptions(screen.getByRole('combobox', { name: 'Keep' }), '3')
    await waitFor(async () => expect((await mock.getSettings()).backup.retention).toBe(3))
    await user.click(screen.getByRole('button', { name: 'Choose a folder…' }))
    expect(await screen.findByRole('radio', { name: 'Folder: Dokumenty/LanguaLinker' })).toBeChecked()
    expectContract(calls, 'backup')
    stop()
  })

  it('restores a validated file after confirmation and reopens the collection', async () => {
    const { calls, services, stop, user } = await open('#/settings/backup')
    await user.click(await screen.findByRole('button', { name: 'Restore from a file…' }))
    const dialog = await screen.findByRole('dialog', { name: 'Restore this backup?' })
    expect(within(dialog).getByText('Restoring replaces your whole current collection with the backup.')).toBeInTheDocument()
    expect(within(dialog).getByText(/A safety copy of the current collection is made first/)).toBeInTheDocument()
    expect(within(dialog).getByRole('columnheader', { name: 'After restore' })).toBeInTheDocument()
    await user.click(within(dialog).getByRole('button', { name: 'Replace and restore' }))
    expect(await screen.findByText('Backup restored. The collection was reopened.')).toBeInTheDocument()
    await waitFor(() => expect(services.router.current.name).toBe('home'))
    expectContract(calls, 'backup', 'home')
    stop()
  })

  it('rejects a corrupt file without opening the dialog, and recovers from a failed restore', async () => {
    const { mock, stop, user } = await open('#/settings/backup')
    mock.setPickedFile('restore', 'demo-restore-corrupt')
    await user.click(await screen.findByRole('button', { name: 'Restore from a file…' }))
    expect(await screen.findByText('The backup is damaged. Your current collection was not changed.')).toBeInTheDocument()
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()

    mock.setPickedFile('restore', 'demo-restore-fails')
    await user.click(screen.getByRole('button', { name: 'Restore from a file…' }))
    const dialog = await screen.findByRole('dialog', { name: 'Restore this backup?' })
    await user.click(within(dialog).getByRole('button', { name: 'Replace and restore' }))
    expect(await within(dialog).findByText('The restore failed. Your current collection was not changed.')).toBeInTheDocument()
    await user.click(await within(dialog).findByRole('button', { name: 'Restore the safety copy instead' }))
    await waitFor(() => expect(within(dialog).getByRole('button', { name: 'Replace and restore' })).toBeEnabled())
    expect((await mock.listBackups()).some((backup) => backup.kind === 'safety')).toBe(true)
    stop()
  })
})

describe('onboarding', () => {
  it('goes L1 → L2 → goal → decks → ready and prepares only the chosen decks', async () => {
    const { calls, services, mock, stop, user } = await open('#/onboarding', 'newUser')
    expect(await screen.findByRole('heading', { level: 2, name: 'Which language do you speak?' })).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Continue' }))
    expect(await screen.findByRole('heading', { level: 2, name: 'What do you want to learn?' })).toHaveFocus()
    await user.click(screen.getByRole('checkbox', { name: 'German' }))
    expect(screen.getByText('Choose at least one language.')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Continue' })).toBeDisabled()
    await user.click(screen.getByRole('checkbox', { name: 'English' }))
    await user.click(screen.getByRole('button', { name: 'Continue' }))
    await user.click(await screen.findByRole('radio', { name: /30 cards a day/ }))
    await user.click(screen.getByRole('button', { name: 'Continue' }))
    expect(await screen.findByText('English — words A1')).toBeInTheDocument()
    expect(screen.queryByText('Niemiecki — słówka A1')).not.toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Continue' }))
    expect(await screen.findByText('1 deck to prepare')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Get started' }))
    await waitFor(() => expect(services.router.current.name).toBe('home'), { timeout: 3000 })
    const decks = (await mock.getDashboard()).decks
    expect(decks.every((deck) => deck.targetLanguage === 'en')).toBe(true)
    expect((await mock.getSettings()).dailyGoal).toBe(30)
    expectContract(calls, 'onboarding', 'home')
    stop()
  })

  it('opened from the settings, it adds missing decks and returns there', async () => {
    const { services, stop, user } = await open('#/settings')
    await user.click(await screen.findByRole('link', { name: 'Set up languages and decks again' }))
    expect(await screen.findByRole('heading', { level: 2, name: 'Which language do you speak?' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Back' })).toBeInTheDocument()
    for (let step = 0; step < 3; step += 1) await user.click(screen.getByRole('button', { name: 'Continue' }))
    expect(screen.getAllByText('Already in your collection').length).toBeGreaterThan(0)
    await user.click(screen.getByRole('button', { name: 'Continue' }))
    await user.click(screen.getByRole('button', { name: 'Save and add decks' }))
    await waitFor(() => expect(services.router.current.name).toBe('settings'), { timeout: 3000 })
    stop()
  })
})
