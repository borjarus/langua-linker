import type { AppApi } from '../../api/appApi'
import { toErrorDto } from '../../api/errors'
import type { AssociationDto, ErrorDto, Id, JobEventDto } from '../../api/types'
import { newOperationId } from '../../services/operationId'

export interface AssociationGeneratorDeps {
  api: AppApi
  /** Receives fresh associations of a note; the study controller ignores notes no longer shown. */
  onupdate: (noteId: Id, associations: AssociationDto[]) => void
  operationId?: () => string
}

/**
 * Association generation for the study screen. Generation is a background job: it never blocks
 * rating, survives moving to the next card, and reports progress through job events that the
 * screen forwards to `handleEvent` (the subscription itself lives in the component's `$effect`).
 */
export class AssociationGenerator {
  /** Notes with a generation job in progress. */
  running = $state<Id[]>([])
  /** Notes whose request went to the offline queue. */
  queued = $state<Id[]>([])
  error = $state<{ noteId: Id; error: ErrorDto } | null>(null)

  readonly #api: AppApi
  readonly #onupdate: AssociationGeneratorDeps['onupdate']
  readonly #operationId: () => string
  /** jobId → noteId */
  readonly #jobs = new Map<Id, Id>()

  constructor({ api, onupdate, operationId = newOperationId }: AssociationGeneratorDeps) {
    this.#api = api
    this.#onupdate = onupdate
    this.#operationId = operationId
  }

  isRunning(noteId: Id): boolean {
    return this.running.includes(noteId)
  }

  isQueued(noteId: Id): boolean {
    return this.queued.includes(noteId)
  }

  errorFor(noteId: Id): ErrorDto | null {
    return this.error?.noteId === noteId ? this.error.error : null
  }

  async generate(noteId: Id): Promise<void> {
    if (this.isRunning(noteId)) return
    this.error = null
    this.running = [...this.running, noteId]
    try {
      const job = await this.#api.generateAssociation({ operationId: this.#operationId(), noteId, variants: 1 })
      // Registered before any further await, so no progress event can be missed.
      this.#jobs.set(job.jobId, noteId)
      // Pending (or offline-queued) placeholders are part of the note now.
      const associations = await this.#refresh(noteId)
      if (job.progress === null && associations?.some((item) => item.status === 'offlineQueued')) {
        this.#jobs.delete(job.jobId)
        this.queued = [...this.queued, noteId]
        this.#done(noteId)
      }
    } catch (caught) {
      this.error = { noteId, error: toErrorDto(caught) }
      this.#done(noteId)
    }
  }

  /** Forwarded job events; unrelated jobs are ignored. Events are signals, so the note is refetched. */
  handleEvent(event: JobEventDto): void {
    const noteId = this.#jobs.get(event.jobId)
    if (noteId === undefined) return
    if (event.status === 'succeeded' || event.status === 'failed' || event.status === 'cancelled') {
      this.#jobs.delete(event.jobId)
      this.#done(noteId)
      void this.#refresh(noteId)
    }
  }

  async favorite(noteId: Id, associationId: Id): Promise<ErrorDto | null> {
    try {
      this.#onupdate(noteId, await this.#api.setFavoriteAssociation({ operationId: this.#operationId(), noteId, associationId }))
      return null
    } catch (caught) {
      return toErrorDto(caught)
    }
  }

  #done(noteId: Id): void {
    this.running = this.running.filter((id) => id !== noteId)
  }

  async #refresh(noteId: Id): Promise<AssociationDto[] | null> {
    try {
      const content = await this.#api.getNoteContent(noteId)
      this.#onupdate(noteId, content.associations)
      return content.associations
    } catch {
      // The card keeps its previous associations; the next visit loads them again.
      return null
    }
  }
}
