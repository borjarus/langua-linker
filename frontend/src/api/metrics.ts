import { invoke } from '@tauri-apps/api/core'
import type { InvokeFn } from './tauriAppApi'

/**
 * Phase 16 opt-in usage metrics outside `AppApi` (ADR-13). Commands are in
 * `src-tauri/src/metrics.rs`; the backend drops every event while the learner has not opted in,
 * and nothing leaves the device. Only numbers and error kinds cross IPC — never a message or a
 * stack, which may contain card text.
 */
export type MetricName = 'retainedD7' | 'coldStartMedian' | 'generationP90' | 'favoriteRate' | 'crashFree'
export type TargetStatus = 'met' | 'missed' | 'notEnoughData' | 'cohortOnly'
export type ClientErrorKind = 'error' | 'unhandledRejection'

export interface TargetCheck {
  metric: MetricName
  target: string
  /** Milliseconds for latencies, a 0–1 ratio otherwise. */
  value: number | null
  samples: number
  status: TargetStatus
}

export interface MetricsSummary {
  since: string
  daysObserved: number
  activeDays: number
  appStarts: number
  retainedD7: boolean | null
  coldStartMedianMs: number | null
  generations: number
  generationFailures: number
  generationRetries: number
  generationP90Ms: number | null
  favorites: number
  favoriteRate: number | null
  nativeCrashes: number
  crashFreeRate: number | null
  clientErrors: number
  sessions: number
  ratings: { again: number; hard: number; good: number; easy: number }
  undos: number
  restores: number
  restoreFailures: number
  targets: TargetCheck[]
}

export interface UsageMetrics {
  enabled: boolean
  /** Pseudonymous id of this installation's metrics (not anonymous). */
  installId: string | null
  summary: MetricsSummary | null
}

export interface MetricsBridge {
  reportStartup(coldStartMs: number): Promise<void>
  reportClientError(kind: ClientErrorKind): Promise<void>
  getUsageMetrics(): Promise<UsageMetrics>
}

export function createMetricsBridge(invokeFn: InvokeFn = invoke): MetricsBridge {
  return {
    reportStartup: async (coldStartMs) => {
      await invokeFn('report_startup', { input: { coldStartMs } })
    },
    reportClientError: async (kind) => {
      await invokeFn('report_client_error', { input: { kind } })
    },
    getUsageMetrics: async () => (await invokeFn('get_usage_metrics')) as UsageMetrics,
  }
}

/** Longest cold start the backend accepts; a longer value is a broken measurement. */
const MAX_COLD_START_MS = 600_000

/**
 * Reports the time from navigation start to the first painted frame. Best effort: a failed
 * report never reaches the learner.
 */
export function reportColdStart(bridge: MetricsBridge, win: Pick<Window, 'requestAnimationFrame' | 'performance'> = window): void {
  win.requestAnimationFrame(() => {
    const elapsed = Math.round(win.performance.now())
    if (elapsed < 0 || elapsed > MAX_COLD_START_MS) return
    bridge.reportStartup(elapsed).catch(() => {})
  })
}

/** Reports the kind of every uncaught error and unhandled rejection; returns the cleanup. */
export function watchClientErrors(bridge: MetricsBridge, target: Pick<Window, 'addEventListener' | 'removeEventListener'> = window): () => void {
  const report = (kind: ClientErrorKind) => () => {
    bridge.reportClientError(kind).catch(() => {})
  }
  const onError = report('error')
  const onRejection = report('unhandledRejection')
  target.addEventListener('error', onError)
  target.addEventListener('unhandledrejection', onRejection)
  return () => {
    target.removeEventListener('error', onError)
    target.removeEventListener('unhandledrejection', onRejection)
  }
}
