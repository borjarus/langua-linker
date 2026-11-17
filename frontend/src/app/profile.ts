export type AppProfile = 'demo' | 'dev' | 'beta' | 'release'

const PROFILES: readonly AppProfile[] = ['demo', 'dev', 'beta', 'release']

/**
 * Build-time profile (Vite `--mode`). Demo uses the in-memory mock; every other profile talks to
 * Rust only. Selection happens once at the root — screens never inspect it.
 */
export const APP_PROFILE: AppProfile = (() => {
  const value = import.meta.env.VITE_APP_PROFILE ?? 'demo'
  if (!PROFILES.includes(value)) throw new Error(`Unknown VITE_APP_PROFILE "${value}"`)
  return value
})()

/** Separate local storage namespace per profile, so demo preferences never leak into production. */
export const storageKey = (profile: AppProfile, key: string): string => `ll:${profile}:${key}`

/** Accepts only an absolute https URL; anything else hides the link instead of opening it. */
export function policyUrl(value: string | undefined): string | null {
  if (!value) return null
  try {
    const url = new URL(value)
    return url.protocol === 'https:' && url.hostname !== '' ? url.href : null
  } catch {
    return null
  }
}

/** Published privacy policy, set per store build (`VITE_PRIVACY_POLICY_URL`, ADR-21). */
export const PRIVACY_POLICY_URL: string | null = policyUrl(import.meta.env.VITE_PRIVACY_POLICY_URL)
