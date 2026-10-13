import { invoke, isTauri } from '@tauri-apps/api/core'
import type { InvokeFn } from './tauriAppApi'

/**
 * Phase 1 native spikes (ADR-07, ADR-12). Not part of `AppApi`: the commands exist only for the
 * dev variant's diagnostics screen and are denied by capabilities in beta/release.
 * Mirrors `SecretStoreProbeDto` in `crates/contracts`.
 */
export interface ProbeStep {
  name: string
  ok: boolean
  error: string | null
}

export interface SecretStoreProbe {
  backend: string
  persistedFromPreviousRun: boolean
  steps: ProbeStep[]
}

/** True inside the Tauri WebView; the browser demo has no native backend to probe. */
export const nativeDiagnosticsAvailable = (): boolean => isTauri()

export async function probeSecretStore(invokeFn: InvokeFn = invoke): Promise<SecretStoreProbe> {
  return (await invokeFn('probe_secret_store')) as SecretStoreProbe
}
