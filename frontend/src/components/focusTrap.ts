const FOCUSABLE = [
  'a[href]',
  'button:not([disabled])',
  'input:not([disabled]):not([type="hidden"])',
  'select:not([disabled])',
  'textarea:not([disabled])',
  '[tabindex]:not([tabindex="-1"])',
].join(',')

export function focusableWithin(node: HTMLElement): HTMLElement[] {
  return [...node.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((element) => !element.hasAttribute('inert'))
}

/**
 * Svelte attachment: moves focus into `node`, keeps Tab/Shift+Tab inside, calls `onescape`
 * on Escape and restores focus to the previously focused element on teardown.
 */
export function focusTrap(onescape: () => void) {
  return (node: HTMLElement) => {
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null
    const initial = node.querySelector<HTMLElement>('[data-autofocus]') ?? focusableWithin(node)[0] ?? node
    initial.focus()

    const onKeydown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.stopPropagation()
        onescape()
        return
      }
      if (event.key !== 'Tab') return
      const items = focusableWithin(node)
      const first = items[0]
      const last = items[items.length - 1]
      if (!first || !last) {
        event.preventDefault()
        node.focus()
      } else if (event.shiftKey && document.activeElement === first) {
        event.preventDefault()
        last.focus()
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault()
        first.focus()
      }
    }

    node.addEventListener('keydown', onKeydown)
    return () => {
      node.removeEventListener('keydown', onKeydown)
      if (previous?.isConnected) previous.focus()
    }
  }
}
