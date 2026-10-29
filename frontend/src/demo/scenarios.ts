import type { AppApiOperation } from '../api/appApi'
import type { ErrorCode, FilePurpose } from '../api/types'

export const DEMO_SCENARIOS = [
  'happy',
  'newUser',
  'empty',
  'noCards',
  'caughtUp',
  'preparing',
  'loading',
  'error',
  'offline',
  'quota',
  'aiErrors',
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
  /** The next mutation of any kind fails once with `code` — save-error flows in the demo. */
  failNextSave(code: ErrorCode): void
  /** Files the demo picker offers per purpose (the mock never opens a real picker). */
  readonly demoFiles: Readonly<Record<FilePurpose, readonly { token: string; name: string }[]>>
  pickedFile(purpose: FilePurpose): string | null
  /** `null` simulates the user closing the picker. */
  setPickedFile(purpose: FilePurpose, token: string | null): void
}
