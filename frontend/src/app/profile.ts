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
