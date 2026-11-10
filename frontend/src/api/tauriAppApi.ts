import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { AppApi, AppApiOperation } from './appApi'
import { AppError, toErrorDto } from './errors'
import { OPERATIONS } from './operations'
import type { CollectionChangedEventDto, JobEventDto, Unsubscribe } from './types'

export type InvokeFn = (command: string, args?: Record<string, unknown>) => Promise<unknown>
export type ListenFn = <T>(event: string, handler: (event: { payload: T }) => void) => Promise<UnlistenFn>

export interface StartupWait {
  totalMs: number
  stepMs: number
}

/** How long a command waits for the backend at app start (a migration can take a while). */
const STARTUP_WAIT: StartupWait = { totalMs: 30_000, stepMs: 150 }

/** Tauri's transport error for a command whose managed state is not registered yet. */
export function isBackendStarting(caught: unknown): boolean {
  return typeof caught === 'string' && caught.startsWith('state not managed')
}

type CallableOperation = {
  [K in AppApiOperation]: (typeof OPERATIONS)[K]['kind'] extends 'subscription' ? never : K
}[AppApiOperation]

/**
 * Production adapter. Commands receive their single argument as `input` (Rust: `fn cmd(input: X)`).
 * Commands not yet implemented in Rust reject with an `ErrorDto`; this adapter never falls back to the mock.
 */
export function createTauriAppApi(invokeFn: InvokeFn = invoke, listenFn: ListenFn = listen, startupWait: StartupWait = STARTUP_WAIT): AppApi {
  const call =
    (operation: CallableOperation) =>
    async (input?: unknown): Promise<never> => {
      const args = input === undefined ? undefined : { input }
      const deadline = Date.now() + startupWait.totalMs
      for (;;) {
        try {
          return (await invokeFn(OPERATIONS[operation].command, args)) as never
        } catch (caught) {
          // The WebView can be up before Rust registered the backend (opening or migrating the
          // collection). Such a command never ran, so asking again is safe even for mutations.
          if (isBackendStarting(caught) && Date.now() < deadline) {
            await new Promise((resolve) => setTimeout(resolve, startupWait.stepMs))
            continue
          }
          throw new AppError(toErrorDto(caught))
        }
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
    getAboutInfo: call('getAboutInfo'),
    getDiagnosticsReport: call('getDiagnosticsReport'),
    getDashboard: call('getDashboard'),
    getDeck: call('getDeck'),
    createDeck: call('createDeck'),
    updateDeck: call('updateDeck'),
    deleteDeck: call('deleteDeck'),
    listPresets: call('listPresets'),
    savePreset: call('savePreset'),
    startStudy: call('startStudy'),
    resumeStudy: call('resumeStudy'),
    updateStudySession: call('updateStudySession'),
    getRatingPreview: call('getRatingPreview'),
    rateCard: call('rateCard'),
    undoReview: call('undoReview'),
    applyCardAction: call('applyCardAction'),
    getSessionSummary: call('getSessionSummary'),
    getNoteDetails: call('getNoteDetails'),
    getNoteContent: call('getNoteContent'),
    getEditorData: call('getEditorData'),
    checkDuplicate: call('checkDuplicate'),
    saveNote: call('saveNote'),
    deleteNote: call('deleteNote'),
    updateNoteContent: call('updateNoteContent'),
    setFavoriteAssociation: call('setFavoriteAssociation'),
    resolveWordLink: call('resolveWordLink'),
    searchWords: call('searchWords'),
    generateAssociation: call('generateAssociation'),
    searchCards: call('searchCards'),
    previewBulkAction: call('previewBulkAction'),
    applyBulkAction: call('applyBulkAction'),
    getStats: call('getStats'),
    getSettings: call('getSettings'),
    updateSettings: call('updateSettings'),
    setAiApiKey: call('setAiApiKey'),
    requestNotificationPermission: call('requestNotificationPermission'),
    getOnboardingState: call('getOnboardingState'),
    completeOnboarding: call('completeOnboarding'),
    previewReset: call('previewReset'),
    resetCollection: call('resetCollection'),
    pickFile: call('pickFile'),
    previewImport: call('previewImport'),
    commitImport: call('commitImport'),
    previewDeckImport: call('previewDeckImport'),
    commitDeckImport: call('commitDeckImport'),
    exportDeck: call('exportDeck'),
    shareFile: call('shareFile'),
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
