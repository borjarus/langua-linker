import { screen, waitFor } from '@testing-library/svelte'
import { beforeEach, describe, expect, it } from 'vitest'
import type { UiLanguage } from '../src/api/types'
import Shell from '../src/app/Shell.svelte'
import type { DemoScenario } from '../src/demo/scenarios'
import { createTestServices, renderWith } from './helpers/services'

async function renderAt(hash: string, options: { scenario?: DemoScenario; language?: UiLanguage } = {}) {
  window.history.replaceState(null, '', hash)
  const services = await createTestServices({ language: options.language ?? 'en', scenario: options.scenario })
  const stop = services.router.start()
  const result = renderWith(Shell, services)
  return { services, stop, ...result }
}

const heading = () => document.querySelector('[data-screen-heading]')

describe('every route opens its screen with the correct mock state', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
  })

  it.each([
    ['#/home', 'Your decks', 'Niemiecki — słówka A1'],
    ['#/deck/1', 'Niemiecki — słówka A1', 'Złożenia (Komposita)'],
    ['#/card/1000', 'Card details', 'Associations'],
    ['#/editor?deckId=1', 'New note', 'Question'],
    ['#/browser', 'Browse cards', 'matching cards'],
    ['#/stats', 'Statistics', 'True retention — mature cards'],
    ['#/settings', 'Settings', 'App language'],
    ['#/settings/transfer', 'Import & export', 'Choose a CSV file'],
    ['#/settings/backup', 'Backup & restore', '3 backups'],
    ['#/settings/attributions', 'Licenses & attributions', 'Svelte'],
    ['#/onboarding', 'Welcome', 'Available decks'],
    ['#/gallery', 'Component gallery', 'Virtual list (50,000 rows)'],
  ])('%s', async (hash, title, content) => {
    const { stop } = await renderAt(hash)
    await waitFor(() => expect(heading()).toHaveTextContent(title))
    await waitFor(() => expect(screen.getAllByText(content, { exact: false }).length).toBeGreaterThan(0))
    stop()
  })

  it('#/study/:deckId without a session offers an explicit start', async () => {
    const { stop } = await renderAt('#/study/1')
    await waitFor(() => expect(heading()).toHaveTextContent('Study'))
    expect(screen.getByRole('button', { name: 'Study' })).toBeInTheDocument()
    stop()
  })

  it('#/study/:deckId?session resumes a session started by a click', async () => {
    window.history.replaceState(null, '', '#/home')
    const services = await createTestServices({ language: 'en' })
    const session = await services.api.startStudy({ operationId: 's', deckId: '1' })
    window.history.replaceState(null, '', `#/study/1?session=${session.sessionId}`)
    const stop = services.router.start()
    renderWith(Shell, services)
    await waitFor(() => expect(screen.getByRole('button', { name: 'Show answer' })).toBeInTheDocument())
    stop()
  })

  it('#/summary/:sessionId shows an empty state for a session without ratings', async () => {
    const services = await createTestServices({ language: 'en' })
    const session = await services.api.startStudy({ operationId: 's', deckId: '1' })
    window.history.replaceState(null, '', `#/summary/${session.sessionId}`)
    const stop = services.router.start()
    renderWith(Shell, services)
    await waitFor(() => expect(heading()).toHaveTextContent('Session summary'))
    await waitFor(() => expect(screen.getByText('Nothing here yet')).toBeInTheDocument())
    stop()
  })

  it('unknown paths render the not-found flow', async () => {
    const { stop } = await renderAt('#/does-not-exist')
    await waitFor(() => expect(heading()).toHaveTextContent('Page not found'))
    expect(screen.getByRole('link', { name: 'Go to decks' })).toHaveAttribute('href', '#/home')
    stop()
  })
})

describe('scenario states render on real screens', () => {
  it('empty collection shows a call to action, not a dead end', async () => {
    const { stop } = await renderAt('#/home', { scenario: 'empty' })
    await waitFor(() => expect(screen.getByText('No decks yet')).toBeInTheDocument())
    expect(screen.getByRole('button', { name: 'Add decks' })).toBeInTheDocument()
    stop()
  })

  it('error scenario shows a retryable error', async () => {
    const { stop } = await renderAt('#/home', { scenario: 'error' })
    expect(await screen.findByText(/The collection is busy/)).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument()
    stop()
  })

  it('loading scenario shows an accessible skeleton', async () => {
    const { stop } = await renderAt('#/home', { scenario: 'loading' })
    await waitFor(() => expect(document.querySelector('main [role="status"][aria-busy="true"]')).toBeInTheDocument())
    stop()
  })

  it('large scenario reports 50k results in the browser', async () => {
    const { stop } = await renderAt('#/browser', { scenario: 'large' })
    await waitFor(() => expect(screen.getByText('50,000 matching cards')).toBeInTheDocument())
    stop()
  })

  it('query-only navigation keeps focus in the search field; a new screen focuses its heading', async () => {
    const { services, stop } = await renderAt('#/browser')
    const search = await screen.findByRole('searchbox')
    search.focus()
    await services.router.replace({ name: 'browser', query: { q: 'Haus' } })
    await new Promise((resolve) => setTimeout(resolve, 20))
    expect(window.location.hash).toBe('#/browser?q=Haus')
    expect(document.activeElement).toBe(search)

    await services.router.navigate({ name: 'stats' })
    await waitFor(() => expect(document.activeElement).toBe(heading()))
    expect(heading()).toHaveTextContent('Statistics')
    stop()
  })

  it('shell shows the demo banner and primary navigation in Polish', async () => {
    const { stop } = await renderAt('#/home', { language: 'pl' })
    expect(screen.getByText(/Tryb demonstracyjny/)).toBeInTheDocument()
    const nav = screen.getByRole('navigation', { name: 'Nawigacja główna' })
    expect(nav.querySelector('[aria-current="page"]')).toHaveTextContent('Talie')
    stop()
  })
})
