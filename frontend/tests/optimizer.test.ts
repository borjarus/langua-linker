import { screen, waitFor, within } from '@testing-library/svelte'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it } from 'vitest'
import type { ErrorDto, JobDto } from '../src/api/types'
import { href } from '../src/app/routes'
import OptimizerScreen from '../src/features/deck/OptimizerScreen.svelte'
import { MockAppApi, type MockAppApiOptions } from '../src/demo/mockAppApi'
import { createTestServices, renderWith } from './helpers/services'

const DEFAULT = 'preset-default'
const LIGHT = 'preset-light'

async function settle(api: MockAppApi, job: JobDto): Promise<JobDto> {
  for (let attempt = 0; attempt < 500; attempt += 1) {
    const current = await api.getJobStatus(job.jobId)
    if (current.status !== 'queued' && current.status !== 'running') return current
    await new Promise((resolve) => setTimeout(resolve, 5))
  }
  throw new Error('job did not finish')
}

async function rejection(promise: Promise<unknown>): Promise<ErrorDto> {
  try {
    await promise
  } catch (caught) {
    return (caught as { dto: ErrorDto }).dto
  }
  throw new Error('resolved')
}

let serial = 0
const op = () => `optimizer-test-${++serial}`

describe('MockAppApi optimizer', () => {
  it('refuses a preset without enough usable answers', async () => {
    const api = new MockAppApi()
    const state = await api.getOptimizer({ presetId: LIGHT })
    expect(state.dataset.usable).toBeLessThan(state.minimumReviews)
    const error = await rejection(api.startOptimizer({ operationId: op(), presetId: LIGHT }))
    expect(error.code).toBe('VALIDATION_FAILED')
    expect(error.details).toEqual({ field: 'presetId', reason: 'notEnoughReviews' })
  })

  it('shows the not-enough state in the empty scenario', async () => {
    const api = new MockAppApi({ scenario: 'empty' })
    expect((await api.getOptimizer({ presetId: DEFAULT })).dataset.usable).toBe(0)
  })

  it('runs, applies and reverts without touching cards', async () => {
    const api = new MockAppApi()
    const dueBefore = (await api.getDashboard()).decks.map((deck) => deck.counts)
    const before = await api.getOptimizer({ presetId: DEFAULT })
    expect(before.dataset.usable).toBeGreaterThanOrEqual(before.minimumReviews)
    expect(before.runs).toEqual([])

    const job = await api.startOptimizer({ operationId: op(), presetId: DEFAULT })
    expect(job.kind).toBe('optimize')
    const done = await settle(api, job)
    expect(done.status).toBe('succeeded')
    const state = await api.getOptimizer({ presetId: DEFAULT })
    const run = state.runs[0]
    if (!run) throw new Error('no run')
    expect(done.result).toEqual({ type: 'optimize', runId: run.runId })
    expect(run).toMatchObject({ status: 'proposed', canApply: true, canRevert: false, recommended: true, fewReviews: true })
    expect(run.weightsBefore).toEqual(before.currentWeights)
    expect(run.trainingReviews + run.validationReviews).toBe(state.dataset.usable)
    // The preset is unchanged until the run is applied.
    expect(state.currentWeights).toEqual(before.currentWeights)

    const stale = await rejection(api.applyOptimizerRun({ operationId: op(), runId: run.runId, expectedVersion: state.presetVersion + 1 }))
    expect(stale.code).toBe('VERSION_CONFLICT')
    const notApplied = await rejection(api.revertOptimizerRun({ operationId: op(), runId: run.runId, expectedVersion: state.presetVersion }))
    expect(notApplied.details).toEqual({ field: 'runId', reason: 'status' })

    const applied = await api.applyOptimizerRun({ operationId: op(), runId: run.runId, expectedVersion: state.presetVersion })
    expect(applied.config.weights).toEqual(run.weightsProposed)
    expect(applied.version).toBe(state.presetVersion + 1)
    const afterApply = await api.getOptimizer({ presetId: DEFAULT })
    expect(afterApply.runs[0]).toMatchObject({ status: 'applied', canApply: false, canRevert: true })

    // A second run on the same answers finds the applied weights already fitted.
    const again = await settle(api, await api.startOptimizer({ operationId: op(), presetId: DEFAULT }))
    expect(again.status).toBe('succeeded')
    const second = (await api.getOptimizer({ presetId: DEFAULT })).runs
    expect(second).toHaveLength(2)
    expect(second[0]?.recommended).toBe(false)

    const reverted = await api.revertOptimizerRun({ operationId: op(), runId: run.runId, expectedVersion: applied.version })
    expect(reverted.config.weights).toEqual(before.currentWeights)
    const final = await api.getOptimizer({ presetId: DEFAULT })
    expect(final.runs.find((item) => item.runId === run.runId)).toMatchObject({ status: 'reverted', canRevert: false })
    // The newer proposal was made against the applied weights: it is stale now.
    expect(final.runs[0]?.canApply).toBe(false)
    const staleApply = await rejection(api.applyOptimizerRun({ operationId: op(), runId: final.runs[0]?.runId ?? '', expectedVersion: final.presetVersion }))
    expect(staleApply.details).toEqual({ field: 'runId', reason: 'stale' })

    expect((await api.getDashboard()).decks.map((deck) => deck.counts)).toEqual(dueBefore)
  })

  it('returns the running job instead of starting a second one, and can be cancelled', async () => {
    const api = new MockAppApi({ jobStepMs: 30 })
    const first = await api.startOptimizer({ operationId: op(), presetId: DEFAULT })
    const second = await api.startOptimizer({ operationId: op(), presetId: DEFAULT })
    expect(second.jobId).toBe(first.jobId)
    expect((await api.getOptimizer({ presetId: DEFAULT })).runningJobId).toBe(first.jobId)
    const cancelled = await api.cancelJob(first.jobId)
    expect(cancelled.status).toBe('cancelled')
    const state = await api.getOptimizer({ presetId: DEFAULT })
    expect(state.runningJobId).toBeNull()
    expect(state.runs).toEqual([])
    api.dispose()
  })
})

describe('OptimizerScreen', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
  })

  async function open(presetId: string, options: MockAppApiOptions = {}) {
    const services = await createTestServices({ language: 'en', ...options })
    renderWith(OptimizerScreen, services, { id: presetId })
    await screen.findByRole('button', { name: 'Optimize weights' })
    return { services, mock: services.mock, user: userEvent.setup() }
  }

  it('disables the start below the minimum and explains why', async () => {
    await open(LIGHT)
    const start = await screen.findByRole('button', { name: 'Optimize weights' })
    expect(start).toBeDisabled()
    expect(start).toHaveAccessibleDescription(/At least 200 answers/)
  })

  it('runs, shows the comparison, applies and restores the previous weights', async () => {
    const { mock, user } = await open(DEFAULT)
    expect(await screen.findByText(/Fewer than 1,000 answers/)).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Optimize weights' }))
    const result = await screen.findByRole('region', { name: 'Latest result' })
    expect(within(result).getByText(/predict your held-out answers better/)).toBeInTheDocument()
    expect(within(result).getByRole('table', { name: 'Current and proposed weights compared' })).toBeInTheDocument()
    expect(within(result).getByText(/No card is rescheduled/)).toBeInTheDocument()

    await user.click(within(result).getByRole('button', { name: 'Use the new weights' }))
    const revert = await screen.findByRole('button', { name: 'Restore previous weights' })
    const applied = (await mock.listPresets()).find((preset) => preset.id === DEFAULT)
    const run = (await mock.getOptimizer({ presetId: DEFAULT })).runs[0]
    expect(applied?.config.weights).toEqual(run?.weightsProposed)

    await user.click(revert)
    await waitFor(() => expect(screen.queryByRole('button', { name: 'Restore previous weights' })).not.toBeInTheDocument())
    const restored = (await mock.listPresets()).find((preset) => preset.id === DEFAULT)
    expect(restored?.config.weights).toEqual(run?.weightsBefore)
    expect(screen.getAllByText('Reverted').length).toBeGreaterThan(0)
  })

  it('cancels a run and leaves the preset unchanged', async () => {
    const { mock, user } = await open(DEFAULT, { jobStepMs: 40 })
    await user.click(screen.getByRole('button', { name: 'Optimize weights' }))
    await user.click(await screen.findByRole('button', { name: /cancel/i }))
    expect(await screen.findByText('Optimization cancelled. Nothing was changed.')).toBeInTheDocument()
    expect((await mock.getOptimizer({ presetId: DEFAULT })).runs).toEqual([])
    mock.dispose()
  })

  it('attaches to a run already in progress', async () => {
    const services = await createTestServices({ language: 'en', jobStepMs: 40 })
    await services.mock.startOptimizer({ operationId: op(), presetId: DEFAULT })
    renderWith(OptimizerScreen, services, { id: DEFAULT })
    expect(await screen.findByRole('region', { name: 'Optimizing weights' })).toBeInTheDocument()
    expect(await screen.findByRole('region', { name: 'Latest result' }, { timeout: 3000 })).toBeInTheDocument()
  })

  it('refreshes after the preset changed elsewhere', async () => {
    const { mock, user } = await open(DEFAULT)
    await user.click(screen.getByRole('button', { name: 'Optimize weights' }))
    const result = await screen.findByRole('region', { name: 'Latest result' })
    // Another screen edits the preset meanwhile.
    const preset = (await mock.listPresets()).find((item) => item.id === DEFAULT)
    if (!preset) throw new Error('no preset')
    await mock.savePreset({ operationId: op(), presetId: DEFAULT, expectedVersion: preset.version, name: preset.name, config: { ...preset.config, newPerDay: 21 } })
    await user.click(within(result).getByRole('button', { name: 'Use the new weights' }))
    expect(await screen.findByText(/The preset changed in the meantime/)).toBeInTheDocument()
  })

  it('has a stable address under the preset', () => {
    expect(href({ name: 'optimizer', params: { id: DEFAULT } })).toBe('#/preset/preset-default/optimizer')
  })
})
