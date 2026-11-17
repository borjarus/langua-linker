import { screen } from '@testing-library/svelte'
import { describe, expect, it, vi } from 'vitest'
import { createMetricsBridge, reportColdStart, watchClientErrors, type MetricsBridge, type UsageMetrics } from '../src/api/metrics'
import UsageMetricsPanel from '../src/features/settings/UsageMetricsPanel.svelte'
import { createTestServices, renderWith } from './helpers/services'

const flush = () => new Promise((resolve) => setTimeout(resolve, 0))

const SAMPLE: UsageMetrics = {
  enabled: true,
  installId: '0123456789abcdef0123456789abcdef',
  summary: {
    since: '2026-11-01T08:00:00.000Z',
    daysObserved: 9,
    activeDays: 5,
    appStarts: 12,
    retainedD7: true,
    coldStartMedianMs: 1340,
    generations: 11,
    generationFailures: 1,
    generationRetries: 2,
    generationP90Ms: 4500,
    favorites: 3,
    favoriteRate: 0.3,
    nativeCrashes: 0,
    crashFreeRate: 1,
    clientErrors: 1,
    sessions: 6,
    ratings: { again: 4, hard: 2, good: 30, easy: 5 },
    undos: 1,
    restores: 0,
    restoreFailures: 0,
    targets: [
      { metric: 'retainedD7', target: '>= 25%', value: 1, samples: 1, status: 'cohortOnly' },
      { metric: 'coldStartMedian', target: '< 2000 ms', value: 1340, samples: 12, status: 'met' },
      { metric: 'generationP90', target: '< 4000 ms', value: 4500, samples: 11, status: 'missed' },
      { metric: 'favoriteRate', target: '>= 20%', value: 0.3, samples: 10, status: 'met' },
      { metric: 'crashFree', target: '>= 99.5%', value: 1, samples: 12, status: 'notEnoughData' },
    ],
  },
}

function bridge(metrics: UsageMetrics = SAMPLE): MetricsBridge {
  return {
    reportStartup: vi.fn(async () => {}),
    reportClientError: vi.fn(async () => {}),
    getUsageMetrics: vi.fn(async () => metrics),
  }
}

describe('metrics bridge', () => {
  it('sends the commands of src-tauri/src/metrics.rs with numbers and kinds only', async () => {
    const invoke = vi.fn(async (command: string) => (command === 'get_usage_metrics' ? SAMPLE : null))
    const metrics = createMetricsBridge(invoke)
    await metrics.reportStartup(812)
    await metrics.reportClientError('unhandledRejection')
    expect(await metrics.getUsageMetrics()).toEqual(SAMPLE)
    expect(invoke.mock.calls).toEqual([
      ['report_startup', { input: { coldStartMs: 812 } }],
      ['report_client_error', { input: { kind: 'unhandledRejection' } }],
      ['get_usage_metrics'],
    ])
  })

  it('reports the cold start after the first frame and ignores broken measurements', () => {
    const metrics = bridge()
    const frames: (() => void)[] = []
    const win = { requestAnimationFrame: (cb: FrameRequestCallback) => frames.push(() => cb(0)), performance: { now: () => 1234.4 } }
    reportColdStart(metrics, win as never)
    expect(metrics.reportStartup).not.toHaveBeenCalled()
    frames.forEach((frame) => frame())
    expect(metrics.reportStartup).toHaveBeenCalledWith(1234)
    const late = bridge()
    reportColdStart(late, { requestAnimationFrame: (cb: FrameRequestCallback) => cb(0), performance: { now: () => 3_600_000 } } as never)
    expect(late.reportStartup).not.toHaveBeenCalled()
  })

  it('reports error kinds without messages, survives a failing report and cleans up', async () => {
    const metrics = bridge()
    vi.mocked(metrics.reportClientError).mockRejectedValueOnce(new Error('ipc down'))
    const target = new EventTarget()
    const stop = watchClientErrors(metrics, target as never)
    target.dispatchEvent(Object.assign(new Event('error'), { message: 'der Hund' }))
    target.dispatchEvent(new Event('unhandledrejection'))
    await flush()
    expect(vi.mocked(metrics.reportClientError).mock.calls).toEqual([['error'], ['unhandledRejection']])
    stop()
    target.dispatchEvent(new Event('error'))
    expect(metrics.reportClientError).toHaveBeenCalledTimes(2)
  })
})

describe('usage metrics panel', () => {
  it('shows each goal with the measured value and status, and the pseudonymous id prefix only', async () => {
    const services = await createTestServices({ language: 'en' })
    renderWith(UsageMetricsPanel, services, { metrics: bridge() })
    expect(await screen.findByText('App start (median)')).toBeInTheDocument()
    expect(screen.getByText('1,340 ms')).toBeInTheDocument()
    expect(screen.getByText('4,500 ms')).toBeInTheDocument()
    expect(screen.getByText('Goal missed')).toBeInTheDocument()
    expect(screen.getByText('Judged across all testers')).toBeInTheDocument()
    expect(screen.getByText('30%')).toBeInTheDocument()
    expect(screen.getByText(/Random ID: 01234567…/)).toBeInTheDocument()
    expect(document.body.textContent).not.toContain(SAMPLE.installId)
  })

  it('says when nothing is collected', async () => {
    const services = await createTestServices({ language: 'en' })
    renderWith(UsageMetricsPanel, services, { metrics: bridge({ enabled: false, installId: null, summary: null }) })
    expect(await screen.findByText('Nothing collected yet.')).toBeInTheDocument()
  })
})
