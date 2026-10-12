import type { AppApiOperation } from '../api/appApi'
import type { ErrorCode } from '../api/types'

export const DEMO_SCENARIOS = [
  'happy',
  'newUser',
  'empty',
  'loading',
  'error',
  'offline',
  'quota',
  'ambiguous',
  'large',
] as const

export type DemoScenario = (typeof DEMO_SCENARIOS)[number]

export function isDemoScenario(value: unknown): value is DemoScenario {
  return typeof value === 'string' && (DEMO_SCENARIOS as readonly string[]).includes(value)
}

/** Demo-only controls; never part of `AppApi`, so screens cannot depend on them. */
export interface DemoControls {
  readonly scenario: DemoScenario
  readonly scenarios: readonly DemoScenario[]
  /** Resets demo memory to the scenario and remembers the choice for the next start. */
  setScenario(scenario: DemoScenario): void
  failNext(operation: AppApiOperation, code: ErrorCode): void
}
