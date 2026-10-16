import { getContext, setContext } from 'svelte'
import type { AppApi } from '../api/appApi'
import type { DemoControls } from '../demo/scenarios'
import type { I18n } from '../services/i18n.svelte'
import type { QueryClient } from '../services/queryClient'
import type { SpeechService } from '../services/speech'
import type { ThemeController } from '../services/theme.svelte'
import type { ToastController } from '../services/toasts.svelte'
import type { AppProfile } from './profile'
import type { Router } from './router.svelte'

/**
 * Everything a screen may use. Created per root (demo, production, each test), never as a
 * module-level singleton, so independent profiles cannot share mutable state.
 */
export interface AppServices {
  profile: AppProfile
  api: AppApi
  i18n: I18n
  router: Router
  queries: QueryClient
  theme: ThemeController
  toasts: ToastController
  /** Text-to-speech port (platform adapter, never simulated by the mock). */
  speech: SpeechService
  /** Present only in the demo profile; drives the banner and scenario switcher. */
  demo: DemoControls | null
}

export const APP_SERVICES = Symbol('langualinker.services')

export function provideServices(services: AppServices): AppServices {
  return setContext(APP_SERVICES, services)
}

export function useServices(): AppServices {
  const services = getContext<AppServices | undefined>(APP_SERVICES)
  if (!services) throw new Error('AppServices context is missing; render inside <App> or a test host.')
  return services
}
