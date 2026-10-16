import { DEFAULT_ROUTE, buildPath, matchPath, parseHash, type Route, type RouteTarget } from './routes'

export type NavigationKind = 'initial' | 'push' | 'replace' | 'pop'

/** Blocks leaving a screen with unsaved changes (draft guard). */
export interface NavigationGuard {
  /** Synchronous so it can also drive `beforeunload`. */
  isBlocking(): boolean
  /** Asks the user; resolve `true` to leave. */
  confirmLeave(to: Route): Promise<boolean>
  /**
   * Whether reloading/closing the page loses work (`beforeunload`). Default `true` (drafts).
   * `false` for guards over durable state, e.g. a study session whose ratings are already saved.
   */
  blocksUnload?: boolean
}

/** An open overlay (dialog, sheet) that the Back gesture/button closes before it navigates. */
export interface DismissibleLayer {
  dismiss(): void
}

interface HistoryState {
  llIndex?: number
}

/**
 * Hash router (ADR-01): works without a path-resolving server, inside Tauri WebViews and in the
 * browser demo. Programmatic navigation uses pushState/replaceState; anchors, back/forward and
 * manual edits arrive through `hashchange` and pass the same guards.
 */
export class Router {
  current = $state<Route>(matchPath(buildPath(DEFAULT_ROUTE)))
  kind = $state<NavigationKind>('initial')

  readonly #win: Window
  readonly #guards = new Set<NavigationGuard>()
  readonly #layers: DismissibleLayer[] = []
  readonly #scroll = new Map<string, number>()
  #hash = ''
  #index = 0

  constructor(win: Window = window) {
    this.#win = win
  }

  start(): () => void {
    const initial = parseHash(this.#win.location.hash)
    const route = initial ?? matchPath(buildPath(DEFAULT_ROUTE))
    this.#hash = `#${initial ? this.#win.location.hash.replace(/^#/, '') : buildPath(DEFAULT_ROUTE)}`
    const state = this.#win.history.state as HistoryState | null
    this.#index = typeof state?.llIndex === 'number' ? state.llIndex : 0
    this.#win.history.replaceState({ llIndex: this.#index } satisfies HistoryState, '', this.#hash)
    this.current = route
    this.kind = 'initial'

    const onHashChange = () => void this.#onExternalChange()
    const onBeforeUnload = (event: BeforeUnloadEvent) => {
      if ([...this.#guards].some((guard) => guard.blocksUnload !== false && guard.isBlocking())) event.preventDefault()
    }
    this.#win.addEventListener('hashchange', onHashChange)
    this.#win.addEventListener('beforeunload', onBeforeUnload)
    return () => {
      this.#win.removeEventListener('hashchange', onHashChange)
      this.#win.removeEventListener('beforeunload', onBeforeUnload)
    }
  }

  addGuard(guard: NavigationGuard): () => void {
    this.#guards.add(guard)
    return () => this.#guards.delete(guard)
  }

  /**
   * Registers an open overlay. A history-back move (Android Back, browser Back) while a layer is
   * open is undone and dismisses the topmost layer instead. Programmatic navigation is unaffected.
   */
  addLayer(layer: DismissibleLayer): () => void {
    this.#layers.push(layer)
    return () => {
      const index = this.#layers.lastIndexOf(layer)
      if (index >= 0) this.#layers.splice(index, 1)
    }
  }

  /** Resolves `false` when a guard kept the user on the current screen. */
  async navigate(target: RouteTarget | string, options: { replace?: boolean } = {}): Promise<boolean> {
    const path = typeof target === 'string' ? target : buildPath(target)
    const hash = `#${path}`
    if (hash === this.#hash) return true
    const to = matchPath(path)
    if (!(await this.#canLeave(to))) return false
    this.#rememberScroll()
    if (options.replace) {
      this.#win.history.replaceState({ llIndex: this.#index } satisfies HistoryState, '', hash)
      this.#commit(to, hash, 'replace')
    } else {
      this.#index += 1
      this.#win.history.pushState({ llIndex: this.#index } satisfies HistoryState, '', hash)
      this.#commit(to, hash, 'push')
    }
    return true
  }

  replace(target: RouteTarget | string): Promise<boolean> {
    return this.navigate(target, { replace: true })
  }

  /** Goes back in history, or to the default screen when this is the first entry. */
  back(): void {
    if (this.#index > 0) this.#win.history.back()
    else void this.replace(DEFAULT_ROUTE)
  }

  /** Called by the shell after the new screen rendered. */
  restoreScroll(): void {
    const top = this.kind === 'pop' ? (this.#scroll.get(this.#hash) ?? 0) : 0
    try {
      this.#win.scrollTo({ top, behavior: 'instant' as ScrollBehavior })
    } catch {
      // jsdom and some WebViews do not implement scrollTo options.
    }
  }

  async #onExternalChange(): Promise<void> {
    const hash = this.#win.location.hash || `#${buildPath(DEFAULT_ROUTE)}`
    if (hash === this.#hash) return
    const to = parseHash(hash) ?? matchPath(buildPath(DEFAULT_ROUTE))
    const state = this.#win.history.state as HistoryState | null
    const targetIndex = typeof state?.llIndex === 'number' ? state.llIndex : null

    const layer = this.#layers.at(-1)
    if (layer && targetIndex !== null && targetIndex < this.#index) {
      this.#undo(targetIndex)
      layer.dismiss()
      return
    }
    if (!(await this.#canLeave(to))) {
      this.#undo(targetIndex)
      return
    }
    this.#rememberScroll()
    if (targetIndex === null) {
      this.#index += 1
      this.#win.history.replaceState({ llIndex: this.#index } satisfies HistoryState, '', hash)
      this.#commit(to, hash, 'push')
    } else {
      this.#index = targetIndex
      this.#commit(to, hash, 'pop')
    }
  }

  /** Undoes a browser-driven move; the resulting hashchange matches #hash and is ignored. */
  #undo(targetIndex: number | null): void {
    if (targetIndex !== null) this.#win.history.go(this.#index - targetIndex)
    else this.#win.history.back()
  }

  async #canLeave(to: Route): Promise<boolean> {
    for (const guard of this.#guards) {
      if (guard.isBlocking() && !(await guard.confirmLeave(to))) return false
    }
    return true
  }

  #rememberScroll(): void {
    this.#scroll.set(this.#hash, this.#win.scrollY)
  }

  #commit(route: Route, hash: string, kind: NavigationKind): void {
    this.#hash = hash
    this.kind = kind
    this.current = route
  }
}
