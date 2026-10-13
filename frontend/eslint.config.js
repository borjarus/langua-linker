import js from '@eslint/js'
import svelte from 'eslint-plugin-svelte'
import globals from 'globals'
import ts from 'typescript-eslint'
import svelteConfig from './svelte.config.js'

export default ts.config(
  { ignores: ['dist/', 'dist-release/', 'dist-beta/', 'node_modules/'] },
  js.configs.recommended,
  ...ts.configs.recommended,
  ...svelte.configs.recommended,
  {
    languageOptions: {
      globals: { ...globals.browser },
    },
  },
  {
    files: ['**/*.svelte', '**/*.svelte.ts', '**/*.svelte.js'],
    languageOptions: {
      parserOptions: {
        projectService: true,
        extraFileExtensions: ['.svelte'],
        parser: ts.parser,
        svelteConfig,
      },
    },
  },
  {
    rules: {
      // Non-reactive bookkeeping (timers, guards, caches) intentionally uses plain collections;
      // state that renders is always declared with $state.
      'svelte/prefer-svelte-reactivity': 'off',
      '@typescript-eslint/no-unused-vars': ['error', { argsIgnorePattern: '^_', varsIgnorePattern: '^_' }],
      // Screens and components reach the backend only through AppApi from context.
      'no-restricted-imports': [
        'error',
        {
          patterns: [
            {
              group: ['@tauri-apps/*'],
              message: 'Use AppApi from context; only src/api/tauriAppApi.ts (and the dev-only src/api/nativeDiagnostics.ts) may import Tauri.',
            },
            { group: ['react', 'react-dom', 'react-router*', 'react-i18next'], message: 'The frontend is Svelte 5; React is not a dependency.' },
          ],
        },
      ],
    },
  },
  {
    files: ['src/api/tauriAppApi.ts', 'src/api/nativeDiagnostics.ts'],
    rules: { 'no-restricted-imports': 'off' },
  },
  {
    files: ['scripts/**/*.mjs'],
    languageOptions: { globals: { ...globals.node } },
  },
)
