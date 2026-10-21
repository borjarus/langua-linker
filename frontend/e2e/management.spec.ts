import AxeBuilder from '@axe-core/playwright'
import { expect, test, type Page } from '@playwright/test'
import { en } from '../src/locales/en'
import { pl } from '../src/locales/pl'
import { layoutProblems, openWithPreferences, type Preferences } from './layout'

// Phase 4 DoD: content management on demo data in Chromium and WebKit — editor → details →
// browser bulk action, deck options — plus rule-4 layout and axe at 200% text on 360 px.

const LOCALES = { pl: pl as unknown as typeof en, en }
const t = en
const fill = (text: string, values: Record<string, string | number>) => text.replace(/{{(\w+)(?:, number)?}}/g, (_, key: string) => String(values[key] ?? ''))

async function axeBlocking(page: Page, browserName: string): Promise<string[]> {
  const axe = new AxeBuilder({ page })
  // See gallery.spec.ts: WebKit reports stale colours for the demo banner; contrast is judged in Chromium.
  const results = await (browserName === 'webkit' ? axe.disableRules(['color-contrast']) : axe).analyze()
  return results.violations
    .filter((violation) => violation.impact === 'serious' || violation.impact === 'critical')
    .map((violation) => `${violation.id}: ${violation.nodes.map((node) => node.target.join(' ')).join(', ')}`)
}

async function openCard(page: Page, query: string) {
  await page.evaluate((q) => (location.hash = `#/browser?q=${encodeURIComponent(q)}`), query)
  await page.getByRole('list', { name: t.browser.resultsLabel }).getByRole('link').first().click()
  await expect(page.getByRole('heading', { level: 1, name: t.routes.card })).toBeVisible()
}

test('editor → card details → edit → browser bulk tag', async ({ page }) => {
  test.setTimeout(90_000)
  await openWithPreferences(page, '/editor?deckId=4', { language: 'en', theme: 'light', textScale: 'normal' })
  await page.getByRole('radio', { name: /^Basic and reversed/ }).check()
  await page.getByRole('textbox', { name: fill(t.editor.front, { language: 'English' }) }).fill('good night')
  await page.getByRole('textbox', { name: t.editor.back }).fill('dobranoc')
  await expect(page.getByRole('heading', { name: fill(t.editor.preview.title_other, { count: 2 }) })).toBeVisible()
  await page.getByRole('button', { name: t.common.save, exact: true }).click()

  await expect(page.getByRole('heading', { level: 1, name: t.routes.card })).toBeVisible()
  await expect(page.getByRole('region', { name: 'Forward (English → Polish)' })).toBeVisible()
  await expect(page.getByRole('region', { name: 'Reverse (Polish → English)' })).toBeVisible()

  await page.getByRole('link', { name: t.card.editNote }).click()
  const back = page.getByRole('textbox', { name: t.editor.back })
  await expect(back).toHaveValue('dobranoc')
  await back.fill('dobranoc (wieczorem)')
  await page.getByRole('button', { name: t.common.save, exact: true }).click()
  await expect(page.getByRole('heading', { level: 1, name: t.routes.card })).toBeVisible()
  await expect(page.getByRole('region', { name: t.card.note, exact: true }).getByText('dobranoc (wieczorem)')).toBeVisible()

  await page.evaluate(() => (location.hash = '#/browser?deck=4'))
  await expect(page.getByText(fill(t.browser.results_other, { count: 10 }))).toBeVisible()
  await page.getByRole('button', { name: fill(t.browser.selectAll_other, { count: 10 }) }).click()
  await page.getByRole('button', { name: fill(t.browser.bulk.open_other, { count: 10 }) }).click()
  await page.getByRole('dialog').getByRole('button', { name: t.browser.bulk.actions.addTag }).click()
  await page.getByRole('combobox', { name: t.browser.bulk.tag }).fill('podróże')
  await page.getByRole('button', { name: t.browser.bulk.review }).click()
  const confirm = page.getByRole('dialog', { name: fill(t.browser.bulk.confirmTitle, { action: t.browser.bulk.actions.addTag }) })
  await expect(confirm.getByText(fill(t.browser.bulk.affected_other, { count: 10 }))).toBeVisible()
  await confirm.getByRole('button', { name: fill(t.browser.bulk.apply_other, { count: 10 }) }).click()
  await expect(page.getByText(fill(t.browser.bulk.done_other, { count: 10 }))).toBeVisible()
})

test('draft guard on browser Back, and deck options', async ({ page }) => {
  await openWithPreferences(page, '/editor?deckId=1', { language: 'en', theme: 'light', textScale: 'normal' })
  await page.getByRole('textbox', { name: fill(t.editor.front, { language: 'German' }) }).fill('das Fenster')
  await page.goBack()
  const guard = page.getByRole('dialog', { name: t.guard.title })
  await expect(guard).toBeVisible()
  await guard.getByRole('button', { name: t.guard.stay }).click()
  await expect(page).toHaveURL(/#\/editor/)
  await page.goBack()
  await page.getByRole('dialog', { name: t.guard.title }).getByRole('button', { name: t.guard.leave }).click()
  await expect(page.getByRole('heading', { level: 1, name: t.dashboard.title })).toBeVisible()

  await page.evaluate(() => (location.hash = '#/deck/1'))
  await page.getByRole('button', { name: t.deck.typeChange.open }).click()
  const type = page.getByRole('dialog', { name: t.deck.typeChange.title })
  await type.getByRole('radio', { name: /^Simple/ }).check()
  await type.getByRole('button', { name: t.deck.typeChange.apply }).click()
  await expect(page.getByText(t.deck.done.typeChanged)).toBeVisible()

  await page.getByRole('button', { name: t.deck.preset.open }).click()
  await page.getByRole('dialog', { name: t.deck.preset.pickTitle }).getByRole('radio', { name: /Lekki/ }).check()
  await page.getByRole('button', { name: t.deck.preset.use }).click()
  await expect(page.getByText(fill(t.deck.done.presetChanged, { name: 'Lekki (10 nowych/dzień)' }))).toBeVisible()
})

for (const language of ['pl', 'en'] as const) {
  const locale = LOCALES[language]
  const preferences: Preferences = { language, theme: 'light', textScale: 'huge' }

  test.describe(`management screens at 200% (${language}, 360 px)`, () => {
    test.use({ viewport: { width: 360, height: 800 } })

    test('details, editor, browser and deck have no layout problems or serious axe violations', async ({ page, browserName }) => {
      test.setTimeout(120_000)
      const check = async (scope?: string) => {
        await page.waitForLoadState('networkidle')
        expect(await layoutProblems(page, scope)).toEqual([])
        expect(await axeBlocking(page, browserName)).toEqual([])
      }

      await openWithPreferences(page, `/browser?q=${encodeURIComponent('"Ich trinke"')}`, preferences)
      await page.getByRole('list', { name: locale.browser.resultsLabel }).getByRole('link').first().click()
      await expect(page.getByRole('heading', { level: 1, name: locale.routes.card })).toBeVisible()
      await expect(page.getByRole('region', { name: locale.card.links.title })).toBeVisible()
      await check()

      await page.getByRole('link', { name: locale.card.editNote }).click()
      await expect(page.getByRole('heading', { level: 1, name: locale.editor.editNote })).toBeVisible()
      // The form, not its loading skeleton: the deck select and link list are what can overflow.
      await expect(page.getByRole('button', { name: locale.editor.links.linkSelection })).toBeVisible()
      await check()

      await page.evaluate(() => (location.hash = '#/browser?deck=4'))
      await page.getByRole('button', { name: fill(locale.disclosure.show, { section: locale.browser.filters.title }) }).click()
      await expect(page.getByRole('list', { name: locale.browser.resultsLabel })).toBeVisible()
      await check()

      await page.evaluate(() => (location.hash = '#/deck/1'))
      await expect(page.getByRole('button', { name: locale.deck.preset.open })).toBeVisible()
      await check()
      await page.getByRole('button', { name: locale.deck.preset.open }).click()
      const picker = page.getByRole('dialog', { name: locale.deck.preset.pickTitle })
      await picker.getByRole('button', { name: fill(locale.deck.preset.editNamed, { name: 'Domyślny (20 nowych/dzień)' }) }).click()
      await expect(page.getByRole('spinbutton', { name: locale.deck.preset.retention })).toBeVisible()
      await check('[role="dialog"]')
    })
  })
}

test('opening a card from the browser keeps Back working', async ({ page }) => {
  await openWithPreferences(page, '/home', { language: 'en', theme: 'light', textScale: 'normal' })
  await openCard(page, 'Kühlschrank')
  await expect(page.getByRole('region', { name: t.grammar.title })).toBeVisible()
  await page.goBack()
  await expect(page.getByRole('heading', { level: 1, name: t.routes.browser })).toBeVisible()
})
