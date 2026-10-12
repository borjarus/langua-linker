import i18next, { type i18n as I18nInstance } from 'i18next'
import type { UiLanguage } from '../api/types'
import { en } from '../locales/en'
import { pl } from '../locales/pl'
import type { TranslationKey } from '../locales/types'

export type TranslationValues = Record<string, string | number>

export const UI_LANGUAGES: readonly UiLanguage[] = ['pl', 'en']

/**
 * i18next core with a runes adapter: `t()` reads `language`, so any template or `$derived`
 * calling it re-renders when the language changes. No react-i18next, no global singleton.
 */
export class I18n {
  language = $state<UiLanguage>('pl')
  readonly missingKeys = new Set<string>()
  readonly #instance: I18nInstance

  private constructor(instance: I18nInstance, language: UiLanguage) {
    this.#instance = instance
    this.language = language
  }

  static async create(language: UiLanguage): Promise<I18n> {
    const instance = i18next.createInstance()
    const i18n = new I18n(instance, language)
    await instance.init({
      resources: { en: { translation: en }, pl: { translation: pl } },
      lng: language,
      fallbackLng: 'en',
      supportedLngs: [...UI_LANGUAGES],
      // Svelte escapes interpolated text; double escaping would corrupt „quotes” and &.
      interpolation: { escapeValue: false },
      returnNull: false,
      saveMissing: true,
      missingKeyHandler: (_lngs, _ns, key) => {
        i18n.missingKeys.add(key)
      },
      initAsync: false,
    })
    return i18n
  }

  t = (key: TranslationKey, values?: TranslationValues): string => {
    return this.#instance.t(key, { lng: this.language, ...values })
  }

  /** For keys that arrive as data (e.g. `ErrorDto.messageKey`); unknown keys fall back safely. */
  tDynamic = (key: string, fallback: TranslationKey, values?: TranslationValues): string => {
    return this.#instance.exists(key, { lng: this.language }) ? this.#instance.t(key, { lng: this.language, ...values }) : this.t(fallback, values)
  }

  async setLanguage(language: UiLanguage): Promise<void> {
    await this.#instance.changeLanguage(language)
    this.language = language
    if (typeof document !== 'undefined') document.documentElement.lang = language
  }

  formatNumber = (value: number, options?: Intl.NumberFormatOptions): string =>
    new Intl.NumberFormat(this.language, options).format(value)

  formatPercent = (value: number): string =>
    new Intl.NumberFormat(this.language, { style: 'percent', maximumFractionDigits: 0 }).format(value)

  formatDate = (iso: string, options: Intl.DateTimeFormatOptions = { dateStyle: 'medium' }): string =>
    new Intl.DateTimeFormat(this.language, options).format(new Date(iso))

  /** Human interval for rating previews: "10 minutes", "4 dni". */
  formatInterval = (seconds: number): string => {
    if (seconds < 60) return this.t('interval.seconds', { count: Math.max(1, Math.round(seconds)) })
    if (seconds < 3_600) return this.t('interval.minutes', { count: Math.round(seconds / 60) })
    if (seconds < 86_400) return this.t('interval.hours', { count: Math.round(seconds / 3_600) })
    if (seconds < 30 * 86_400) return this.t('interval.days', { count: Math.round(seconds / 86_400) })
    return this.t('interval.months', { count: Math.round(seconds / (30 * 86_400)) })
  }
}
