import type { Page } from '@playwright/test'

export interface Preferences {
  language: 'pl' | 'en'
  theme: 'light' | 'dark'
  textScale: 'normal' | 'large' | 'huge'
}

/**
 * Seeds the demo profile's local preferences, then opens `path` from the dashboard, as on a
 * device, so history Back has an entry to return to.
 */
export async function openWithPreferences(page: Page, path: string, preferences: Preferences): Promise<void> {
  await page.addInitScript((prefs) => {
    localStorage.setItem('ll:demo:language', prefs.language)
    localStorage.setItem('ll:demo:theme', prefs.theme)
    localStorage.setItem('ll:demo:textScale', prefs.textScale)
  }, preferences)
  await page.goto('/#/home')
  await page.evaluate((hash) => (location.hash = hash), `#${path}`)
}

export interface LayoutProblem {
  kind: 'page-scroll' | 'outside-viewport' | 'clipped-content' | 'overlap'
  detail: string
}

/**
 * Diff rule 4: no clipped control, no overlapping controls, no horizontal page scroll.
 * `scope` (e.g. `[role="dialog"]`) checks only the controls of an open modal: the page behind it
 * is inert, so its controls overlapping the dialog is expected.
 */
export async function layoutProblems(page: Page, scope?: string): Promise<LayoutProblem[]> {
  return page.evaluate((scope) => {
    const problems: { kind: 'page-scroll' | 'outside-viewport' | 'clipped-content' | 'overlap'; detail: string }[] = []
    const root = document.documentElement
    const width = root.clientWidth
    if (root.scrollWidth > width) problems.push({ kind: 'page-scroll', detail: `scrollWidth ${root.scrollWidth} > ${width}` })

    const describe = (el: Element) => {
      const name = el.getAttribute('aria-label') ?? el.textContent?.trim().slice(0, 40) ?? ''
      return `${el.tagName.toLowerCase()}${el.className ? `.${String(el.className).split(' ').join('.')}` : ''} "${name}"`
    }
    const visible = (el: Element) => {
      const rect = el.getBoundingClientRect()
      return rect.width > 0 && rect.height > 0 && getComputedStyle(el).visibility !== 'hidden'
    }
    const CONTROLS = 'button, a[href], input, select, textarea, [role="button"]'
    // The virtual list scrolls its own content (allowed by rule 4); its rows are not page controls.
    const within = (root: string) => [...document.querySelectorAll(`${root} ${CONTROLS.split(', ').join(`, ${root} `)}`)]
    const groups = (
      scope
        ? [within(scope)]
        : [
            within('main').filter((el) => !el.closest('[role="list"][aria-label]')),
            [...document.querySelectorAll('nav a[href]')],
            within('.banner'),
          ]
    ).map((group) => group.filter(visible))

    for (const group of groups) {
      const rects = group.map((el) => {
        const rect = el.getBoundingClientRect()
        return { el, left: rect.left + scrollX, right: rect.right + scrollX, top: rect.top + scrollY, bottom: rect.bottom + scrollY }
      })
      for (const { el, left, right } of rects) {
        if (left < -0.5 || right > width + 0.5) problems.push({ kind: 'outside-viewport', detail: `${describe(el)} ${left.toFixed(1)}–${right.toFixed(1)}` })
        const html = el as HTMLElement
        if (!(el instanceof HTMLInputElement) && !(el instanceof HTMLSelectElement) && !(el instanceof HTMLTextAreaElement) && html.scrollWidth > html.clientWidth + 1) {
          problems.push({ kind: 'clipped-content', detail: `${describe(el)} scrollWidth ${html.scrollWidth} > ${html.clientWidth}` })
        }
      }
      for (let i = 0; i < rects.length; i += 1) {
        for (let j = i + 1; j < rects.length; j += 1) {
          const a = rects[i]!
          const b = rects[j]!
          if (a.el.contains(b.el) || b.el.contains(a.el)) continue
          const overlapX = Math.min(a.right, b.right) - Math.max(a.left, b.left)
          const overlapY = Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top)
          if (overlapX > 1 && overlapY > 1) problems.push({ kind: 'overlap', detail: `${describe(a.el)} × ${describe(b.el)}` })
        }
      }
    }
    return problems
  }, scope)
}
