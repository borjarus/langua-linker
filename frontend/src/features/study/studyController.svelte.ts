import type { AppApi } from '../../api/appApi'
import { toErrorDto } from '../../api/errors'
import type {
  DeckCountsDto,
  ErrorDto,
  Id,
  Rating,
  RatingPreviewDto,
  ReviewResultDto,
  StudyCardDto,
  StudySessionDto,
} from '../../api/types'
import { newOperationId } from '../../services/operationId'

/** Plan §4.3 state machine; `rating_selected` is folded into the transition into `saving`. */
export type StudyPhase = 'idle' | 'loading' | 'question' | 'reveal' | 'saving' | 'save_failed' | 'finished' | 'error'

export interface StudyControllerDeps {
  api: AppApi
  /** Monotonic clock for active answer time. */
  now?: () => number
  operationId?: () => string
}

/**
 * Reference screen controller. Every mutation runs from an explicit method called by an event
 * handler; nothing here is triggered by `$effect`, so re-renders can never repeat a rating.
 */
export class StudyController {
  phase = $state<StudyPhase>('idle')
  sessionId = $state<Id | null>(null)
  card = $state<StudyCardDto | null>(null)
  preview = $state<RatingPreviewDto | null>(null)
  selected = $state<Rating | null>(null)
  counts = $state<DeckCountsDto | null>(null)
  reviewedCount = $state(0)
  undoAvailable = $state(false)
  error = $state<ErrorDto | null>(null)

  readonly canRate = $derived(this.phase === 'reveal' || this.phase === 'save_failed')
  readonly busy = $derived(this.phase === 'saving' || this.phase === 'loading')

  readonly #api: AppApi
  readonly #now: () => number
  readonly #operationId: () => string
  #pending: { operationId: string; rating: Rating } | null = null
  #shownAt = 0

  constructor({ api, now = () => performance.now(), operationId = newOperationId }: StudyControllerDeps) {
    this.#api = api
    this.#now = now
    this.#operationId = operationId
  }

  /** Starting a session is a mutation, so it is called from a click handler, never on mount. */
  static start(api: AppApi, deckId: Id | null, operationId = newOperationId()): Promise<StudySessionDto> {
    return api.startStudy({ operationId, deckId })
  }

  async resume(sessionId: Id): Promise<void> {
    this.phase = 'loading'
    this.error = null
    try {
      const session = await this.#api.resumeStudy(sessionId)
      this.sessionId = session.sessionId
      this.counts = session.counts
      this.reviewedCount = session.reviewedCount
      this.undoAvailable = session.undoAvailable
      this.#show(session.current)
    } catch (caught) {
      this.error = toErrorDto(caught)
      this.phase = 'error'
    }
  }

  async reveal(): Promise<void> {
    if (this.phase !== 'question' || !this.card || !this.sessionId) return
    this.phase = 'reveal'
    await this.#refreshPreview()
  }

  async rate(rating: Rating): Promise<void> {
    if (!this.canRate || !this.card || !this.sessionId) return
    // Retrying the same intent keeps its operationId; choosing another rating is a new intent.
    if (!this.#pending || this.#pending.rating !== rating) this.#pending = { operationId: this.#operationId(), rating }
    const { operationId } = this.#pending
    this.selected = rating
    this.phase = 'saving'
    this.error = null
    try {
      const result = await this.#api.rateCard({
        operationId,
        sessionId: this.sessionId,
        cardId: this.card.cardId,
        cardVersion: this.card.version,
        previewToken: this.preview?.previewToken ?? '',
        rating,
        answerMs: Math.round(this.#now() - this.#shownAt),
      })
      this.#pending = null
      this.#apply(result)
    } catch (caught) {
      this.error = toErrorDto(caught)
      this.phase = 'save_failed'
      if (this.error.code === 'PREVIEW_STALE') await this.#refreshPreview()
    }
  }

  async #refreshPreview(): Promise<void> {
    const card = this.card
    if (!card || !this.sessionId) return
    try {
      const preview = await this.#api.getRatingPreview({ sessionId: this.sessionId, cardId: card.cardId, cardVersion: card.version })
      if (this.card?.cardId === card.cardId) this.preview = preview
    } catch {
      // Previews are optional: the backend recomputes the schedule on commit.
      this.preview = null
    }
  }

  retry(): Promise<void> {
    return this.selected ? this.rate(this.selected) : Promise.resolve()
  }

  async undo(): Promise<void> {
    if (!this.undoAvailable || !this.sessionId || this.busy) return
    this.phase = 'saving'
    try {
      this.#apply(await this.#api.undoReview({ operationId: this.#operationId(), sessionId: this.sessionId }))
    } catch (caught) {
      this.error = toErrorDto(caught)
      this.phase = this.card ? 'question' : 'finished'
    }
  }

  #apply(result: ReviewResultDto): void {
    this.counts = result.counts
    this.reviewedCount = result.reviewedCount
    this.undoAvailable = result.undoAvailable
    this.#show(result.next)
  }

  #show(card: StudyCardDto | null): void {
    this.card = card
    this.preview = null
    this.selected = null
    this.phase = card ? 'question' : 'finished'
    this.#shownAt = this.#now()
  }
}
