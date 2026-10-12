import { MockAppApi } from './mockAppApi'
import { DEMO_SCENARIOS, isDemoScenario, type DemoControls, type DemoScenario } from './scenarios'

/**
 * Marker string checked by `scripts/verify-release-bundle.mjs`: it must never appear in a release bundle.
 * It is written to `<html data-demo>` so the demo build always contains it.
 */
export const DEMO_BUILD_MARKER = 'LL_DEMO_ADAPTER_INCLUDED'

const SCENARIO_KEY = 'll:demo:scenario'

function readStoredScenario(): DemoScenario {
  try {
    const stored = globalThis.localStorage?.getItem(SCENARIO_KEY)
    return isDemoScenario(stored) ? stored : 'happy'
  } catch {
    return 'happy'
  }
}

function storeScenario(scenario: DemoScenario): void {
  try {
    globalThis.localStorage?.setItem(SCENARIO_KEY, scenario)
  } catch {
    // Storage may be unavailable (private mode); the scenario then lasts for this page only.
  }
}

export function createDemoApi(): { api: MockAppApi; controls: DemoControls; marker: string } {
  const api = new MockAppApi({ scenario: readStoredScenario(), latencyMs: 180, jobStepMs: 350 })
  const controls: DemoControls = {
    get scenario() {
      return api.scenario
    },
    scenarios: DEMO_SCENARIOS,
    setScenario(scenario) {
      api.setScenario(scenario)
      storeScenario(scenario)
    },
    failNext: (operation, code) => api.failNext(operation, code),
  }
  return { api, controls, marker: DEMO_BUILD_MARKER }
}
