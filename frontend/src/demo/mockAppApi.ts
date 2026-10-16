import type { AppApi, AppApiOperation } from '../api/appApi'
import { AppError } from '../api/errors'
import {
  API_VERSION,
  RATINGS,
  type AssociationDto,
  type AttributionDto,
  type BackupDto,
  type BulkActionInput,
  type BulkResultDto,
  type CardActionInput,
  type CardActionResultDto,
  type CardDetailsDto,
  type CardPageDto,
  type CardRowDto,
  type CardState,
  type CollectionChangedEventDto,
  type CollectionScope,
  type CommitImportInput,
  type CompleteOnboardingInput,
  type CreateDeckInput,
  type DashboardDto,
  type DeckCountsDto,
  type DeckOverviewDto,
  type DeckSummaryDto,
  type DeleteDeckInput,
  type DeleteDeckResultDto,
  type DuplicateCheckDto,
  type DuplicateCheckInput,
  type EditorDataDto,
  type ErrorCode,
  type FileHandleDto,
  type GenerateAssociationInput,
  type GrammarTipDto,
  type Id,
  type ImportInput,
  type ImportPreviewDto,
  type JobDto,
  type JobEventDto,
  type JobKind,
  type NoteContentDto,
  type NoteDetailsDto,
  type OnboardingStateDto,
  type PickFileInput,
  type PreviewInput,
  type QueueState,
  type RateCardInput,
  type Rating,
  type RatingPreviewDto,
  type ResolveWordLinkInput,
  type RestoreInput,
  type RestorePreviewDto,
  type ResumableSessionDto,
  type ReviewResultDto,
  type SaveNoteInput,
  type SearchInput,
  type SessionCardDto,
  type SessionSummaryDto,
  type SetFavoriteAssociationInput,
  type SettingsDto,
  type StartStudyInput,
  type StatsDto,
  type StatsInput,
  type StudyCardDto,
  type StudySessionDto,
  type UndoInput,
  type Unsubscribe,
  type UpdateDeckInput,
  type UpdateSettingsInput,
  type UpdateStudySessionInput,
  type ValidateRestoreInput,
  type WordLinkDto,
} from '../api/types'
import {
  BUNDLED_DECKS,
  DEMO_EPOCH_MS,
  LARGE_COLLECTION_SIZE,
  capabilitiesFor,
  createCollection,
  largeRow,
  type CardRecord,
  type Collection,
  type DeckRecord,
  type NoteRecord,
} from './fixtures'
import { DAY_MS, HOUR_MS, createRandom, iso } from './random'
import type { DemoScenario } from './scenarios'

export interface MockAppApiOptions {
  scenario?: DemoScenario
  seed?: number
  nowMs?: number
  /** Simulated IPC latency; 0 in tests. */
  latencyMs?: number
  /** Delay between job progress steps; 0 in tests. */
  jobStepMs?: number
}

interface SessionRating {
  operationId: string
  cardId: string
  rating: Rating
  answerMs: number
  undone: boolean
}

interface UndoEntry {
  operationId: string
  card: CardRecord
  cursor: number
  queueLength: number
  logId: string
}

interface SessionRecord {
  id: string
  deckId: string | null
  queue: string[]
  cursor: number
  status: 'active' | 'paused' | 'finished'
  startedAtMs: number
  finishedAtMs: number | null
  ratings: SessionRating[]
  undo: UndoEntry[]
}

/** Outcomes cycled by the `aiErrors` scenario: provider error, unusable output, moderation. */
const AI_FAILURES = ['PROVIDER_ERROR', 'INVALID_PROVIDER_OUTPUT', 'MODERATION_BLOCKED'] as const

const SHELL_OPERATIONS: ReadonlySet<AppApiOperation> = new Set(['getAppInfo', 'getSettings', 'getOnboardingState', 'getAttributions'])
const MAX_FIELD_LENGTH = 2000
const PRESET_VERSION = 1
const CUTOFF_HOUR = 4
const EMPTY_COLLECTION = (): Collection => ({ decks: [], notes: [], cards: [], associations: [], tips: [], wordLinks: [], logs: [] })

const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T
const normalizeText = (text: string): string => text.normalize('NFC').trim().toLocaleLowerCase('pl')
const fail = (code: ErrorCode, details: Record<string, string | number | boolean | null> | null = null): never => {
  throw AppError.of(code, details)
}

export class MockAppApi implements AppApi {
  readonly #options: Required<Omit<MockAppApiOptions, 'scenario'>>
  #scenario: DemoScenario = 'happy'
  #nowMs = DEMO_EPOCH_MS
  #collection: Collection = EMPTY_COLLECTION()
  #pendingCollection: Collection | null = null
  #sessions = new Map<string, SessionRecord>()
  #operations = new Map<string, unknown>()
  #failures = new Map<AppApiOperation, ErrorCode>()
  #jobs = new Map<string, JobDto>()
  #jobTimers = new Map<string, ReturnType<typeof setTimeout>>()
  #jobListeners = new Set<(event: JobEventDto) => void>()
  #collectionListeners = new Set<(event: CollectionChangedEventDto) => void>()
  #filterSnapshots = new Map<string, string[]>()
  /** Decks whose bundled content is still being prepared (`preparing` scenario). */
  #preparing = new Set<string>()
  #firstCompletedSessionId: string | null = null
  #generations = 0
  #settings!: SettingsDto
  #onboardingCompleted = true
  #backups: BackupDto[] = []
  #importPreviewToken: string | null = null
  #restorePreviewToken: string | null = null
  #revision = 0
  #sequence = 1

  constructor(options: MockAppApiOptions = {}) {
    this.#options = {
      seed: options.seed ?? 42,
      nowMs: options.nowMs ?? DEMO_EPOCH_MS,
      latencyMs: options.latencyMs ?? 0,
      jobStepMs: options.jobStepMs ?? 0,
    }
    this.setScenario(options.scenario ?? 'happy')
  }

  // ------------------------------------------------------------ demo controls

  get scenario(): DemoScenario {
    return this.#scenario
  }

  /** Resets all demo memory; nothing persists across scenarios. */
  setScenario(scenario: DemoScenario): void {
    this.dispose()
    this.#scenario = scenario
    this.#nowMs = this.#options.nowMs
    this.#sessions.clear()
    this.#operations.clear()
    this.#failures.clear()
    this.#jobs.clear()
    this.#filterSnapshots.clear()
    this.#revision = 0
    this.#sequence = 1
    this.#importPreviewToken = null
    this.#restorePreviewToken = null
    this.#preparing = new Set()
    this.#firstCompletedSessionId = null
    this.#generations = 0

    const full = createCollection({ seed: this.#options.seed, nowMs: this.#nowMs, ambiguousLinks: scenario === 'ambiguous' })
    this.#onboardingCompleted = scenario !== 'newUser'
    this.#pendingCollection = scenario === 'newUser' ? full : null
    this.#collection = scenario === 'empty' || scenario === 'newUser' ? EMPTY_COLLECTION() : full
    if (scenario === 'offline') {
      for (const association of this.#collection.associations.filter((_, index) => index % 4 === 1)) {
        association.status = 'offlineQueued'
        association.favorite = false
      }
    }
    if (scenario === 'noCards') {
      // Decks exist but hold no cards yet: distinct from "no decks" and from "nothing due today".
      this.#collection = { ...EMPTY_COLLECTION(), decks: full.decks }
    }
    if (scenario === 'caughtUp') {
      // Every card is scheduled after today: "no reviews due today", not "no cards".
      for (const card of this.#collection.cards) {
        if (card.state !== 'Review') {
          Object.assign(card, { state: 'Review', queue: 'Review', intervalSeconds: 3 * 86_400, stability: card.stability ?? 3, difficulty: card.difficulty ?? 5, reps: Math.max(card.reps, 1) })
        }
        card.dueAtMs = this.#nowMs + (2 + (Number(card.id) % 9)) * DAY_MS
      }
    }
    if (scenario === 'aiErrors') {
      this.#collection.notes.forEach((note, index) => {
        const errorCode = AI_FAILURES[index % AI_FAILURES.length] ?? 'PROVIDER_ERROR'
        this.#collection.associations.push({
          id: this.#nextId(), noteId: note.id, text: '', provenance: 'generated', favorite: false,
          status: errorCode === 'MODERATION_BLOCKED' ? 'blocked' : 'error', errorCode,
        })
      })
    }
    this.#settings = {
      uiLanguage: 'pl',
      nativeLanguage: 'pl',
      theme: 'system',
      dailyGoal: 20,
      timeZone: 'Europe/Warsaw',
      dayCutoffHour: CUTOFF_HOUR,
      notifications: 'prompt',
      reminderTime: null,
      ttsEnabled: true,
      ai: { consent: 'granted', provider: 'demo', model: 'demo-mnemonic-v1', endpoint: null, hasApiKey: false },
      telemetryOptIn: false,
      version: 1,
    }
    this.#backups = scenario === 'empty' || scenario === 'newUser'
      ? []
      : [3, 10, 17].map((daysAgo, index) => ({
          id: `backup-${index + 1}`,
          createdAt: iso(this.#nowMs - daysAgo * DAY_MS),
          sizeBytes: 182_000 + index * 9_000,
          schemaVersion: 1,
          cardCount: this.#collection.cards.length,
          location: index === 0 ? 'external' : 'app',
        }))
    if (scenario === 'preparing') {
      // The English deck finishes preparing in the background and announces it via collection_changed.
      this.#preparing.add('3')
      this.#startJob('prepareContent', 6, () => this.#preparing.clear(), ['decks'])
    }
  }

  failNext(operation: AppApiOperation, code: ErrorCode): void {
    this.#failures.set(operation, code)
  }

  dispose(): void {
    for (const timer of this.#jobTimers.values()) clearTimeout(timer)
    this.#jobTimers.clear()
  }

  // ------------------------------------------------------------ plumbing

  async #query<T>(operation: AppApiOperation, run: () => T): Promise<T> {
    await this.#gate(operation)
    if (this.#scenario === 'error' && !SHELL_OPERATIONS.has(operation)) fail('DB_BUSY')
    return clone(run())
  }

  async #mutate<T>(operation: AppApiOperation, operationId: string, scopes: CollectionScope[], run: () => T): Promise<T> {
    await this.#gate(operation)
    if (!operationId) fail('VALIDATION_FAILED', { field: 'operationId' })
    const key = `${operation}:${operationId}`
    if (this.#operations.has(key)) return clone(this.#operations.get(key) as T)
    const result = run()
    this.#operations.set(key, clone(result))
    this.#revision += 1
    if (scopes.length > 0) this.#emitCollectionChanged(scopes)
    return clone(result)
  }

  async #gate(operation: AppApiOperation): Promise<void> {
    if (this.#scenario === 'loading' && !SHELL_OPERATIONS.has(operation)) {
      await new Promise<never>(() => {})
    }
    if (this.#options.latencyMs > 0) {
      await new Promise((resolve) => setTimeout(resolve, this.#options.latencyMs))
    } else {
      await Promise.resolve()
    }
    const injected = this.#failures.get(operation)
    if (injected) {
      this.#failures.delete(operation)
      fail(injected)
    }
  }

  #nextId(prefix = ''): string {
    this.#sequence += 1
    return `${prefix}${90_000 + this.#sequence}`
  }

  #emitCollectionChanged(scopes: CollectionScope[]): void {
    const event: CollectionChangedEventDto = { scopes, changedAt: iso(this.#nowMs) }
    queueMicrotask(() => {
      for (const listener of this.#collectionListeners) listener(clone(event))
    })
  }

  #emitJob(job: JobDto): void {
    const event: JobEventDto = { jobId: job.jobId, kind: job.kind, sequence: job.sequence, status: job.status, progress: job.progress }
    for (const listener of this.#jobListeners) listener(clone(event))
  }

  /** `failWith` ends the job as `failed` (after `onDone` ran), e.g. a provider error in `aiErrors`. */
  #startJob(kind: JobKind, steps: number, onDone?: () => void, scopes: CollectionScope[] = [], failWith?: ErrorCode): JobDto {
    const job: JobDto = {
      jobId: this.#nextId('job-'),
      kind,
      status: 'queued',
      sequence: 0,
      progress: { done: 0, total: steps },
      startedAt: iso(this.#nowMs),
      finishedAt: null,
      error: null,
    }
    this.#jobs.set(job.jobId, job)
    const step = () => {
      const current = this.#jobs.get(job.jobId)
      if (!current || current.status === 'cancelled') return
      const done = (current.progress?.done ?? 0) + 1
      const finished = done >= steps
      const next: JobDto = {
        ...current,
        status: finished ? (failWith ? 'failed' : 'succeeded') : 'running',
        sequence: current.sequence + 1,
        progress: { done: Math.min(done, steps), total: steps },
        finishedAt: finished ? iso(this.#nowMs) : null,
        error: finished && failWith ? AppError.of(failWith).dto : null,
      }
      this.#jobs.set(job.jobId, next)
      if (finished) {
        this.#jobTimers.delete(job.jobId)
        onDone?.()
        this.#revision += 1
        if (scopes.length > 0) this.#emitCollectionChanged(scopes)
      } else {
        this.#jobTimers.set(job.jobId, setTimeout(step, this.#options.jobStepMs))
      }
      this.#emitJob(next)
    }
    this.#jobTimers.set(job.jobId, setTimeout(step, this.#options.jobStepMs))
    queueMicrotask(() => this.#emitJob(job))
    return job
  }

  #studyDayStart(): number {
    const shifted = this.#nowMs - CUTOFF_HOUR * HOUR_MS
    return shifted - (shifted % DAY_MS) + CUTOFF_HOUR * HOUR_MS
  }

  #deck(deckId: Id): DeckRecord {
    return this.#collection.decks.find((deck) => deck.id === deckId) ?? fail('NOT_FOUND', { entity: 'deck', id: deckId })
  }

  #subtree(deckId: Id): Set<string> {
    const ids = new Set([deckId])
    let grew = true
    while (grew) {
      grew = false
      for (const deck of this.#collection.decks) {
        if (deck.parentId && ids.has(deck.parentId) && !ids.has(deck.id)) {
          ids.add(deck.id)
          grew = true
        }
      }
    }
    return ids
  }

  #liveCards(): CardRecord[] {
    return this.#collection.cards.filter((card) => !card.deleted)
  }

  #isStudyable(card: CardRecord): boolean {
    if (this.#preparing.has(card.deckId)) return false
    if (card.deleted || card.queue === 'Suspended' || card.queue === 'UserBuried' || card.queue === 'SchedBuried') return false
    if (card.state === 'New') return true
    return (card.dueAtMs ?? 0) <= this.#studyDayStart() + DAY_MS
  }

  #counts(deckIds: Set<string>): DeckCountsDto {
    const counts: DeckCountsDto = { new: 0, learning: 0, review: 0, overdue: 0, total: 0 }
    const dayStart = this.#studyDayStart()
    for (const card of this.#liveCards()) {
      if (!deckIds.has(card.deckId)) continue
      counts.total += 1
      if (!this.#isStudyable(card)) continue
      if (card.state === 'New') counts.new += 1
      else if (card.state === 'Review') {
        counts.review += 1
        if ((card.dueAtMs ?? 0) < dayStart) counts.overdue += 1
      } else counts.learning += 1
    }
    return counts
  }

  #deckSummary(deck: DeckRecord): DeckSummaryDto {
    const ids = this.#subtree(deck.id)
    let counts = this.#counts(ids)
    let progress = 0
    const cards = this.#liveCards().filter((card) => ids.has(card.deckId))
    if (cards.length > 0) progress = cards.filter((card) => card.intervalSeconds >= 21 * 86_400).length / cards.length
    if (this.#scenario === 'large' && deck.id === '1') {
      counts = { new: 4_812, learning: 37, review: 1_204, overdue: 311, total: LARGE_COLLECTION_SIZE }
      progress = 0.42
    }
    return {
      id: deck.id,
      guid: deck.guid,
      name: deck.name,
      parentId: deck.parentId,
      targetLanguage: deck.targetLanguage,
      nativeLanguage: 'pl',
      deckType: deck.deckType,
      capabilities: capabilitiesFor(deck.deckType),
      counts,
      progress: Math.round(progress * 100) / 100,
      bundled: deck.bundled,
      contentState: this.#preparing.has(deck.id) ? 'preparing' : 'ready',
      version: deck.version,
    }
  }

  #note(noteId: Id): NoteRecord {
    return this.#collection.notes.find((note) => note.id === noteId) ?? fail('NOT_FOUND', { entity: 'note', id: noteId })
  }

  #card(cardId: Id): CardRecord {
    const card = this.#collection.cards.find((candidate) => candidate.id === cardId) ?? fail('NOT_FOUND', { entity: 'card', id: cardId })
    if (card.deleted) fail('DELETED', { entity: 'card', id: cardId })
    return card
  }

  #associations(noteId: Id): AssociationDto[] {
    return this.#collection.associations
      .filter((association) => association.noteId === noteId)
      .map(({ errorCode, ...association }) => ({
        ...association,
        model: association.provenance === 'generated' ? this.#settings.ai.model : null,
        error:
          association.status === 'error' || association.status === 'blocked'
            ? AppError.of(errorCode ?? (association.status === 'blocked' ? 'MODERATION_BLOCKED' : 'PROVIDER_ERROR')).dto
            : null,
      }))
  }

  #tips(noteId: Id): GrammarTipDto[] {
    return this.#collection.tips.filter((tip) => tip.noteId === noteId).sort((a, b) => a.order - b.order)
  }

  #wordLinks(noteId: Id): WordLinkDto[] {
    return this.#collection.wordLinks
      .filter((link) => link.sentenceNoteId === noteId)
      .map((link) => {
        const target = link.wordNoteId ? this.#collection.notes.find((note) => note.id === link.wordNoteId) : undefined
        return {
          id: link.id,
          sentenceNoteId: link.sentenceNoteId,
          surface: link.surface,
          startIndex: link.startIndex,
          endIndex: link.endIndex,
          lemma: link.lemma,
          resolution: link.resolution,
          wordNoteId: link.wordNoteId,
          translation: target?.back ?? null,
          candidates: link.candidateNoteIds.flatMap((id) => {
            const candidate = this.#collection.notes.find((note) => note.id === id)
            return candidate ? [{ noteId: candidate.id, lemma: candidate.front, translation: candidate.back }] : []
          }),
        }
      })
  }

  #studyCard(card: CardRecord): StudyCardDto {
    const note = this.#note(card.noteId)
    const deck = this.#deck(card.deckId)
    const reverse = card.ordinal === 1
    return {
      cardId: card.id,
      noteId: note.id,
      deckId: deck.id,
      version: card.version,
      deckType: deck.deckType,
      capabilities: capabilitiesFor(deck.deckType),
      direction: reverse ? 'reverse' : 'forward',
      state: card.state,
      flag: card.flag,
      front: reverse ? note.back : note.front,
      back: reverse ? note.front : note.back,
      targetLanguage: deck.targetLanguage,
      associations: this.#associations(note.id),
      grammarTips: this.#tips(note.id),
      wordLinks: this.#wordLinks(note.id),
    }
  }

  #session(sessionId: Id): SessionRecord {
    return this.#sessions.get(sessionId) ?? fail('NOT_FOUND', { entity: 'session', id: sessionId })
  }

  #deckName(deckId: Id | null): string | null {
    return deckId === null ? null : (this.#collection.decks.find((deck) => deck.id === deckId)?.name ?? null)
  }

  #validRatings(session: SessionRecord): SessionRating[] {
    return session.ratings.filter((rating) => !rating.undone)
  }

  #finish(session: SessionRecord): void {
    if (session.status === 'finished') return
    session.status = 'finished'
    session.finishedAtMs = this.#nowMs
    if (this.#firstCompletedSessionId === null && this.#validRatings(session).length > 0) this.#firstCompletedSessionId = session.id
  }

  /** Advances past cards that were suspended/buried/deleted after the queue was built. */
  #currentCard(session: SessionRecord): CardRecord | null {
    while (session.cursor < session.queue.length) {
      const id = session.queue[session.cursor]
      const card = this.#collection.cards.find((candidate) => candidate.id === id)
      if (card && !card.deleted && card.queue !== 'Suspended' && card.queue !== 'UserBuried') return card
      session.cursor += 1
    }
    this.#finish(session)
    return null
  }

  #sessionCounts(session: SessionRecord): DeckCountsDto {
    const remaining = session.queue.slice(session.cursor).flatMap((id) => {
      const card = this.#collection.cards.find((candidate) => candidate.id === id)
      return card ? [card] : []
    })
    return {
      new: remaining.filter((card) => card.state === 'New').length,
      learning: remaining.filter((card) => card.state === 'Learning' || card.state === 'Relearning').length,
      review: remaining.filter((card) => card.state === 'Review').length,
      overdue: remaining.filter((card) => card.state === 'Review' && (card.dueAtMs ?? 0) < this.#studyDayStart()).length,
      total: remaining.length,
    }
  }

  #sessionDto(session: SessionRecord): StudySessionDto {
    const card = this.#currentCard(session)
    const current = session.status === 'finished' ? null : card
    return {
      sessionId: session.id,
      deckId: session.deckId,
      deckName: this.#deckName(session.deckId),
      status: session.status,
      current: current ? this.#studyCard(current) : null,
      counts: this.#sessionCounts(session),
      reviewedCount: session.ratings.filter((rating) => !rating.undone).length,
      undoAvailable: session.undo.length > 0,
      startedAt: iso(session.startedAtMs),
    }
  }

  #previewToken(card: CardRecord): string {
    return `p${PRESET_VERSION}-${card.id}-v${card.version}`
  }

  /** Fixture intervals only — real FSRS-6 arrives in Phase 9. */
  #intervalSeconds(card: CardRecord, rating: Rating): number {
    if (card.state === 'New' || card.state === 'Learning' || card.state === 'Relearning') {
      return { Again: 60, Hard: 360, Good: 600, Easy: 4 * 86_400 }[rating]
    }
    const current = Math.max(card.intervalSeconds, 86_400)
    return { Again: 600, Hard: Math.round(current * 1.2), Good: Math.round(current * 2.5), Easy: Math.round(current * 3.5) }[rating]
  }

  #nextState(card: CardRecord, rating: Rating): CardState {
    if (rating === 'Again') return card.state === 'Review' ? 'Relearning' : card.state === 'New' ? 'Learning' : card.state
    if (card.state === 'New' && rating !== 'Easy') return 'Learning'
    return 'Review'
  }

  #queueFor(state: CardState): QueueState {
    return state === 'New' ? 'New' : state === 'Review' ? 'Review' : 'Learning'
  }

  #noteDetails(note: NoteRecord): NoteDetailsDto {
    const cards: CardDetailsDto[] = this.#collection.cards
      .filter((card) => card.noteId === note.id && !card.deleted)
      .map((card) => {
        const history = this.#collection.logs
          .filter((log) => log.cardId === card.id)
          .sort((a, b) => b.reviewedAtMs - a.reviewedAtMs)
        const lastReview = history[0]?.reviewedAtMs
        const retrievability =
          card.stability !== null && lastReview !== undefined
            ? {
                value: Math.round((1 + (this.#nowMs - lastReview) / DAY_MS / (9 * card.stability)) ** -1 * 1000) / 1000,
                computedAt: iso(this.#nowMs),
              }
            : null
        return {
          id: card.id,
          noteId: card.noteId,
          deckId: card.deckId,
          ordinal: card.ordinal,
          direction: card.ordinal === 1 ? 'reverse' : 'forward',
          state: card.state,
          queue: card.queue,
          flag: card.flag,
          dueAt: card.dueAtMs === null ? null : iso(card.dueAtMs),
          stability: card.stability,
          difficulty: card.difficulty,
          retrievability,
          reps: card.reps,
          lapses: card.lapses,
          version: card.version,
          history: history.map((log) => ({
            id: log.id,
            reviewedAt: iso(log.reviewedAtMs),
            rating: log.rating,
            stateBefore: log.stateBefore,
            intervalSeconds: log.intervalSeconds,
          })),
        }
      })
    return {
      note: {
        id: note.id,
        guid: note.guid,
        noteTypeKind: note.noteTypeKind,
        front: note.front,
        back: note.back,
        tags: [...note.tags],
        version: note.version,
        createdAt: iso(note.createdAtMs),
        updatedAt: iso(note.updatedAtMs),
      },
      cards,
      associations: this.#associations(note.id),
      grammarTips: this.#tips(note.id),
      wordLinks: this.#wordLinks(note.id),
    }
  }

  // ------------------------------------------------------------ app

  getAppInfo() {
    return this.#query('getAppInfo', () => ({ apiVersion: API_VERSION, appName: 'LanguaLinker', appVersion: '0.1.0', mode: 'demo' as const }))
  }

  getAttributions() {
    return this.#query('getAttributions', (): AttributionDto[] => [
      { name: 'Svelte', license: 'MIT', url: 'https://svelte.dev', scope: 'dependency' },
      { name: 'Tauri', license: 'MIT OR Apache-2.0', url: 'https://tauri.app', scope: 'dependency' },
      { name: 'i18next', license: 'MIT', url: 'https://www.i18next.com', scope: 'dependency' },
      { name: 'Demo deck content', license: 'Demo fixtures — not for release', url: null, scope: 'content' },
    ])
  }

  // ------------------------------------------------------------ dashboard / decks

  getDashboard() {
    return this.#query('getDashboard', (): DashboardDto => {
      const decks = this.#collection.decks.map((deck) => this.#deckSummary(deck))
      const roots = decks.filter((deck) => deck.parentId === null)
      for (const session of this.#sessions.values()) this.#currentCard(session)
      const open = [...this.#sessions.values()].filter((session) => session.status !== 'finished').at(-1)
      const resumable: ResumableSessionDto | null = open
        ? {
            sessionId: open.id,
            deckId: open.deckId,
            deckName: this.#deckName(open.deckId),
            status: open.status === 'paused' ? 'paused' : 'active',
            reviewedCount: this.#validRatings(open).length,
            remaining: this.#sessionCounts(open).total,
          }
        : null
      const dayStart = this.#studyDayStart()
      const futureDues = this.#liveCards()
        .filter((card) => card.queue !== 'Suspended' && card.dueAtMs !== null && card.dueAtMs > dayStart + DAY_MS)
        .map((card) => card.dueAtMs as number)
      return {
        decks,
        streakDays: this.#collection.logs.length > 0 ? 6 : 0,
        studiedToday: this.#collection.logs.filter((log) => log.reviewedAtMs >= dayStart).length,
        dueToday: roots.reduce((sum, deck) => sum + deck.counts.review + deck.counts.learning, 0),
        dailyGoal: this.#settings.dailyGoal,
        resumable,
        nextDueAt: futureDues.length > 0 ? iso(Math.min(...futureDues)) : null,
        generatedAt: iso(this.#nowMs),
      }
    })
  }

  getDeck(deckId: Id) {
    return this.#query('getDeck', (): DeckOverviewDto => this.#deckOverview(deckId))
  }

  #deckOverview(deckId: Id): DeckOverviewDto {
    const deck = this.#deck(deckId)
    const caps = capabilitiesFor(deck.deckType)
    const noteIds = new Set(this.#collection.notes.filter((note) => this.#subtree(deckId).has(note.deckId)).map((note) => note.id))
    return {
      deck: this.#deckSummary(deck),
      children: this.#collection.decks.filter((child) => child.parentId === deckId).map((child) => this.#deckSummary(child)),
      description: deck.description,
      presetId: deck.presetId,
      presetName: deck.presetId === 'preset-light' ? 'Lekki (10 nowych/dzień)' : 'Domyślny (20 nowych/dzień)',
      presetDependentDecks: this.#collection.decks.filter((other) => other.presetId === deck.presetId).length,
      hiddenSectionCounts: {
        associations: caps.associations ? 0 : this.#collection.associations.filter((item) => noteIds.has(item.noteId)).length,
        grammarTips: caps.grammarTips ? 0 : this.#collection.tips.filter((item) => noteIds.has(item.noteId)).length,
        wordLinks: caps.wordLinks ? 0 : this.#collection.wordLinks.filter((item) => noteIds.has(item.sentenceNoteId)).length,
      },
    }
  }

  createDeck(input: CreateDeckInput) {
    return this.#mutate('createDeck', input.operationId, ['decks'], () => {
      const name = input.name.trim()
      if (!name) fail('VALIDATION_FAILED', { field: 'name' })
      if (input.parentId) this.#deck(input.parentId)
      const id = this.#nextId()
      this.#collection.decks.push({
        id,
        guid: `deck-${id}`,
        name,
        parentId: input.parentId,
        targetLanguage: input.targetLanguage,
        deckType: input.deckType,
        description: '',
        bundled: false,
        presetId: 'preset-default',
        version: 1,
      })
      return this.#deckOverview(id)
    })
  }

  updateDeck(input: UpdateDeckInput) {
    return this.#mutate('updateDeck', input.operationId, ['decks'], () => {
      const deck = this.#deck(input.deckId)
      if (deck.version !== input.expectedVersion) fail('VERSION_CONFLICT', { currentVersion: deck.version })
      const { name, parentId, deckType, presetId } = input.patch
      if (name !== undefined) {
        if (!name.trim()) fail('VALIDATION_FAILED', { field: 'name' })
        deck.name = name.trim()
      }
      if (parentId !== undefined) {
        if (parentId !== null && this.#subtree(deck.id).has(parentId)) fail('VALIDATION_FAILED', { field: 'parentId', reason: 'cycle' })
        if (parentId !== null) this.#deck(parentId)
        deck.parentId = parentId
      }
      // DeckType hides sections; it never deletes associations/tips/links.
      if (deckType !== undefined) deck.deckType = deckType
      if (presetId !== undefined) deck.presetId = presetId
      deck.version += 1
      return this.#deckOverview(deck.id)
    })
  }

  deleteDeck(input: DeleteDeckInput) {
    return this.#mutate('deleteDeck', input.operationId, ['decks', 'cards'], (): DeleteDeckResultDto => {
      const deck = this.#deck(input.deckId)
      if (deck.version !== input.expectedVersion) fail('VERSION_CONFLICT', { currentVersion: deck.version })
      const ids = this.#subtree(deck.id)
      let deletedCards = 0
      for (const card of this.#liveCards()) {
        if (ids.has(card.deckId)) {
          card.deleted = true
          deletedCards += 1
        }
      }
      this.#collection.decks = this.#collection.decks.filter((candidate) => !ids.has(candidate.id))
      return { deletedDecks: ids.size, deletedCards }
    })
  }

  // ------------------------------------------------------------ study

  startStudy(input: StartStudyInput) {
    return this.#mutate('startStudy', input.operationId, [], (): StudySessionDto => {
      const deckIds = input.deckId === null ? new Set(this.#collection.decks.map((deck) => deck.id)) : this.#subtree(this.#deck(input.deckId).id)
      const order = (card: CardRecord) => (card.state === 'Learning' || card.state === 'Relearning' ? 0 : card.state === 'Review' ? 1 : 2)
      // A custom session (e.g. the Again cards from a summary) ignores due dates but not suspension.
      const queue = input.cardIds
        ? input.cardIds.filter((id) => {
            const card = this.#collection.cards.find((candidate) => candidate.id === id)
            return card !== undefined && !card.deleted && card.queue !== 'Suspended' && card.queue !== 'UserBuried'
          })
        : this.#liveCards()
            .filter((card) => deckIds.has(card.deckId) && this.#isStudyable(card))
            .sort((a, b) => order(a) - order(b) || (a.dueAtMs ?? 0) - (b.dueAtMs ?? 0) || Number(a.id) - Number(b.id))
            .slice(0, 50)
            .map((card) => card.id)
      // One durable session at a time: a new session closes the previous unfinished one.
      for (const previous of this.#sessions.values()) this.#finish(previous)
      const session: SessionRecord = {
        id: this.#nextId('session-'),
        deckId: input.deckId,
        queue,
        cursor: 0,
        status: queue.length === 0 ? 'finished' : 'active',
        startedAtMs: this.#nowMs,
        finishedAtMs: queue.length === 0 ? this.#nowMs : null,
        ratings: [],
        undo: [],
      }
      this.#sessions.set(session.id, session)
      return this.#sessionDto(session)
    })
  }

  resumeStudy(sessionId: Id) {
    return this.#query('resumeStudy', () => this.#sessionDto(this.#session(sessionId)))
  }

  updateStudySession(input: UpdateStudySessionInput) {
    return this.#mutate('updateStudySession', input.operationId, input.action === 'end' ? ['stats'] : [], (): StudySessionDto => {
      const session = this.#session(input.sessionId)
      this.#currentCard(session)
      if (session.status === 'finished') {
        if (input.action === 'end') return this.#sessionDto(session)
        fail('VALIDATION_FAILED', { reason: 'sessionFinished' })
      }
      switch (input.action) {
        case 'pause':
          session.status = 'paused'
          break
        case 'resume':
          session.status = 'active'
          break
        case 'end':
          this.#finish(session)
          break
      }
      return this.#sessionDto(session)
    })
  }

  getRatingPreview(input: PreviewInput) {
    return this.#query('getRatingPreview', (): RatingPreviewDto => {
      const card = this.#card(input.cardId)
      if (card.version !== input.cardVersion) fail('VERSION_CONFLICT', { currentVersion: card.version })
      const intervals = Object.fromEntries(
        RATINGS.map((rating) => {
          const seconds = this.#intervalSeconds(card, rating)
          return [rating, { intervalSeconds: seconds, dueAt: iso(this.#nowMs + seconds * 1000) }]
        }),
      ) as RatingPreviewDto['intervals']
      return { cardId: card.id, cardVersion: card.version, previewToken: this.#previewToken(card), intervals }
    })
  }

  rateCard(input: RateCardInput) {
    return this.#mutate('rateCard', input.operationId, ['cards', 'stats'], (): ReviewResultDto => {
      const session = this.#session(input.sessionId)
      if (session.status === 'finished') fail('VALIDATION_FAILED', { reason: 'sessionFinished' })
      if (session.status === 'paused') fail('VALIDATION_FAILED', { reason: 'sessionPaused' })
      const card = this.#currentCard(session)
      if (!card || card.id !== input.cardId) fail('VERSION_CONFLICT', { reason: 'notCurrentCard' })
      const current = card as CardRecord
      if (current.version !== input.cardVersion) fail('VERSION_CONFLICT', { currentVersion: current.version })
      if (input.previewToken !== this.#previewToken(current)) fail('PREVIEW_STALE')

      const before = { ...current }
      const answerMs = Math.max(0, Math.min(input.answerMs, 60_000))
      this.#nowMs += answerMs
      const seconds = this.#intervalSeconds(current, input.rating)
      current.state = this.#nextState(current, input.rating)
      current.queue = this.#queueFor(current.state)
      current.intervalSeconds = seconds
      current.dueAtMs = this.#nowMs + seconds * 1000
      current.reps += 1
      if (input.rating === 'Again' && before.state === 'Review') current.lapses += 1
      current.version += 1
      const logId = this.#nextId()
      this.#collection.logs.push({ id: logId, cardId: current.id, reviewedAtMs: this.#nowMs, rating: input.rating, stateBefore: before.state, intervalSeconds: seconds })
      session.undo.push({ operationId: input.operationId, card: before, cursor: session.cursor, queueLength: session.queue.length, logId })
      session.ratings.push({ operationId: input.operationId, cardId: current.id, rating: input.rating, answerMs, undone: false })
      if (input.rating === 'Again') session.queue.push(current.id)
      session.cursor += 1
      const next = this.#currentCard(session)
      return {
        operationId: input.operationId,
        sessionId: session.id,
        reviewed: { cardId: current.id, version: current.version, state: current.state, dueAt: iso(current.dueAtMs) },
        next: next ? this.#studyCard(next) : null,
        counts: this.#sessionCounts(session),
        reviewedCount: session.ratings.filter((rating) => !rating.undone).length,
        undoAvailable: session.undo.length > 0,
        sessionStatus: session.status,
      }
    })
  }

  undoReview(input: UndoInput) {
    return this.#mutate('undoReview', input.operationId, ['cards', 'stats'], (): ReviewResultDto => {
      const session = this.#session(input.sessionId)
      const entry = session.undo.pop() ?? fail('VALIDATION_FAILED', { reason: 'nothingToUndo' })
      const card = this.#card(entry.card.id)
      const restoredVersion = card.version + 1
      Object.assign(card, entry.card, { version: restoredVersion })
      session.queue.length = entry.queueLength
      session.cursor = entry.cursor
      session.status = 'active'
      session.finishedAtMs = null
      const rating = session.ratings.find((candidate) => candidate.operationId === entry.operationId)
      if (rating) rating.undone = true
      this.#collection.logs = this.#collection.logs.filter((log) => log.id !== entry.logId)
      return {
        operationId: input.operationId,
        sessionId: session.id,
        reviewed: { cardId: card.id, version: card.version, state: card.state, dueAt: card.dueAtMs === null ? null : iso(card.dueAtMs) },
        next: this.#studyCard(card),
        counts: this.#sessionCounts(session),
        reviewedCount: session.ratings.filter((candidate) => !candidate.undone).length,
        undoAvailable: session.undo.length > 0,
        sessionStatus: session.status,
      }
    })
  }

  applyCardAction(input: CardActionInput) {
    return this.#mutate('applyCardAction', input.operationId, ['cards'], (): CardActionResultDto => {
      const card = this.#card(input.cardId)
      if (card.version !== input.expectedVersion) fail('VERSION_CONFLICT', { currentVersion: card.version })
      switch (input.action) {
        case 'suspend':
          card.queue = 'Suspended'
          break
        case 'bury':
          card.queue = 'UserBuried'
          break
        case 'unsuspend':
        case 'unbury':
          card.queue = this.#queueFor(card.state)
          break
        case 'flag':
          card.flag = input.flag ?? 'none'
          break
        case 'delete':
          card.deleted = true
          break
      }
      card.version += 1
      return { cardId: card.id, version: card.version, queue: card.queue, flag: card.flag, deleted: card.deleted }
    })
  }

  getSessionSummary(sessionId: Id) {
    return this.#query('getSessionSummary', (): SessionSummaryDto => {
      const session = this.#session(sessionId)
      this.#currentCard(session)
      const valid = this.#validRatings(session)
      const ratings = Object.fromEntries(RATINGS.map((rating) => [rating, valid.filter((item) => item.rating === rating).length])) as Record<Rating, number>
      const reviewedIds = new Set(valid.map((item) => item.cardId))
      const dues = [...reviewedIds].flatMap((id) => {
        const due = this.#collection.cards.find((card) => card.id === id)?.dueAtMs
        return due ? [due] : []
      })
      const againIds = [...new Set(valid.filter((item) => item.rating === 'Again').map((item) => item.cardId))]
      const againCards = againIds.flatMap((id): SessionCardDto[] => {
        const card = this.#collection.cards.find((candidate) => candidate.id === id && !candidate.deleted)
        const note = card && this.#collection.notes.find((candidate) => candidate.id === card.noteId)
        const deck = card && this.#collection.decks.find((candidate) => candidate.id === card.deckId)
        if (!card || !note || !deck) return []
        const reverse = card.ordinal === 1
        return [{
          cardId: card.id, front: reverse ? note.back : note.front, back: reverse ? note.front : note.back,
          targetLanguage: deck.targetLanguage, direction: reverse ? 'reverse' : 'forward',
        }]
      })
      const scope = session.deckId === null ? null : this.#subtree(session.deckId)
      const remainingDue = this.#liveCards().filter((card) => (scope === null || scope.has(card.deckId)) && this.#isStudyable(card)).length
      // Another root deck (outside this session's scope and not its ancestor) with cards due today.
      const nextDeck = scope === null
        ? null
        : (this.#collection.decks
            .filter((deck) => deck.parentId === null && !scope.has(deck.id) && !this.#subtree(deck.id).has(session.deckId ?? ''))
            .map((deck) => {
              const counts = this.#counts(this.#subtree(deck.id))
              return { id: deck.id, name: deck.name, due: counts.new + counts.learning + counts.review }
            })
            .filter((deck) => deck.due > 0)
            .sort((a, b) => b.due - a.due || Number(a.id) - Number(b.id))[0] ?? null)
      return {
        sessionId: session.id,
        deckId: session.deckId,
        deckName: this.#deckName(session.deckId),
        status: session.status,
        ratings,
        reviewedCount: valid.length,
        activeMs: valid.reduce((sum, item) => sum + item.answerMs, 0),
        againCards,
        nextDueAt: dues.length > 0 ? iso(Math.min(...dues)) : null,
        remainingDue,
        nextDeck,
        firstCompleted: this.#firstCompletedSessionId === session.id,
        startedAt: iso(session.startedAtMs),
        finishedAt: session.finishedAtMs === null ? null : iso(session.finishedAtMs),
      }
    })
  }

  // ------------------------------------------------------------ details / editor

  getNoteDetails(cardId: Id) {
    return this.#query('getNoteDetails', () => this.#noteDetails(this.#note(this.#card(cardId).noteId)))
  }

  getNoteContent(noteId: Id) {
    return this.#query('getNoteContent', (): NoteContentDto => {
      const note = this.#note(noteId)
      const deck = this.#deck(note.deckId)
      return {
        noteId: note.id,
        deckId: deck.id,
        front: note.front,
        back: note.back,
        targetLanguage: deck.targetLanguage,
        capabilities: capabilitiesFor(deck.deckType),
        associations: this.#associations(note.id),
        grammarTips: this.#tips(note.id),
      }
    })
  }

  getEditorData() {
    return this.#query('getEditorData', (): EditorDataDto => ({
      decks: this.#collection.decks.map((deck) => ({
        id: deck.id,
        name: deck.name,
        deckType: deck.deckType,
        capabilities: capabilitiesFor(deck.deckType),
        targetLanguage: deck.targetLanguage,
      })),
      noteTypes: ['Basic', 'BasicAndReversed', 'Sentence'],
      tags: [...new Set(this.#collection.notes.flatMap((note) => note.tags))].sort(),
      maxFieldLength: MAX_FIELD_LENGTH,
    }))
  }

  checkDuplicate(input: DuplicateCheckInput) {
    return this.#query('checkDuplicate', (): DuplicateCheckDto => {
      const needle = normalizeText(input.front)
      if (!needle) return { duplicates: [] }
      const deckIds = this.#subtree(input.deckId)
      return {
        duplicates: this.#collection.notes
          .filter((note) => deckIds.has(note.deckId) && note.id !== input.excludeNoteId && normalizeText(note.front) === needle)
          .map((note) => ({ noteId: note.id, front: note.front, deckName: this.#deck(note.deckId).name })),
      }
    })
  }

  saveNote(input: SaveNoteInput) {
    return this.#mutate('saveNote', input.operationId, ['notes', 'cards'], (): NoteDetailsDto => {
      const front = input.front.normalize('NFC').trim()
      const back = input.back.normalize('NFC').trim()
      if (!front) fail('VALIDATION_FAILED', { field: 'front', reason: 'required' })
      if (!back) fail('VALIDATION_FAILED', { field: 'back', reason: 'required' })
      for (const [field, value] of [['front', front], ['back', back]] as const) {
        if (value.length > MAX_FIELD_LENGTH) fail('VALIDATION_FAILED', { field, reason: 'tooLong', max: MAX_FIELD_LENGTH })
      }
      this.#deck(input.deckId)
      let note: NoteRecord
      if (input.noteId === null) {
        const id = this.#nextId()
        note = { id, guid: `note-${id}`, deckId: input.deckId, noteTypeKind: input.noteTypeKind, front, back, tags: [...input.tags], version: 1, createdAtMs: this.#nowMs, updatedAtMs: this.#nowMs }
        this.#collection.notes.push(note)
      } else {
        note = this.#note(input.noteId)
        if (note.version !== input.expectedVersion) fail('VERSION_CONFLICT', { currentVersion: note.version })
        Object.assign(note, { deckId: input.deckId, noteTypeKind: input.noteTypeKind, front, back, tags: [...input.tags], version: note.version + 1, updatedAtMs: this.#nowMs })
      }
      const wanted = input.noteTypeKind === 'BasicAndReversed' ? [0, 1] : [0]
      for (const ordinal of [0, 1]) {
        const existing = this.#collection.cards.find((card) => card.noteId === note.id && card.ordinal === ordinal)
        if (wanted.includes(ordinal)) {
          if (existing) existing.deleted = false
          else
            this.#collection.cards.push({
              id: this.#nextId(), noteId: note.id, deckId: input.deckId, ordinal, state: 'New', queue: 'New', flag: 'none', dueAtMs: null,
              intervalSeconds: 0, stability: null, difficulty: null, reps: 0, lapses: 0, version: 1, deleted: false,
            })
        } else if (existing) {
          // Archived, not destroyed: history survives a type change.
          existing.deleted = true
        }
      }
      if (input.manualAssociation?.trim()) {
        this.#collection.associations.push({ id: this.#nextId(), noteId: note.id, text: input.manualAssociation.trim(), provenance: 'manual', favorite: false, status: 'ready' })
      }
      return this.#noteDetails(note)
    })
  }

  setFavoriteAssociation(input: SetFavoriteAssociationInput) {
    return this.#mutate('setFavoriteAssociation', input.operationId, ['notes'], () => {
      const target = this.#collection.associations.find((item) => item.id === input.associationId && item.noteId === input.noteId)
      if (!target) fail('NOT_FOUND', { entity: 'association', id: input.associationId })
      for (const association of this.#collection.associations) {
        if (association.noteId === input.noteId) association.favorite = association.id === input.associationId
      }
      return this.#associations(input.noteId)
    })
  }

  resolveWordLink(input: ResolveWordLinkInput) {
    return this.#mutate('resolveWordLink', input.operationId, ['notes'], (): WordLinkDto => {
      const link = this.#collection.wordLinks.find((candidate) => candidate.id === input.linkId) ?? fail('NOT_FOUND', { entity: 'wordLink', id: input.linkId })
      this.#note(input.wordNoteId)
      link.wordNoteId = input.wordNoteId
      link.resolution = 'userOverridden'
      const dto = this.#wordLinks(link.sentenceNoteId).find((candidate) => candidate.id === link.id)
      return dto ?? fail('INTERNAL')
    })
  }

  generateAssociation(input: GenerateAssociationInput) {
    return this.#mutate('generateAssociation', input.operationId, [], (): JobDto => {
      const note = this.#note(input.noteId)
      if (this.#settings.ai.consent !== 'granted') fail('PERMISSION_DENIED', { reason: 'aiConsent' })
      if (this.#scenario === 'quota') fail('QUOTA_EXCEEDED', { resetsAt: iso(this.#nowMs + DAY_MS) })
      const variants = Math.max(1, Math.min(input.variants, 3))
      if (this.#scenario === 'offline') {
        this.#collection.associations.push({ id: this.#nextId(), noteId: note.id, text: '', provenance: 'generated', favorite: false, status: 'offlineQueued' })
        const job: JobDto = { jobId: this.#nextId('job-'), kind: 'llm', status: 'queued', sequence: 0, progress: null, startedAt: iso(this.#nowMs), finishedAt: null, error: null }
        this.#jobs.set(job.jobId, job)
        return job
      }
      const pendingIds = Array.from({ length: variants }, () => this.#nextId())
      for (const id of pendingIds) {
        this.#collection.associations.push({ id, noteId: note.id, text: '', provenance: 'generated', favorite: false, status: 'pending' })
      }
      if (this.#scenario === 'aiErrors') {
        const errorCode = AI_FAILURES[this.#generations++ % AI_FAILURES.length] ?? 'PROVIDER_ERROR'
        return this.#startJob('llm', 3, () => {
          for (const association of this.#collection.associations.filter((item) => pendingIds.includes(item.id))) {
            association.status = errorCode === 'MODERATION_BLOCKED' ? 'blocked' : 'error'
            association.errorCode = errorCode
          }
        }, ['notes'], errorCode)
      }
      return this.#startJob('llm', 3, () => {
        pendingIds.forEach((id, index) => {
          const association = this.#collection.associations.find((item) => item.id === id)
          if (association) {
            association.status = 'ready'
            association.text = `Wariant ${index + 1}: „${note.front}” → obraz „${note.back}” (demo)`
          }
        })
      }, ['notes'])
    })
  }

  // ------------------------------------------------------------ browser

  searchCards(input: SearchInput) {
    return this.#query('searchCards', (): CardPageDto => {
      if (input.pageSize < 1 || input.pageSize > 500) fail('VALIDATION_FAILED', { field: 'pageSize' })
      const offset = input.cursor === null ? 0 : Number.parseInt(input.cursor, 10)
      if (!Number.isFinite(offset) || offset < 0) fail('VALIDATION_FAILED', { field: 'cursor' })
      const filter = parseSearch(input.query)
      const filterVersion = `${this.#revision}:${JSON.stringify([input.query, input.deckId, input.states, input.flags, input.sort, input.direction])}`

      if (this.#scenario === 'large') {
        const matches: number[] = []
        for (let index = 0; index < LARGE_COLLECTION_SIZE; index += 1) {
          const row = largeRow(index)
          if (filter.text && !normalizeText(`${row.front} ${row.back}`).includes(filter.text)) continue
          if (input.states.length > 0 && !input.states.includes(row.state)) continue
          matches.push(index)
        }
        if (input.direction === 'desc') matches.reverse()
        const items = matches.slice(offset, offset + input.pageSize).map((index): CardRowDto => {
          const row = largeRow(index)
          return {
            cardId: String(1_000_000 + index), noteId: String(2_000_000 + index), deckId: '1', deckName: 'Niemiecki — słówka A1',
            front: row.front, back: row.back, state: row.state, queue: this.#queueFor(row.state), flag: 'none',
            dueAt: row.state === 'New' ? null : iso(this.#nowMs + row.dueOffsetDays * DAY_MS), tags: [],
          }
        })
        this.#filterSnapshots.set(filterVersion, matches.map((index) => String(1_000_000 + index)))
        const nextOffset = offset + items.length
        return { items, nextCursor: nextOffset < matches.length ? String(nextOffset) : null, total: matches.length, filterVersion }
      }

      const deckIds = input.deckId === null ? null : this.#subtree(input.deckId)
      const dayEnd = this.#studyDayStart() + DAY_MS
      const rows = this.#liveCards()
        .flatMap((card): CardRowDto[] => {
          const note = this.#collection.notes.find((candidate) => candidate.id === card.noteId)
          const deck = this.#collection.decks.find((candidate) => candidate.id === card.deckId)
          if (!note || !deck) return []
          if (deckIds && !deckIds.has(card.deckId)) return []
          if (filter.deck && !normalizeText(deck.name).startsWith(filter.deck)) return []
          if (filter.tag && !note.tags.some((tag) => normalizeText(tag) === filter.tag)) return []
          if (filter.text && !normalizeText(`${note.front} ${note.back}`).includes(filter.text)) return []
          if (filter.is === 'new' && card.state !== 'New') return []
          if (filter.is === 'due' && (card.state === 'New' || (card.dueAtMs ?? Infinity) > dayEnd)) return []
          if (filter.is === 'suspended' && card.queue !== 'Suspended') return []
          if (input.states.length > 0 && !input.states.includes(card.state)) return []
          if (input.flags.length > 0 && !input.flags.includes(card.flag)) return []
          const reverse = card.ordinal === 1
          return [{
            cardId: card.id, noteId: note.id, deckId: deck.id, deckName: deck.name,
            front: reverse ? note.back : note.front, back: reverse ? note.front : note.back,
            state: card.state, queue: card.queue, flag: card.flag,
            dueAt: card.dueAtMs === null ? null : iso(card.dueAtMs), tags: [...note.tags],
          }]
        })
      const cardById = new Map(this.#collection.cards.map((card) => [card.id, card]))
      const key = (row: CardRowDto): string | number => {
        const card = cardById.get(row.cardId)
        switch (input.sort) {
          case 'due': return card?.dueAtMs ?? Number.MAX_SAFE_INTEGER
          case 'created': return Number(row.noteId)
          case 'front': return normalizeText(row.front)
          case 'deck': return normalizeText(row.deckName)
          case 'reps': return card?.reps ?? 0
          case 'lapses': return card?.lapses ?? 0
        }
      }
      const direction = input.direction === 'asc' ? 1 : -1
      rows.sort((a, b) => {
        const ka = key(a)
        const kb = key(b)
        const primary = typeof ka === 'string' && typeof kb === 'string' ? ka.localeCompare(kb, 'pl') : Number(ka) - Number(kb)
        return primary * direction || Number(a.cardId) - Number(b.cardId)
      })
      this.#filterSnapshots.set(filterVersion, rows.map((row) => row.cardId))
      const items = rows.slice(offset, offset + input.pageSize)
      const nextOffset = offset + items.length
      return { items, nextCursor: nextOffset < rows.length ? String(nextOffset) : null, total: rows.length, filterVersion }
    })
  }

  applyBulkAction(input: BulkActionInput) {
    return this.#mutate('applyBulkAction', input.operationId, ['cards'], (): BulkResultDto => {
      let ids: string[]
      if (input.selection.kind === 'ids') {
        ids = input.selection.cardIds
      } else {
        const snapshot = this.#filterSnapshots.get(input.selection.filterVersion)
        if (!snapshot || !input.selection.filterVersion.startsWith(`${this.#revision}:`)) fail('PREVIEW_STALE')
        const excluded = new Set(input.selection.excludedCardIds)
        ids = (snapshot ?? []).filter((id) => !excluded.has(id))
      }
      if (this.#scenario === 'large') return { affected: ids.length }
      let affected = 0
      for (const id of ids) {
        const card = this.#collection.cards.find((candidate) => candidate.id === id && !candidate.deleted)
        if (!card) continue
        const action = input.action
        switch (action.type) {
          case 'suspend': card.queue = 'Suspended'; break
          case 'unsuspend': card.queue = this.#queueFor(card.state); break
          case 'flag': card.flag = action.flag; break
          case 'changeDeck': this.#deck(action.deckId); card.deckId = action.deckId; break
          case 'addTag': {
            const note = this.#collection.notes.find((candidate) => candidate.id === card.noteId)
            if (note && !note.tags.includes(action.tag)) note.tags.push(action.tag)
            break
          }
          case 'delete': card.deleted = true; break
        }
        card.version += 1
        affected += 1
      }
      return { affected }
    })
  }

  // ------------------------------------------------------------ stats / settings / onboarding

  getStats(input: StatsInput) {
    return this.#query('getStats', (): StatsDto => {
      const days = { '1m': 30, '3m': 90, '1y': 365, all: 730 }[input.range]
      const cards = this.#liveCards().filter((card) => input.deckId === null || this.#subtree(input.deckId).has(card.deckId))
      const empty = cards.length === 0
      const random = createRandom(this.#options.seed + days)
      const dayStart = this.#studyDayStart()
      const studyDay = (ms: number) => iso(ms).slice(0, 10)
      const reviewsByDay = empty
        ? []
        : Array.from({ length: days }, (_, index) => {
            const count = random() < 0.2 ? 0 : Math.floor(random() * 40)
            return { day: studyDay(dayStart - (days - 1 - index) * DAY_MS), count, activeMs: count * (4_000 + Math.floor(random() * 6_000)) }
          })
      const forecast = empty
        ? []
        : Array.from({ length: 30 }, (_, index) => ({
            day: studyDay(dayStart + index * DAY_MS),
            due: cards.filter((card) => card.dueAtMs !== null && card.dueAtMs >= dayStart + index * DAY_MS && card.dueAtMs < dayStart + (index + 1) * DAY_MS).length,
          }))
      const cardStates = { New: 0, Learning: 0, Review: 0, Relearning: 0, Suspended: 0 }
      for (const card of cards) {
        if (card.queue === 'Suspended') cardStates.Suspended += 1
        else cardStates[card.state] += 1
      }
      return {
        range: input.range,
        reviewsByDay,
        forecast,
        cardStates,
        trueRetention: empty ? { young: null, mature: null } : { young: { passed: 171, total: 200 }, mature: { passed: 88, total: 97 } },
        streakDays: empty ? 0 : 6,
      }
    })
  }

  getSettings() {
    return this.#query('getSettings', () => this.#settings)
  }

  updateSettings(input: UpdateSettingsInput) {
    return this.#mutate('updateSettings', input.operationId, ['settings'], (): SettingsDto => {
      if (this.#settings.version !== input.expectedVersion) fail('VERSION_CONFLICT', { currentVersion: this.#settings.version })
      const { aiConsent, ...patch } = input.patch
      if (patch.dailyGoal !== undefined && (!Number.isInteger(patch.dailyGoal) || patch.dailyGoal < 1 || patch.dailyGoal > 500)) fail('VALIDATION_FAILED', { field: 'dailyGoal' })
      if (patch.dayCutoffHour !== undefined && (!Number.isInteger(patch.dayCutoffHour) || patch.dayCutoffHour < 0 || patch.dayCutoffHour > 23)) fail('VALIDATION_FAILED', { field: 'dayCutoffHour' })
      if (patch.reminderTime && !/^([01]\d|2[0-3]):[0-5]\d$/.test(patch.reminderTime)) fail('VALIDATION_FAILED', { field: 'reminderTime' })
      this.#settings = {
        ...this.#settings,
        ...patch,
        ai: aiConsent ? { ...this.#settings.ai, consent: aiConsent } : this.#settings.ai,
        version: this.#settings.version + 1,
      }
      return this.#settings
    })
  }

  getOnboardingState() {
    return this.#query('getOnboardingState', (): OnboardingStateDto => ({
      completed: this.#onboardingCompleted,
      step: this.#onboardingCompleted ? 'ready' : 'nativeLanguage',
      availableDecks: BUNDLED_DECKS,
    }))
  }

  completeOnboarding(input: CompleteOnboardingInput) {
    return this.#mutate('completeOnboarding', input.operationId, [], (): JobDto => {
      if (input.dailyGoal < 1 || input.dailyGoal > 500) fail('VALIDATION_FAILED', { field: 'dailyGoal' })
      const unknown = input.deckGuids.find((guid) => !BUNDLED_DECKS.some((deck) => deck.guid === guid))
      if (unknown) fail('VALIDATION_FAILED', { field: 'deckGuids', value: unknown })
      return this.#startJob('prepareContent', 4, () => {
        const source = this.#pendingCollection ?? this.#collection
        const roots = new Set(source.decks.filter((deck) => input.deckGuids.includes(deck.guid)).map((deck) => deck.id))
        const keep = new Set(source.decks.filter((deck) => roots.has(deck.id) || (deck.parentId !== null && roots.has(deck.parentId))).map((deck) => deck.id))
        const notes = source.notes.filter((note) => keep.has(note.deckId))
        const noteIds = new Set(notes.map((note) => note.id))
        const cards = source.cards.filter((card) => keep.has(card.deckId))
        const cardIds = new Set(cards.map((card) => card.id))
        this.#collection = {
          decks: source.decks.filter((deck) => keep.has(deck.id)),
          notes,
          cards,
          associations: source.associations.filter((item) => noteIds.has(item.noteId)),
          tips: source.tips.filter((item) => noteIds.has(item.noteId)),
          wordLinks: source.wordLinks.filter((item) => noteIds.has(item.sentenceNoteId)),
          logs: source.logs.filter((log) => cardIds.has(log.cardId)),
        }
        this.#pendingCollection = null
        this.#onboardingCompleted = true
        this.#settings = { ...this.#settings, dailyGoal: input.dailyGoal, version: this.#settings.version + 1 }
      }, ['decks', 'cards', 'settings'])
    })
  }

  // ------------------------------------------------------------ transfer / backup (demo never touches user files)

  pickFile(input: PickFileInput) {
    return this.#query('pickFile', (): FileHandleDto | null =>
      input.purpose === 'import'
        ? { token: 'demo-import-csv', name: 'slowka-niemiecki.csv', sizeBytes: 4_812 }
        : { token: 'demo-restore-ok', name: 'langualinker-2026-02-27.llbackup', sizeBytes: 182_000 },
    )
  }

  previewImport(input: ImportInput) {
    return this.#query('previewImport', (): ImportPreviewDto => {
      if (input.fileToken === 'demo-import-apkg') fail('UNSUPPORTED_FORMAT', { format: 'apkg' })
      if (input.fileToken !== 'demo-import-csv') fail('NOT_FOUND', { entity: 'file' })
      this.#importPreviewToken = `import-${this.#revision}-${input.duplicatePolicy}`
      const delimiter = input.delimiter ?? ';'
      return {
        fileName: 'slowka-niemiecki.csv',
        detectedDelimiter: ';',
        encoding: 'UTF-8',
        hasBom: true,
        columns: input.hasHeader ? ['Vorderseite', 'Rückseite', 'Tags'] : ['1', '2', '3'],
        sampleRows: [
          ['die Größe', 'rozmiar', 'A1'],
          ['der Fußgängerüberweg', 'przejście dla pieszych', 'A2 złożenia'],
          ['schön', 'ładny, piękny', 'A1'],
        ].map((row) => (delimiter === ';' ? row : [row.join(delimiter)])),
        mapping: input.mapping ?? ['front', 'back', 'tags'],
        counts: { create: 42, update: input.duplicatePolicy === 'update' ? 3 : 0, skip: input.duplicatePolicy === 'skip' ? 3 : 0, duplicate: input.duplicatePolicy === 'duplicate' ? 3 : 0, error: 1 },
        rowErrors: [{ row: 17, error: AppError.of('VALIDATION_FAILED', { field: 'front', reason: 'required' }).dto }],
        previewToken: this.#importPreviewToken,
      }
    })
  }

  commitImport(input: CommitImportInput) {
    return this.#mutate('commitImport', input.operationId, [], (): JobDto => {
      if (input.previewToken !== this.#importPreviewToken) fail('PREVIEW_STALE')
      this.#importPreviewToken = null
      return this.#startJob('import', 10, undefined, ['decks', 'cards'])
    })
  }

  listBackups() {
    return this.#query('listBackups', () => [...this.#backups].sort((a, b) => b.createdAt.localeCompare(a.createdAt)))
  }

  createBackup(input: { operationId: string }) {
    return this.#mutate('createBackup', input.operationId, [], (): JobDto =>
      this.#startJob('backup', 5, () => {
        this.#backups.push({ id: this.#nextId('backup-'), createdAt: iso(this.#nowMs), sizeBytes: 190_000, schemaVersion: 1, cardCount: this.#liveCards().length, location: 'app' })
      }, ['backups']),
    )
  }

  validateRestore(input: ValidateRestoreInput) {
    return this.#query('validateRestore', (): RestorePreviewDto => {
      if (input.fileToken === 'demo-restore-corrupt') fail('BACKUP_CORRUPT')
      if (input.fileToken === 'demo-restore-newer') fail('SCHEMA_TOO_NEW', { schemaVersion: 99 })
      if (input.fileToken !== 'demo-restore-ok') fail('NOT_FOUND', { entity: 'file' })
      this.#restorePreviewToken = `restore-${this.#revision}`
      return {
        fileName: 'langualinker-2026-02-27.llbackup',
        createdAt: iso(this.#nowMs - 3 * DAY_MS),
        schemaVersion: 1,
        counts: { decks: 5, notes: 26, cards: 30, reviews: 118 },
        compatible: true,
        issues: [],
        previewToken: this.#restorePreviewToken,
      }
    })
  }

  restoreBackup(input: RestoreInput) {
    return this.#mutate('restoreBackup', input.operationId, [], (): JobDto => {
      if (input.previewToken !== this.#restorePreviewToken) fail('PREVIEW_STALE')
      this.#restorePreviewToken = null
      return this.#startJob('restore', 6, undefined, ['decks', 'cards', 'notes', 'settings', 'stats', 'backups'])
    })
  }

  // ------------------------------------------------------------ jobs / events

  getJobStatus(jobId: Id) {
    return this.#query('getJobStatus', () => this.#jobs.get(jobId) ?? fail('NOT_FOUND', { entity: 'job', id: jobId }))
  }

  cancelJob(jobId: Id) {
    return this.#mutate('cancelJob', `cancel-${jobId}`, [], (): JobDto => {
      const job = this.#jobs.get(jobId) ?? fail('NOT_FOUND', { entity: 'job', id: jobId })
      if (job.status === 'succeeded' || job.status === 'failed' || job.status === 'cancelled') return job
      const timer = this.#jobTimers.get(jobId)
      if (timer) clearTimeout(timer)
      this.#jobTimers.delete(jobId)
      const cancelled: JobDto = { ...job, status: 'cancelled', sequence: job.sequence + 1, finishedAt: iso(this.#nowMs) }
      this.#jobs.set(jobId, cancelled)
      this.#emitJob(cancelled)
      return cancelled
    })
  }

  onJobEvent(listener: (event: JobEventDto) => void): Unsubscribe {
    this.#jobListeners.add(listener)
    return () => this.#jobListeners.delete(listener)
  }

  onCollectionChanged(listener: (event: CollectionChangedEventDto) => void): Unsubscribe {
    this.#collectionListeners.add(listener)
    return () => this.#collectionListeners.delete(listener)
  }
}

interface ParsedSearch {
  text: string
  deck: string | null
  tag: string | null
  is: 'new' | 'due' | 'suspended' | null
}

/** Minimal search grammar for the demo: free text, quoted phrases, deck:, tag:, is:new|due|suspended. */
export function parseSearch(query: string): ParsedSearch {
  const result: ParsedSearch = { text: '', deck: null, tag: null, is: null }
  const words: string[] = []
  const pattern = /\s*(?:(\w+):)?(?:"([^"]*)"|(\S+))/gy
  const quoteCount = (query.match(/"/g) ?? []).length
  if (quoteCount % 2 === 1) fail('VALIDATION_FAILED', { field: 'query', reason: 'unbalancedQuote', position: query.lastIndexOf('"') })
  let match: RegExpExecArray | null
  while ((match = pattern.exec(query)) !== null) {
    const [, field, quoted, bare] = match
    const value = normalizeText(quoted ?? bare ?? '')
    if (!field) {
      if (value) words.push(value)
      continue
    }
    switch (field.toLowerCase()) {
      case 'deck': result.deck = value; break
      case 'tag': result.tag = value; break
      case 'is':
        if (value !== 'new' && value !== 'due' && value !== 'suspended') fail('VALIDATION_FAILED', { field: 'query', reason: 'unknownValue', token: `is:${value}` })
        result.is = value as ParsedSearch['is']
        break
      default:
        fail('VALIDATION_FAILED', { field: 'query', reason: 'unknownField', token: field })
    }
  }
  result.text = words.join(' ')
  return result
}
