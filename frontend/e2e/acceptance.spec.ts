import { expect, test, type Page } from '@playwright/test'
import { en } from '../src/locales/en'
import { layoutProblems, openWithPreferences } from './layout'

// Phase 6 gate, step 3: the acceptance scenarios on the demo build in Chromium and WebKit —
// new user, large collection, nothing due, offline, save error, ambiguous word, reset, restore.
// Each scenario ends on a usable screen with no rule-4 layout problem.

const t = en
const fill = (text: string, values: Record<string, string | number>) => text.replace(/{{(\w+)(?:, number)?}}/g, (_, key: string) => String(values[key] ?? ''))

async function start(page: Page, scenario: string, path: string): Promise<void> {
  await page.addInitScript((value) => localStorage.setItem('ll:demo:scenario', value), scenario)
  await openWithPreferences(page, path, { language: 'en', theme: 'light', textScale: 'normal' })
}

const ratingButton = (page: Page, rating: keyof typeof t.study.ratings) =>
  page.getByRole('group', { name: t.study.rateGroup }).getByRole('button', { name: new RegExp(`^${t.study.ratings[rating]}`) })

test.afterEach(async ({ page }) => {
  // With a modal open the page behind it is inert, so only the dialog's controls are checked.
  const modal = (await page.getByRole('dialog').count()) > 0
  expect(await layoutProblems(page, modal ? '[role="dialog"]' : undefined)).toEqual([])
})

test('new user: onboarding leads to a dashboard with bundled decks', async ({ page }) => {
  await start(page, 'newUser', '/onboarding')
  await expect(page.getByRole('heading', { level: 2, name: t.onboarding.titles.nativeLanguage })).toBeVisible()
  await page.getByRole('button', { name: t.onboarding.next }).click()
  await page.getByRole('checkbox', { name: t.deck.targetLanguages.en }).check()
  await page.getByRole('button', { name: t.onboarding.next }).click()
  await page.getByRole('button', { name: t.onboarding.next }).click()
  await page.getByRole('button', { name: t.onboarding.next }).click()
  await page.getByRole('button', { name: t.onboarding.start }).click()
  await expect(page.getByRole('heading', { level: 1, name: t.dashboard.title })).toBeVisible({ timeout: 15_000 })
  await expect(page.getByRole('button', { name: t.dashboard.studyAll })).toBeVisible()
})

test('large collection: 50 000 matches page into a bounded list', async ({ page }) => {
  await start(page, 'large', '/browser')
  await expect(page.getByText(fill(t.browser.results_other, { count: '50,000' }))).toBeVisible({ timeout: 15_000 })
  const rows = page.getByRole('list', { name: t.browser.resultsLabel }).getByRole('listitem')
  expect(await rows.count()).toBeLessThan(80)
  await expect(rows.first()).toHaveAttribute('aria-setsize', '50000')
})

test('nothing due: the dashboard says so and offers browsing, not study', async ({ page }) => {
  await start(page, 'caughtUp', '/home')
  await expect(page.getByText(t.dashboard.caughtUpTitle)).toBeVisible()
  await expect(page.getByText(/Next review:/)).toBeVisible()
  await expect(page.getByRole('button', { name: t.dashboard.studyAll })).toHaveCount(0)
})

test('offline: study still works and generation is queued', async ({ page }) => {
  await start(page, 'offline', '/home')
  await page.getByRole('button', { name: t.dashboard.studyAll }).click()
  await page.getByRole('button', { name: t.study.showAnswer }).click()
  await ratingButton(page, 'Good').click()
  await expect(page.getByRole('button', { name: t.study.showAnswer })).toBeVisible()
  // Leaving goes through the session guard; the rating is already saved.
  await page.evaluate(() => (location.hash = '#/card/1000'))
  await page.getByRole('dialog').getByRole('button', { name: t.study.leave }).click()
  await page.getByRole('button', { name: t.association.generate }).click()
  await expect(page.getByText(t.association.queued)).toBeVisible()
})

test('save error: the card stays and a retry saves the same rating', async ({ page }) => {
  await start(page, 'happy', '/home')
  await page.getByRole('button', { name: t.dashboard.studyAll }).click()
  await page.getByRole('button', { name: t.study.showAnswer }).click()
  const front = await page.locator('.front').first().textContent()
  await page.getByRole('button', { name: t.demo.failNextSave }).click()
  await ratingButton(page, 'Good').click()
  await expect(page.getByText(t.errors.DB_BUSY)).toBeVisible()
  await expect(page.locator('.front').first()).toHaveText(front ?? '')
  await page.getByRole('button', { name: t.common.retry }).click()
  await expect(page.getByRole('button', { name: t.study.showAnswer })).toBeVisible()
})

test('ambiguous word: the learner chooses and the choice is kept', async ({ page }) => {
  await start(page, 'ambiguous', '/home')
  await page.evaluate(() => (location.hash = '#/study/2'))
  await page.getByRole('main').getByRole('button', { name: t.deck.study, exact: true }).click()
  await page.getByRole('button', { name: new RegExp(t.wordLink.resolution.ambiguous) }).first().click()
  const sheet = page.getByRole('dialog')
  await sheet.getByRole('radio').first().check()
  await sheet.getByRole('button', { name: t.wordSheet.choose }).click()
  await expect(sheet.getByText(t.wordSheet.choiceSaved)).toBeVisible()
})

test('reset: progress is reset after the typed phrase and the app returns home', async ({ page }) => {
  await start(page, 'happy', '/settings/privacy')
  await page.getByRole('button', { name: t.privacy.reset.progress.open }).click()
  const dialog = page.getByRole('dialog', { name: t.privacy.reset.progress.title })
  await dialog.getByRole('textbox', { name: /RESETUJ POSTĘP/ }).fill('RESETUJ POSTĘP')
  await dialog.getByRole('button', { name: t.privacy.reset.progress.confirm }).click()
  await expect(page.getByText(t.privacy.reset.progress.done)).toBeVisible({ timeout: 15_000 })
  await expect(page.getByRole('heading', { level: 1, name: t.dashboard.title })).toBeVisible()
})

test('restore: a validated backup replaces the collection and reopens it', async ({ page }) => {
  await start(page, 'happy', '/settings/backup')
  await page.getByRole('button', { name: t.backup.restore.fromFile }).click()
  const dialog = page.getByRole('dialog', { name: t.backup.restore.title })
  await dialog.getByRole('button', { name: t.backup.restore.confirm }).click()
  await expect(page.getByText(t.backup.restore.done)).toBeVisible({ timeout: 15_000 })
  await expect(page.getByRole('heading', { level: 1, name: t.dashboard.title })).toBeVisible()
})
