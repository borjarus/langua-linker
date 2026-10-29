import type { AppApi, AppApiOperation } from '../api/appApi'
import { AppError } from '../api/errors'
import {
  API_VERSION,
  DECK_TYPES,
  FSRS_WEIGHT_COUNT,
  RATINGS,
  type AboutDto,
  type AiProviderOptionDto,
  type AiSettingsDto,
  type AssociationDto,
  type AttributionDto,
  type BackupDto,
  type BackupInput,
  type BulkAction,
  type BulkActionInput,
  type BulkPreviewDto,
  type BulkPreviewInput,
  type BulkResultDto,
  type BulkSelection,
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
  type CsvColumnTarget,
  type DashboardDto,
  type DeckCountsDto,
  type DeckOverviewDto,
  type DeckSummaryDto,
  type DeckType,
  type DeleteDeckInput,
  type DeleteDeckResultDto,
  type DeleteNoteInput,
  type DeleteNoteResultDto,
  type DiagnosticsReportDto,
  type DuplicateCheckDto,
  type DuplicateCheckInput,
  type EditorDataDto,
  type ErrorCode,
  type ExportDeckInput,
  type FileHandleDto,
  type FilePurpose,
  type GenerateAssociationInput,
  type GrammarTipDto,
  type Id,
  type ImportInput,
  type ImportPreviewDto,
  type JobDto,
  type JobEventDto,
  type JobKind,
  type JobResultDto,
  type NoteContentDto,
  type NoteDetailsDto,
  type NoteTypeDto,
  type OnboardingStateDto,
  type PickFileInput,
  type PresetConfigDto,
  type PresetDto,
  type PreviewInput,
  type QueueState,
  type RateCardInput,
  type RatioDto,
  type RequestPermissionInput,
  type ResetInput,
  type ResetPreviewDto,
  type ResetPreviewInput,
  type ResetScope,
  type Rating,
  type RatingPreviewDto,
  type ResolveWordLinkInput,
  type RestoreInput,
  type RestorePreviewDto,
  type ResumableSessionDto,
  type ReviewResultDto,
  type SaveNoteInput,
  type SavePresetInput,
  type SearchInput,
  type SearchWordsInput,
  type SessionCardDto,
  type SessionSummaryDto,
  type SetAiApiKeyInput,
  type SetFavoriteAssociationInput,
  type SettingsDto,
  type ShareFileInput,
  type ShareResultDto,
  type StartStudyInput,
  type StatsDto,
  type StatsInput,
  type StudyCardDto,
  type StudySessionDto,
  type UndoInput,
  type Unsubscribe,
  type UpdateDeckInput,
  type UpdateNoteContentInput,
  type UpdateSettingsInput,
  type UpdateStudySessionInput,
  type ValidateRestoreInput,
  type WordLinkDraftDto,
  type WordLinkDto,
  type WordMatchDto,
} from '../api/types'
import {
  BUNDLED_DECKS,
  DEMO_CSV_FILES,
  DEMO_EPOCH_MS,
  DEMO_FILES,
  LARGE_COLLECTION_SIZE,
  capabilitiesFor,
  createCollection,
  createPresets,
  largeRow,
  type CardRecord,
  type Collection,
  type DeckRecord,
  type NoteRecord,
  type PresetRecord,
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

interface ImportOperation {
  kind: 'create' | 'update'
  noteId?: string
  front: string
  back: string
  tags: string[]
}

interface ImportPlan {
  token: string
  revision: number
  ops: ImportOperation[]
  deckId: string | null
  deckName: string
  counts: ImportPreviewDto['counts']
  rowErrors: ImportPreviewDto['rowErrors']
}

interface RestorePlan {
  token: string
  revision: number
  snapshot: Collection
  /** `demo-restore-fails`: validates, then fails while restoring. */
  failing: boolean
}

interface ResetPlan {
  token: string
  scope: ResetScope
  phrase: string
  /** Backups that existed at preview time; only a newer one counts as a safety copy. */
  backupsBefore: Set<string>
}

/** What a finished job reports; `error` fails it while keeping the result (e.g. a restore's safety copy). */
interface JobOutcome {
  result: JobResultDto | null
  error?: ErrorCode
}

/** Interval from which a review card counts as mature (Anki convention). */
const MATURE_DAYS = 21
/** Fewer reviews than this give "not enough data", never a misleading percentage. */
const MIN_RETENTION_SAMPLE = 10
const STABILITY_BUCKETS: [number, number | null][] = [[0, 1], [1, 3], [3, 7], [7, 14], [14, 30], [30, 90], [90, null]]
const AI_PROVIDERS: AiProviderOptionDto[] = [
  { id: 'demo', name: 'Demo', requiresKey: false, models: ['demo-mnemonic-v1'], customEndpoint: false },
  { id: 'anthropic', name: 'Anthropic', requiresKey: true, models: ['claude-haiku-5-5', 'claude-sonnet-5-5'], customEndpoint: false },
  { id: 'openaiCompatible', name: 'OpenAI-compatible', requiresKey: true, models: [], customEndpoint: true },
]

/** Outcomes cycled by the `aiErrors` scenario: provider error, unusable output, moderation. */
const AI_FAILURES = ['PROVIDER_ERROR', 'INVALID_PROVIDER_OUTPUT', 'MODERATION_BLOCKED'] as const

const SHELL_OPERATIONS: ReadonlySet<AppApiOperation> = new Set(['getAppInfo', 'getSettings', 'getOnboardingState', 'getAttributions'])
const MAX_FIELD_LENGTH = 2000
const MAX_TIP_LENGTH = 500
/** Card templates per note type — the backend owns this map; the editor reads it from `getEditorData`. */
const NOTE_TYPES: NoteTypeDto[] = [
  { kind: 'Basic', templates: [{ ordinal: 0, direction: 'forward' }] },
  { kind: 'BasicAndReversed', templates: [{ ordinal: 0, direction: 'forward' }, { ordinal: 1, direction: 'reverse' }] },
  { kind: 'Sentence', templates: [{ ordinal: 0, direction: 'forward' }] },
]
/** Leading articles and particles ignored by the word matcher (`der Tee` matches `Tee`). */
const ARTICLES = /^(der|die|das|den|dem|des|ein|eine|the|a|an|to)\s+/iu
const PRESET_VERSION = 1
const CUTOFF_HOUR = 4
const EMPTY_COLLECTION = (): Collection => ({ decks: [], notes: [], cards: [], associations: [], tips: [], wordLinks: [], logs: [] })

const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T
const normalizeText = (text: string): string => text.normalize('NFC').trim().toLocaleLowerCase('pl')
/** Word-matcher key: no article, no parenthetical sense label, case and diacritics folded. */
const wordKey = (text: string): string =>
  normalizeText(text.replace(/\s*\([^)]*\)\s*$/u, '').replace(ARTICLES, ''))
    .normalize('NFD')
    .replace(/\p{M}/gu, '')
    .replace(/ß/gu, 'ss')
const fail = (code: ErrorCode, details: Record<string, string | number | boolean | null> | null = null): never => {
  throw AppError.of(code, details)
}

export class MockAppApi implements AppApi {
  readonly #options: Required<Omit<MockAppApiOptions, 'scenario'>>
  #scenario: DemoScenario = 'happy'
  #nowMs = DEMO_EPOCH_MS
  #collection: Collection = EMPTY_COLLECTION()
  #presets: PresetRecord[] = createPresets()
  #pendingCollection: Collection | null = null
  #sessions = new Map<string, SessionRecord>()
  #operations = new Map<string, unknown>()
  #failures = new Map<AppApiOperation, ErrorCode>()
  /** Fails whichever mutation runs next (demo banner); `#failures` targets one operation. */
  #failNextSave: ErrorCode | null = null
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
  #snapshots = new Map<string, Collection>()
  #backupSerial = 0
  #previewSerial = 0
  #importPlan: ImportPlan | null = null
  #restorePlan: RestorePlan | null = null
  #resetPlan: ResetPlan | null = null
  /** Held only here, like the SecretStore: no operation ever returns it. */
  #apiKey: string | null = null
  #notificationAnswer: 'granted' | 'denied' = 'granted'
  #pickedFiles: Record<FilePurpose, string | null> = { import: 'demo-import-csv', restore: 'demo-restore-ok', backupFolder: 'demo-folder-documents' }
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
    this.#failNextSave = null
    this.#jobs.clear()
    this.#filterSnapshots.clear()
    this.#revision = 0
    this.#sequence = 1
    this.#importPlan = null
    this.#restorePlan = null
    this.#resetPlan = null
    this.#apiKey = null
    this.#notificationAnswer = 'granted'
    this.#backupSerial = 0
    this.#previewSerial = 0
    this.#preparing = new Set()
    this.#firstCompletedSessionId = null
    this.#generations = 0
    this.#presets = createPresets()

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
    this.#settings = this.#defaultSettings(scenario)
    this.#snapshots.clear()
    this.#backups = []
    if (scenario !== 'empty' && scenario !== 'newUser') {
      ;[17, 10, 3].forEach((daysAgo, index) => {
        const backup: BackupDto = {
          id: `backup-${index + 1}`,
          createdAt: iso(this.#nowMs - daysAgo * DAY_MS),
          sizeBytes: 182_000 + index * 9_000,
          schemaVersion: 1,
          cardCount: this.#collection.cards.length,
          location: index === 2 ? 'external' : 'app',
          kind: index === 1 ? 'manual' : 'automatic',
        }
        this.#backups.push(backup)
        this.#snapshots.set(backup.id, clone(this.#collection))
      })
    }
    if (scenario === 'preparing') {
      // The English deck finishes preparing in the background and announces it via collection_changed.
      this.#preparing.add('3')
      this.#startJob('prepareContent', 6, () => this.#preparing.clear(), ['decks'])
    }
  }

  #defaultSettings(scenario: DemoScenario): SettingsDto {
    return {
      uiLanguage: 'pl',
      nativeLanguage: 'pl',
      defaultTargetLanguage: 'de',
      theme: 'system',
      dailyGoal: 20,
      timeZone: 'Europe/Warsaw',
      dayCutoffHour: CUTOFF_HOUR,
      defaultPresetId: 'preset-default',
      notifications: 'prompt',
      reminderTime: null,
      ttsEnabled: true,
      ttsRate: 1,
      ai: {
        consent: scenario === 'newUser' ? 'unset' : 'granted',
        provider: 'demo',
        model: 'demo-mnemonic-v1',
        endpoint: null,
        hasApiKey: false,
        providers: AI_PROVIDERS,
        quota: { used: scenario === 'quota' ? 50 : 12, limit: 50, resetsAt: iso(this.#nowMs + DAY_MS) },
      },
      telemetryOptIn: false,
      backup: { automatic: true, retention: 5, target: 'app', externalFolder: null },
      version: 1,
    }
  }

  /** How the demo "OS" answers the next notification permission prompt. */
  setNotificationAnswer(answer: 'granted' | 'denied'): void {
    this.#notificationAnswer = answer
  }

  failNext(operation: AppApiOperation, code: ErrorCode): void {
    this.#failures.set(operation, code)
  }

  failNextSave(code: ErrorCode): void {
    this.#failNextSave = code
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
    const injected = this.#failNextSave
    if (injected) {
      this.#failNextSave = null
      fail(injected)
    }
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

  /**
   * `onDone` runs when the last step finishes and may return the job's result or a failure;
   * `failWith` ends the job as `failed` (after `onDone` ran), e.g. a provider error in `aiErrors`.
   */
  #startJob(kind: JobKind, steps: number, onDone?: () => JobOutcome | void, scopes: CollectionScope[] = [], failWith?: ErrorCode): JobDto {
    const job: JobDto = {
      jobId: this.#nextId('job-'),
      kind,
      status: 'queued',
      sequence: 0,
      progress: { done: 0, total: steps },
      startedAt: iso(this.#nowMs),
      finishedAt: null,
      error: null,
      result: null,
    }
    this.#jobs.set(job.jobId, job)
    const step = () => {
      const current = this.#jobs.get(job.jobId)
      if (!current || current.status === 'cancelled') return
      const done = (current.progress?.done ?? 0) + 1
      const finished = done >= steps
      const outcome = finished ? onDone?.() : undefined
      const error = outcome?.error ?? failWith
      const next: JobDto = {
        ...current,
        status: finished ? (error ? 'failed' : 'succeeded') : 'running',
        sequence: current.sequence + 1,
        progress: { done: Math.min(done, steps), total: steps },
        finishedAt: finished ? iso(this.#nowMs) : null,
        error: finished && error ? AppError.of(error).dto : null,
        result: outcome?.result ?? null,
      }
      this.#jobs.set(job.jobId, next)
      if (finished) {
        this.#jobTimers.delete(job.jobId)
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
    const noteCards = this.#collection.cards.filter((card) => card.noteId === note.id)
    const cards: CardDetailsDto[] = noteCards
      .filter((card) => !card.deleted)
      .sort((a, b) => a.ordinal - b.ordinal)
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
          deckName: this.#deckName(card.deckId) ?? '',
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
    const deck = this.#deck(noteCards.find((card) => !card.deleted)?.deckId ?? note.deckId)
    return {
      deckId: deck.id,
      targetLanguage: deck.targetLanguage,
      capabilities: capabilitiesFor(deck.deckType),
      archivedOrdinals: noteCards.filter((card) => card.archived).map((card) => card.ordinal).sort((a, b) => a - b),
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
      { name: 'TanStack Virtual', license: 'MIT', url: 'https://tanstack.com/virtual', scope: 'dependency' },
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
    // Only the deck's own notes: subdecks have their own type.
    const noteIds = new Set(this.#collection.notes.filter((note) => note.deckId === deckId).map((note) => note.id))
    const totals = {
      associations: this.#collection.associations.filter((item) => noteIds.has(item.noteId) && item.status === 'ready').length,
      grammarTips: this.#collection.tips.filter((item) => noteIds.has(item.noteId)).length,
      wordLinks: this.#collection.wordLinks.filter((item) => noteIds.has(item.sentenceNoteId)).length,
    }
    const hidden = (deckType: DeckType) => {
      const caps = capabilitiesFor(deckType)
      return {
        associations: caps.associations ? 0 : totals.associations,
        grammarTips: caps.grammarTips ? 0 : totals.grammarTips,
        wordLinks: caps.wordLinks ? 0 : totals.wordLinks,
      }
    }
    const preset = this.#preset(deck.presetId)
    return {
      deck: this.#deckSummary(deck),
      subdecks: this.#descendants(deckId).map((child) => this.#deckSummary(child)),
      description: deck.description,
      presetId: preset.id,
      presetName: preset.name,
      presetDependentDecks: this.#collection.decks.filter((other) => other.presetId === preset.id).length,
      hiddenSectionCounts: hidden(deck.deckType),
      typeOptions: DECK_TYPES.map((deckType) => ({ deckType, capabilities: capabilitiesFor(deckType), hiddenCounts: hidden(deckType) })),
    }
  }

  /** Every deck below `deckId`, parents before children, siblings by name. */
  #descendants(deckId: Id): DeckRecord[] {
    const children = this.#collection.decks
      .filter((deck) => deck.parentId === deckId)
      .sort((a, b) => a.name.localeCompare(b.name, 'pl'))
    return children.flatMap((child) => [child, ...this.#descendants(child.id)])
  }

  #preset(presetId: Id): PresetRecord {
    return this.#presets.find((preset) => preset.id === presetId) ?? fail('NOT_FOUND', { entity: 'preset', id: presetId })
  }

  #presetDto(preset: PresetRecord): PresetDto {
    const { id, name, version, builtIn, ...config } = preset
    return { id, name, version, builtIn, dependentDecks: this.#collection.decks.filter((deck) => deck.presetId === id).length, config }
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
        presetId: this.#settings.defaultPresetId,
        version: 1,
      })
      return this.#deckOverview(id)
    })
  }

  updateDeck(input: UpdateDeckInput) {
    return this.#mutate('updateDeck', input.operationId, ['decks'], () => {
      const deck = this.#deck(input.deckId)
      if (deck.version !== input.expectedVersion) fail('VERSION_CONFLICT', { currentVersion: deck.version })
      const { name, description, parentId, deckType, presetId } = input.patch
      if (name !== undefined) {
        if (!name.trim()) fail('VALIDATION_FAILED', { field: 'name' })
        if (name.trim().length > 120) fail('VALIDATION_FAILED', { field: 'name', reason: 'tooLong', max: 120 })
        deck.name = name.trim()
      }
      if (description !== undefined) {
        if (description.length > MAX_FIELD_LENGTH) fail('VALIDATION_FAILED', { field: 'description', reason: 'tooLong', max: MAX_FIELD_LENGTH })
        deck.description = description.trim()
      }
      if (presetId !== undefined) this.#preset(presetId)
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

  listPresets() {
    return this.#query('listPresets', (): PresetDto[] =>
      [...this.#presets].sort((a, b) => Number(b.builtIn) - Number(a.builtIn) || a.name.localeCompare(b.name, 'pl')).map((preset) => this.#presetDto(preset)),
    )
  }

  savePreset(input: SavePresetInput) {
    return this.#mutate('savePreset', input.operationId, ['decks'], (): PresetDto => {
      const name = input.name.trim()
      if (!name) fail('VALIDATION_FAILED', { field: 'name', reason: 'required' })
      validatePresetConfig(input.config)
      const config = clone(input.config)
      if (input.presetId === null) {
        const preset: PresetRecord = { id: this.#nextId('preset-'), name, version: 1, builtIn: false, ...config }
        this.#presets.push(preset)
        return this.#presetDto(preset)
      }
      const preset = this.#preset(input.presetId)
      if (preset.version !== input.expectedVersion) fail('VERSION_CONFLICT', { currentVersion: preset.version })
      Object.assign(preset, config, { name, version: preset.version + 1 })
      return this.#presetDto(preset)
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
            .sort((a, b) => order(a) - order(b) || (a.dueAtMs ?? 0) - (b.dueAtMs ?? 0) || (a.position ?? 0) - (b.position ?? 0) || Number(a.id) - Number(b.id))
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
      if (current.state !== 'New') current.position = null
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
          card.archived = false
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
        parentId: deck.parentId,
        deckType: deck.deckType,
        capabilities: capabilitiesFor(deck.deckType),
        targetLanguage: deck.targetLanguage,
      })),
      noteTypes: NOTE_TYPES,
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
      const tags = [...new Set(input.tags.map((tag) => tag.normalize('NFC').trim()).filter(Boolean))]
      if (tags.some((tag) => /\s/u.test(tag) || tag.length > 60)) fail('VALIDATION_FAILED', { field: 'tags', reason: 'invalidTag' })
      const tips = input.grammarTips.map((tip) => tip.normalize('NFC').trim()).filter(Boolean)
      if (tips.some((tip) => tip.length > MAX_TIP_LENGTH)) fail('VALIDATION_FAILED', { field: 'grammarTips', reason: 'tooLong', max: MAX_TIP_LENGTH })
      const deck = this.#deck(input.deckId)
      const template = NOTE_TYPES.find((type) => type.kind === input.noteTypeKind) ?? fail('VALIDATION_FAILED', { field: 'noteTypeKind' })
      if (input.wordLinks !== null) this.#validateLinks(front, input.wordLinks, capabilitiesFor(deck.deckType).wordLinks)

      let note: NoteRecord
      if (input.noteId === null) {
        const id = this.#nextId()
        note = { id, guid: `note-${id}`, deckId: input.deckId, noteTypeKind: input.noteTypeKind, front, back, tags, version: 1, createdAtMs: this.#nowMs, updatedAtMs: this.#nowMs }
        this.#collection.notes.push(note)
      } else {
        note = this.#note(input.noteId)
        if (note.version !== input.expectedVersion) fail('VERSION_CONFLICT', { currentVersion: note.version })
        Object.assign(note, { deckId: input.deckId, noteTypeKind: input.noteTypeKind, front, back, tags, version: note.version + 1, updatedAtMs: this.#nowMs })
      }

      // Text edits keep schedules; a type change archives (never destroys) cards and restores them when switched back.
      const wanted = new Set(template.templates.map((item) => item.ordinal))
      for (const ordinal of [0, 1]) {
        const cards = this.#collection.cards.filter((card) => card.noteId === note.id && card.ordinal === ordinal)
        const live = cards.find((card) => !card.deleted)
        const archived = cards.find((card) => card.archived)
        if (wanted.has(ordinal)) {
          if (live) {
            live.deckId = input.deckId
          } else if (archived) {
            Object.assign(archived, { deleted: false, archived: false, deckId: input.deckId, version: archived.version + 1 })
          } else {
            this.#collection.cards.push({
              id: this.#nextId(), noteId: note.id, deckId: input.deckId, ordinal, state: 'New', queue: 'New', flag: 'none', dueAtMs: null,
              intervalSeconds: 0, stability: null, difficulty: null, reps: 0, lapses: 0, version: 1, deleted: false, archived: false, position: this.#nextPosition(),
            })
          }
        } else if (live) {
          Object.assign(live, { deleted: true, archived: true, version: live.version + 1 })
        }
      }

      if (input.manualAssociation?.trim()) {
        this.#collection.associations.push({ id: this.#nextId(), noteId: note.id, text: input.manualAssociation.trim(), provenance: 'manual', favorite: false, status: 'ready' })
      }
      let order = Math.max(-1, ...this.#tips(note.id).map((tip) => tip.order))
      for (const text of tips) this.#collection.tips.push({ id: this.#nextId(), noteId: note.id, text, order: ++order })
      this.#storeLinks(note, input.wordLinks)
      return this.#noteDetails(note)
    })
  }

  #nextPosition(): number {
    return Math.max(0, ...this.#collection.cards.map((card) => card.position ?? 0)) + 1
  }

  #validateLinks(front: string, links: WordLinkDraftDto[], allowed: boolean): void {
    if (links.length > 0 && !allowed) fail('VALIDATION_FAILED', { field: 'wordLinks', reason: 'notSupported' })
    const sorted = [...links].sort((a, b) => a.startIndex - b.startIndex)
    let previousEnd = 0
    for (const link of sorted) {
      const inRange = Number.isInteger(link.startIndex) && Number.isInteger(link.endIndex) && link.startIndex >= previousEnd && link.endIndex > link.startIndex && link.endIndex <= front.length
      if (!inRange || front.slice(link.startIndex, link.endIndex) !== link.surface.normalize('NFC')) {
        fail('VALIDATION_FAILED', { field: 'wordLinks', reason: 'range', surface: link.surface })
      }
      if (link.wordNoteId !== null) this.#note(link.wordNoteId)
      previousEnd = link.endIndex
    }
  }

  /** `null` keeps stored links, dropping only those whose words the edit removed. */
  #storeLinks(note: NoteRecord, drafts: WordLinkDraftDto[] | null): void {
    const existing = this.#collection.wordLinks.filter((link) => link.sentenceNoteId === note.id)
    const others = this.#collection.wordLinks.filter((link) => link.sentenceNoteId !== note.id)
    if (drafts === null) {
      this.#collection.wordLinks = [...others, ...existing.filter((link) => note.front.slice(link.startIndex, link.endIndex) === link.surface)]
      return
    }
    const next = drafts.map((draft) => {
      const kept = existing.find((link) => link.startIndex === draft.startIndex && link.endIndex === draft.endIndex && link.wordNoteId === draft.wordNoteId)
      if (kept) return kept
      const target = draft.wordNoteId === null ? null : this.#note(draft.wordNoteId)
      return {
        id: this.#nextId(), sentenceNoteId: note.id, surface: draft.surface.normalize('NFC'), startIndex: draft.startIndex, endIndex: draft.endIndex,
        lemma: target?.front ?? null, resolution: target ? ('userOverridden' as const) : ('unresolved' as const), wordNoteId: target?.id ?? null, candidateNoteIds: [],
      }
    })
    this.#collection.wordLinks = [...others, ...next]
  }

  deleteNote(input: DeleteNoteInput) {
    return this.#mutate('deleteNote', input.operationId, ['notes', 'cards'], (): DeleteNoteResultDto => {
      const note = this.#note(input.noteId)
      if (note.version !== input.expectedVersion) fail('VERSION_CONFLICT', { currentVersion: note.version })
      let deletedCards = 0
      for (const card of this.#collection.cards.filter((candidate) => candidate.noteId === note.id)) {
        if (!card.deleted) deletedCards += 1
        Object.assign(card, { deleted: true, archived: false })
      }
      this.#collection.notes = this.#collection.notes.filter((candidate) => candidate.id !== note.id)
      this.#collection.associations = this.#collection.associations.filter((item) => item.noteId !== note.id)
      this.#collection.tips = this.#collection.tips.filter((item) => item.noteId !== note.id)
      this.#collection.wordLinks = this.#collection.wordLinks.filter((link) => link.sentenceNoteId !== note.id)
      // Sentences that pointed at this word keep the phrase, now without a target.
      for (const link of this.#collection.wordLinks.filter((candidate) => candidate.wordNoteId === note.id)) {
        Object.assign(link, { wordNoteId: null, resolution: 'unresolved' })
      }
      return { deletedCards }
    })
  }

  updateNoteContent(input: UpdateNoteContentInput) {
    return this.#mutate('updateNoteContent', input.operationId, ['notes'], (): NoteDetailsDto => {
      const note = this.#note(input.noteId)
      const change = input.change
      const text = (value: string, max: number): string => {
        const trimmed = value.normalize('NFC').trim()
        if (!trimmed) fail('VALIDATION_FAILED', { field: 'text', reason: 'required' })
        if (trimmed.length > max) fail('VALIDATION_FAILED', { field: 'text', reason: 'tooLong', max })
        return trimmed
      }
      const tip = (tipId: Id) => this.#collection.tips.find((item) => item.id === tipId && item.noteId === note.id) ?? fail('NOT_FOUND', { entity: 'grammarTip', id: tipId })
      const association = (associationId: Id) =>
        this.#collection.associations.find((item) => item.id === associationId && item.noteId === note.id) ?? fail('NOT_FOUND', { entity: 'association', id: associationId })
      const renumber = (ids: Id[]) => ids.forEach((id, order) => (tip(id).order = order))
      switch (change.type) {
        case 'addTip':
          this.#collection.tips.push({ id: this.#nextId(), noteId: note.id, text: text(change.text, MAX_TIP_LENGTH), order: this.#tips(note.id).length })
          break
        case 'editTip':
          tip(change.tipId).text = text(change.text, MAX_TIP_LENGTH)
          break
        case 'deleteTip':
          tip(change.tipId)
          this.#collection.tips = this.#collection.tips.filter((item) => item.id !== change.tipId)
          renumber(this.#tips(note.id).map((item) => item.id))
          break
        case 'reorderTips': {
          const current = this.#tips(note.id).map((item) => item.id)
          const same = change.tipIds.length === current.length && new Set(change.tipIds).size === current.length && change.tipIds.every((id) => current.includes(id))
          if (!same) fail('VALIDATION_FAILED', { field: 'tipIds', reason: 'mismatch' })
          renumber(change.tipIds)
          break
        }
        case 'addAssociation':
          this.#collection.associations.push({ id: this.#nextId(), noteId: note.id, text: text(change.text, MAX_TIP_LENGTH), provenance: 'manual', favorite: false, status: 'ready' })
          break
        case 'editAssociation': {
          const target = association(change.associationId)
          if (target.provenance !== 'manual') fail('VALIDATION_FAILED', { field: 'associationId', reason: 'notEditable' })
          target.text = text(change.text, MAX_TIP_LENGTH)
          break
        }
        case 'deleteAssociation':
          association(change.associationId)
          this.#collection.associations = this.#collection.associations.filter((item) => item.id !== change.associationId)
          break
        case 'unlinkWord': {
          const exists = this.#collection.wordLinks.some((link) => link.id === change.linkId && link.sentenceNoteId === note.id)
          if (!exists) fail('NOT_FOUND', { entity: 'wordLink', id: change.linkId })
          this.#collection.wordLinks = this.#collection.wordLinks.filter((link) => link.id !== change.linkId)
          break
        }
      }
      // Tips, associations and links are separate entities: an open editor draft of the note stays valid.
      note.updatedAtMs = this.#nowMs
      return this.#noteDetails(note)
    })
  }

  searchWords(input: SearchWordsInput) {
    return this.#query('searchWords', (): WordMatchDto[] => {
      if (!Number.isInteger(input.limit) || input.limit < 1 || input.limit > 50) fail('VALIDATION_FAILED', { field: 'limit' })
      const text = input.text.normalize('NFC').trim()
      if (!text) return []
      const key = wordKey(text)
      const bare = (front: string) => front.replace(/\s*\([^)]*\)\s*$/u, '').replace(ARTICLES, '')
      return this.#collection.notes
        .flatMap((note): WordMatchDto[] => {
          const deck = this.#collection.decks.find((candidate) => candidate.id === note.deckId)
          if (!deck || note.noteTypeKind === 'Sentence' || deck.targetLanguage !== input.targetLanguage) return []
          const exact = note.front === text || bare(note.front) === text
          if (!exact && wordKey(note.front) !== key) return []
          return [{ noteId: note.id, lemma: note.front, translation: note.back, deckName: deck.name, match: exact ? 'exact' : 'normalized' }]
        })
        .sort((a, b) => Number(a.match === 'normalized') - Number(b.match === 'normalized') || a.lemma.localeCompare(b.lemma, 'pl'))
        .slice(0, input.limit)
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
      const provider = this.#settings.ai.providers.find((item) => item.id === this.#settings.ai.provider)
      if (provider?.requiresKey && this.#apiKey === null) fail('PERMISSION_DENIED', { reason: 'apiKey' })
      if (this.#scenario === 'quota') fail('QUOTA_EXCEEDED', { resetsAt: iso(this.#nowMs + DAY_MS) })
      const variants = Math.max(1, Math.min(input.variants, 3))
      if (this.#scenario === 'offline') {
        this.#collection.associations.push({ id: this.#nextId(), noteId: note.id, text: '', provenance: 'generated', favorite: false, status: 'offlineQueued' })
        const job: JobDto = { jobId: this.#nextId('job-'), kind: 'llm', status: 'queued', sequence: 0, progress: null, startedAt: iso(this.#nowMs), finishedAt: null, error: null, result: null }
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
      const tagFilter = input.tag ? normalizeText(input.tag) : null
      const filterVersion = `${this.#revision}:${JSON.stringify([input.query, input.deckId, input.tag, input.states, input.flags, input.sort, input.direction])}`

      if (this.#scenario === 'large') {
        const matches: number[] = []
        for (let index = 0; index < LARGE_COLLECTION_SIZE; index += 1) {
          const row = largeRow(index)
          if (filter.text && !normalizeText(`${row.front} ${row.back}`).includes(filter.text)) continue
          if (input.states.length > 0 && !input.states.includes(row.state)) continue
          if (tagFilter || filter.tag || (input.flags.length > 0 && !input.flags.includes('none'))) continue
          matches.push(index)
        }
        if (input.direction === 'desc') matches.reverse()
        const items = matches.slice(offset, offset + input.pageSize).map((index): CardRowDto => {
          const row = largeRow(index)
          return {
            cardId: String(1_000_000 + index), noteId: String(2_000_000 + index), deckId: '1', deckName: 'Niemiecki — słówka A1',
            front: row.front, back: row.back, state: row.state, queue: this.#queueFor(row.state), flag: 'none',
            dueAt: row.state === 'New' ? null : iso(this.#nowMs + row.dueOffsetDays * DAY_MS),
            position: row.state === 'New' ? index + 1 : null, direction: 'forward', tags: [],
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
          if (tagFilter && !note.tags.some((tag) => normalizeText(tag) === tagFilter)) return []
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
            dueAt: card.dueAtMs === null ? null : iso(card.dueAtMs),
            position: card.state === 'New' ? card.position : null, direction: reverse ? 'reverse' : 'forward', tags: [...note.tags],
          }]
        })
      const cardById = new Map(this.#collection.cards.map((card) => [card.id, card]))
      const key = (row: CardRowDto): string | number => {
        const card = cardById.get(row.cardId)
        switch (input.sort) {
          // New cards follow their queue position after every scheduled card.
          case 'due': return card?.dueAtMs ?? Number.MAX_SAFE_INTEGER / 2 + (card?.position ?? 0)
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

  #selectionIds(selection: BulkSelection): string[] {
    if (selection.kind === 'ids') return [...new Set(selection.cardIds)]
    const snapshot = this.#filterSnapshots.get(selection.filterVersion)
    if (!snapshot || !selection.filterVersion.startsWith(`${this.#revision}:`)) fail('PREVIEW_STALE')
    const excluded = new Set(selection.excludedCardIds)
    return (snapshot ?? []).filter((id) => !excluded.has(id))
  }

  #validateBulkAction(action: BulkAction): void {
    if ((action.type === 'addTag' || action.type === 'removeTag') && (!action.tag.trim() || /\s/u.test(action.tag.trim()))) fail('VALIDATION_FAILED', { field: 'tag' })
    if (action.type === 'changeDeck') this.#deck(action.deckId)
    if (action.type === 'reposition' && (!Number.isInteger(action.start) || action.start < 1 || !Number.isInteger(action.step) || action.step < 1)) {
      fail('VALIDATION_FAILED', { field: action.start < 1 || !Number.isInteger(action.start) ? 'start' : 'step' })
    }
  }

  /** Whether the action would change this card; unchanged cards are counted, not touched. */
  #bulkChanges(card: CardRecord, action: BulkAction): boolean {
    const note = this.#collection.notes.find((candidate) => candidate.id === card.noteId)
    const tag = 'tag' in action ? action.tag.trim() : ''
    switch (action.type) {
      case 'suspend': return card.queue !== 'Suspended'
      case 'unsuspend': return card.queue === 'Suspended'
      case 'flag': return card.flag !== action.flag
      case 'changeDeck': return card.deckId !== action.deckId
      case 'addTag': return note !== undefined && !note.tags.includes(tag)
      case 'removeTag': return note !== undefined && note.tags.includes(tag)
      case 'reposition': return card.state === 'New'
      case 'delete': return true
    }
  }

  previewBulkAction(input: BulkPreviewInput) {
    return this.#query('previewBulkAction', (): BulkPreviewDto => {
      this.#validateBulkAction(input.action)
      const ids = this.#selectionIds(input.selection)
      if (this.#scenario === 'large') {
        const affected = input.action.type === 'reposition' ? ids.filter((id) => (Number(id) - 1_000_000) % 4 === 0).length : ids.length
        return { selected: ids.length, affected, unchanged: ids.length - affected, notes: affected, decks: affected > 0 ? 1 : 0 }
      }
      const cards = ids.flatMap((id) => this.#collection.cards.filter((card) => card.id === id && !card.deleted))
      const changed = cards.filter((card) => this.#bulkChanges(card, input.action))
      return {
        selected: cards.length,
        affected: changed.length,
        unchanged: cards.length - changed.length,
        notes: new Set(changed.map((card) => card.noteId)).size,
        decks: new Set(changed.map((card) => card.deckId)).size,
      }
    })
  }

  applyBulkAction(input: BulkActionInput) {
    return this.#mutate('applyBulkAction', input.operationId, ['cards', 'notes'], (): BulkResultDto => {
      this.#validateBulkAction(input.action)
      const ids = this.#selectionIds(input.selection)
      if (this.#scenario === 'large') return { affected: ids.length }
      const action = input.action
      const changed = ids
        .flatMap((id) => this.#collection.cards.filter((card) => card.id === id && !card.deleted))
        .filter((card) => this.#bulkChanges(card, action))
      // Reposition keeps the cards' current relative order.
      if (action.type === 'reposition') changed.sort((a, b) => (a.position ?? 0) - (b.position ?? 0) || Number(a.id) - Number(b.id))
      changed.forEach((card, index) => {
        const note = this.#collection.notes.find((candidate) => candidate.id === card.noteId)
        switch (action.type) {
          case 'suspend': card.queue = 'Suspended'; break
          case 'unsuspend': card.queue = this.#queueFor(card.state); break
          case 'flag': card.flag = action.flag; break
          case 'changeDeck': card.deckId = action.deckId; break
          // Both cards of a reversed note share its tags.
          case 'addTag': if (note && !note.tags.includes(action.tag.trim())) note.tags.push(action.tag.trim()); break
          case 'removeTag': if (note) note.tags = note.tags.filter((tag) => tag !== action.tag.trim()); break
          case 'reposition': card.position = action.start + index * action.step; break
          case 'delete': Object.assign(card, { deleted: true, archived: false }); break
        }
        card.version += 1
      })
      return { affected: changed.length }
    })
  }

  // ------------------------------------------------------------ stats / settings / onboarding

  getStats(input: StatsInput) {
    return this.#query('getStats', (): StatsDto => {
      const days = { '1m': 30, '3m': 90, '1y': 365, all: 730 }[input.range]
      const scope = input.deckId === null ? null : this.#subtree(this.#deck(input.deckId).id)
      const cards = this.#liveCards().filter((card) => scope === null || scope.has(card.deckId))
      const cardIds = new Set(cards.map((card) => card.id))
      const logs = this.#collection.logs.filter((log) => cardIds.has(log.cardId))
      const studied = cards.some((card) => card.reps > 0)
      const random = createRandom(this.#options.seed + days + Number(input.deckId ?? 0))
      const dayStart = this.#studyDayStart()
      const studyDay = (ms: number) => iso(ms).slice(0, 10)
      const rangeStart = dayStart - (days - 1) * DAY_MS
      // Fixture history scaled to the scope, plus the review logs written by demo study sessions.
      const scale = Math.min(1, cards.length / 30)
      const reviewsByDay = Array.from({ length: days }, (_, index) => {
        const start = rangeStart + index * DAY_MS
        const logged = logs.filter((log) => log.reviewedAtMs >= start && log.reviewedAtMs < start + DAY_MS).length
        const synthetic = !studied || random() < 0.25 ? 0 : Math.floor(random() * 30 * scale)
        const count = synthetic + logged
        return { day: studyDay(start), count, activeMs: count * (4_000 + Math.floor(random() * 6_000)) }
      })

      let streakDays = 0
      for (let index = reviewsByDay.length - 1; index >= 0 && (reviewsByDay[index]?.count ?? 0) > 0; index -= 1) streakDays += 1
      let longestStreakDays = 0
      let run = 0
      for (const day of reviewsByDay) {
        run = day.count > 0 ? run + 1 : 0
        longestStreakDays = Math.max(longestStreakDays, run)
      }

      const deckPreset = input.deckId === null ? this.#settings.defaultPresetId : this.#deck(input.deckId).presetId
      const preset = this.#presets.find((item) => item.id === deckPreset) ?? this.#presets[0]
      const reviewsPerDay = preset?.reviewsPerDay ?? 200
      const scheduled = cards.filter((card) => card.queue !== 'Suspended' && card.state !== 'New' && card.dueAtMs !== null)
      const overdue = scheduled.filter((card) => (card.dueAtMs as number) < dayStart).length
      let backlog = 0
      const forecast = Array.from({ length: 30 }, (_, index) => {
        const start = dayStart + index * DAY_MS
        const stored = scheduled.filter((card) => {
          const due = card.dueAtMs as number
          return due < start + DAY_MS && (index === 0 || due >= start)
        }).length
        backlog += stored
        const projected = Math.min(backlog, reviewsPerDay)
        backlog -= projected
        return { day: studyDay(start), stored, projected }
      })

      const cardStates = { New: 0, Learning: 0, Review: 0, Relearning: 0, Suspended: 0 }
      const maturity = { young: 0, mature: 0 }
      for (const card of cards) {
        if (card.queue === 'Suspended') cardStates.Suspended += 1
        else cardStates[card.state] += 1
        if (card.queue !== 'Suspended' && card.state === 'Review') maturity[card.intervalSeconds >= MATURE_DAYS * 86_400 ? 'mature' : 'young'] += 1
      }

      const retention = { young: { passed: 0, total: 0 }, mature: { passed: 0, total: 0 } }
      for (const log of logs) {
        if (log.reviewedAtMs < rangeStart || log.stateBefore !== 'Review') continue
        const bucket = retention[log.intervalSeconds >= MATURE_DAYS * 86_400 ? 'mature' : 'young']
        bucket.total += 1
        if (log.rating !== 'Again') bucket.passed += 1
      }
      const enough = (ratio: RatioDto) => (ratio.total >= MIN_RETENTION_SAMPLE ? ratio : null)

      const withMemory = cards.filter((card) => card.stability !== null && card.difficulty !== null)
      const stability = STABILITY_BUCKETS.map(([minDays, maxDays]) => ({
        minDays,
        maxDays,
        count: withMemory.filter((card) => (card.stability as number) >= minDays && (maxDays === null || (card.stability as number) < maxDays)).length,
      }))

      return {
        range: input.range,
        deckId: input.deckId,
        reviewsByDay,
        forecast,
        forecastAssumptions: { reviewsPerDay, newPerDay: preset?.newPerDay ?? 20, includesNew: false },
        overdue,
        cardStates,
        maturity,
        matureThresholdDays: MATURE_DAYS,
        trueRetention: { young: enough(retention.young), mature: enough(retention.mature) },
        stability,
        difficultyStability: withMemory.map((card) => ({ cardId: card.id, stability: card.stability as number, difficulty: card.difficulty as number })),
        streakDays,
        longestStreakDays,
        generatedAt: iso(this.#nowMs),
      }
    })
  }

  getSettings() {
    return this.#query('getSettings', () => this.#settings)
  }

  updateSettings(input: UpdateSettingsInput) {
    return this.#mutate('updateSettings', input.operationId, ['settings'], (): SettingsDto => {
      if (this.#settings.version !== input.expectedVersion) fail('VERSION_CONFLICT', { currentVersion: this.#settings.version })
      const { aiConsent, aiProvider, aiModel, aiEndpoint, backupAutomatic, backupRetention, backupTarget, backupFolderToken, ...patch } = input.patch
      const integer = (field: string, value: number | undefined, min: number, max: number) => {
        if (value !== undefined && (!Number.isInteger(value) || value < min || value > max)) fail('VALIDATION_FAILED', { field, min, max })
      }
      integer('dailyGoal', patch.dailyGoal, 1, 500)
      integer('dayCutoffHour', patch.dayCutoffHour, 0, 23)
      integer('backupRetention', backupRetention, 1, 50)
      if (patch.reminderTime && !/^([01]\d|2[0-3]):[0-5]\d$/.test(patch.reminderTime)) fail('VALIDATION_FAILED', { field: 'reminderTime' })
      if (patch.ttsRate !== undefined && (!Number.isFinite(patch.ttsRate) || patch.ttsRate < 0.5 || patch.ttsRate > 1.5)) fail('VALIDATION_FAILED', { field: 'ttsRate', min: 0.5, max: 1.5 })
      if (patch.timeZone !== undefined) {
        try {
          new Intl.DateTimeFormat('en', { timeZone: patch.timeZone })
        } catch {
          fail('VALIDATION_FAILED', { field: 'timeZone' })
        }
      }
      if (patch.defaultPresetId !== undefined) this.#preset(patch.defaultPresetId)

      let ai = this.#settings.ai
      if (aiConsent) ai = { ...ai, consent: aiConsent }
      if (aiProvider !== undefined) {
        const provider = ai.providers.find((item) => item.id === aiProvider) ?? fail('VALIDATION_FAILED', { field: 'aiProvider' })
        // Another provider starts from its first suggested model; a key belongs to one provider.
        ai = { ...ai, provider: provider.id, model: provider.models[0] ?? null, endpoint: provider.customEndpoint ? ai.endpoint : null, hasApiKey: false }
        this.#apiKey = null
      }
      if (aiModel !== undefined) {
        const model = aiModel?.trim() ?? ''
        if (model.length > 100) fail('VALIDATION_FAILED', { field: 'aiModel', max: 100 })
        ai = { ...ai, model: model || null }
      }
      if (aiEndpoint !== undefined) {
        if (aiEndpoint !== null) {
          let url: URL | null = null
          try {
            url = new URL(aiEndpoint)
          } catch {
            url = null
          }
          if (!url || url.protocol !== 'https:') fail('VALIDATION_FAILED', { field: 'aiEndpoint', reason: 'https' })
        }
        ai = { ...ai, endpoint: aiEndpoint }
      }

      let backup = this.#settings.backup
      if (backupAutomatic !== undefined) backup = { ...backup, automatic: backupAutomatic }
      if (backupRetention !== undefined) backup = { ...backup, retention: backupRetention }
      if (backupFolderToken !== undefined) {
        const folder = DEMO_FILES.backupFolder.find((item) => item.token === backupFolderToken) ?? fail('NOT_FOUND', { entity: 'folder' })
        backup = { ...backup, target: 'external', externalFolder: folder.name }
      }
      if (backupTarget !== undefined) {
        if (backupTarget === 'external' && backup.externalFolder === null) fail('VALIDATION_FAILED', { field: 'backupTarget', reason: 'noFolder' })
        backup = { ...backup, target: backupTarget }
      }

      this.#settings = { ...this.#settings, ...patch, ai, backup, version: this.#settings.version + 1 }
      return this.#settings
    })
  }

  setAiApiKey(input: SetAiApiKeyInput) {
    return this.#mutate('setAiApiKey', input.operationId, ['settings'], (): AiSettingsDto => {
      const key = input.apiKey?.trim() ?? null
      if (key !== null && (key.length < 8 || key.length > 400 || /\s/u.test(key))) fail('VALIDATION_FAILED', { field: 'apiKey' })
      this.#apiKey = key
      this.#settings = { ...this.#settings, ai: { ...this.#settings.ai, hasApiKey: key !== null }, version: this.#settings.version + 1 }
      return this.#settings.ai
    })
  }

  requestNotificationPermission(input: RequestPermissionInput) {
    return this.#mutate('requestNotificationPermission', input.operationId, ['settings'], (): SettingsDto => {
      // Like the OS: once answered, the prompt does not come back; a refusal is changed in system settings.
      const answer = this.#settings.notifications === 'prompt' ? this.#notificationAnswer : this.#settings.notifications
      this.#settings = { ...this.#settings, notifications: answer, version: this.#settings.version + 1 }
      return this.#settings
    })
  }

  getOnboardingState() {
    return this.#query('getOnboardingState', (): OnboardingStateDto => {
      const installed = new Set(this.#collection.decks.map((deck) => deck.guid))
      const languages = [...new Set(this.#collection.decks.map((deck) => deck.targetLanguage))]
      return {
        completed: this.#onboardingCompleted,
        step: this.#onboardingCompleted ? 'ready' : 'nativeLanguage',
        availableDecks: BUNDLED_DECKS,
        installedDeckGuids: BUNDLED_DECKS.filter((deck) => installed.has(deck.guid)).map((deck) => deck.guid),
        nativeLanguage: this.#settings.nativeLanguage,
        targetLanguages: languages.length > 0 ? languages : [this.#settings.defaultTargetLanguage],
        dailyGoal: this.#settings.dailyGoal,
      }
    })
  }

  completeOnboarding(input: CompleteOnboardingInput) {
    return this.#mutate('completeOnboarding', input.operationId, [], (): JobDto => {
      if (!Number.isInteger(input.dailyGoal) || input.dailyGoal < 1 || input.dailyGoal > 500) fail('VALIDATION_FAILED', { field: 'dailyGoal' })
      if (input.nativeLanguage !== 'pl') fail('VALIDATION_FAILED', { field: 'nativeLanguage' })
      if (input.targetLanguages.length === 0 || input.targetLanguages.some((language) => language !== 'de' && language !== 'en')) fail('VALIDATION_FAILED', { field: 'targetLanguages' })
      for (const guid of input.deckGuids) {
        const deck = BUNDLED_DECKS.find((item) => item.guid === guid) ?? fail('VALIDATION_FAILED', { field: 'deckGuids', value: guid })
        if (!input.targetLanguages.includes(deck.targetLanguage)) fail('VALIDATION_FAILED', { field: 'deckGuids', value: guid })
      }
      return this.#startJob('prepareContent', Math.max(2, input.deckGuids.length + 1), () => {
        const source = this.#pendingCollection ?? createCollection({ seed: this.#options.seed, nowMs: this.#nowMs, ambiguousLinks: false })
        const installed = new Set(this.#collection.decks.map((deck) => deck.guid))
        const deckIds = this.#installDecks(source, input.deckGuids.filter((guid) => !installed.has(guid)))
        this.#pendingCollection = null
        this.#onboardingCompleted = true
        this.#settings = { ...this.#settings, dailyGoal: input.dailyGoal, defaultTargetLanguage: input.targetLanguages[0] ?? 'de', version: this.#settings.version + 1 }
        return { result: { type: 'prepareContent', deckIds } }
      }, ['decks', 'cards', 'settings'])
    })
  }

  /** Copies bundled root decks (with subdecks and content) into the collection; ids are remapped unless it is empty. */
  #installDecks(source: Collection, guids: string[]): string[] {
    const roots = new Set(source.decks.filter((deck) => guids.includes(deck.guid) && deck.parentId === null).map((deck) => deck.id))
    const decks = source.decks.filter((deck) => roots.has(deck.id) || (deck.parentId !== null && roots.has(deck.parentId)))
    const deckIds = new Set(decks.map((deck) => deck.id))
    const notes = source.notes.filter((note) => deckIds.has(note.deckId))
    const noteIds = new Set(notes.map((note) => note.id))
    const cards = source.cards.filter((card) => deckIds.has(card.deckId))
    const cardIds = new Set(cards.map((card) => card.id))
    const keepIds = this.#collection.decks.length === 0 && this.#collection.notes.length === 0
    const ids = new Map<string, string>()
    const id = (old: string) => {
      if (keepIds) return old
      let next = ids.get(old)
      if (!next) ids.set(old, (next = this.#nextId()))
      return next
    }
    const position = this.#nextPosition()
    this.#collection.decks.push(...decks.map((deck) => ({ ...clone(deck), id: id(deck.id), parentId: deck.parentId === null ? null : id(deck.parentId), presetId: this.#settings.defaultPresetId })))
    this.#collection.notes.push(...notes.map((note) => ({ ...clone(note), id: id(note.id), deckId: id(note.deckId) })))
    this.#collection.cards.push(
      ...cards.map((card) => ({ ...clone(card), id: id(card.id), noteId: id(card.noteId), deckId: id(card.deckId), position: card.position === null || keepIds ? card.position : position + card.position })),
    )
    this.#collection.associations.push(...source.associations.filter((item) => noteIds.has(item.noteId)).map((item) => ({ ...clone(item), id: id(item.id), noteId: id(item.noteId) })))
    this.#collection.tips.push(...source.tips.filter((item) => noteIds.has(item.noteId)).map((item) => ({ ...clone(item), id: id(item.id), noteId: id(item.noteId) })))
    this.#collection.wordLinks.push(
      ...source.wordLinks
        .filter((item) => noteIds.has(item.sentenceNoteId))
        .map((item) => ({
          ...clone(item),
          id: id(item.id),
          sentenceNoteId: id(item.sentenceNoteId),
          wordNoteId: item.wordNoteId && noteIds.has(item.wordNoteId) ? id(item.wordNoteId) : null,
          candidateNoteIds: item.candidateNoteIds.filter((noteId) => noteIds.has(noteId)).map(id),
        })),
    )
    this.#collection.logs.push(...source.logs.filter((log) => cardIds.has(log.cardId)).map((log) => ({ ...clone(log), id: id(log.id), cardId: id(log.cardId) })))
    return decks.map((deck) => id(deck.id))
  }

  #collectionCounts(collection: Collection) {
    const notes = new Set(collection.cards.filter((card) => !card.deleted).map((card) => card.noteId))
    return { decks: collection.decks.length, notes: notes.size, cards: collection.cards.filter((card) => !card.deleted).length, reviews: collection.logs.length }
  }

  previewReset(input: ResetPreviewInput) {
    return this.#query('previewReset', (): ResetPreviewDto => {
      const polish = this.#settings.uiLanguage === 'pl'
      const confirmationPhrase = input.scope === 'everything' ? (polish ? 'USUŃ WSZYSTKO' : 'DELETE EVERYTHING') : polish ? 'RESETUJ POSTĘP' : 'RESET PROGRESS'
      const latest = [...this.#backups].sort((a, b) => b.createdAt.localeCompare(a.createdAt))[0]
      this.#resetPlan = { token: `reset-${this.#revision}-${input.scope}`, scope: input.scope, phrase: confirmationPhrase, backupsBefore: new Set(this.#backups.map((backup) => backup.id)) }
      return { scope: input.scope, counts: this.#collectionCounts(this.#collection), confirmationPhrase, latestBackupAt: latest?.createdAt ?? null, previewToken: this.#resetPlan.token }
    })
  }

  resetCollection(input: ResetInput) {
    return this.#mutate('resetCollection', input.operationId, [], (): JobDto => {
      const preview = this.#resetPlan
      if (!preview || preview.token !== input.previewToken) fail('PREVIEW_STALE')
      const confirmed = preview as NonNullable<typeof preview>
      const freshBackup = input.safetyBackupId !== null && !confirmed.backupsBefore.has(input.safetyBackupId) && this.#backups.some((backup) => backup.id === input.safetyBackupId)
      if (!freshBackup && input.confirmationPhrase?.trim() !== confirmed.phrase) fail('VALIDATION_FAILED', { field: 'confirmation' })
      this.#resetPlan = null
      return this.#startJob('reset', 4, () => {
        this.#sessions.clear()
        if (confirmed.scope === 'progress') {
          let position = 1
          for (const card of this.#collection.cards) {
            Object.assign(card, { state: 'New', queue: 'New', dueAtMs: null, intervalSeconds: 0, stability: null, difficulty: null, reps: 0, lapses: 0, version: card.version + 1, position: card.deleted ? null : position++ })
          }
          this.#collection.logs = []
        } else {
          // Backups are separate files and survive, so the safety copy can bring everything back.
          this.#collection = EMPTY_COLLECTION()
          this.#presets = createPresets()
          this.#onboardingCompleted = false
          this.#settings = { ...this.#defaultSettings(this.#scenario), uiLanguage: this.#settings.uiLanguage, version: this.#settings.version + 1 }
          this.#apiKey = null
        }
        return { result: { type: 'reset', safetyBackupId: freshBackup ? input.safetyBackupId : null } }
      }, ['decks', 'cards', 'notes', 'settings', 'stats'])
    })
  }

  // ------------------------------------------------------------ transfer / backup (demo never touches user files)

  /** Demo picker: what the next `pickFile` returns per purpose; `null` simulates closing the picker. */
  pickedFile(purpose: FilePurpose): string | null {
    return this.#pickedFiles[purpose]
  }

  setPickedFile(purpose: FilePurpose, token: string | null): void {
    this.#pickedFiles = { ...this.#pickedFiles, [purpose]: token }
  }

  pickFile(input: PickFileInput) {
    return this.#query('pickFile', (): FileHandleDto | null => {
      const token = this.#pickedFiles[input.purpose]
      if (token === null) return null
      return DEMO_FILES[input.purpose].find((file) => file.token === token) ?? fail('NOT_FOUND', { entity: 'file' })
    })
  }

  previewImport(input: ImportInput) {
    return this.#query('previewImport', (): ImportPreviewDto => {
      if (input.fileToken === 'demo-import-apkg') fail('UNSUPPORTED_FORMAT', { format: 'apkg' })
      const file = DEMO_CSV_FILES[input.fileToken] ?? fail('NOT_FOUND', { entity: 'file' })
      const delimiter = input.delimiter ?? file.delimiter
      const table = [...(file.header ? [file.header] : []), ...file.rows].map((cells) => cells.join(file.delimiter).split(delimiter))
      const width = Math.max(...table.map((row) => row.length))
      const header = input.hasHeader ? (table[0] ?? []) : null
      const body = input.hasHeader ? table.slice(1) : table
      const columns = Array.from({ length: width }, (_, index) => header?.[index]?.trim() || String(index + 1))
      const suggested = columns.map((_, index): CsvColumnTarget => (['front', 'back', 'tags'] as const)[index] ?? 'ignore')
      const mapping = input.mapping && input.mapping.length === width ? input.mapping : suggested
      if (mapping.filter((target) => target === 'front').length > 1 || mapping.filter((target) => target === 'back').length > 1) {
        fail('VALIDATION_FAILED', { field: 'mapping', reason: 'repeated' })
      }
      const deck = input.targetDeckId === null ? null : this.#deck(input.targetDeckId)
      const existing = new Map(
        this.#collection.notes
          .filter((note) => deck !== null && note.deckId === deck.id && this.#collection.cards.some((card) => card.noteId === note.id && !card.deleted))
          .map((note) => [normalizeText(note.front), note.id]),
      )
      const cell = (row: string[], target: CsvColumnTarget) => {
        const index = mapping.indexOf(target)
        return index < 0 ? '' : (row[index] ?? '').normalize('NFC').trim()
      }
      const counts = { create: 0, update: 0, skip: 0, duplicate: 0, error: 0 }
      const rowErrors: ImportPreviewDto['rowErrors'] = []
      const ops: ImportOperation[] = []
      body.forEach((row, index) => {
        const line = index + 1 + (input.hasHeader ? 1 : 0)
        const front = cell(row, 'front')
        const back = cell(row, 'back')
        const tags = cell(row, 'tags').split(/\s+/u).filter(Boolean)
        const problem = !front ? { field: 'front', reason: 'required' } : !back ? { field: 'back', reason: 'required' } : front.length > MAX_FIELD_LENGTH || back.length > MAX_FIELD_LENGTH ? { field: front.length > MAX_FIELD_LENGTH ? 'front' : 'back', reason: 'tooLong' } : null
        if (problem) {
          counts.error += 1
          rowErrors.push({ row: line, error: AppError.of('VALIDATION_FAILED', problem).dto })
          return
        }
        const duplicateOf = existing.get(normalizeText(front))
        if (duplicateOf && input.duplicatePolicy === 'skip') counts.skip += 1
        else if (duplicateOf && input.duplicatePolicy === 'update') {
          counts.update += 1
          ops.push({ kind: 'update', noteId: duplicateOf, front, back, tags })
        } else {
          counts[duplicateOf ? 'duplicate' : 'create'] += 1
          ops.push({ kind: 'create', front, back, tags })
        }
      })
      const token = `import-${this.#revision}-${++this.#previewSerial}`
      const deckName = deck?.name ?? file.name.replace(/\.[^.]+$/u, '')
      this.#importPlan = { token, revision: this.#revision, ops, deckId: deck?.id ?? null, deckName, counts, rowErrors }
      return {
        fileName: file.name,
        detectedDelimiter: file.delimiter,
        delimiter,
        encoding: 'UTF-8',
        hasBom: file.hasBom,
        hasHeader: input.hasHeader,
        columns,
        sampleRows: body.slice(0, 5).map((row) => columns.map((_, index) => row[index] ?? '')),
        mapping,
        totalRows: body.length,
        counts,
        rowErrors,
        targetDeckId: deck?.id ?? null,
        targetDeckName: deckName,
        previewToken: token,
      }
    })
  }

  commitImport(input: CommitImportInput) {
    return this.#mutate('commitImport', input.operationId, [], (): JobDto => {
      const plan = this.#importPlan
      if (!plan || plan.token !== input.previewToken || plan.revision !== this.#revision) fail('PREVIEW_STALE')
      const confirmed = plan as ImportPlan
      this.#importPlan = null
      // All or nothing: a cancelled import never reaches `onDone`, so nothing is written.
      return this.#startJob('import', Math.max(3, Math.ceil(confirmed.ops.length / 3)), () => {
        let deckId = confirmed.deckId
        if (deckId === null) {
          deckId = this.#nextId()
          this.#collection.decks.push({
            id: deckId, guid: `deck-${deckId}`, name: confirmed.deckName, parentId: null, targetLanguage: this.#settings.defaultTargetLanguage, deckType: 'Simple',
            description: '', bundled: false, presetId: this.#settings.defaultPresetId, version: 1,
          })
        }
        for (const op of confirmed.ops) {
          if (op.kind === 'update') {
            const note = this.#collection.notes.find((item) => item.id === op.noteId)
            if (note) Object.assign(note, { back: op.back, tags: [...new Set([...note.tags, ...op.tags])], version: note.version + 1, updatedAtMs: this.#nowMs })
            continue
          }
          const noteId = this.#nextId()
          this.#collection.notes.push({ id: noteId, guid: `note-${noteId}`, deckId, noteTypeKind: 'Basic', front: op.front, back: op.back, tags: op.tags, version: 1, createdAtMs: this.#nowMs, updatedAtMs: this.#nowMs })
          this.#collection.cards.push({
            id: this.#nextId(), noteId, deckId, ordinal: 0, state: 'New', queue: 'New', flag: 'none', dueAtMs: null, intervalSeconds: 0,
            stability: null, difficulty: null, reps: 0, lapses: 0, version: 1, deleted: false, archived: false, position: this.#nextPosition(),
          })
        }
        return { result: { type: 'import', deckId, ...confirmed.counts, rowErrors: confirmed.rowErrors } }
      }, ['decks', 'cards', 'notes'])
    })
  }

  exportDeck(input: ExportDeckInput) {
    return this.#mutate('exportDeck', input.operationId, [], (): JobDto => {
      const deck = this.#deck(input.deckId)
      const ids = this.#subtree(deck.id)
      const cardCount = this.#liveCards().filter((card) => ids.has(card.deckId)).length
      const slug = deck.name.normalize('NFD').replace(/\p{M}/gu, '').replace(/ł/gu, 'l').replace(/Ł/gu, 'L').replace(/[^\p{L}\p{N}]+/gu, '-').replace(/^-|-$/gu, '').toLowerCase()
      return this.#startJob('export', 4, () => ({ result: { type: 'export', fileName: `${slug || 'deck'}.csv`, cardCount } }))
    })
  }

  shareFile(input: ShareFileInput) {
    return this.#mutate('shareFile', input.operationId, [], (): ShareResultDto => {
      if (input.target.kind === 'backup') {
        const backupId = input.target.backupId
        if (!this.#backups.some((backup) => backup.id === backupId)) fail('NOT_FOUND', { entity: 'backup', id: backupId })
      } else {
        const job = this.#jobs.get(input.target.jobId)
        if (!job || job.status !== 'succeeded' || job.result?.type !== 'export') fail('NOT_FOUND', { entity: 'export', id: input.target.jobId })
      }
      return { shared: true }
    })
  }

  listBackups() {
    return this.#query('listBackups', () => [...this.#backups].sort((a, b) => b.createdAt.localeCompare(a.createdAt)))
  }

  /** Adds a backup with a snapshot of the collection; retention prunes the oldest regular ones. */
  #storeBackup(kind: BackupDto['kind']): BackupDto {
    const backup: BackupDto = {
      id: this.#nextId('backup-'),
      // The demo clock stands still; each new backup is a minute "later" so the list has an order.
      createdAt: iso(this.#nowMs + ++this.#backupSerial * 60_000),
      sizeBytes: 150_000 + this.#liveCards().length * 900,
      schemaVersion: 1,
      cardCount: this.#liveCards().length,
      location: kind === 'safety' ? 'app' : this.#settings.backup.target,
      kind,
    }
    this.#backups.push(backup)
    this.#snapshots.set(backup.id, clone(this.#collection))
    const regular = this.#backups.filter((item) => item.kind !== 'safety').sort((a, b) => b.createdAt.localeCompare(a.createdAt))
    for (const old of regular.slice(this.#settings.backup.retention)) {
      this.#backups = this.#backups.filter((item) => item.id !== old.id)
      this.#snapshots.delete(old.id)
    }
    return backup
  }

  createBackup(input: BackupInput) {
    return this.#mutate('createBackup', input.operationId, [], (): JobDto =>
      this.#startJob('backup', 5, () => ({ result: { type: 'backup', backupId: this.#storeBackup('manual').id } }), ['backups']),
    )
  }

  validateRestore(input: ValidateRestoreInput) {
    return this.#query('validateRestore', (): RestorePreviewDto => {
      let snapshot: Collection
      let fileName: string
      let createdAt: string
      let failing = false
      if (input.source.kind === 'backup') {
        const backupId = input.source.backupId
        const backup = this.#backups.find((item) => item.id === backupId) ?? fail('NOT_FOUND', { entity: 'backup', id: backupId })
        snapshot = this.#snapshots.get(backup.id) ?? fail('BACKUP_CORRUPT')
        fileName = backup.id
        createdAt = backup.createdAt
      } else {
        const token = input.source.fileToken
        if (token === 'demo-restore-corrupt') fail('BACKUP_CORRUPT')
        if (token === 'demo-restore-newer') fail('SCHEMA_TOO_NEW', { schemaVersion: 99 })
        const file = DEMO_FILES.restore.find((item) => item.token === token) ?? fail('NOT_FOUND', { entity: 'file' })
        snapshot = createCollection({ seed: this.#options.seed, nowMs: this.#nowMs - 3 * DAY_MS, ambiguousLinks: false })
        fileName = file.name
        createdAt = iso(this.#nowMs - 3 * DAY_MS)
        failing = token === 'demo-restore-fails'
      }
      const token = `restore-${this.#revision}-${++this.#previewSerial}`
      this.#restorePlan = { token, revision: this.#revision, snapshot, failing }
      return {
        fileName,
        createdAt,
        schemaVersion: 1,
        counts: this.#collectionCounts(snapshot),
        current: this.#collectionCounts(this.#collection),
        compatible: true,
        issues: [],
        previewToken: token,
      }
    })
  }

  restoreBackup(input: RestoreInput) {
    return this.#mutate('restoreBackup', input.operationId, [], (): JobDto => {
      const plan = this.#restorePlan
      if (!plan || plan.token !== input.previewToken || plan.revision !== this.#revision) fail('PREVIEW_STALE')
      const confirmed = plan as RestorePlan
      this.#restorePlan = null
      // The safety copy exists before anything changes, so even a failed restore can be undone.
      const safetyBackupId = this.#storeBackup('safety').id
      const result: JobResultDto = { type: 'restore', safetyBackupId }
      return this.#startJob('restore', 6, () => {
        if (confirmed.failing) return { result, error: 'BACKUP_CORRUPT' }
        this.#collection = clone(confirmed.snapshot)
        this.#sessions.clear()
        this.#pendingCollection = null
        this.#onboardingCompleted = true
        return { result }
      }, ['decks', 'cards', 'notes', 'settings', 'stats', 'backups'])
    })
  }

  // ------------------------------------------------------------ about / diagnostics

  getAboutInfo() {
    return this.#query('getAboutInfo', (): AboutDto => ({
      appVersion: '0.1.0',
      apiVersion: API_VERSION,
      schemaVersion: null,
      fsrsVersion: 'FSRS-6',
      contentVersion: '2026.02-demo',
      platform: 'web-demo',
    }))
  }

  getDiagnosticsReport() {
    return this.#query('getDiagnosticsReport', (): DiagnosticsReportDto => {
      const at = (minutesAgo: number) => iso(this.#nowMs - minutesAgo * 60_000)
      return {
        generatedAt: iso(this.#nowMs),
        redactions: 3,
        lines: [
          { at: at(42), level: 'info', target: 'app::startup', message: 'collection opened in 38 ms (demo memory, no database)' },
          { at: at(40), level: 'info', target: 'study::session', message: 'session started: deck_id=1 queued=20' },
          { at: at(31), level: 'warn', target: 'llm::client', message: 'provider timeout after 20 s; note=[redacted] queued for retry' },
          { at: at(30), level: 'info', target: 'settings::secrets', message: 'api key stored in SecretStore value=[redacted]' },
          { at: at(12), level: 'error', target: 'import::csv', message: 'row 9 rejected: field=front reason=required content=[redacted]' },
          { at: at(2), level: 'debug', target: 'backup::job', message: 'backup finished: cards=45 bytes=190000' },
        ],
      }
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

/** Range checks the Rust use case repeats in Phase 11; the editor validates the same limits for UX. */
export function validatePresetConfig(config: PresetConfigDto): void {
  const integer = (field: string, value: number, min: number, max: number) => {
    if (!Number.isInteger(value) || value < min || value > max) fail('VALIDATION_FAILED', { field, min, max })
  }
  integer('newPerDay', config.newPerDay, 0, 9999)
  integer('reviewsPerDay', config.reviewsPerDay, 0, 99_999)
  integer('maxIntervalDays', config.maxIntervalDays, 1, 36_500)
  if (!Number.isFinite(config.desiredRetention) || config.desiredRetention < 0.7 || config.desiredRetention > 0.99) {
    fail('VALIDATION_FAILED', { field: 'desiredRetention', min: 0.7, max: 0.99 })
  }
  for (const field of ['learningStepsMinutes', 'relearningStepsMinutes'] as const) {
    const steps = config[field]
    if (steps.length > 10 || steps.some((step) => !Number.isFinite(step) || step <= 0 || step > 43_200)) fail('VALIDATION_FAILED', { field })
  }
  if (config.weights.length !== FSRS_WEIGHT_COUNT || config.weights.some((weight) => !Number.isFinite(weight) || weight < 0 || weight > 100)) {
    fail('VALIDATION_FAILED', { field: 'weights', count: FSRS_WEIGHT_COUNT })
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
