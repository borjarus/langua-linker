import { expect, test, type Page } from '@playwright/test'
import { en } from '../src/locales/en'
import { openWithPreferences } from './layout'

// ADR-03 evidence: frame profiling of the card browser over 50 000 matches (`large` demo scenario).
// Scrolls the list one step per animation frame and records frame gaps, so a slow range calculation,
// render or page fetch shows up as long frames. Results are attached to the test report.

interface FrameStats {
  frames: number
  p50: number
  p95: number
  max: number
  over50ms: number
  maxRows: number
  lastPosition: number
}

async function scrollAndMeasure(page: Page, pixelsPerFrame: number, frames: number): Promise<FrameStats> {
  return page.getByRole('list', { name: en.browser.resultsLabel }).evaluate(
    (list, { pixelsPerFrame, frames }) =>
      new Promise<FrameStats>((resolve) => {
        const gaps: number[] = []
        let maxRows = 0
        let last = performance.now()
        let remaining = frames
        const step = (now: number) => {
          gaps.push(now - last)
          last = now
          maxRows = Math.max(maxRows, list.querySelectorAll('[role="listitem"]').length)
          if (remaining-- > 0) {
            list.scrollTop += pixelsPerFrame
            requestAnimationFrame(step)
            return
          }
          const sorted = gaps.slice(1).sort((a, b) => a - b)
          const at = (q: number) => Math.round(sorted[Math.min(sorted.length - 1, Math.floor(q * sorted.length))] ?? 0)
          const rows = list.querySelectorAll('[role="listitem"]')
          resolve({
            frames: sorted.length,
            p50: at(0.5),
            p95: at(0.95),
            max: Math.round(sorted.at(-1) ?? 0),
            over50ms: sorted.filter((gap) => gap > 50).length,
            maxRows,
            lastPosition: Number(rows[rows.length - 1]?.getAttribute('aria-posinset') ?? 0),
          })
        }
        requestAnimationFrame(step)
      }),
    { pixelsPerFrame, frames },
  )
}

for (const { name, cpuSlowdown } of [
  { name: 'desktop', cpuSlowdown: 1 },
  { name: 'mid-range phone (4× CPU throttling, Chromium only)', cpuSlowdown: 4 },
]) {
  test(`50 000-card browser scrolls with bounded DOM and frame budget — ${name}`, async ({ page, browserName }) => {
    test.skip(cpuSlowdown > 1 && browserName !== 'chromium', 'CPU throttling needs the Chrome DevTools Protocol')
    test.setTimeout(90_000)
    await page.addInitScript(() => localStorage.setItem('ll:demo:scenario', 'large'))
    await openWithPreferences(page, '/browser', { language: 'en', theme: 'light', textScale: 'huge' })
    const list = page.getByRole('list', { name: en.browser.resultsLabel })
    await expect(list.getByRole('listitem').first()).toHaveAttribute('aria-setsize', '50000')

    if (cpuSlowdown > 1) {
      const cdp = await page.context().newCDPSession(page)
      await cdp.send('Emulation.setCPUThrottlingRate', { rate: cpuSlowdown })
    }

    // A slow fling (~3 rows per frame), then a fast one (~1 screen per frame) that crosses many 100-card pages.
    const slow = await scrollAndMeasure(page, 200, 300)
    const fast = await scrollAndMeasure(page, 2_000, 300)
    await test.info().attach('frame-stats.json', { body: JSON.stringify({ browserName, cpuSlowdown, slow, fast }, null, 2), contentType: 'application/json' })
    console.log(`[ADR-03] ${browserName} ×${cpuSlowdown}`, JSON.stringify({ slow, fast }))

    for (const stats of [slow, fast]) {
      expect(stats.maxRows).toBeLessThan(60)
      // Frame budget is gated in desktop Chromium only: Playwright's WebKit build on Windows/Linux and
      // CPU throttling are far noisier than real devices, so those runs are recorded for the ADR, not asserted.
      if (browserName === 'chromium' && cpuSlowdown === 1) {
        expect(stats.p50).toBeLessThanOrEqual(20)
        expect(stats.p95).toBeLessThanOrEqual(34)
        expect(stats.max).toBeLessThanOrEqual(150)
      }
    }
    expect(fast.lastPosition).toBeGreaterThan(slow.lastPosition)

    // After scrolling stops, the rows of the reached page are loaded (no skeletons left in view).
    await expect(list.getByRole('link').first()).toBeVisible()
  })
}
