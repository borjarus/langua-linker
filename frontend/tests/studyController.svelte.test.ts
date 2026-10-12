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
