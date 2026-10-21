import AxeBuilder from '@axe-core/playwright'
import { expect, test, type Page } from '@playwright/test'
import { en } from '../src/locales/en'
import { pl } from '../src/locales/pl'
import { layoutProblems, openWithPreferences, type Preferences } from './layout'

// Phase 3 DoD: the demo learning flow end to end in Chromium and WebKit, plus rule-4 layout and
// axe on the learning screens at 200% text. Mock data only — this is a prototype, not FSRS.

const LOCALES = { pl: pl as unknown as typeof en, en }
const t = en

async function useScenario(page: Page, scenario: string): Promise<void> {
  await page.addInitScript((value) => localStorage.setItem('ll:demo:scenario', value), scenario)
}

const ratingGroup = (page: Page, locale: typeof en = t) => page.getByRole('group', { name: locale.study.rateGroup })
const reviewed = (page: Page, count: number) => page.locator('main').getByText(new RegExp(`^${count} reviews? ·`))

async function axeBlocking(page: Page, browserName: string): Promise<string[]> {
  const axe = new AxeBuilder({ page })
  // See gallery.spec.ts: WebKit reports stale colours for the demo banner; contrast is judged in Chromium.
  const results = await (browserName === 'webkit' ? axe.disableRules(['color-contrast']) : axe).analyze()
  return results.violations
    .filter((violation) => violation.impact === 'serious' || violation.impact === 'critical')
    .map((violation) => `${violation.id}: ${violation.nodes.map((node) => node.target.join(' ')).join(', ')}`)
}

test('onboarding → 20 ratings → summary → dashboard', async ({ page }) => {
  test.setTimeout(120_000)
  await useScenario(page, 'newUser')
  await openWithPreferences(page, '/onboarding', { language: 'en', theme: 'light', textScale: 'normal' })
  // Native language → target languages (both: all bundled decks, enough cards for 20 ratings) → goal → decks.
  await page.getByRole('button', { name: t.onboarding.next }).click()
  await page.getByRole('checkbox', { name: t.deck.targetLanguages.en }).check()
  for (let step = 0; step < 3; step += 1) await page.getByRole('button', { name: t.onboarding.next }).click()
  await page.getByRole('button', { name: t.onboarding.start }).click()
  await expect(page.getByRole('heading', { level: 1, name: t.dashboard.title })).toBeVisible({ timeout: 15_000 })

  await page.getByRole('button', { name: t.dashboard.studyAll }).click()
  await expect(page.getByRole('button', { name: t.study.showAnswer })).toBeVisible()

  // Alternating Again/Good keeps the queue long enough (Again re-queues the card).
  const pattern = ['Again', 'Good', 'Again', 'Easy', 'Hard'] as const
  for (let index = 0; index < 20; index += 1) {
    await page.getByRole('button', { name: t.study.showAnswer }).click()
    const rating = pattern[index % pattern.length] ?? 'Good'
    await ratingGroup(page).getByRole('button', { name: new RegExp(`^${t.study.ratings[rating]}`) }).click()
    await expect(reviewed(page, index + 1)).toBeVisible()
  }

  await page.getByRole('button', { name: t.study.end }).click()
  await expect(page.getByRole('heading', { level: 1, name: t.routes.summary })).toBeVisible()
  await expect(page.getByText(/20 reviews/)).toBeVisible()
  await expect(page.getByRole('region', { name: t.summary.againTitle })).toBeVisible()
  await expect(page.getByRole('region', { name: t.summary.reminderTitle })).toBeVisible()

  await page.getByRole('link', { name: t.common.goHome }).last().click()
  await expect(page.getByRole('heading', { level: 1, name: t.dashboard.title })).toBeVisible()
  await expect(page.getByRole('region', { name: t.dashboard.resumeTitle })).toHaveCount(0)
})

test('keyboard, undo, card actions and the leave guard', async ({ page }) => {
  await openWithPreferences(page, '/home', { language: 'en', theme: 'light', textScale: 'normal' })
  await page.getByRole('button', { name: t.deck.studyDeck.replace('{{name}}', 'Niemiecki — słówka A1') }).click()
  const front = page.locator('main .front')
  await expect(page.getByRole('button', { name: t.study.showAnswer })).toBeVisible()
  const first = await front.textContent()

  await page.locator('main h1').focus()
  await page.keyboard.press('Space')
  await expect(ratingGroup(page)).toBeVisible()
  await page.keyboard.press('3')
  await expect(reviewed(page, 1)).toBeVisible()

  await page.getByRole('button', { name: t.study.undo }).click()
  await expect(page.getByText(t.study.undone)).toBeVisible()
  await expect(front).toHaveText(first ?? '')

  await page.getByRole('button', { name: t.study.moreActions }).click()
  await page.getByRole('dialog', { name: t.study.moreActions }).getByRole('button', { name: t.study.actions.suspend }).click()
  await expect(page.getByText(t.study.done.suspend)).toBeVisible()
  await expect(front).not.toHaveText(first ?? '')

  // Leaving through the navigation asks first; staying keeps the session.
  await page.getByRole('navigation').getByRole('link', { name: t.nav.stats }).click()
  const leave = page.getByRole('dialog', { name: t.study.leaveTitle })
  await expect(leave).toBeVisible()
  await leave.getByRole('button', { name: t.study.stay }).click()
  await expect(leave).toBeHidden()
  await expect(page).toHaveURL(/#\/study\/1\?session=/)

  await page.goBack()
  await page.getByRole('dialog', { name: t.study.leaveTitle }).getByRole('button', { name: t.study.leave }).click()
  await expect(page.getByRole('heading', { level: 1, name: t.dashboard.title })).toBeVisible()
  await expect(page.getByRole('region', { name: t.dashboard.resumeTitle })).toBeVisible()
})

for (const language of ['pl', 'en'] as const) {
  const locale = LOCALES[language]
  const preferences: Preferences = { language, theme: 'light', textScale: 'huge' }

  test.describe(`learning screens at 200% (${language}, 360 px)`, () => {
    test.use({ viewport: { width: 360, height: 800 } })

    test('dashboard, study and summary have no layout problems or serious axe violations', async ({ page, browserName }) => {
      test.setTimeout(90_000)
      await openWithPreferences(page, '/home', preferences)
      await expect(page.getByRole('button', { name: locale.dashboard.studyAll })).toBeVisible()
      expect(await layoutProblems(page)).toEqual([])

      await page.getByRole('button', { name: locale.dashboard.studyAll }).click()
      await page.getByRole('button', { name: locale.study.showAnswer }).click()
      await expect(ratingGroup(page, locale)).toBeVisible()
      await page.waitForLoadState('networkidle')
      expect(await layoutProblems(page)).toEqual([])
      expect(await axeBlocking(page, browserName)).toEqual([])

      await ratingGroup(page, locale).getByRole('button', { name: new RegExp(`^${locale.study.ratings.Again}`) }).click()
      await page.getByRole('button', { name: locale.study.showAnswer }).click()
      await ratingGroup(page, locale).getByRole('button', { name: new RegExp(`^${locale.study.ratings.Good}`) }).click()
      await expect(page.getByRole('button', { name: locale.study.showAnswer })).toBeVisible()
      await page.getByRole('button', { name: locale.study.end }).click()
      await expect(page.getByRole('heading', { level: 1, name: locale.routes.summary })).toBeVisible()
      await expect(page.getByRole('region', { name: locale.summary.againTitle })).toBeVisible()
      await page.waitForLoadState('networkidle')
      expect(await layoutProblems(page)).toEqual([])
      expect(await axeBlocking(page, browserName)).toEqual([])
    })
  })
}
