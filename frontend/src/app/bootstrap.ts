import type { AppApi } from '../api/appApi'
import { createPlatformBridge, type PlatformBridge } from '../api/platform'
import { createTauriAppApi } from '../api/tauriAppApi'
import type { ThemePreference, UiLanguage } from '../api/types'
import type { DemoControls } from '../demo/scenarios'
import { I18n, UI_LANGUAGES } from '../services/i18n.svelte'
import { newOperationId } from '../services/operationId'
import { QueryClient } from '../services/queryClient'
import { createNativeSpeechService, createWebSpeechService } from '../services/speech'
import { ThemeController, type TextScale } from '../services/theme.svelte'
import { ToastController } from '../services/toasts.svelte'
import type { AppServices } from './context'
import { watchLaunchLinks } from './launchLink'
import { APP_PROFILE, storageKey, type AppProfile } from './profile'
import { Router } from './router.svelte'

/** Statically false in beta/release builds, so the demo chunk is tree-shaken out of the bundle. */
const DEMO_ADAPTER_AVAILABLE = import.meta.env.VITE_APP_PROFILE !== 'release' && import.meta.env.VITE_APP_PROFILE !== 'beta'

export interface LocalPreferences {
  language: UiLanguage
  theme: ThemePreference
  textScale: TextScale
}

function readPreferences(profile: AppProfile): LocalPreferences {
  const read = (key: string): string | null => {
    try {
      return localStorage.getItem(storageKey(profile, key))
    } catch {
      return null
    }
  }
  const language = read('language')
  const theme = read('theme')
  const textScale = read('textScale')
  const browserLanguage: UiLanguage = typeof navigator !== 'undefined' && navigator.language.toLowerCase().startsWith('en') ? 'en' : 'pl'
  return {
    language: UI_LANGUAGES.includes(language as UiLanguage) ? (language as UiLanguage) : browserLanguage,
    theme: theme === 'light' || theme === 'dark' || theme === 'system' ? theme : 'system',
    textScale: textScale === 'large' || textScale === 'huge' || textScale === 'normal' ? textScale : 'normal',
  }
}

export function savePreference(profile: AppProfile, key: keyof LocalPreferences, value: string): void {
  try {
    localStorage.setItem(storageKey(profile, key), value)
  } catch {
    // Preferences are a convenience; failure to persist must not break the UI.
  }
}

/** Stores the shown UI language in the settings when they differ; a failure only logs. */
export async function syncUiLanguage(services: Pick<AppServices, 'api' | 'i18n'>): Promise<boolean> {
  try {
    const settings = await services.api.getSettings()
    if (settings.uiLanguage === services.i18n.language) return false
    await services.api.updateSettings({ operationId: newOperationId(), expectedVersion: settings.version, patch: { uiLanguage: services.i18n.language } })
    return true
  } catch {
    return false
  }
}

/** Builds one isolated service graph for the root component. */
export async function createServices(profile: AppProfile = APP_PROFILE): Promise<{ services: AppServices; start: () => () => void }> {
  let api: AppApi
  let demo: DemoControls | null = null
  // Native adapters exist only next to the real backend; the browser demo never simulates them.
  let platform: PlatformBridge | null = null
  if (DEMO_ADAPTER_AVAILABLE && profile === 'demo') {
    const { createDemoApi } = await import('../demo/index')
    const created = createDemoApi()
    api = created.api
    demo = created.controls
    document.documentElement.dataset.demo = created.marker
  } else {
    api = createTauriAppApi()
    platform = createPlatformBridge()
  }

  const preferences = readPreferences(profile)
  const i18n = await I18n.create(preferences.language)
  document.documentElement.lang = preferences.language
  const theme = new ThemeController()
  theme.preference = preferences.theme
  theme.textScale = preferences.textScale
  theme.prime()
  const services: AppServices = {
    profile,
    api,
    i18n,
    router: new Router(window),
    queries: new QueryClient(),
    theme,
    toasts: new ToastController(),
    speech: platform ? createNativeSpeechService(platform, createWebSpeechService()) : createWebSpeechService(),
    demo,
  }

  const start = () => {
    const stopRouter = services.router.start()
    const stopTheme = services.theme.start()
    const stopCollection = services.api.onCollectionChanged(() => services.queries.invalidate())
    const stopLinks = platform ? watchLaunchLinks(platform, services.router) : () => {}
    // The shown language starts from the device language; native text (reminders) follows the
    // stored one, so the first start stores what the learner actually sees (Phase 15).
    if (platform) void syncUiLanguage(services)
    // Speech never continues in the background (plan Phase 15 step 9). Back in the foreground the
    // settings are read again: the notification permission may have changed in the system settings.
    const onHidden = () => {
      if (document.visibilityState === 'hidden') services.speech.stop()
      else services.queries.invalidate(['settings'])
    }
    document.addEventListener('visibilitychange', onHidden)
    return () => {
      document.removeEventListener('visibilitychange', onHidden)
      stopLinks()
      stopCollection()
      stopTheme()
      stopRouter()
      services.toasts.dispose()
    }
  }
  return { services, start }
}
