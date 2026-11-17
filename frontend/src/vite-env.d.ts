/// <reference types="svelte" />
/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_APP_PROFILE?: 'demo' | 'dev' | 'beta' | 'release'
  /** Published privacy policy (store builds; scripts/release-check.mjs requires it for a tag). */
  readonly VITE_PRIVACY_POLICY_URL?: string
}

interface ImportMeta {
  readonly env: ImportMetaEnv
}
