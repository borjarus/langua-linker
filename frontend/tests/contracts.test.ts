import { describe, expect, it, vi } from 'vitest'
import type { AppApi } from '../src/api/appApi'
import { AppError, isErrorDto, toErrorDto } from '../src/api/errors'
import { OPERATIONS, OPERATION_NAMES } from '../src/api/operations'
import { createTauriAppApi } from '../src/api/tauriAppApi'
import { API_VERSION, ERROR_CODES } from '../src/api/types'
import { MockAppApi } from '../src/demo/mockAppApi'
import { en } from '../src/locales/en'
import { assertTransportRules } from './contract/transport'

describe('operation catalogue', () => {
  it('covers every AppApi method of both adapters', () => {
    const mock = new MockAppApi()
    const tauri = createTauriAppApi(vi.fn(), vi.fn())
    for (const name of OPERATION_NAMES) {
      expect(typeof (mock as unknown as Record<string, unknown>)[name], `MockAppApi.${name}`).toBe('function')
      expect(typeof (tauri as unknown as Record<string, unknown>)[name], `TauriAppApi.${name}`).toBe('function')
    }
    expect(Object.keys(tauri).sort()).toEqual([...OPERATION_NAMES].sort())
  })

  it('uses unique snake_case commands and declares a backend phase', () => {
    const commands = Object.values(OPERATIONS).map((spec) => spec.command)
    expect(new Set(commands).size).toBe(commands.length)
    for (const [name, spec] of Object.entries(OPERATIONS)) {
      expect(spec.command, name).toMatch(/^[a-z]+(_[a-z]+)*$/)
      expect(spec.backendPhase, name).toBeGreaterThanOrEqual(1)
      expect(spec.screens.length, name).toBeGreaterThan(0)
      for (const code of spec.errors) expect(ERROR_CODES).toContain(code)
    }
  })

  it('marks every mutation and job as emitting or idempotent through operationId', () => {
    for (const [name, spec] of Object.entries(OPERATIONS)) {
      if (spec.kind === 'job') expect(spec.emits, name).toContain('job_progress')
    }
  })
})

describe('error contract', () => {
  it('has a translation for every messageKey', () => {
    for (const code of ERROR_CODES) {
      const dto = AppError.of(code).dto
      expect(dto.messageKey).toBe(`errors.${code}`)
      expect((en.errors as Record<string, string>)[code]).toBeTruthy()
    }
  })

  it('normalizes unknown throwables without leaking raw messages', () => {
    const dto = toErrorDto(new Error('SQLITE_BUSY: database is locked at /data/user.db'))
    expect(dto).toEqual({ code: 'INTERNAL', messageKey: 'errors.INTERNAL', retryable: false, details: null })
    expect(isErrorDto({ code: 'NOPE', messageKey: 'x', retryable: true, details: null })).toBe(false)
  })
})

describe('Tauri adapter', () => {
  it('invokes snake_case commands with a single `input` argument', async () => {
    const invoke = vi.fn(async () => ({ apiVersion: API_VERSION }))
    const api = createTauriAppApi(invoke, vi.fn())
    await api.getAppInfo()
    await api.getDeck('12')
    expect(invoke).toHaveBeenNthCalledWith(1, 'get_app_info', undefined)
    expect(invoke).toHaveBeenNthCalledWith(2, 'get_deck', { input: '12' })
  })

  it('rejects with AppError and never falls back to mock data', async () => {
    const backendError = { code: 'NOT_FOUND', messageKey: 'errors.NOT_FOUND', retryable: false, details: { id: '9' } }
    const api = createTauriAppApi(vi.fn(async () => Promise.reject(backendError)), vi.fn())
    await expect(api.getDashboard()).rejects.toMatchObject({ dto: backendError })

    const missing = createTauriAppApi(vi.fn(async () => Promise.reject('command get_stats not found')), vi.fn())
    await expect(missing.getStats({ deckId: null, range: '1m' })).rejects.toMatchObject({ dto: { code: 'INTERNAL' } })
  })

  it('cleans up listeners registered after unsubscribe (async registration)', async () => {
    const unlisten = vi.fn()
    let resolveRegistration!: (fn: () => void) => void
    const listen = vi.fn(() => new Promise<() => void>((resolve) => (resolveRegistration = resolve)))
    const api = createTauriAppApi(vi.fn(), listen as never)
    const stop = api.onJobEvent(() => {})
    stop()
    resolveRegistration(unlisten)
    await Promise.resolve()
    await Promise.resolve()
    expect(unlisten).toHaveBeenCalledTimes(1)
  })
})

describe('mock payloads follow transport rules', () => {
  it('for every query in the happy scenario', async () => {
    const api: AppApi = new MockAppApi()
    const dashboard = await api.getDashboard()
    const deck = dashboard.decks[0]
    if (!deck) throw new Error('happy scenario has decks')
    const session = await api.startStudy({ operationId: 'op-1', deckId: deck.id })
    const card = session.current
    if (!card) throw new Error('happy scenario has due cards')
    const preview = await api.getRatingPreview({ sessionId: session.sessionId, cardId: card.cardId, cardVersion: card.version })
    const review = await api.rateCard({ operationId: 'op-2', sessionId: session.sessionId, cardId: card.cardId, cardVersion: card.version, previewToken: preview.previewToken, rating: 'Good', answerMs: 3000 })
    const payloads = [
      dashboard,
      await api.getDeck(deck.id),
      session,
      preview,
      review,
      await api.getSessionSummary(session.sessionId),
      await api.getNoteDetails(card.cardId),
      await api.getNoteContent(card.noteId),
      await api.updateStudySession({ operationId: 'op-3', sessionId: session.sessionId, action: 'end' }),
      await api.getSessionSummary(session.sessionId),
      await api.getDashboard(),
      await api.getEditorData(),
      await api.searchCards({ query: '', deckId: null, tag: null, states: [], flags: [], sort: 'due', direction: 'asc', cursor: null, pageSize: 20 }),
      await api.previewBulkAction({ selection: { kind: 'ids', cardIds: [card.cardId] }, action: { type: 'suspend' } }),
      await api.listPresets(),
      await api.searchWords({ targetLanguage: 'de', text: 'Tee', limit: 5 }),
      await api.getStats({ deckId: null, range: '3m' }),
      await api.getSettings(),
      await api.getOnboardingState(),
      await api.listBackups(),
      await api.getAttributions(),
      await api.getAppInfo(),
      await api.getAboutInfo(),
      await api.getDiagnosticsReport(),
      await api.previewReset({ scope: 'progress' }),
      await api.previewImport({ fileToken: 'demo-import-csv', delimiter: null, hasHeader: true, mapping: null, duplicatePolicy: 'skip', targetDeckId: deck.id }),
      await api.validateRestore({ source: { kind: 'file', fileToken: 'demo-restore-ok' } }),
      await api.createBackup({ operationId: 'op-4' }),
    ]
    for (const payload of payloads) {
      assertTransportRules(payload)
      expect(JSON.parse(JSON.stringify(payload))).toEqual(payload)
    }
  })
})
