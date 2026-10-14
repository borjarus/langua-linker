import type { ThemePreference } from '../api/types'

export type TextScale = 'normal' | 'large' | 'huge'
const TEXT_SCALE_PERCENT: Record<TextScale, number> = { normal: 100, large: 150, huge: 200 }

/**
 * Theme and text-scale controller. Applies `data-theme` and the root font size; components use
 * CSS variables only. Text scale is a development/accessibility aid; on devices the OS setting applies.
 */
export class ThemeController {
  preference = $state<ThemePreference>('system')
  systemDark = $state(false)
  textScale = $state<TextScale>('normal')
  readonly resolved = $derived<'light' | 'dark'>(
    this.preference === 'system' ? (this.systemDark ? 'dark' : 'light') : this.preference,
  )

  readonly #root: HTMLElement | null

  constructor(root: HTMLElement | null = typeof document === 'undefined' ? null : document.documentElement) {
    this.#root = root
  }

  /** Applies the stored preference before the first render, so dark mode does not flash light. */
  prime(): void {
    this.systemDark = this.#media()?.matches ?? false
    this.#apply()
  }

  start(): () => void {
    const media = this.#media()
    this.systemDark = media?.matches ?? false
    const onChange = (event: MediaQueryListEvent) => {
      this.systemDark = event.matches
      this.#apply()
    }
    media?.addEventListener('change', onChange)
    this.#apply()
    return () => media?.removeEventListener('change', onChange)
  }

  setPreference(preference: ThemePreference): void {
    this.preference = preference
    this.#apply()
  }

  setTextScale(scale: TextScale): void {
    this.textScale = scale
    this.#apply()
  }

  #media(): MediaQueryList | null {
    return typeof window !== 'undefined' && 'matchMedia' in window ? window.matchMedia('(prefers-color-scheme: dark)') : null
  }

  #apply(): void {
    if (!this.#root) return
    this.#root.dataset.theme = this.resolved
    this.#root.style.colorScheme = this.resolved
    this.#root.style.fontSize = this.textScale === 'normal' ? '' : `${TEXT_SCALE_PERCENT[this.textScale]}%`
  }
}
