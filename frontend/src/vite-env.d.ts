/// <reference types="svelte" />
/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_APP_PROFILE?: 'demo' | 'dev' | 'beta' | 'release'
}

interface ImportMeta {
  readonly env: ImportMetaEnv
}
