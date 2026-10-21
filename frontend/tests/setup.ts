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

// VirtualList (TanStack Virtual) sizes its window from the scroll container's offsetHeight, which jsdom reports as 0.
const offsetHeight = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'offsetHeight')
Object.defineProperty(HTMLElement.prototype, 'offsetHeight', {
  configurable: true,
  get(this: HTMLElement) {
    return this.matches('[role="list"][tabindex]') ? 480 : (offsetHeight?.get?.call(this) ?? 0)
  },
})

// TanStack Virtual scrolls with element.scrollTo; mirror it onto scrollTop and fire the scroll event it listens for.
Element.prototype.scrollTo = function (this: Element, options?: ScrollToOptions | number) {
  if (typeof options === 'object' && options.top !== undefined) this.scrollTop = options.top
  this.dispatchEvent(new Event('scroll'))
} as Element['scrollTo']

afterEach(() => cleanup())
