import { describe, expect, it, vi } from 'vitest'
import type { ImportInput, JobDto } from '../src/api/types'
import { MockAppApi } from '../src/demo/mockAppApi'

/** Runs a job to its end and returns the durable status (with `result`), as screens do. */
async function finish(api: MockAppApi, started: JobDto): Promise<JobDto> {
  let job = started
  await vi.waitFor(async () => {
    job = await api.getJobStatus(started.jobId)
    expect(['succeeded', 'failed', 'cancelled']).toContain(job.status)
  })
  return job
}

const importInput = (overrides: Partial<ImportInput> = {}): ImportInput => ({
  fileToken: 'demo-import-csv',
  delimiter: null,
  hasHeader: true,
  mapping: null,
  duplicatePolicy: 'skip',
  targetDeckId: '1',
  ...overrides,
})

describe('statistics', () => {
  it('reports every day of the range, a 30-day forecast, and "not enough data" as null', async () => {
    const api = new MockAppApi()
    const stats = await api.getStats({ deckId: null, range: '3m' })
    expect(stats.reviewsByDay).toHaveLength(90)
    expect(stats.forecast).toHaveLength(30)
    expect(stats.forecast[0]?.stored).toBeGreaterThanOrEqual(stats.overdue)
    expect(stats.streakDays).toBeLessThanOrEqual(stats.longestStreakDays)
    expect(stats.maturity.young + stats.maturity.mature).toBeLessThanOrEqual(stats.cardStates.Review)
    for (const ratio of [stats.trueRetention.young, stats.trueRetention.mature]) {
      if (ratio) expect(ratio.total).toBeGreaterThanOrEqual(10)
    }

    const empty = await new MockAppApi({ scenario: 'empty' }).getStats({ deckId: null, range: '1m' })
    expect(empty.trueRetention).toEqual({ young: null, mature: null })
    expect(empty.reviewsByDay.every((day) => day.count === 0)).toBe(true)
    expect(Object.values(empty.cardStates).every((count) => count === 0)).toBe(true)
  })

  it('keeps stored due dates and the forecast with limits apart', async () => {
    const api = new MockAppApi()
    const preset = (await api.listPresets()).find((item) => item.id === 'preset-default')
    if (!preset) throw new Error('default preset')
    await api.savePreset({ operationId: 'p', presetId: preset.id, expectedVersion: preset.version, name: preset.name, config: { ...preset.config, reviewsPerDay: 1 } })
    const stats = await api.getStats({ deckId: '1', range: '1m' })
    expect(stats.forecastAssumptions.reviewsPerDay).toBe(1)
    expect(stats.forecast.every((day) => day.projected <= 1)).toBe(true)
    const stored = stats.forecast.reduce((sum, day) => sum + day.stored, 0)
    const projected = stats.forecast.reduce((sum, day) => sum + day.projected, 0)
    expect(projected).toBeLessThanOrEqual(stored)
    await expect(api.getStats({ deckId: 'missing', range: '1m' })).rejects.toMatchObject({ dto: { code: 'NOT_FOUND' } })
  })
})

describe('settings, AI and notifications', () => {
  it('validates fields and protects against stale versions', async () => {
    const api = new MockAppApi()
    const settings = await api.getSettings()
    const update = (operationId: string, patch: object, expectedVersion = settings.version) => api.updateSettings({ operationId, expectedVersion, patch })
    await expect(update('a', { dailyGoal: 0 })).rejects.toMatchObject({ dto: { code: 'VALIDATION_FAILED', details: { field: 'dailyGoal' } } })
    await expect(update('b', { timeZone: 'Mars/Olympus' })).rejects.toMatchObject({ dto: { details: { field: 'timeZone' } } })
    await expect(update('c', { aiEndpoint: 'http://insecure.example' })).rejects.toMatchObject({ dto: { details: { field: 'aiEndpoint' } } })
    await expect(update('d', { backupTarget: 'external' })).rejects.toMatchObject({ dto: { details: { field: 'backupTarget' } } })
    const saved = await update('e', { dayCutoffHour: 5, ttsRate: 1.2, timeZone: 'Europe/Berlin' })
    expect(saved).toMatchObject({ dayCutoffHour: 5, ttsRate: 1.2, timeZone: 'Europe/Berlin', version: settings.version + 1 })
    await expect(update('f', { dailyGoal: 30 })).rejects.toMatchObject({ dto: { code: 'VERSION_CONFLICT' } })
  })

  it('never returns the API key, drops it on a provider change, and needs it to generate', async () => {
    const api = new MockAppApi()
    let settings = await api.updateSettings({ operationId: 'p', expectedVersion: 1, patch: { aiProvider: 'anthropic' } })
    expect(settings.ai).toMatchObject({ provider: 'anthropic', model: 'claude-haiku-5-5', hasApiKey: false })
    const noteId = (await api.searchCards({ query: 'Kühlschrank', deckId: null, tag: null, states: [], flags: [], sort: 'created', direction: 'asc', cursor: null, pageSize: 1 })).items[0]?.noteId ?? ''
    await expect(api.generateAssociation({ operationId: 'g1', noteId, variants: 1 })).rejects.toMatchObject({ dto: { code: 'PERMISSION_DENIED', details: { reason: 'apiKey' } } })

    const ai = await api.setAiApiKey({ operationId: 'k', apiKey: 'sk-demo-1234567890' })
    expect(ai.hasApiKey).toBe(true)
    expect(JSON.stringify(await api.getSettings())).not.toContain('sk-demo')
    await expect(api.generateAssociation({ operationId: 'g2', noteId, variants: 1 })).resolves.toMatchObject({ kind: 'llm' })
    await expect(api.setAiApiKey({ operationId: 'bad', apiKey: 'short' })).rejects.toMatchObject({ dto: { code: 'VALIDATION_FAILED' } })

    settings = await api.getSettings()
    settings = await api.updateSettings({ operationId: 'p2', expectedVersion: settings.version, patch: { aiProvider: 'openaiCompatible' } })
    expect(settings.ai).toMatchObject({ hasApiKey: false, model: null })
  })

  it('declining AI blocks only generation, with a reason', async () => {
    const api = new MockAppApi()
    await api.updateSettings({ operationId: 'c', expectedVersion: 1, patch: { aiConsent: 'declined' } })
    const session = await api.startStudy({ operationId: 's', deckId: '1' })
    const card = session.current
    if (!card) throw new Error('due card')
    await expect(api.generateAssociation({ operationId: 'g', noteId: card.noteId, variants: 1 })).rejects.toMatchObject({ dto: { code: 'PERMISSION_DENIED', details: { reason: 'aiConsent' } } })
    const preview = await api.getRatingPreview({ sessionId: session.sessionId, cardId: card.cardId, cardVersion: card.version })
    await expect(api.rateCard({ operationId: 'r', sessionId: session.sessionId, cardId: card.cardId, cardVersion: card.version, previewToken: preview.previewToken, rating: 'Good', answerMs: 2000 })).resolves.toMatchObject({ reviewedCount: 1 })
  })

  it('a refused notification prompt is a state, and it does not come back', async () => {
    const api = new MockAppApi()
    api.setNotificationAnswer('denied')
    expect((await api.requestNotificationPermission({ operationId: 'n1' })).notifications).toBe('denied')
    api.setNotificationAnswer('granted')
    expect((await api.requestNotificationPermission({ operationId: 'n2' })).notifications).toBe('denied')
  })
})

describe('onboarding again', () => {
  it('adds only missing bundled decks, with fresh ids, and keeps the collection', async () => {
    const api = new MockAppApi()
    const before = await api.getDashboard()
    const deck = await api.getDeck('3')
    await api.deleteDeck({ operationId: 'd', deckId: '3', expectedVersion: deck.deck.version })
    const state = await api.getOnboardingState()
    expect(state.completed).toBe(true)
    expect(state.installedDeckGuids).not.toContain('deck-en-words-a1')
    const job = await finish(api, await api.completeOnboarding({ operationId: 'o', nativeLanguage: 'pl', targetLanguages: ['de', 'en'], dailyGoal: 25, deckGuids: state.availableDecks.map((item) => item.guid) }))
    expect(job.result).toMatchObject({ type: 'prepareContent' })
    const after = await api.getDashboard()
    expect(after.decks).toHaveLength(before.decks.length)
    const english = after.decks.find((item) => item.guid === 'deck-en-words-a1')
    expect(english?.id).not.toBe('3')
    expect(english?.counts.total).toBeGreaterThan(0)
    expect((await api.getSettings()).dailyGoal).toBe(25)
  })

  it('rejects a deck in a language the user did not choose', async () => {
    const api = new MockAppApi({ scenario: 'newUser' })
    await expect(api.completeOnboarding({ operationId: 'o', nativeLanguage: 'pl', targetLanguages: ['de'], dailyGoal: 20, deckGuids: ['deck-en-words-a1'] })).rejects.toMatchObject({ dto: { code: 'VALIDATION_FAILED' } })
  })
})

describe('reset and delete', () => {
  it('requires a fresh safety backup or the exact phrase', async () => {
    const api = new MockAppApi()
    const preview = await api.previewReset({ scope: 'progress' })
    expect(preview.counts.reviews).toBeGreaterThan(0)
    await expect(api.resetCollection({ operationId: 'r1', previewToken: preview.previewToken, safetyBackupId: 'backup-1', confirmationPhrase: null })).rejects.toMatchObject({ dto: { code: 'VALIDATION_FAILED' } })
    await expect(api.resetCollection({ operationId: 'r2', previewToken: preview.previewToken, safetyBackupId: null, confirmationPhrase: 'wrong' })).rejects.toMatchObject({ dto: { code: 'VALIDATION_FAILED' } })
    const backup = await finish(api, await api.createBackup({ operationId: 'b' }))
    const backupId = backup.result?.type === 'backup' ? backup.result.backupId : null
    const job = await finish(api, await api.resetCollection({ operationId: 'r3', previewToken: preview.previewToken, safetyBackupId: backupId, confirmationPhrase: null }))
    expect(job.result).toEqual({ type: 'reset', safetyBackupId: backupId })
    const stats = await api.getStats({ deckId: null, range: '1m' })
    expect(stats.cardStates.Review + stats.cardStates.Learning + stats.cardStates.Relearning).toBe(0)
    expect((await api.getDashboard()).decks.length).toBeGreaterThan(0)
  })

  it('deleting everything keeps backups, so the safety copy restores the collection', async () => {
    const api = new MockAppApi()
    const cards = (await api.getDashboard()).decks.reduce((sum, deck) => sum + (deck.parentId === null ? deck.counts.total : 0), 0)
    const preview = await api.previewReset({ scope: 'everything' })
    expect(preview.confirmationPhrase).toBe('USUŃ WSZYSTKO')
    await finish(api, await api.resetCollection({ operationId: 'x', previewToken: preview.previewToken, safetyBackupId: null, confirmationPhrase: ' USUŃ WSZYSTKO ' }))
    expect((await api.getDashboard()).decks).toHaveLength(0)
    expect((await api.getOnboardingState()).completed).toBe(false)
    const backups = await api.listBackups()
    expect(backups).toHaveLength(3)
    const restore = await api.validateRestore({ source: { kind: 'backup', backupId: backups[0]?.id ?? '' } })
    expect(restore.current.cards).toBe(0)
    await finish(api, await api.restoreBackup({ operationId: 'rs', previewToken: restore.previewToken }))
    const restored = (await api.getDashboard()).decks.reduce((sum, deck) => sum + (deck.parentId === null ? deck.counts.total : 0), 0)
    expect(restored).toBe(cards)
  })
})

describe('CSV import and export', () => {
  it('previews without writing, applies the duplicate policy and reports row errors', async () => {
    const api = new MockAppApi()
    const before = (await api.getDeck('1')).deck.counts.total
    const skip = await api.previewImport(importInput())
    expect(skip).toMatchObject({ delimiter: ';', hasBom: true, totalRows: 15, mapping: ['front', 'back', 'tags'], counts: { create: 12, skip: 2, update: 0, duplicate: 0, error: 1 } })
    expect(skip.rowErrors).toEqual([expect.objectContaining({ row: 9, error: expect.objectContaining({ details: { field: 'front', reason: 'required' } }) })])
    expect((await api.previewImport(importInput({ duplicatePolicy: 'update' }))).counts).toMatchObject({ update: 2, skip: 0 })
    expect((await api.getDeck('1')).deck.counts.total).toBe(before)

    // A wrong delimiter splits rows at the wrong place: all but the row with a comma lack a back.
    const wrong = await api.previewImport(importInput({ delimiter: ',' }))
    expect(wrong.counts).toMatchObject({ create: 1, error: 14 })
    await expect(api.previewImport(importInput({ mapping: ['front', 'front', 'tags'] }))).rejects.toMatchObject({ dto: { code: 'VALIDATION_FAILED', details: { field: 'mapping' } } })
    await expect(api.previewImport(importInput({ fileToken: 'demo-import-apkg' }))).rejects.toMatchObject({ dto: { code: 'UNSUPPORTED_FORMAT', details: { format: 'apkg' } } })
  })

  it('commits the previewed plan, rejects a stale preview, and a cancelled import writes nothing', async () => {
    const api = new MockAppApi()
    const before = (await api.getDeck('1')).deck.counts.total
    const preview = await api.previewImport(importInput())
    const job = await finish(api, await api.commitImport({ operationId: 'i', previewToken: preview.previewToken }))
    expect(job.result).toMatchObject({ type: 'import', deckId: '1', create: 12, skip: 2, error: 1 })
    expect((await api.getDeck('1')).deck.counts.total).toBe(before + 12)
    await expect(api.commitImport({ operationId: 'i2', previewToken: preview.previewToken })).rejects.toMatchObject({ dto: { code: 'PREVIEW_STALE' } })

    const slow = new MockAppApi({ jobStepMs: 50 })
    const decks = (await slow.getDashboard()).decks.length
    const next = await slow.previewImport(importInput({ fileToken: 'demo-import-comma', hasHeader: false, targetDeckId: null }))
    expect(next.targetDeckName).toBe('english-phrases')
    const started = await slow.commitImport({ operationId: 'c', previewToken: next.previewToken })
    await slow.cancelJob(started.jobId)
    expect((await finish(slow, started)).status).toBe('cancelled')
    await new Promise((resolve) => setTimeout(resolve, 120))
    expect((await slow.getDashboard()).decks).toHaveLength(decks)
    slow.dispose()
  })

  it('exports a deck and shares only a finished export', async () => {
    const api = new MockAppApi()
    const job = await finish(api, await api.exportDeck({ operationId: 'e', deckId: '5', format: 'csv', includeScheduling: true }))
    expect(job.result).toEqual({ type: 'export', fileName: 'zlozenia-komposita.csv', cardCount: 3 })
    await expect(api.shareFile({ operationId: 's', target: { kind: 'export', jobId: job.jobId } })).resolves.toEqual({ shared: true })
    await expect(api.shareFile({ operationId: 's2', target: { kind: 'export', jobId: 'job-missing' } })).rejects.toMatchObject({ dto: { code: 'NOT_FOUND' } })
  })
})

describe('backup and restore', () => {
  it('keeps the newest backups by retention, never the safety copies', async () => {
    const api = new MockAppApi()
    const settings = await api.getSettings()
    await api.updateSettings({ operationId: 'r', expectedVersion: settings.version, patch: { backupRetention: 2 } })
    await finish(api, await api.createBackup({ operationId: 'b1' }))
    const list = await api.listBackups()
    expect(list).toHaveLength(2)
    expect(list[0]?.kind).toBe('manual')
  })

  it('validates files: corrupt and newer backups are rejected before anything changes', async () => {
    const api = new MockAppApi()
    await expect(api.validateRestore({ source: { kind: 'file', fileToken: 'demo-restore-corrupt' } })).rejects.toMatchObject({ dto: { code: 'BACKUP_CORRUPT' } })
    await expect(api.validateRestore({ source: { kind: 'file', fileToken: 'demo-restore-newer' } })).rejects.toMatchObject({ dto: { code: 'SCHEMA_TOO_NEW' } })
  })

  it('a failed restore leaves the collection and reports the safety copy', async () => {
    const api = new MockAppApi()
    const note = await api.saveNote({ operationId: 'n', noteId: null, expectedVersion: null, deckId: '4', noteTypeKind: 'Basic', front: 'good night', back: 'dobranoc', tags: [], manualAssociation: null, grammarTips: [], wordLinks: null })
    const preview = await api.validateRestore({ source: { kind: 'file', fileToken: 'demo-restore-fails' } })
    const job = await finish(api, await api.restoreBackup({ operationId: 'r', previewToken: preview.previewToken }))
    expect(job.status).toBe('failed')
    expect(job.error?.code).toBe('BACKUP_CORRUPT')
    const safetyId = job.result?.type === 'restore' ? job.result.safetyBackupId : ''
    expect((await api.listBackups()).find((backup) => backup.id === safetyId)?.kind).toBe('safety')
    await expect(api.getNoteDetails(note.cards[0]?.id ?? '')).resolves.toMatchObject({ note: { front: 'good night' } })
  })

  it('a successful restore replaces the collection with the backup', async () => {
    const api = new MockAppApi()
    const note = await api.saveNote({ operationId: 'n', noteId: null, expectedVersion: null, deckId: '4', noteTypeKind: 'Basic', front: 'good night', back: 'dobranoc', tags: [], manualAssociation: null, grammarTips: [], wordLinks: null })
    const preview = await api.validateRestore({ source: { kind: 'file', fileToken: 'demo-restore-ok' } })
    expect(preview.current.notes).toBe(preview.counts.notes + 1)
    await finish(api, await api.restoreBackup({ operationId: 'r', previewToken: preview.previewToken }))
    await expect(api.getNoteDetails(note.cards[0]?.id ?? '')).rejects.toMatchObject({ dto: { code: 'NOT_FOUND' } })
  })
})
