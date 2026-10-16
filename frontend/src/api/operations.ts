import type { AppApiOperation } from './appApi'
import type { ErrorCode } from './types'

export type OperationKind = 'query' | 'mutation' | 'job' | 'subscription'
export type OperationPermission = 'none' | 'file' | 'notifications' | 'network' | 'secret'
export type AppEventName = 'job_progress' | 'collection_changed'
export type ScreenId =
  | 'shell'
  | 'home'
  | 'deck'
  | 'study'
  | 'summary'
  | 'card'
  | 'editor'
  | 'browser'
  | 'stats'
  | 'settings'
  | 'onboarding'
  | 'transfer'
  | 'backup'
  | 'attributions'

export interface OperationSpec {
  kind: OperationKind
  /** Tauri command (snake_case) or event name for subscriptions. */
  command: string
  /** Error codes the screen must handle in addition to INTERNAL. */
  errors: readonly ErrorCode[]
  /** Events emitted as a consequence (read signals, not truth). */
  emits: readonly AppEventName[]
  permission: OperationPermission
  screens: readonly ScreenId[]
  /** Plan phase that connects the real Rust implementation. */
  backendPhase: number
}

const op = (spec: OperationSpec): OperationSpec => spec

/**
 * Operation catalogue: input/result types live on `AppApi`; everything else lives here.
 * `satisfies Record<AppApiOperation, …>` makes a missing or extra operation a compile error.
 */
export const OPERATIONS = {
  getAppInfo: op({ kind: 'query', command: 'get_app_info', errors: [], emits: [], permission: 'none', screens: ['shell', 'attributions'], backendPhase: 1 }),
  getAttributions: op({ kind: 'query', command: 'get_attributions', errors: [], emits: [], permission: 'none', screens: ['attributions'], backendPhase: 8 }),

  getDashboard: op({ kind: 'query', command: 'get_dashboard', errors: ['DB_BUSY'], emits: [], permission: 'none', screens: ['home'], backendPhase: 10 }),
  getDeck: op({ kind: 'query', command: 'get_deck', errors: ['NOT_FOUND', 'DELETED', 'DB_BUSY'], emits: [], permission: 'none', screens: ['deck'], backendPhase: 11 }),
  createDeck: op({ kind: 'mutation', command: 'create_deck', errors: ['VALIDATION_FAILED', 'DB_BUSY', 'STORAGE_FULL'], emits: ['collection_changed'], permission: 'none', screens: ['home', 'deck'], backendPhase: 11 }),
  updateDeck: op({ kind: 'mutation', command: 'update_deck', errors: ['VALIDATION_FAILED', 'VERSION_CONFLICT', 'NOT_FOUND', 'DB_BUSY'], emits: ['collection_changed'], permission: 'none', screens: ['deck'], backendPhase: 11 }),
  deleteDeck: op({ kind: 'mutation', command: 'delete_deck', errors: ['VERSION_CONFLICT', 'NOT_FOUND', 'DB_BUSY'], emits: ['collection_changed'], permission: 'none', screens: ['deck'], backendPhase: 11 }),

  startStudy: op({ kind: 'mutation', command: 'start_study', errors: ['NOT_FOUND', 'DB_BUSY'], emits: [], permission: 'none', screens: ['home', 'deck', 'study', 'summary'], backendPhase: 10 }),
  resumeStudy: op({ kind: 'query', command: 'resume_study', errors: ['NOT_FOUND', 'DB_BUSY'], emits: [], permission: 'none', screens: ['home', 'study'], backendPhase: 10 }),
  updateStudySession: op({ kind: 'mutation', command: 'update_study_session', errors: ['VALIDATION_FAILED', 'NOT_FOUND', 'DB_BUSY'], emits: [], permission: 'none', screens: ['study'], backendPhase: 10 }),
  getRatingPreview: op({ kind: 'query', command: 'get_rating_preview', errors: ['NOT_FOUND', 'VERSION_CONFLICT'], emits: [], permission: 'none', screens: ['study'], backendPhase: 10 }),
  rateCard: op({ kind: 'mutation', command: 'rate_card', errors: ['PREVIEW_STALE', 'VERSION_CONFLICT', 'NOT_FOUND', 'DB_BUSY', 'STORAGE_FULL'], emits: ['collection_changed'], permission: 'none', screens: ['study'], backendPhase: 10 }),
  undoReview: op({ kind: 'mutation', command: 'undo_review', errors: ['VERSION_CONFLICT', 'NOT_FOUND', 'DB_BUSY'], emits: ['collection_changed'], permission: 'none', screens: ['study'], backendPhase: 10 }),
  applyCardAction: op({ kind: 'mutation', command: 'apply_card_action', errors: ['VERSION_CONFLICT', 'NOT_FOUND', 'DB_BUSY'], emits: ['collection_changed'], permission: 'none', screens: ['study', 'card'], backendPhase: 10 }),
  getSessionSummary: op({ kind: 'query', command: 'get_session_summary', errors: ['NOT_FOUND'], emits: [], permission: 'none', screens: ['summary'], backendPhase: 10 }),

  getNoteDetails: op({ kind: 'query', command: 'get_note_details', errors: ['NOT_FOUND', 'DELETED'], emits: [], permission: 'none', screens: ['card', 'editor'], backendPhase: 11 }),
  getNoteContent: op({ kind: 'query', command: 'get_note_content', errors: ['NOT_FOUND', 'DELETED'], emits: [], permission: 'none', screens: ['study'], backendPhase: 11 }),
  getEditorData: op({ kind: 'query', command: 'get_editor_data', errors: ['DB_BUSY'], emits: [], permission: 'none', screens: ['editor'], backendPhase: 11 }),
  checkDuplicate: op({ kind: 'query', command: 'check_duplicate', errors: ['VALIDATION_FAILED'], emits: [], permission: 'none', screens: ['editor'], backendPhase: 11 }),
  saveNote: op({ kind: 'mutation', command: 'save_note', errors: ['VALIDATION_FAILED', 'VERSION_CONFLICT', 'NOT_FOUND', 'DB_BUSY', 'STORAGE_FULL'], emits: ['collection_changed'], permission: 'none', screens: ['editor'], backendPhase: 11 }),
  setFavoriteAssociation: op({ kind: 'mutation', command: 'set_favorite_association', errors: ['NOT_FOUND', 'DB_BUSY'], emits: ['collection_changed'], permission: 'none', screens: ['study', 'card'], backendPhase: 11 }),
  resolveWordLink: op({ kind: 'mutation', command: 'resolve_word_link', errors: ['VALIDATION_FAILED', 'NOT_FOUND'], emits: ['collection_changed'], permission: 'none', screens: ['study', 'card', 'editor'], backendPhase: 11 }),
  generateAssociation: op({ kind: 'job', command: 'generate_association', errors: ['NETWORK_UNAVAILABLE', 'PROVIDER_ERROR', 'PROVIDER_TIMEOUT', 'INVALID_PROVIDER_OUTPUT', 'MODERATION_BLOCKED', 'QUOTA_EXCEEDED', 'PERMISSION_DENIED'], emits: ['job_progress', 'collection_changed'], permission: 'network', screens: ['study', 'card', 'editor'], backendPhase: 13 }),

  searchCards: op({ kind: 'query', command: 'search_cards', errors: ['VALIDATION_FAILED', 'DB_BUSY'], emits: [], permission: 'none', screens: ['browser'], backendPhase: 11 }),
  applyBulkAction: op({ kind: 'mutation', command: 'apply_bulk_action', errors: ['VALIDATION_FAILED', 'PREVIEW_STALE', 'DB_BUSY'], emits: ['collection_changed'], permission: 'none', screens: ['browser'], backendPhase: 11 }),

  getStats: op({ kind: 'query', command: 'get_stats', errors: ['DB_BUSY'], emits: [], permission: 'none', screens: ['stats'], backendPhase: 12 }),
  getSettings: op({ kind: 'query', command: 'get_settings', errors: [], emits: [], permission: 'none', screens: ['settings', 'shell', 'summary'], backendPhase: 12 }),
  updateSettings: op({ kind: 'mutation', command: 'update_settings', errors: ['VALIDATION_FAILED', 'VERSION_CONFLICT'], emits: ['collection_changed'], permission: 'none', screens: ['settings', 'onboarding', 'summary'], backendPhase: 12 }),
  getOnboardingState: op({ kind: 'query', command: 'get_onboarding_state', errors: [], emits: [], permission: 'none', screens: ['onboarding', 'shell'], backendPhase: 10 }),
  completeOnboarding: op({ kind: 'job', command: 'complete_onboarding', errors: ['VALIDATION_FAILED', 'STORAGE_FULL'], emits: ['job_progress', 'collection_changed'], permission: 'none', screens: ['onboarding'], backendPhase: 10 }),

  pickFile: op({ kind: 'query', command: 'pick_file', errors: ['PERMISSION_DENIED', 'CANCELLED'], emits: [], permission: 'file', screens: ['transfer', 'backup'], backendPhase: 14 }),
  previewImport: op({ kind: 'query', command: 'preview_import', errors: ['UNSUPPORTED_FORMAT', 'VALIDATION_FAILED'], emits: [], permission: 'file', screens: ['transfer'], backendPhase: 14 }),
  commitImport: op({ kind: 'job', command: 'commit_import', errors: ['PREVIEW_STALE', 'STORAGE_FULL', 'CANCELLED'], emits: ['job_progress', 'collection_changed'], permission: 'file', screens: ['transfer'], backendPhase: 14 }),
  listBackups: op({ kind: 'query', command: 'list_backups', errors: ['PERMISSION_DENIED'], emits: [], permission: 'none', screens: ['backup'], backendPhase: 12 }),
  createBackup: op({ kind: 'job', command: 'create_backup', errors: ['STORAGE_FULL', 'PERMISSION_DENIED', 'CANCELLED'], emits: ['job_progress', 'collection_changed'], permission: 'file', screens: ['backup'], backendPhase: 12 }),
  validateRestore: op({ kind: 'query', command: 'validate_restore', errors: ['BACKUP_CORRUPT', 'SCHEMA_TOO_NEW', 'UNSUPPORTED_FORMAT'], emits: [], permission: 'file', screens: ['backup'], backendPhase: 12 }),
  restoreBackup: op({ kind: 'job', command: 'restore_backup', errors: ['PREVIEW_STALE', 'BACKUP_CORRUPT', 'STORAGE_FULL'], emits: ['job_progress', 'collection_changed'], permission: 'file', screens: ['backup'], backendPhase: 12 }),

  getJobStatus: op({ kind: 'query', command: 'get_job_status', errors: ['NOT_FOUND'], emits: [], permission: 'none', screens: ['onboarding', 'transfer', 'backup', 'study'], backendPhase: 7 }),
  cancelJob: op({ kind: 'mutation', command: 'cancel_job', errors: ['NOT_FOUND'], emits: ['job_progress'], permission: 'none', screens: ['onboarding', 'transfer', 'backup'], backendPhase: 7 }),
  onJobEvent: op({ kind: 'subscription', command: 'job_progress', errors: [], emits: [], permission: 'none', screens: ['onboarding', 'transfer', 'backup', 'study'], backendPhase: 7 }),
  onCollectionChanged: op({ kind: 'subscription', command: 'collection_changed', errors: [], emits: [], permission: 'none', screens: ['shell'], backendPhase: 7 }),
} as const satisfies Record<AppApiOperation, OperationSpec>

export const OPERATION_NAMES = Object.keys(OPERATIONS) as AppApiOperation[]
