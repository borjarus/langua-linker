import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { screen, waitFor } from '@testing-library/svelte'
import { beforeEach, describe, expect, it } from 'vitest'
import { createTauriAppApi, type InvokeFn } from '../src/api/tauriAppApi'
import Shell from '../src/app/Shell.svelte'
import { en } from '../src/locales/en'
import type { ContractSamples } from './contract/samples'
import { createTestServices, renderWith } from './helpers/services'

// Phase 6 DoD: "adapter replacement does not require rewriting Svelte components". Every screen
// renders through the production adapter, answered with the frozen v1 command payloads, without
// demo services — so no demo banner and no prototype notice.

const samples = JSON.parse(readFileSync(resolve(process.cwd(), '../crates/contracts/v1/samples.json'), 'utf8')) as ContractSamples

/** Answers each command with its first successful frozen payload, like a Rust backend would. */
const invoke: InvokeFn = async (command) => {
  const sample = samples.calls.find((call) => call.command === command && !call.error)
  if (!sample) throw new Error(`command ${command} not found`)
  return structuredClone(sample.output)
}

const argOf = (operation: string): string => String(samples.calls.find((call) => call.operation === operation)?.args[0])

const NOTICES = [
  en.demo.banner,
  en.common.prototype,
  en.common.prototypeManagement,
  en.editor.demoDraft,
  en.stats.prototype,
  en.settings.prototype,
  en.ai.prototype,
  en.transfer.prototype,
  en.backup.prototype,
]

beforeEach(() => {
  document.body.innerHTML = ''
})

describe('every screen runs on the production adapter unchanged', () => {
  it.each([
    ['#/home', en.dashboard.title, 'Niemiecki — słówka A1'],
    ['#/deck/1', 'Niemiecki — słówka A1', 'Złożenia (Komposita)'],
    ['#/study/1', en.routes.study, en.routes.study],
    [`#/summary/${argOf('getSessionSummary')}`, en.routes.summary, 'Niemiecki — słówka A1'],
    [`#/card/${argOf('getNoteDetails')}`, en.routes.card, 'Associations'],
    ['#/editor?deckId=1', en.editor.newNote, 'Question'],
    ['#/browser', en.routes.browser, 'matching cards'],
    ['#/stats', en.routes.stats, 'True retention — mature cards'],
    ['#/settings', en.routes.settings, 'App language'],
    ['#/settings/ai', en.routes.settingsAi, 'Allow generating associations with AI'],
    ['#/settings/privacy', en.routes.settingsPrivacy, 'Share anonymous usage statistics'],
    ['#/settings/about', en.routes.settingsAbout, 'Interface contract'],
    ['#/settings/transfer', en.routes.settingsTransfer, 'Choose a CSV file'],
    ['#/settings/backup', en.routes.settingsBackup, 'backups'],
    ['#/settings/attributions', en.routes.settingsAttributions, 'Svelte'],
    ['#/onboarding', en.routes.onboarding, 'Which language do you speak?'],
  ])('%s', async (hash, title, content) => {
    window.history.replaceState(null, '', hash)
    const services = await createTestServices({ language: 'en', api: createTauriAppApi(invoke, async () => () => {}) })
    services.profile = 'release'
    services.demo = null
    const stop = services.router.start()
    renderWith(Shell, services)
    await waitFor(() => expect(document.querySelector('[data-screen-heading]')).toHaveTextContent(title))
    await waitFor(() => expect(screen.getAllByText(content, { exact: false }).length).toBeGreaterThan(0))
    const text = document.body.textContent ?? ''
    for (const notice of NOTICES) expect(text).not.toContain(notice)
    expect(text).not.toContain(en.errors.INTERNAL)
    expect(screen.queryByRole('combobox', { name: en.demo.scenario })).not.toBeInTheDocument()
    stop()
  })
})
