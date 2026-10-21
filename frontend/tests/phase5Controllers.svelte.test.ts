import { describe, expect, it, vi } from 'vitest'
import type { AppApi } from '../src/api/appApi'
import { AppError } from '../src/api/errors'
import type { JobDto, JobEventDto } from '../src/api/types'
import { MockAppApi } from '../src/demo/mockAppApi'
import { SettingsController } from '../src/features/settings/settingsController.svelte'
import { JobTracker } from '../src/services/jobTracker.svelte'

const job = (overrides: Partial<JobDto> = {}): JobDto => ({
  jobId: 'job-1', kind: 'import', status: 'queued', sequence: 0, progress: { done: 0, total: 3 },
  startedAt: '2026-03-02T08:00:00.000Z', finishedAt: null, error: null, result: null, ...overrides,
})

describe('JobTracker', () => {
  it('drops foreign and out-of-order events and reads the result from getJobStatus', async () => {
    let emit: (event: JobEventDto) => void = () => {}
    const final = job({ status: 'succeeded', sequence: 3, result: { type: 'export', fileName: 'a.csv', cardCount: 2 } })
    const api = {
      onJobEvent: (listener: (event: JobEventDto) => void) => ((emit = listener), () => {}),
      getJobStatus: vi.fn(async () => final),
    } as unknown as AppApi
    const tracker = new JobTracker(api)
    tracker.listen()
    const finished = vi.fn()
    tracker.track(job(), finished)

    emit({ jobId: 'job-2', kind: 'import', sequence: 5, status: 'running', progress: null })
    expect(tracker.job?.sequence).toBe(0)
    emit({ jobId: 'job-1', kind: 'import', sequence: 2, status: 'running', progress: { done: 2, total: 3 } })
    emit({ jobId: 'job-1', kind: 'import', sequence: 1, status: 'running', progress: { done: 1, total: 3 } })
    expect(tracker.job?.progress).toEqual({ done: 2, total: 3 })
    expect(tracker.active).toBe(true)

    emit({ jobId: 'job-1', kind: 'import', sequence: 3, status: 'succeeded', progress: { done: 3, total: 3 } })
    await vi.waitFor(() => expect(finished).toHaveBeenCalledWith(final))
    expect(tracker.job?.result).toEqual(final.result)
    expect(api.getJobStatus).toHaveBeenCalledTimes(1)
  })

  it('keeps a rejected start as an error and tracks nothing', async () => {
    const tracker = new JobTracker(new MockAppApi())
    const started = await tracker.start(async () => Promise.reject(AppError.of('PREVIEW_STALE')))
    expect(started).toBeNull()
    expect(tracker.error?.code).toBe('PREVIEW_STALE')
    expect(tracker.job).toBeNull()
  })
})

describe('SettingsController', () => {
  it('queues changes so each save carries the version of the previous one', async () => {
    const api = new MockAppApi()
    const controller = new SettingsController(api)
    await controller.load()
    const results = await Promise.all([
      controller.update('dailyGoal', { dailyGoal: 40 }),
      controller.update('dayCutoffHour', { dayCutoffHour: 6 }),
      controller.update('ttsRate', { ttsRate: 0.8 }),
    ])
    expect(results).toEqual([true, true, true])
    expect(controller.settings).toMatchObject({ dailyGoal: 40, dayCutoffHour: 6, ttsRate: 0.8, version: 4 })
    expect(controller.saved).toBe('ttsRate')
  })

  it('reports a field error and reloads on a conflict instead of overwriting', async () => {
    const api = new MockAppApi()
    const controller = new SettingsController(api)
    await controller.load()
    expect(await controller.update('dailyGoal', { dailyGoal: 0 })).toBe(false)
    expect(controller.errorFor('dailyGoal')?.code).toBe('VALIDATION_FAILED')
    expect(controller.errorFor('theme')).toBeNull()

    await api.updateSettings({ operationId: 'elsewhere', expectedVersion: 1, patch: { dailyGoal: 15 } })
    expect(await controller.update('theme', { theme: 'dark' })).toBe(false)
    expect(controller.errorFor('theme')?.code).toBe('VERSION_CONFLICT')
    expect(controller.settings).toMatchObject({ dailyGoal: 15, theme: 'system', version: 2 })
    expect(await controller.update('theme', { theme: 'dark' })).toBe(true)
  })
})
