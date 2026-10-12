import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { AppApi, AppApiOperation } from './appApi'
import { AppError, toErrorDto } from './errors'
import { OPERATIONS } from './operations'
import type { CollectionChangedEventDto, JobEventDto, Unsubscribe } from './types'

export type InvokeFn = (command: string, args?: Record<string, unknown>) => Promise<unknown>
export type ListenFn = <T>(event: string, handler: (event: { payload: T }) => void) => Promise<UnlistenFn>

type CallableOperation = {
  [K in AppApiOperation]: (typeof OPERATIONS)[K]['kind'] extends 'subscription' ? never : K
}[AppApiOperation]

/**
 * Production adapter. Commands receive their single argument as `input` (Rust: `fn cmd(input: X)`).
 * Commands not yet implemented in Rust reject with an `ErrorDto`; this adapter never falls back to the mock.
 */
export function createTauriAppApi(invokeFn: InvokeFn = invoke, listenFn: ListenFn = listen): AppApi {
  const call =
    (operation: CallableOperation) =>
    async (input?: unknown): Promise<never> => {
      try {
        const args = input === undefined ? undefined : { input }
        return (await invokeFn(OPERATIONS[operation].command, args)) as never
      } catch (caught) {
        throw new AppError(toErrorDto(caught))
      }
    }

  const subscribe = <T>(event: string, listener: (payload: T) => void): Unsubscribe => {
    let disposed = false
    let unlisten: UnlistenFn | undefined
    void listenFn<T>(event, ({ payload }) => {
      if (!disposed) listener(payload)
    }).then(
      (cleanup) => {
        if (disposed) cleanup()
        else unlisten = cleanup
      },
      () => {
        // Registration failure leaves the screen on explicit refresh; it never switches adapters.
      },
    )
    return () => {
      disposed = true
      unlisten?.()
    }
  }

  return {
    getAppInfo: call('getAppInfo'),
    getAttributions: call('getAttributions'),
    getDashboard: call('getDashboard'),
    getDeck: call('getDeck'),
    createDeck: call('createDeck'),
    updateDeck: call('updateDeck'),
    deleteDeck: call('deleteDeck'),
    startStudy: call('startStudy'),
    resumeStudy: call('resumeStudy'),
    getRatingPreview: call('getRatingPreview'),
    rateCard: call('rateCard'),
    undoReview: call('undoReview'),
    applyCardAction: call('applyCardAction'),
    getSessionSummary: call('getSessionSummary'),
    getNoteDetails: call('getNoteDetails'),
    getEditorData: call('getEditorData'),
    checkDuplicate: call('checkDuplicate'),
    saveNote: call('saveNote'),
    setFavoriteAssociation: call('setFavoriteAssociation'),
    resolveWordLink: call('resolveWordLink'),
    generateAssociation: call('generateAssociation'),
    searchCards: call('searchCards'),
    applyBulkAction: call('applyBulkAction'),
    getStats: call('getStats'),
    getSettings: call('getSettings'),
    updateSettings: call('updateSettings'),
    getOnboardingState: call('getOnboardingState'),
    completeOnboarding: call('completeOnboarding'),
    pickFile: call('pickFile'),
    previewImport: call('previewImport'),
    commitImport: call('commitImport'),
    listBackups: call('listBackups'),
    createBackup: call('createBackup'),
    validateRestore: call('validateRestore'),
    restoreBackup: call('restoreBackup'),
    getJobStatus: call('getJobStatus'),
    cancelJob: call('cancelJob'),
    onJobEvent: (listener) => subscribe<JobEventDto>(OPERATIONS.onJobEvent.command, listener),
    onCollectionChanged: (listener) =>
      subscribe<CollectionChangedEventDto>(OPERATIONS.onCollectionChanged.command, listener),
  }
}
