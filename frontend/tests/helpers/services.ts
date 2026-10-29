import { render } from '@testing-library/svelte'
import type { Component } from 'svelte'
import type { AppApi } from '../../src/api/appApi'
import type { UiLanguage } from '../../src/api/types'
import { APP_SERVICES, type AppServices } from '../../src/app/context'
import { Router } from '../../src/app/router.svelte'
import { DEMO_FILES } from '../../src/demo/fixtures'
import { MockAppApi, type MockAppApiOptions } from '../../src/demo/mockAppApi'
import { DEMO_SCENARIOS } from '../../src/demo/scenarios'
import { I18n } from '../../src/services/i18n.svelte'
import { QueryClient } from '../../src/services/queryClient'
import type { SpeechService } from '../../src/services/speech'
import { ThemeController } from '../../src/services/theme.svelte'
import { ToastController } from '../../src/services/toasts.svelte'

export interface TestServicesOptions extends MockAppApiOptions {
  language?: UiLanguage
  api?: AppApi
  speech?: SpeechService
}

/** jsdom has no speech engine; tests that need voices pass their own fake. */
export const unsupportedSpeech: SpeechService = {
  availability: () => 'unsupported',
  speak: () => {},
  stop: () => {},
  onChange: () => () => {},
}

/** A fresh, isolated service graph per test — the same shape the real root builds. */
export async function createTestServices(options: TestServicesOptions = {}): Promise<AppServices & { mock: MockAppApi }> {
  const mock = new MockAppApi(options)
  const api = options.api ?? mock
  return {
    profile: 'demo',
    api,
    mock,
    i18n: await I18n.create(options.language ?? 'en'),
    router: new Router(window),
    queries: new QueryClient(),
    theme: new ThemeController(document.documentElement),
    toasts: new ToastController(),
    speech: options.speech ?? unsupportedSpeech,
    demo: {
      get scenario() {
        return mock.scenario
      },
      scenarios: DEMO_SCENARIOS,
      setScenario: (scenario) => mock.setScenario(scenario),
      failNext: (operation, code) => mock.failNext(operation, code),
      failNextSave: (code) => mock.failNextSave(code),
      demoFiles: DEMO_FILES,
      pickedFile: (purpose) => mock.pickedFile(purpose),
      setPickedFile: (purpose, token) => mock.setPickedFile(purpose, token),
    },
  }
}

export function renderWith<Props extends object>(component: Component<Props>, services: AppServices, props?: NoInfer<Props>) {
  const options = { props: props ?? ({} as Props), context: new Map<unknown, unknown>([[APP_SERVICES, services]]) }
  // testing-library's option typing does not model Svelte 5 context maps; the shape is correct at runtime.
  return render(component, options as never)
}
