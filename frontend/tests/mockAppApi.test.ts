import { describe, expect, it, vi } from 'vitest'
import type { JobEventDto, SearchInput } from '../src/api/types'
import { MockAppApi, parseSearch } from '../src/demo/mockAppApi'

const search = (overrides: Partial<SearchInput> = {}): SearchInput => ({
  query: '',
  deckId: null,
  states: [],
  flags: [],
  sort: 'due',
  direction: 'asc',
  cursor: null,
  pageSize: 50,
  ...overrides,
})

async function startSession(api: MockAppApi) {
  const session = await api.startStudy({ operationId: 'start', deckId: null })
  const card = session.current
  if (!card) throw new Error('expected a due card')
  const preview = await api.getRatingPreview({ sessionId: session.sessionId, cardId: card.cardId, cardVersion: card.version })
  return { session, card, preview }
}

describe('MockAppApi determinism', () => {
  it('renders identical data for the same seed', async () => {
    const a = await new MockAppApi({ seed: 7 }).getDashboard()
    const b = await new MockAppApi({ seed: 7 }).getDashboard()
    expect(a).toEqual(b)
  })

  it('resets demo memory when a scenario is selected', async () => {
    const api = new MockAppApi()
    await api.createDeck({ operationId: 'c', name: 'Moja talia', parentId: null, targetLanguage: 'de', deckType: 'Simple' })
    expect((await api.getDashboard()).decks.some((deck) => deck.name === 'Moja talia')).toBe(true)
    api.setScenario('happy')
    expect((await api.getDashboard()).decks.some((deck) => deck.name === 'Moja talia')).toBe(false)
  })
})

describe('MockAppApi study flow', () => {
  it('commits a rating once per operationId (retry returns the same result)', async () => {
    const api = new MockAppApi()
    const { session, card, preview } = await startSession(api)
    const input = { operationId: 'rate-1', sessionId: session.sessionId, cardId: card.cardId, cardVersion: card.version, previewToken: preview.previewToken, rating: 'Good' as const, answerMs: 2500 }
    const first = await api.rateCard(input)
    const second = await api.rateCard(input)
    expect(second).toEqual(first)
    expect(first.reviewedCount).toBe(1)
    expect((await api.getSessionSummary(session.sessionId)).reviewedCount).toBe(1)
  })

  it('rejects stale previews and stale card versions', async () => {
    const api = new MockAppApi()
    const { session, card, preview } = await startSession(api)
    const base = { sessionId: session.sessionId, cardId: card.cardId, rating: 'Hard' as const, answerMs: 1000 }
    await expect(api.rateCard({ ...base, operationId: 'a', cardVersion: card.version, previewToken: 'stale' })).rejects.toMatchObject({ dto: { code: 'PREVIEW_STALE', retryable: true } })
    await expect(api.rateCard({ ...base, operationId: 'b', cardVersion: card.version + 1, previewToken: preview.previewToken })).rejects.toMatchObject({ dto: { code: 'VERSION_CONFLICT' } })
  })

  it('undo restores the card as current and removes the rating from the summary', async () => {
    const api = new MockAppApi()
    const { session, card, preview } = await startSession(api)
    await api.rateCard({ operationId: 'r', sessionId: session.sessionId, cardId: card.cardId, cardVersion: card.version, previewToken: preview.previewToken, rating: 'Again', answerMs: 1000 })
    const undone = await api.undoReview({ operationId: 'u', sessionId: session.sessionId })
    expect(undone.next?.cardId).toBe(card.cardId)
    expect(undone.next?.version).toBeGreaterThan(card.version)
    expect(undone.reviewedCount).toBe(0)
    const summary = await api.getSessionSummary(session.sessionId)
    expect(summary.ratings.Again).toBe(0)
  })

  it('finishes when no further card exists and reports an empty session for empty collections', async () => {
    const api = new MockAppApi({ scenario: 'empty' })
    const session = await api.startStudy({ operationId: 's', deckId: null })
    expect(session.status).toBe('finished')
    expect(session.current).toBeNull()
  })

  it('injects a one-off failure for save-error flows', async () => {
    const api = new MockAppApi()
    const { session, card, preview } = await startSession(api)
    api.failNext('rateCard', 'DB_BUSY')
    const input = { operationId: 'x', sessionId: session.sessionId, cardId: card.cardId, cardVersion: card.version, previewToken: preview.previewToken, rating: 'Good' as const, answerMs: 1000 }
    await expect(api.rateCard(input)).rejects.toMatchObject({ dto: { code: 'DB_BUSY' } })
    await expect(api.rateCard(input)).resolves.toMatchObject({ operationId: 'x' })
  })
})

describe('MockAppApi scenarios', () => {
  it('error: queries fail with a retryable DB_BUSY while the shell still loads', async () => {
    const api = new MockAppApi({ scenario: 'error' })
    await expect(api.getDashboard()).rejects.toMatchObject({ dto: { code: 'DB_BUSY', retryable: true } })
    await expect(api.getSettings()).resolves.toMatchObject({ uiLanguage: 'pl' })
  })

  it('loading: queries never settle', async () => {
    const api = new MockAppApi({ scenario: 'loading' })
    const outcome = await Promise.race([api.getDashboard().then(() => 'settled'), new Promise((resolve) => setTimeout(() => resolve('pending'), 20))])
    expect(outcome).toBe('pending')
  })

  it('offline: generation is queued, local content still works', async () => {
    const api = new MockAppApi({ scenario: 'offline' })
    const { card } = await startSession(api)
    const job = await api.generateAssociation({ operationId: 'g', noteId: card.noteId, variants: 1 })
    expect(job.status).toBe('queued')
    const details = await api.getNoteDetails(card.cardId)
    expect(details.associations.some((association) => association.status === 'offlineQueued')).toBe(true)
  })

  it('quota: generation is rejected with QUOTA_EXCEEDED', async () => {
    const api = new MockAppApi({ scenario: 'quota' })
    const { card } = await startSession(api)
    await expect(api.generateAssociation({ operationId: 'g', noteId: card.noteId, variants: 1 })).rejects.toMatchObject({ dto: { code: 'QUOTA_EXCEEDED' } })
  })

  it('ambiguous: sentence links require a choice and resolve to userOverridden', async () => {
    const api = new MockAppApi({ scenario: 'ambiguous' })
    const rows = await api.searchCards(search({ query: 'Tee' }))
    const sentence = rows.items.find((row) => row.front.startsWith('Ich trinke'))
    if (!sentence) throw new Error('fixture sentence missing')
    const details = await api.getNoteDetails(sentence.cardId)
    const link = details.wordLinks.find((candidate) => candidate.resolution === 'ambiguous')
    expect(link?.candidates.length).toBeGreaterThan(0)
    const candidate = link?.candidates[0]
    if (!link || !candidate) throw new Error('expected ambiguous link')
    const resolved = await api.resolveWordLink({ operationId: 'w', linkId: link.id, wordNoteId: candidate.noteId })
    expect(resolved.resolution).toBe('userOverridden')
  })

  it('large: pages 50k cards with a stable cursor and total', async () => {
    const api = new MockAppApi({ scenario: 'large' })
    const first = await api.searchCards(search({ pageSize: 100 }))
    expect(first.total).toBe(50_000)
    expect(first.items).toHaveLength(100)
    const second = await api.searchCards(search({ pageSize: 100, cursor: first.nextCursor }))
    expect(second.items[0]?.cardId).not.toBe(first.items[0]?.cardId)
    const last = await api.searchCards(search({ pageSize: 100, cursor: '49950' }))
    expect(last.items).toHaveLength(50)
    expect(last.nextCursor).toBeNull()
  })

  it('newUser: onboarding job emits ordered progress and prepares only chosen decks', async () => {
    const api = new MockAppApi({ scenario: 'newUser' })
    expect((await api.getOnboardingState()).completed).toBe(false)
    expect((await api.getDashboard()).decks).toHaveLength(0)
    const events: JobEventDto[] = []
    api.onJobEvent((event) => events.push(event))
    const state = await api.getOnboardingState()
    const german = state.availableDecks.filter((deck) => deck.targetLanguage === 'de').map((deck) => deck.guid)
    await api.completeOnboarding({ operationId: 'o', targetLanguages: ['de'], dailyGoal: 15, deckGuids: german })
    await vi.waitFor(() => expect(events.at(-1)?.status).toBe('succeeded'))
    const sequences = events.map((event) => event.sequence)
    expect(sequences).toEqual([...sequences].sort((a, b) => a - b))
    const decks = (await api.getDashboard()).decks
    expect(decks.length).toBeGreaterThan(0)
    expect(decks.every((deck) => deck.targetLanguage === 'de')).toBe(true)
  })
})

describe('MockAppApi content management', () => {
  it('creates two independent cards for a reversed note and protects against stale saves', async () => {
    const api = new MockAppApi()
    const saved = await api.saveNote({ operationId: 'n1', noteId: null, expectedVersion: null, deckId: '4', noteTypeKind: 'BasicAndReversed', front: 'good night', back: 'dobranoc', tags: [], manualAssociation: null })
    expect(saved.cards).toHaveLength(2)
    expect(new Set(saved.cards.map((card) => card.direction))).toEqual(new Set(['forward', 'reverse']))
    await expect(
      api.saveNote({ operationId: 'n2', noteId: saved.note.id, expectedVersion: saved.note.version + 5, deckId: '4', noteTypeKind: 'Basic', front: 'x', back: 'y', tags: [], manualAssociation: null }),
    ).rejects.toMatchObject({ dto: { code: 'VERSION_CONFLICT' } })
  })

  it('validates required fields', async () => {
    const api = new MockAppApi()
    await expect(api.saveNote({ operationId: 'v', noteId: null, expectedVersion: null, deckId: '1', noteTypeKind: 'Basic', front: '  ', back: 'x', tags: [], manualAssociation: null })).rejects.toMatchObject({ dto: { code: 'VALIDATION_FAILED', details: { field: 'front' } } })
  })

  it('detects duplicates with Unicode normalization', async () => {
    const api = new MockAppApi()
    const decomposed = 'der Kühlschrank'
    const result = await api.checkDuplicate({ deckId: '1', front: ` ${decomposed.toUpperCase()} `, excludeNoteId: null })
    expect(result.duplicates).toHaveLength(1)
  })

  it('keeps hidden sections when the deck type changes (hide, never delete)', async () => {
    const api = new MockAppApi()
    const before = await api.getDeck('1')
    const after = await api.updateDeck({ operationId: 'u', deckId: '1', expectedVersion: before.deck.version, patch: { deckType: 'Simple' } })
    expect(after.hiddenSectionCounts.associations).toBeGreaterThan(0)
    const restored = await api.updateDeck({ operationId: 'u2', deckId: '1', expectedVersion: after.deck.version, patch: { deckType: 'Linguistic' } })
    expect(restored.hiddenSectionCounts.associations).toBe(0)
  })

  it('applies bulk actions over a filter snapshot and rejects stale snapshots', async () => {
    const api = new MockAppApi()
    const page = await api.searchCards(search({ query: 'deck:niemiecki' }))
    const result = await api.applyBulkAction({ operationId: 'b', selection: { kind: 'query', filterVersion: page.filterVersion, excludedCardIds: [] }, action: { type: 'suspend' } })
    expect(result.affected).toBe(page.total)
    await expect(
      api.applyBulkAction({ operationId: 'b2', selection: { kind: 'query', filterVersion: page.filterVersion, excludedCardIds: [] }, action: { type: 'unsuspend' } }),
    ).rejects.toMatchObject({ dto: { code: 'PREVIEW_STALE' } })
  })

  it('parses the search grammar and reports parser errors', () => {
    expect(parseSearch('deck:Niemiecki tag:"dom" is:due Tee')).toEqual({ text: 'tee', deck: 'niemiecki', tag: 'dom', is: 'due' })
    expect(() => parseSearch('"unbalanced')).toThrowError()
    expect(() => parseSearch('foo:bar')).toThrowError()
  })
})
