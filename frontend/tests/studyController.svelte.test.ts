import { flushSync } from 'svelte'
import { describe, expect, it, vi } from 'vitest'
import { MockAppApi } from '../src/demo/mockAppApi'
import { StudyController } from '../src/features/study/studyController.svelte'

async function setup() {
  const api = new MockAppApi()
  const rateSpy = vi.spyOn(api, 'rateCard')
  let ids = 0
  const controller = new StudyController({ api, now: () => 1_000, operationId: () => `op-${++ids}` })
  const session = await StudyController.start(api, null, 'start')
  await controller.resume(session.sessionId)
  return { api, controller, rateSpy }
}

describe('StudyController (reference controller)', () => {
  it('follows question → reveal → saving → next', async () => {
    const { controller } = await setup()
    expect(controller.phase).toBe('question')
    const first = controller.card?.cardId
    await controller.reveal()
    expect(controller.phase).toBe('reveal')
    expect(controller.preview?.intervals.Good.intervalSeconds).toBeGreaterThan(0)
    const pending = controller.rate('Good')
    expect(controller.phase).toBe('saving')
    await pending
    expect(controller.phase).toBe('question')
    expect(controller.card?.cardId).not.toBe(first)
    expect(controller.reviewedCount).toBe(1)
  })

  it('blocks a second rating while saving (double tap)', async () => {
    const { controller, rateSpy } = await setup()
    await controller.reveal()
    const first = controller.rate('Good')
    const second = controller.rate('Easy')
    await Promise.all([first, second])
    expect(rateSpy).toHaveBeenCalledTimes(1)
  })

  it('keeps card, rating and operationId after a failed save and reuses them on retry', async () => {
    const { api, controller, rateSpy } = await setup()
    const card = controller.card
    await controller.reveal()
    api.failNext('rateCard', 'DB_BUSY')
    await controller.rate('Hard')
    expect(controller.phase).toBe('save_failed')
    expect(controller.card).toEqual(card)
    expect(controller.selected).toBe('Hard')
    expect(controller.error?.code).toBe('DB_BUSY')

    await controller.retry()
    expect(controller.phase).toBe('question')
    const operationIds = rateSpy.mock.calls.map(([input]) => input.operationId)
    expect(operationIds).toHaveLength(2)
    expect(operationIds[0]).toBe(operationIds[1])
  })

  it('a rating right after reveal waits for the preview instead of sending a stale token', async () => {
    const api = new MockAppApi({ latencyMs: 15 })
    const controller = new StudyController({ api })
    await controller.resume((await StudyController.start(api, '1', 'start')).sessionId)
    const revealing = controller.reveal()
    await controller.rate('Good')
    await revealing
    expect(controller.error).toBeNull()
    expect(controller.reviewedCount).toBe(1)
  })

  it('does not rate before the answer is revealed', async () => {
    const { controller, rateSpy } = await setup()
    await controller.rate('Good')
    expect(rateSpy).not.toHaveBeenCalled()
  })

  it('undo brings back the previous card', async () => {
    const { controller } = await setup()
    const first = controller.card?.cardId
    await controller.reveal()
    await controller.rate('Again')
    expect(controller.undoAvailable).toBe(true)
    await controller.undo()
    expect(controller.card?.cardId).toBe(first)
    expect(controller.reviewedCount).toBe(0)
  })

  it('reactive reads and effects never repeat a mutation', async () => {
    const { controller, rateSpy } = await setup()
    let renders = 0
    const dispose = $effect.root(() => {
      $effect(() => {
        // A component re-rendering on every state change must not trigger saves.
        void controller.phase
        void controller.card
        void controller.preview
        void controller.canRate
        renders += 1
      })
    })
    flushSync()
    await controller.reveal()
    flushSync()
    await controller.rate('Good')
    flushSync()
    expect(renders).toBeGreaterThan(2)
    expect(rateSpy).toHaveBeenCalledTimes(1)
    dispose()
  })
})

describe('StudyController session lifecycle (Phase 3)', () => {
  async function single(clock: { now: number }) {
    const api = new MockAppApi()
    const session = await api.startStudy({ operationId: 'all', deckId: '1' })
    const cardId = session.current?.cardId ?? ''
    // A one-card session makes "no next card" deterministic.
    const one = await StudyController.start(api, '1', 'one', [cardId])
    let ids = 0
    const controller = new StudyController({ api, now: () => clock.now, operationId: () => `op-${++ids}` })
    await controller.resume(one.sessionId)
    return { api, controller }
  }

  it('reaches finished when no next card exists and stops blocking navigation', async () => {
    const { controller } = await single({ now: 0 })
    expect(controller.inSession).toBe(true)
    await controller.reveal()
    await controller.rate('Good')
    expect(controller.phase).toBe('finished')
    expect(controller.card).toBeNull()
    expect(controller.inSession).toBe(false)
    await controller.rate('Good')
    expect(controller.reviewedCount).toBe(1)
  })

  it('pauses: no reveal or rating while paused, and paused time is not answer time', async () => {
    const clock = { now: 0 }
    const { api, controller } = await single(clock)
    const rateSpy = vi.spyOn(api, 'rateCard')
    clock.now = 2_000
    expect(await controller.pause()).toBeNull()
    expect(controller.paused).toBe(true)
    await controller.reveal()
    expect(controller.phase).toBe('question')

    clock.now = 600_000
    await controller.continue()
    expect(controller.paused).toBe(false)
    clock.now = 603_000
    await controller.reveal()
    await controller.rate('Good')
    expect(rateSpy.mock.calls[0]?.[0].answerMs).toBe(5_000)
  })

  it('ends early and keeps committed ratings', async () => {
    const { api, controller } = await setup()
    await controller.reveal()
    await controller.rate('Good')
    expect(await controller.end()).toBeNull()
    expect(controller.phase).toBe('finished')
    const summary = await api.getSessionSummary(controller.sessionId ?? '')
    expect(summary).toMatchObject({ status: 'finished', reviewedCount: 1 })
  })

  it('treats a missing session as lost (recovery), not as a generic error', async () => {
    const controller = new StudyController({ api: new MockAppApi() })
    await controller.resume('session-404')
    expect(controller.phase).toBe('lost')
  })

  it('suspending moves to the next card; flagging keeps the card with a new version and fresh preview', async () => {
    const { controller } = await setup()
    const first = controller.card
    await controller.reveal()
    expect(await controller.applyAction('flag', 'red')).toBeNull()
    expect(controller.card?.cardId).toBe(first?.cardId)
    expect(controller.card?.flag).toBe('red')
    expect(controller.preview?.cardVersion).toBe(controller.card?.version)
    await controller.rate('Good')
    expect(controller.error).toBeNull()

    const second = controller.card?.cardId
    expect(await controller.applyAction('suspend')).toBeNull()
    expect(controller.card?.cardId).not.toBe(second)
    expect(controller.phase).toBe('question')
  })

  it('reports a stale card action without changing the card', async () => {
    const { api, controller } = await setup()
    const card = controller.card
    api.failNext('applyCardAction', 'VERSION_CONFLICT')
    expect((await controller.applyAction('bury'))?.code).toBe('VERSION_CONFLICT')
    expect(controller.card).toEqual(card)
    expect(controller.acting).toBe(false)
  })
})
