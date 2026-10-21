import type { AppApi } from '../api/appApi'
import { toErrorDto } from '../api/errors'
import type { ErrorDto, JobDto, JobEventDto } from '../api/types'

const TERMINAL: ReadonlySet<JobDto['status']> = new Set(['succeeded', 'failed', 'cancelled'])

export const isTerminal = (job: Pick<JobDto, 'status'>): boolean => TERMINAL.has(job.status)

/**
 * Follows one long-running job on a screen. Job events are read signals: they move the progress
 * bar, and on a terminal status the tracker reads `getJobStatus` — the durable truth with the
 * job's `result` — before calling `onfinish`. The component subscribes with
 * `$effect(() => tracker.listen())`, so unmounting stops listening.
 */
export class JobTracker {
  job = $state<JobDto | null>(null)
  error = $state<ErrorDto | null>(null)

  readonly #api: AppApi
  #onfinish: ((job: JobDto) => void) | null = null
  #settling = false

  constructor(api: AppApi) {
    this.#api = api
  }

  get active(): boolean {
    return this.job !== null && !isTerminal(this.job)
  }

  listen(): () => void {
    return this.#api.onJobEvent((event) => this.#handle(event))
  }

  /** Starts a job and follows it; a rejected start is kept in `error`, nothing is tracked. */
  async start(begin: () => Promise<JobDto>, onfinish?: (job: JobDto) => void): Promise<JobDto | null> {
    this.error = null
    try {
      const job = await begin()
      this.track(job, onfinish)
      return job
    } catch (caught) {
      this.error = toErrorDto(caught)
      return null
    }
  }

  track(job: JobDto, onfinish?: (job: JobDto) => void): void {
    this.job = job
    this.#onfinish = onfinish ?? null
    this.#settling = false
    if (isTerminal(job)) void this.#settle(job.jobId)
  }

  async cancel(): Promise<void> {
    if (!this.job || !this.active) return
    try {
      await this.#api.cancelJob(this.job.jobId)
    } catch (caught) {
      this.error = toErrorDto(caught)
    }
  }

  reset(): void {
    this.job = null
    this.error = null
    this.#onfinish = null
  }

  #handle(event: JobEventDto): void {
    const job = this.job
    // Events of other jobs and late, out-of-order ones are dropped by sequence.
    if (!job || event.jobId !== job.jobId || event.sequence <= job.sequence) return
    this.job = { ...job, status: event.status, sequence: event.sequence, progress: event.progress }
    if (isTerminal(event)) void this.#settle(job.jobId)
  }

  async #settle(jobId: string): Promise<void> {
    if (this.#settling) return
    this.#settling = true
    let final = this.job
    try {
      final = await this.#api.getJobStatus(jobId)
    } catch (caught) {
      this.error = toErrorDto(caught)
    }
    if (!final || this.job?.jobId !== jobId) return
    this.job = final
    this.#onfinish?.(final)
  }
}
