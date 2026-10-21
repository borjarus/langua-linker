import AxeBuilder from '@axe-core/playwright'
import { expect, test, type Page } from '@playwright/test'
import { en } from '../src/locales/en'
import { pl } from '../src/locales/pl'
import { layoutProblems, openWithPreferences, type Preferences } from './layout'

// Phase 5 DoD: onboarding → study → editor → statistics → CSV preview → restore on demo data in
// Chromium and WebKit, and the nine Phase 5 screens at 200% text on 360 px in PL/EN, light and
// dark, with rule-4 layout and axe.

const LOCALES = { pl: pl as unknown as typeof en, en }
const t = en
const fill = (text: string, values: Record<string, string | number>) => text.replace(/{{(\w+)(?:, number)?}}/g, (_, key: string) => String(values[key] ?? ''))

async function useScenario(page: Page, scenario: string): Promise<void> {
  await page.addInitScript((value) => localStorage.setItem('ll:demo:scenario', value), scenario)
}

async function axeBlocking(page: Page, browserName: string): Promise<string[]> {
  const axe = new AxeBuilder({ page })
  // See gallery.spec.ts: WebKit reports stale colours for the demo banner; contrast is judged in Chromium.
  const results = await (browserName === 'webkit' ? axe.disableRules(['color-contrast']) : axe).analyze()
  return results.violations
    .filter((violation) => violation.impact === 'serious' || violation.impact === 'critical')
    .map((violation) => `${violation.id}: ${violation.nodes.map((node) => node.target.join(' ')).join(', ')}`)
}

test('onboarding → study → editor → statistics → CSV preview → restore', async ({ page }) => {
  test.setTimeout(120_000)
  await useScenario(page, 'newUser')
  await openWithPreferences(page, '/onboarding', { language: 'en', theme: 'light', textScale: 'normal' })
  await expect(page.getByRole('heading', { level: 2, name: t.onboarding.titles.nativeLanguage })).toBeVisible()
  await page.getByRole('button', { name: t.onboarding.next }).click()
  await page.getByRole('checkbox', { name: t.deck.targetLanguages.en }).check()
  await page.getByRole('button', { name: t.onboarding.next }).click()
  await page.getByRole('radio', { name: new RegExp(`^${fill(t.onboarding.goal.option_other, { count: 10 })}`) }).check()
  await page.getByRole('button', { name: t.onboarding.next }).click()
  await expect(page.getByText('English — words A1')).toBeVisible()
  await page.getByRole('button', { name: t.onboarding.next }).click()
  await expect(page.getByText(fill(t.onboarding.ready.decks_other, { count: 3 }))).toBeVisible()
  await page.getByRole('button', { name: t.onboarding.start }).click()
  await expect(page.getByRole('heading', { level: 1, name: t.dashboard.title })).toBeVisible({ timeout: 15_000 })

  await page.getByRole('button', { name: t.dashboard.studyAll }).click()
  for (let index = 0; index < 3; index += 1) {
    await page.getByRole('button', { name: t.study.showAnswer }).click()
    await page.getByRole('group', { name: t.study.rateGroup }).getByRole('button', { name: new RegExp(`^${t.study.ratings.Good}`) }).click()
  }
  await page.getByRole('button', { name: t.study.end }).click()
  await expect(page.getByRole('heading', { level: 1, name: t.routes.summary })).toBeVisible()

  await page.evaluate(() => (location.hash = '#/editor?deckId=3'))
  await page.getByRole('textbox', { name: fill(t.editor.front, { language: 'English' }) }).fill('the bridge')
  await page.getByRole('textbox', { name: t.editor.back }).fill('most')
  await page.getByRole('button', { name: t.common.save, exact: true }).click()
  await expect(page.getByRole('heading', { level: 1, name: t.routes.card })).toBeVisible()

  await page.getByRole('navigation').getByRole('link', { name: t.nav.stats }).click()
  await expect(page.getByRole('heading', { level: 2, name: t.stats.forecast.title })).toBeVisible()
  const forecast = page.getByRole('region', { name: t.stats.forecast.title })
  await forecast.getByRole('button', { name: t.stats.chart.showTable }).click()
  await expect(forecast.getByRole('columnheader', { name: t.stats.forecast.projected })).toBeVisible()
  await page.getByRole('radio', { name: t.stats.ranges['3m'] }).check()
  await expect(page.getByText(t.stats.buckets.week)).toBeVisible()

  await page.evaluate(() => (location.hash = '#/settings/transfer'))
  await page.getByRole('button', { name: t.transfer.import.pick }).click()
  const dry = page.getByRole('region', { name: t.transfer.preview.title })
  await expect(dry).toBeVisible()
  await expect(dry.getByText(fill(t.transfer.rowErrors.title_one, { count: 1 }))).toBeVisible()
  await page.getByRole('radio', { name: new RegExp(t.transfer.duplicates.update) }).check()
  await expect(page.getByRole('button', { name: /^Import \d+ notes$/ })).toBeEnabled()

  await page.evaluate(() => (location.hash = '#/settings/backup'))
  await page.getByRole('button', { name: t.backup.restore.fromFile }).click()
  const dialog = page.getByRole('dialog', { name: t.backup.restore.title })
  await expect(dialog.getByText(t.backup.restore.replaces)).toBeVisible()
  await dialog.getByRole('button', { name: t.backup.restore.confirm }).click()
  await expect(page.getByText(t.backup.restore.done)).toBeVisible()
  await expect(page.getByRole('heading', { level: 1, name: t.dashboard.title })).toBeVisible()
})

test('declined AI, telemetry and notifications leave learning working', async ({ page }) => {
  await openWithPreferences(page, '/settings/ai', { language: 'en', theme: 'light', textScale: 'normal' })
  await page.getByRole('radio', { name: t.ai.consent.declined }).check()
  await expect(page.getByText(t.ai.consent.declinedNote)).toBeVisible()
  await page.evaluate(() => (location.hash = '#/settings/privacy'))
  await expect(page.getByRole('checkbox', { name: t.privacy.telemetry.optIn })).not.toBeChecked()
  await page.evaluate(() => (location.hash = '#/home'))
  await page.getByRole('button', { name: t.dashboard.studyAll }).click()
  await page.getByRole('button', { name: t.study.showAnswer }).click()
  await page.getByRole('group', { name: t.study.rateGroup }).getByRole('button', { name: new RegExp(`^${t.study.ratings.Good}`) }).click()
  await expect(page.locator('main').getByText(/^1 review ·/)).toBeVisible()
})

for (const language of ['pl', 'en'] as const) {
  for (const theme of ['light', 'dark'] as const) {
    const locale = LOCALES[language]
    const preferences: Preferences = { language, theme, textScale: 'huge' }

    test.describe(`Phase 5 screens at 200% (${language}, ${theme}, 360 px)`, () => {
      test.use({ viewport: { width: 360, height: 800 } })

      test('statistics, settings, onboarding, transfer and backup have no layout problems or serious axe violations', async ({ page, browserName }) => {
        test.setTimeout(180_000)
        const check = async (scope?: string) => {
          await page.waitForLoadState('networkidle')
          expect(await layoutProblems(page, scope)).toEqual([])
          expect(await axeBlocking(page, browserName)).toEqual([])
        }
        const go = async (hash: string, ready: string) => {
          await page.evaluate((value) => (location.hash = value), hash)
          await expect(page.getByRole('heading', { level: 2, name: ready }).first()).toBeVisible()
        }

        await openWithPreferences(page, '/stats', preferences)
        await expect(page.getByRole('heading', { level: 2, name: locale.stats.forecast.title })).toBeVisible()
        await check()
        await page.getByRole('region', { name: locale.stats.forecast.title }).getByRole('button', { name: locale.stats.chart.showTable }).click()
        await check()

        await go('#/settings', locale.settings.languages)
        await check()
        await go('#/settings/ai', locale.ai.consent.title)
        await check()
        await go('#/settings/privacy', locale.privacy.telemetry.title)
        await check()
        await page.getByRole('button', { name: locale.privacy.reset.everything.open }).click()
        await expect(page.getByRole('dialog').getByRole('textbox')).toBeVisible()
        await check('[role="dialog"]')
        await page.getByRole('dialog').getByRole('button', { name: locale.common.cancel }).click()

        await go('#/settings/about', locale.about.versions)
        await page.getByRole('button', { name: locale.about.diagnostics.show }).click()
        await expect(page.getByRole('list', { name: locale.about.diagnostics.title })).toBeVisible()
        await check()
        await go('#/settings/attributions', locale.attributions.scopes.content)
        await check()

        await go('#/settings/transfer', locale.transfer.import.title)
        await page.getByRole('button', { name: locale.transfer.import.pick }).click()
        await expect(page.getByRole('region', { name: locale.transfer.preview.title })).toBeVisible()
        await check()

        await go('#/settings/backup', locale.backup.createTitle)
        await expect(page.getByRole('heading', { level: 2, name: locale.backup.list })).toBeVisible()
        await check()
        await page.getByRole('button', { name: locale.backup.restore.fromFile }).click()
        await expect(page.getByRole('dialog', { name: locale.backup.restore.title })).toBeVisible()
        await check('[role="dialog"]')
        await page.getByRole('dialog').getByRole('button', { name: locale.common.cancel }).click()

        await go('#/onboarding?from=settings', locale.onboarding.titles.nativeLanguage)
        await check()
        for (let step = 0; step < 3; step += 1) await page.getByRole('button', { name: locale.onboarding.next }).click()
        await expect(page.getByRole('heading', { level: 2, name: locale.onboarding.titles.decks })).toBeVisible()
        await check()
      })
    })
  }
}
