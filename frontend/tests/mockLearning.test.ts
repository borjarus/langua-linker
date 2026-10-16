import { describe, expect, it, vi } from 'vitest'
import type { JobEventDto } from '../src/api/types'
import { MockAppApi } from '../src/demo/mockAppApi'

async function rateCurrent(api: MockAppApi, sessionId: string, rating: 'Again' | 'Good', operationId: string) {
  const session = await api.resumeStudy(sessionId)
  const card = session.current
  if (!card) throw new Error('expected a current card')
  const preview = await api.getRatingPreview({ sessionId, cardId: card.cardId, cardVersion: card.version })
  return api.rateCard({ operationId, sessionId, cardId: card.cardId, cardVersion: card.version, previewToken: preview.previewToken, rating, answerMs: 4_000 })
}

const dueOf = (deck: { counts: { new: number; learning: number; review: number } }) => deck.counts.new + deck.counts.learning + deck.counts.review

describe('MockAppApi learning flow (Phase 3)', () => {
  it('offers the latest unfinished session for resume and closes it when a new one starts', async () => {
    const api = new MockAppApi()
    const first = await api.startStudy({ operationId: 'a', deckId: '1' })
    await rateCurrent(api, first.sessionId, 'Good', 'r1')
    expect((await api.getDashboard()).resumable).toMatchObject({
      sessionId: first.sessionId,
      deckId: '1',
      deckName: 'Niemiecki — słówka A1',
      status: 'active',
      reviewedCount: 1,
    })

    const second = await api.startStudy({ operationId: 'b', deckId: null })
    expect((await api.getDashboard()).resumable?.sessionId).toBe(second.sessionId)
    expect((await api.resumeStudy(first.sessionId)).status).toBe('finished')
  })

  it('pauses, resumes and ends a session; a paused session rejects ratings', async () => {
    const api = new MockAppApi()
    const session = await api.startStudy({ operationId: 's', deckId: '1' })
    expect((await api.updateStudySession({ operationId: 'p', sessionId: session.sessionId, action: 'pause' })).status).toBe('paused')
    expect((await api.getDashboard()).resumable?.status).toBe('paused')
    await expect(rateCurrent(api, session.sessionId, 'Good', 'r')).rejects.toMatchObject({ dto: { code: 'VALIDATION_FAILED' } })

    await api.updateStudySession({ operationId: 'c', sessionId: session.sessionId, action: 'resume' })
    await rateCurrent(api, session.sessionId, 'Good', 'r2')
    const ended = await api.updateStudySession({ operationId: 'e', sessionId: session.sessionId, action: 'end' })
    expect(ended).toMatchObject({ status: 'finished', current: null, reviewedCount: 1 })
    expect((await api.getDashboard()).resumable).toBeNull()
    // Ending twice is harmless; resuming a finished session is a validation error.
    expect((await api.updateStudySession({ operationId: 'e2', sessionId: session.sessionId, action: 'end' })).status).toBe('finished')
    await expect(api.updateStudySession({ operationId: 'r3', sessionId: session.sessionId, action: 'resume' })).rejects.toMatchObject({
      dto: { code: 'VALIDATION_FAILED' },
    })
  })

  it('summarises Again cards, remaining work and the next deck; only the first finished session offers a reminder', async () => {
    const api = new MockAppApi()
    const session = await api.startStudy({ operationId: 's', deckId: '3' })
    await rateCurrent(api, session.sessionId, 'Again', 'r1')
    await rateCurrent(api, session.sessionId, 'Good', 'r2')
    await api.updateStudySession({ operationId: 'e', sessionId: session.sessionId, action: 'end' })
    const summary = await api.getSessionSummary(session.sessionId)
    expect(summary).toMatchObject({ deckName: 'English — words A1', status: 'finished', reviewedCount: 2, firstCompleted: true })
    expect(summary.ratings).toEqual({ Again: 1, Hard: 0, Good: 1, Easy: 0 })
    expect(summary.againCards).toHaveLength(1)
    expect(summary.againCards[0]).toMatchObject({ targetLanguage: 'en', direction: 'forward' })
    expect(summary.remainingDue).toBeGreaterThan(0)
    expect(summary.nextDeck?.id).not.toBe('3')
    expect(summary.nextDeck?.due).toBeGreaterThan(0)

    const again = await api.startStudy({ operationId: 'again', deckId: '3', cardIds: summary.againCards.map((card) => card.cardId) })
    expect(again.counts.total).toBe(1)
    expect(again.current?.cardId).toBe(summary.againCards[0]?.cardId)
    await rateCurrent(api, again.sessionId, 'Good', 'r3')
    expect((await api.getSessionSummary(again.sessionId)).firstCompleted).toBe(false)
  })

  it('returns note content for the word sheet', async () => {
    const api = new MockAppApi()
    const session = await api.startStudy({ operationId: 's', deckId: '2' })
    const link = session.current?.wordLinks.find((candidate) => candidate.wordNoteId !== null)
    if (!link?.wordNoteId) throw new Error('expected a resolved word link')
    const content = await api.getNoteContent(link.wordNoteId)
    expect(content.noteId).toBe(link.wordNoteId)
    expect(content.capabilities.grammarTips).toBe(true)
    await expect(api.getNoteContent('nope')).rejects.toMatchObject({ dto: { code: 'NOT_FOUND' } })
  })
})

describe('MockAppApi dashboard scenarios (Phase 3)', () => {
  it('noCards: decks exist without cards (not "no decks", not "nothing due")', async () => {
    const dashboard = await new MockAppApi({ scenario: 'noCards' }).getDashboard()
    expect(dashboard.decks.length).toBeGreaterThan(0)
    expect(dashboard.decks.every((deck) => deck.counts.total === 0)).toBe(true)
  })

  it('caughtUp: cards exist, nothing is due today and the next review date is known', async () => {
    const api = new MockAppApi({ scenario: 'caughtUp' })
    const dashboard = await api.getDashboard()
    expect(dashboard.decks.some((deck) => deck.counts.total > 0)).toBe(true)
    expect(dashboard.decks.every((deck) => dueOf(deck) === 0)).toBe(true)
    expect(dashboard.nextDueAt).not.toBeNull()
    expect((await api.startStudy({ operationId: 's', deckId: null })).status).toBe('finished')
  })

  it('preparing: a deck is preparing, then becomes ready and announces it', async () => {
    vi.useFakeTimers()
    try {
      const api = new MockAppApi({ scenario: 'preparing' })
      const changed = vi.fn()
      api.onCollectionChanged(changed)
      expect((await api.getDashboard()).decks.find((deck) => deck.id === '3')?.contentState).toBe('preparing')
      await vi.runAllTimersAsync()
      expect(changed).toHaveBeenCalledWith(expect.objectContaining({ scopes: ['decks'] }))
      expect((await api.getDashboard()).decks.find((deck) => deck.id === '3')?.contentState).toBe('ready')
    } finally {
      vi.useRealTimers()
    }
  })

  it('aiErrors: failed associations carry distinct error codes and generation jobs fail', async () => {
    const api = new MockAppApi({ scenario: 'aiErrors' })
    const codes = new Set<string>()
    for (const deckId of ['1', '2', '3']) {
      const session = await api.startStudy({ operationId: `s-${deckId}`, deckId })
      for (const association of session.current?.associations ?? []) if (association.error) codes.add(association.error.code)
    }
    expect(codes.size).toBeGreaterThan(1)
    expect([...codes].every((code) => ['PROVIDER_ERROR', 'INVALID_PROVIDER_OUTPUT', 'MODERATION_BLOCKED'].includes(code))).toBe(true)

    const session = await api.startStudy({ operationId: 's', deckId: '1' })
    const noteId = session.current?.noteId ?? ''
    const events: JobEventDto[] = []
    api.onJobEvent((event) => events.push(event))
    const job = await api.generateAssociation({ operationId: 'g', noteId, variants: 1 })
    await vi.waitFor(() => expect(events.at(-1)?.status).toBe('failed'))
    expect((await api.getJobStatus(job.jobId)).error?.code).toBe('PROVIDER_ERROR')
    const content = await api.getNoteContent(noteId)
    expect(content.associations.some((association) => association.status === 'error' && association.error?.code === 'PROVIDER_ERROR')).toBe(true)
  })
})
