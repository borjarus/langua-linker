import '@testing-library/jest-dom/vitest'
import { cleanup } from '@testing-library/svelte'
import { afterEach } from 'vitest'

// jsdom does not implement scrolling; the router restores scroll positions after navigation.
window.scrollTo = () => {}

afterEach(() => cleanup())
