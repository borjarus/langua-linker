import { flushSync } from 'svelte'
import { describe, expect, it, vi } from 'vitest'
import { MockAppApi } from '../src/demo/mockAppApi'
import { BrowserController, DEFAULT_FILTERS } from '../src/features/browser/browserController.svelte'
import { EditorController } from '../src/features/editor/editorController.svelte'

async function editorFor(deckId: string) {
  const api = new MockAppApi()
  const editor = new EditorController({ api })
  editor.init(await api.getEditorData(), null, deckId)
  return { api, editor }
}

describe('EditorController', () => {
  it('derives validity and dirtiness; nothing is saved without an explicit save', async () => {
    const { api, editor } = await editorFor('1')
    const save = vi.spyOn(api, 'saveNote')
    expect(editor.dirty).toBe(false)
    expect(editor.valid).toBe(false)
    editor.front = 'das Fenster'
    flushSync()
    expect(editor.dirty).toBe(true)
    expect(editor.problems).toMatchObject({ front: null, back: 'required' })
    editor.back = 'x'.repeat(2001)
    expect(editor.problems.back).toBe('tooLong')
    expect(save).not.toHaveBeenCalled()
    editor.back = 'okno'
    expect(await editor.save()).not.toBeNull()
    expect(save).toHaveBeenCalledTimes(1)
    expect(editor.dirty).toBe(false)
  })

  it('keeps one operationId for a retryable failure and a new one after success', async () => {
    const ids = ['op-1', 'op-2', 'op-3']
    const api = new MockAppApi()
    const editor = new EditorController({ api, operationId: () => ids.shift() ?? 'op-x' })
    editor.init(await api.getEditorData(), null, '1')
    editor.front = 'das Fenster'
    editor.back = 'okno'
    const save = vi.spyOn(api, 'saveNote')
    api.failNext('saveNote', 'DB_BUSY')
    expect(await editor.save()).toBeNull()
    expect(editor.error?.code).toBe('DB_BUSY')
    await editor.save()
    editor.front = 'das Fenster (2)'
    await editor.save()
    const [failed, retried, next] = save.mock.calls.map(([input]) => input.operationId)
    expect(retried).toBe(failed)
    expect(next).not.toBe(retried)
  })

  it('moves links with their words after an edit and drops links whose words are gone', async () => {
    const { editor } = await editorFor('2')
    editor.noteTypeKind = 'Sentence'
    editor.front = 'Ich trinke Tee.'
    expect(editor.addLink(11, 14, null)).toBe(true)
    expect(editor.addLink(4, 10, null)).toBe(true)
    expect(editor.addLink(12, 13, null)).toBe(false)
    editor.front = 'Heute trinke ich Tee.'
    flushSync()
    const states = Object.fromEntries(editor.linkStates.map((state) => [state.link.surface, [state.status, state.link.startIndex]]))
    expect(states).toEqual({ trinke: ['moved', 6], Tee: ['moved', 17] })
    editor.front = 'Heute trinke ich Kaffee.'
    flushSync()
    expect(editor.linkStates.find((state) => state.link.surface === 'Tee')?.status).toBe('lost')
  })

  it('previews templates of a type change against the stored cards', async () => {
    const api = new MockAppApi()
    const page = await api.searchCards({ query: '"thank you"', deckId: null, tag: null, states: [], flags: [], sort: 'created', direction: 'asc', cursor: null, pageSize: 5 })
    const details = await api.getNoteDetails(page.items[0]?.cardId ?? '')
    const editor = new EditorController({ api })
    editor.init(await api.getEditorData(), details, null)
    expect(editor.templates.map((item) => item.status)).toEqual(['kept', 'kept'])
    editor.noteTypeKind = 'Basic'
    expect(editor.templates.map((item) => [item.direction, item.status])).toEqual([['forward', 'kept'], ['reverse', 'archived']])
  })
})

describe('BrowserController', () => {
  it('selects all matches minus exclusions against the filter snapshot', async () => {
    const api = new MockAppApi()
    const browser = new BrowserController({ api, pageSize: 3 })
    await browser.search({ ...DEFAULT_FILTERS, deckId: '4' })
    expect(browser.total).toBe(8)
    expect(browser.rows.filter(Boolean)).toHaveLength(3)
    browser.selectAllMatches()
    const first = browser.rows[0]?.cardId ?? ''
    browser.toggle(first)
    expect(browser.selectedCount).toBe(7)
    expect(browser.isSelected(first)).toBe(false)
    expect(browser.selectionDto()).toEqual({ kind: 'query', filterVersion: browser.filterVersion, excludedCardIds: [first] })
    const result = await browser.apply({ type: 'flag', flag: 'blue' })
    expect(result).toEqual({ affected: 7 })
    // Applying re-searches; the selection never outlives its snapshot.
    expect(browser.selectedCount).toBe(0)
  })

  it('drops responses of an earlier search', async () => {
    const api = new MockAppApi()
    const browser = new BrowserController({ api })
    const first = browser.search({ ...DEFAULT_FILTERS, deckId: '4' })
    const second = browser.search({ ...DEFAULT_FILTERS, deckId: '3' })
    await Promise.all([first, second])
    expect(browser.total).toBe(5)
  })
})
