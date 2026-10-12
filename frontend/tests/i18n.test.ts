import { screen } from '@testing-library/svelte'
import { flushSync } from 'svelte'
import { describe, expect, it } from 'vitest'
import { ERROR_CODES } from '../src/api/types'
import { en } from '../src/locales/en'
import { pl } from '../src/locales/pl'
import { PLURAL_SUFFIXES, type LocaleResource } from '../src/locales/types'
import { I18n } from '../src/services/i18n.svelte'
import LanguageProbe from './fixtures/LanguageProbe.svelte'
import { createTestServices, renderWith } from './helpers/services'

function flatten(resource: LocaleResource, prefix = ''): Map<string, string> {
  const entries = new Map<string, string>()
  for (const [key, value] of Object.entries(resource)) {
    if (typeof value === 'string') entries.set(`${prefix}${key}`, value)
    else for (const [nested, text] of flatten(value, `${prefix}${key}.`)) entries.set(nested, text)
  }
  return entries
}

const PLURAL = new RegExp(`_(${PLURAL_SUFFIXES.join('|')})$`)

/** Base keys with the plural categories each locale defines for them. */
function shape(resource: LocaleResource) {
  const plain = new Set<string>()
  const plurals = new Map<string, Set<string>>()
  for (const key of flatten(resource).keys()) {
    const match = PLURAL.exec(key)
    if (!match) plain.add(key)
    else {
      const base = key.slice(0, match.index)
      plurals.set(base, (plurals.get(base) ?? new Set()).add(match[1] ?? ''))
    }
  }
  return { plain, plurals }
}

describe('locale resources', () => {
  const english = shape(en as unknown as LocaleResource)
  const polish = shape(pl)

  it('have identical non-plural keys in PL and EN', () => {
    expect([...polish.plain].sort()).toEqual([...english.plain].sort())
  })

  it('have the same plural base keys', () => {
    expect([...polish.plurals.keys()].sort()).toEqual([...english.plurals.keys()].sort())
  })

  it.each([
    ['pl', polish],
    ['en', english],
  ] as const)('define every CLDR plural category required by %s', (language, locale) => {
    const required = new Intl.PluralRules(language).resolvedOptions().pluralCategories
    for (const [base, categories] of locale.plurals) {
      for (const category of required) expect(categories.has(category), `${language}:${base}_${category}`).toBe(true)
    }
  })

  it('translate every error code', () => {
    for (const code of ERROR_CODES) {
      expect(flatten(en as unknown as LocaleResource).has(`errors.${code}`), code).toBe(true)
      expect(flatten(pl).has(`errors.${code}`), code).toBe(true)
    }
  })

  it('keep interpolation placeholders aligned between languages', () => {
    const placeholders = (text: string) => [...text.matchAll(/{{(\w+)(?:,[^}]*)?}}/g)].map((match) => match[1]).sort()
    const english = flatten(en as unknown as LocaleResource)
    for (const [key, text] of flatten(pl)) {
      const base = key.replace(PLURAL, '')
      const counterpart = english.get(key) ?? english.get(`${base}_other`)
      if (counterpart) expect(placeholders(text), key).toEqual(placeholders(counterpart))
    }
  })
})

describe('I18n service', () => {
  it('selects Polish plural forms', async () => {
    const i18n = await I18n.create('pl')
    expect(i18n.t('deck.cards', { count: 1 })).toBe('1 karta')
    expect(i18n.t('deck.cards', { count: 3 })).toBe('3 karty')
    expect(i18n.t('deck.cards', { count: 5 })).toBe('5 kart')
    expect(i18n.t('deck.cards', { count: 22 })).toBe('22 karty')
    expect(i18n.t('deck.cards', { count: 25 })).toBe('25 kart')
    expect(i18n.t('deck.cards', { count: 1.5 })).toBe('1,5 karty')
  })

  it('does not HTML-escape interpolated values (Svelte escapes on render)', async () => {
    const i18n = await I18n.create('pl')
    expect(i18n.t('deck.open', { name: 'Tom & Jerry „B”' })).toBe('Otwórz Tom & Jerry „B”')
  })

  it('formats numbers, percentages and intervals per language', async () => {
    const i18n = await I18n.create('pl')
    expect(i18n.formatNumber(50000).replace(/\s/gu, ' ')).toBe('50 000')
    expect(i18n.formatInterval(600)).toBe('10 minut')
    expect(i18n.formatInterval(4 * 86_400)).toBe('4 dni')
    await i18n.setLanguage('en')
    expect(i18n.formatNumber(50000)).toBe('50,000')
    expect(i18n.formatInterval(86_400)).toBe('1 day')
  })

  it('falls back safely for unknown dynamic keys', async () => {
    const i18n = await I18n.create('en')
    expect(i18n.tDynamic('errors.DOES_NOT_EXIST', 'errors.INTERNAL')).toBe(en.errors.INTERNAL)
  })

  it('re-renders components immediately when the language changes', async () => {
    const services = await createTestServices({ language: 'en' })
    renderWith(LanguageProbe, services)
    expect(screen.getByText('Your decks')).toBeInTheDocument()
    await services.i18n.setLanguage('pl')
    flushSync()
    expect(screen.getByText('Twoje talie')).toBeInTheDocument()
    expect(document.documentElement.lang).toBe('pl')
  })
})
