export type ToastTone = 'info' | 'success' | 'error'

export interface ToastItem {
  id: number
  message: string
  tone: ToastTone
}

/** Transient notifications. Messages are already translated by the caller. */
export class ToastController {
  items = $state<ToastItem[]>([])
  readonly #timers = new Map<number, ReturnType<typeof setTimeout>>()
  #nextId = 1

  show(message: string, tone: ToastTone = 'info', timeoutMs = 5000): number {
    const id = this.#nextId++
    this.items = [...this.items, { id, message, tone }]
    if (timeoutMs > 0) this.#timers.set(id, setTimeout(() => this.dismiss(id), timeoutMs))
    return id
  }

  dismiss(id: number): void {
    const timer = this.#timers.get(id)
    if (timer) clearTimeout(timer)
    this.#timers.delete(id)
    this.items = this.items.filter((item) => item.id !== id)
  }

  dispose(): void {
    for (const timer of this.#timers.values()) clearTimeout(timer)
    this.#timers.clear()
    this.items = []
  }
}
