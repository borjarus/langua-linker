import type { AppApi } from '../../api/appApi'
import { toErrorDto } from '../../api/errors'
import type {
  AssociationDto,
  CardActionKind,
  CardFlag,
  DeckCountsDto,
  ErrorDto,
  Id,
  Rating,
  RatingPreviewDto,
  ReviewResultDto,
  StudyCardDto,
  StudySessionAction,
  StudySessionDto,
  WordLinkDto,
} from '../../api/types'
import { newOperationId } from '../../services/operationId'

/**
 * Plan §4.3 state machine; `rating_selected` is folded into the transition into `saving`.
 * `lost`: the session no longer exists (demo memory reset, process death before Phase 10).
 */
export type StudyPhase = 'idle' | 'loading' | 'question' | 'reveal' | 'saving' | 'save_failed' | 'finished' | 'lost' | 'error'

export interface StudyControllerDeps {
  api: AppApi
  /** Monotonic clock for active answer time. */
  now?: () => number
  operationId?: () => string
}

/**
 * Study screen controller. Every mutation runs from an explicit method called by an event
 * handler; nothing here is triggered by `$effect`, so re-renders can never repeat a rating.
 */
export class StudyController {
  phase = $state<StudyPhase>('idle')
  sessionId = $state<Id | null>(null)
  deckId = $state<Id | null>(null)
  deckName = $state<string | null>(null)
  card = $state<StudyCardDto | null>(null)
  preview = $state<RatingPreviewDto | null>(null)
  selected = $state<Rating | null>(null)
  counts = $state<DeckCountsDto | null>(null)
  reviewedCount = $state(0)
  undoAvailable = $state(false)
  error = $state<ErrorDto | null>(null)
  paused = $state(false)
  /** Ratings already committed when an existing session was opened; > 0 means "resumed". */
  resumedWith = $state(0)
  /** A card action or session update is in flight. */
  acting = $state(false)
  /** Bumped when user input changed the visible side or card; the screen moves focus on it. */
  step = $state(0)

  readonly canRate = $derived(!this.paused && !this.acting && (this.phase === 'reveal' || this.phase === 'save_failed'))
  readonly busy = $derived(this.phase === 'saving' || this.phase === 'loading' || this.acting)
  /** An unfinished session is on screen: leaving asks first. */
  readonly inSession = $derived(['question', 'reveal', 'saving', 'save_failed'].includes(this.phase))
  /** Cards in this session so far: rated plus remaining. */
  readonly total = $derived(this.reviewedCount + (this.counts?.total ?? 0))

  readonly #api: AppApi
  readonly #now: () => number
  readonly #operationId: () => string
  #pending: { operationId: string; rating: Rating } | null = null
  #shownAt = 0
  #pausedMs = 0
  #pausedAt: number | null = null
  #previewRequest: Promise<void> | null = null

  constructor({ api, now = () => performance.now(), operationId = newOperationId }: StudyControllerDeps) {
    this.#api = api
    this.#now = now
    this.#operationId = operationId
  }

  /** Starting a session is a mutation, so it is called from a click handler, never on mount. */
  static start(api: AppApi, deckId: Id | null, operationId = newOperationId(), cardIds?: Id[]): Promise<StudySessionDto> {
    return api.startStudy(cardIds ? { operationId, deckId, cardIds } : { operationId, deckId })
  }

  async resume(sessionId: Id): Promise<void> {
    this.phase = 'loading'
    this.error = null
    try {
      const session = await this.#api.resumeStudy(sessionId)
      this.resumedWith = session.reviewedCount
      this.#load(session, false)
    } catch (caught) {
      this.error = toErrorDto(caught)
      this.phase = this.error.code === 'NOT_FOUND' ? 'lost' : 'error'
    }
  }

  async reveal(): Promise<void> {
    if (this.phase !== 'question' || this.paused || !this.card || !this.sessionId) return
    this.phase = 'reveal'
    this.step += 1
    await this.#refreshPreview()
  }

  async rate(rating: Rating): Promise<void> {
    const card = this.card
    const sessionId = this.sessionId
    if (!this.canRate || !card || !sessionId) return
    // Retrying the same intent keeps its operationId; choosing another rating is a new intent.
    if (!this.#pending || this.#pending.rating !== rating) this.#pending = { operationId: this.#operationId(), rating }
    const { operationId } = this.#pending
    this.selected = rating
    this.phase = 'saving'
    this.error = null
    const answerMs = this.#answerMs()
    try {
      // A fast tap after "Show answer" must not commit without the preview token it was shown.
      if (this.preview?.cardId !== card.cardId || this.preview.cardVersion !== card.version) await (this.#previewRequest ?? this.#refreshPreview())
      const result = await this.#api.rateCard({
        operationId,
        sessionId,
        cardId: card.cardId,
        cardVersion: card.version,
        previewToken: this.preview?.previewToken ?? '',
        rating,
        answerMs,
      })
      this.#pending = null
      this.#apply(result)
    } catch (caught) {
      this.error = toErrorDto(caught)
      this.phase = 'save_failed'
      if (this.error.code === 'PREVIEW_STALE') await this.#refreshPreview()
    }
  }

  retry(): Promise<void> {
    return this.selected ? this.rate(this.selected) : Promise.resolve()
  }

  async undo(): Promise<ErrorDto | null> {
    if (!this.undoAvailable || !this.sessionId || this.busy || this.paused) return null
    const before = this.phase
    this.phase = 'saving'
    try {
      this.#apply(await this.#api.undoReview({ operationId: this.#operationId(), sessionId: this.sessionId }))
      return null
    } catch (caught) {
      this.phase = before === 'saving' ? 'question' : before
      return toErrorDto(caught)
    }
  }

  /** Pausing stops the answer clock; the backend keeps the session resumable. */
  async pause(): Promise<ErrorDto | null> {
    if (this.paused || !this.inSession || this.busy) return null
    const error = await this.#updateSession('pause')
    if (!error) {
      this.paused = true
      this.suspendClock()
    }
    return error
  }

  async continue(): Promise<ErrorDto | null> {
    if (!this.paused || this.acting) return null
    const error = await this.#updateSession('resume')
    if (!error) {
      this.paused = false
      this.resumeClock()
    }
    return error
  }

  /** Ends the session early; every committed rating is kept. */
  async end(): Promise<ErrorDto | null> {
    if (!this.sessionId || this.busy || this.phase === 'finished') return null
    const error = await this.#updateSession('end')
    if (!error) {
      this.paused = false
      this.card = null
      this.preview = null
      this.phase = 'finished'
    }
    return error
  }

  /** Suspend, bury, delete or flag the current card. Hiding actions move on to the next card. */
  async applyAction(action: Exclude<CardActionKind, 'unsuspend' | 'unbury'>, flag?: CardFlag): Promise<ErrorDto | null> {
    const card = this.card
    if (!card || !this.sessionId || this.busy) return null
    this.acting = true
    try {
      const result = await this.#api.applyCardAction({ operationId: this.#operationId(), cardId: card.cardId, expectedVersion: card.version, action, ...(flag ? { flag } : {}) })
      if (action === 'flag') {
        this.card = { ...card, flag: result.flag, version: result.version }
        // The version is part of the preview token: refresh it so the next rating is not stale.
        if (this.phase === 'reveal' || this.phase === 'save_failed') await this.#refreshPreview()
      } else {
        this.#load(await this.#api.resumeStudy(this.sessionId), true)
      }
      return null
    } catch (caught) {
      return toErrorDto(caught)
    } finally {
      this.acting = false
    }
  }

  async resolveWordLink(link: WordLinkDto, wordNoteId: Id): Promise<ErrorDto | null> {
    try {
      const resolved = await this.#api.resolveWordLink({ operationId: this.#operationId(), linkId: link.id, wordNoteId })
      const card = this.card
      if (card) this.card = { ...card, wordLinks: card.wordLinks.map((item) => (item.id === resolved.id ? resolved : item)) }
      return null
    } catch (caught) {
      return toErrorDto(caught)
    }
  }

  /** Applies fresh associations if the note is still on screen. */
  setAssociations(noteId: Id, associations: AssociationDto[]): void {
    if (this.card?.noteId === noteId) this.card = { ...this.card, associations }
  }

  /** The app went to the background: time away is not answer time. */
  suspendClock(): void {
    this.#pausedAt ??= this.#now()
  }

  resumeClock(): void {
    if (this.#pausedAt === null || this.paused) return
    this.#pausedMs += this.#now() - this.#pausedAt
    this.#pausedAt = null
  }

  #answerMs(): number {
    const pausedNow = this.#pausedAt === null ? 0 : this.#now() - this.#pausedAt
    return Math.max(0, Math.round(this.#now() - this.#shownAt - this.#pausedMs - pausedNow))
  }

  async #updateSession(action: StudySessionAction): Promise<ErrorDto | null> {
    if (!this.sessionId) return null
    this.acting = true
    try {
      await this.#api.updateStudySession({ operationId: this.#operationId(), sessionId: this.sessionId, action })
      return null
    } catch (caught) {
      return toErrorDto(caught)
    } finally {
      this.acting = false
    }
  }

  #refreshPreview(): Promise<void> {
    const request = this.#loadPreview().finally(() => {
      if (this.#previewRequest === request) this.#previewRequest = null
    })
    this.#previewRequest = request
    return request
  }

  async #loadPreview(): Promise<void> {
    const card = this.card
    if (!card || !this.sessionId) return
    try {
      const preview = await this.#api.getRatingPreview({ sessionId: this.sessionId, cardId: card.cardId, cardVersion: card.version })
      if (this.card?.cardId === card.cardId && this.card.version === preview.cardVersion) this.preview = preview
    } catch {
      // Without a preview the buttons show no intervals; a stale token is refreshed on PREVIEW_STALE.
      this.preview = null
    }
  }

  #load(session: StudySessionDto, fromUser: boolean): void {
    this.sessionId = session.sessionId
    this.deckId = session.deckId
    this.deckName = session.deckName
    this.counts = session.counts
    this.reviewedCount = session.reviewedCount
    this.undoAvailable = session.undoAvailable
    this.paused = session.status === 'paused'
    if (this.paused) this.suspendClock()
    this.#show(session.status === 'finished' ? null : session.current, fromUser)
  }

  #apply(result: ReviewResultDto): void {
    this.counts = result.counts
    this.reviewedCount = result.reviewedCount
    this.undoAvailable = result.undoAvailable
    this.#show(result.next, true)
  }

  #show(card: StudyCardDto | null, fromUser: boolean): void {
    this.card = card
    this.preview = null
    this.selected = null
    this.phase = card ? 'question' : 'finished'
    this.#shownAt = this.#now()
    this.#pausedMs = 0
    this.#pausedAt = this.paused ? this.#shownAt : null
    if (fromUser) this.step += 1
  }
}
