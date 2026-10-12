/// <reference types="vitest/config" />
import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'

// Set by `tauri android|ios dev` so a physical device can reach the dev server over the LAN.
const host = process.env.TAURI_DEV_HOST

export default defineConfig(({ mode }) => ({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: 'ws', host, port: 5183 } : undefined,
    watch: { ignored: ['**/src-tauri/**'] },
  },
  test: {
    environment: 'jsdom',
    include: ['tests/**/*.test.ts'],
    setupFiles: ['tests/setup.ts'],
  },
  // Component tests need Svelte's browser entry under jsdom.
  resolve: mode === 'test' ? { conditions: ['browser'] } : undefined,
}))
