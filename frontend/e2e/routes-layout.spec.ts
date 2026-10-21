import { expect, test } from '@playwright/test'
import { layoutProblems, openWithPreferences } from './layout'

// Diff rule 4 for every screen, not only the gallery: at 200% text on the narrowest phone width
// no control may be clipped, overlap another control, or cause horizontal page scrolling.
const PATHS = [
  '/home', '/deck/1', '/study/1', '/summary/missing', '/card/1', '/editor', '/browser', '/stats', '/settings',
  '/settings/ai', '/settings/privacy', '/settings/about', '/settings/transfer', '/settings/backup', '/settings/attributions',
  '/onboarding', '/no-such-screen',
]

for (const language of ['pl', 'en'] as const) {
  for (const path of PATHS) {
    test(`${path} at 200% (${language}, 360 px) has no layout problems`, async ({ page }) => {
      await page.setViewportSize({ width: 360, height: 800 })
      await openWithPreferences(page, path, { language, theme: 'light', textScale: 'huge' })
      await expect(page.getByRole('heading', { level: 1 })).toBeVisible()
      await page.waitForLoadState('networkidle')
      expect(await layoutProblems(page)).toEqual([])
    })
  }
}
