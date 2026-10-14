import '@testing-library/jest-dom/vitest'
import { cleanup } from '@testing-library/svelte'
import { afterEach } from 'vitest'

// jsdom does not implement scrolling; the router restores scroll positions after navigation.
window.scrollTo = () => {}

// jsdom has no layout and no ResizeObserver; Shell measures the bottom nav with bind:offsetHeight.
globalThis.ResizeObserver ??= class {
  observe() {}
  unobserve() {}
  disconnect() {}
}

afterEach(() => cleanup())
