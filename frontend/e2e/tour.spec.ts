import { expect, test, type Page } from '@playwright/test'
import { en } from '../src/locales/en'
import { pl } from '../src/locales/pl'
import { layoutProblems, openWithPreferences } from './layout'

// Phase 6 gate, step 8: screenshot baseline of the whole application on demo data — every screen
// at 360 px in PL, light and dark, per engine. The layout check always runs; pixels are compared
// only with PW_SNAPSHOTS=1 and baselines come from the `E2E baselines` workflow (diff rule 6).
// GUI milestone ≠ working domain: the screenshots show the mock, banner included.

const t = pl as unknown as typeof en
const THEMES = ['light', 'dark'] as const

const SCREENS: { id: string; path: string; heading: string }[] = [
  { id: 'home', path: '/home', heading: t.dashboard.title },
  { id: 'deck', path: '/deck/1', heading: 'Niemiecki — słówka A1' },
  { id: 'card', path: '/card/1000', heading: t.routes.card },
  { id: 'editor', path: '/editor?deckId=1', heading: t.editor.newNote },
  { id: 'browser', path: '/browser', heading: t.routes.browser },
  { id: 'stats', path: '/stats', heading: t.routes.stats },
  { id: 'settings', path: '/settings', heading: t.routes.settings },
  { id: 'settings-ai', path: '/settings/ai', heading: t.routes.settingsAi },
  { id: 'settings-privacy', path: '/settings/privacy', heading: t.routes.settingsPrivacy },
  { id: 'settings-about', path: '/settings/about', heading: t.routes.settingsAbout },
  { id: 'settings-transfer', path: '/settings/transfer', heading: t.routes.settingsTransfer },
  { id: 'settings-backup', path: '/settings/backup', heading: t.routes.settingsBackup },
  { id: 'settings-attributions', path: '/settings/attributions', heading: t.routes.settingsAttributions },
  { id: 'onboarding', path: '/onboarding', heading: t.routes.onboarding },
  { id: 'not-found', path: '/does-not-exist', heading: t.routes.notFound },
]

async function snapshot(page: Page, name: string): Promise<void> {
  expect(await layoutProblems(page)).toEqual([])
  if (process.env.PW_SNAPSHOTS === '1') await expect(page).toHaveScreenshot(name, { fullPage: true })
}

for (const theme of THEMES) {
  test.describe(`application tour ${theme}`, () => {
    test.use({ viewport: { width: 360, height: 800 }, colorScheme: theme })

    for (const screen of SCREENS) {
      test(`${screen.id} matches the visual baseline`, async ({ page }) => {
        await openWithPreferences(page, screen.path, { language: 'pl', theme, textScale: 'normal' })
        await expect(page.getByRole('heading', { level: 1, name: screen.heading })).toBeVisible()
        // Every screen settles on content, never on a skeleton.
        await expect(page.locator('.skeleton')).toHaveCount(0)
        await snapshot(page, `tour-${screen.id}-${theme}.png`)
      })
    }

    test('study → summary matches the visual baseline', async ({ page }) => {
      await openWithPreferences(page, '/home', { language: 'pl', theme, textScale: 'normal' })
      await page.getByRole('button', { name: t.dashboard.studyAll }).click()
      await page.getByRole('button', { name: t.study.showAnswer }).click()
      await expect(page.getByRole('group', { name: t.study.rateGroup })).toBeVisible()
      await snapshot(page, `tour-study-${theme}.png`)
      await page.getByRole('group', { name: t.study.rateGroup }).getByRole('button', { name: new RegExp(`^${t.study.ratings.Good}`) }).click()
      await page.getByRole('button', { name: t.study.end }).click()
      await expect(page.getByRole('heading', { level: 1, name: t.routes.summary })).toBeVisible()
      await snapshot(page, `tour-summary-${theme}.png`)
    })
  })
}
