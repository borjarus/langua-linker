import AxeBuilder from '@axe-core/playwright'
import { expect, test } from '@playwright/test'
import { en } from '../src/locales/en'
import { pl } from '../src/locales/pl'
import { layoutProblems, openWithPreferences } from './layout'

// Gallery matrix from docs/testing/ui-test-matrix.md: {PL, EN} × {light, dark} × {100%, 200%} at
// 360 px and 1280 px, per engine project. Layout, interaction and axe checks always run; pixel
// snapshots only with PW_SNAPSHOTS=1 (baselines come from Linux CI, diff rule 6).

// `pl` is typed loosely; key parity with `en` is enforced by tests/i18n.test.ts.
const LOCALES = { pl: pl as unknown as typeof en, en }
const LANGUAGES = ['pl', 'en'] as const
const THEMES = ['light', 'dark'] as const
const SCALES = [
  { name: '100', textScale: 'normal' },
  { name: '200', textScale: 'huge' },
] as const
const VIEWPORTS = [
  { width: 360, height: 800 },
  { width: 1280, height: 800 },
] as const

for (const language of LANGUAGES) {
  const t = LOCALES[language]
  for (const theme of THEMES) {
    for (const scale of SCALES) {
      for (const viewport of VIEWPORTS) {
        const id = `${language}-${theme}-${scale.name}-${viewport.width}`

        test.describe(`gallery ${id}`, () => {
          test.use({ viewport, colorScheme: theme })

          test.beforeEach(async ({ page }) => {
            await openWithPreferences(page, '/gallery', { language, theme, textScale: scale.textScale })
            await expect(page.getByRole('heading', { level: 1, name: t.routes.gallery })).toBeVisible()
            await expect(page.locator('html')).toHaveAttribute('lang', language)
            await expect(page.locator('html')).toHaveAttribute('data-theme', theme)
          })

          test('layout has no clipped, overlapping or off-screen controls', async ({ page }) => {
            expect(await layoutProblems(page)).toEqual([])
          })

          test('dialog, sheet and toast work; Back closes the dialog', async ({ page }) => {
            // Keyboard activation: WebKit does not focus a button on click, so focus restoration
            // is only defined for keyboard users there (as in Safari).
            const openDialog = page.getByRole('button', { name: t.gallery.openDialog })
            await openDialog.focus()
            await page.keyboard.press('Enter')
            const dialog = page.getByRole('dialog', { name: t.gallery.dialogTitle })
            await expect(dialog).toBeVisible()
            await expect(dialog.getByRole('button', { name: t.common.cancel })).toBeInViewport()
            await page.keyboard.press('Escape')
            await expect(dialog).toBeHidden()
            await expect(openDialog).toBeFocused()

            await openDialog.click()
            await expect(dialog).toBeVisible()
            await page.goBack()
            await expect(dialog).toBeHidden()
            await expect(page).toHaveURL(/#\/gallery$/)

            await page.getByRole('button', { name: t.gallery.openSheet }).click()
            const sheet = page.getByRole('dialog', { name: t.gallery.sheetTitle })
            await expect(sheet).toBeInViewport()
            await sheet.getByRole('button', { name: t.dialog.close }).click()
            await expect(sheet).toBeHidden()

            await page.getByRole('button', { name: t.gallery.showToast }).click()
            const toast = page.getByText(t.gallery.toastMessage)
            await expect(toast).toBeInViewport({ ratio: 1 })
            const nav = page.getByRole('navigation').first()
            const [toastBox, navBox] = [await toast.boundingBox(), await nav.boundingBox()]
            if (viewport.width < 832 && toastBox && navBox) expect(toastBox.y + toastBox.height).toBeLessThanOrEqual(navBox.y)
          })

          test('has no serious or critical axe violations', async ({ page, browserName }) => {
            // Playwright's WebKit reports stale computed colours for the demo banner in dark mode
            // (the rendered pixels are correct), so contrast is judged in Chromium; tokens are shared.
            const axe = new AxeBuilder({ page })
            const results = await (browserName === 'webkit' ? axe.disableRules(['color-contrast']) : axe).analyze()
            const blocking = results.violations
              .filter((violation) => violation.impact === 'serious' || violation.impact === 'critical')
              .map((violation) => `${violation.id}: ${violation.nodes.map((node) => node.target.join(' ')).join(', ')}`)
            expect(blocking).toEqual([])
          })

          test('matches the visual baseline', async ({ page }) => {
            test.skip(process.env.PW_SNAPSHOTS !== '1', 'pixel baselines are generated and compared on Linux CI only')
            await expect(page).toHaveScreenshot(`gallery-${id}.png`, { fullPage: true })
          })
        })
      }
    }
  }
}
