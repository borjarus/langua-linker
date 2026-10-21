import type { AppApi } from '../../api/appApi'
import { toErrorDto } from '../../api/errors'
import type { ErrorDto, SettingsDto, SettingsPatch } from '../../api/types'
import { newOperationId } from '../../services/operationId'

export interface FieldError {
  field: string
  error: ErrorDto
}

/**
 * Settings form state shared by the settings screens. Each change is saved by an explicit handler
 * (never by an `$effect`); changes are queued so every save carries the version returned by the
 * previous one, and a conflict reloads the latest settings instead of overwriting them.
 */
export class SettingsController {
  settings = $state<SettingsDto | null>(null)
  loadError = $state<ErrorDto | null>(null)
  /** Field whose change is being saved, for a busy indicator next to it. */
  saving = $state<string | null>(null)
  saved = $state<string | null>(null)
  error = $state<FieldError | null>(null)

  readonly #api: AppApi
  #queue: Promise<unknown> = Promise.resolve()

  constructor(api: AppApi) {
    this.#api = api
  }

  async load(): Promise<void> {
    this.loadError = null
    try {
      this.settings = await this.#api.getSettings()
    } catch (caught) {
      this.loadError = toErrorDto(caught)
    }
  }

  /** Saves one field's change; resolves `true` once stored. */
  update(field: string, patch: SettingsPatch): Promise<boolean> {
    const run = async (): Promise<boolean> => {
      const current = this.settings
      if (!current) return false
      this.saving = field
      this.saved = null
      if (this.error?.field === field) this.error = null
      try {
        this.settings = await this.#api.updateSettings({ operationId: newOperationId(), expectedVersion: current.version, patch })
        this.saved = field
        return true
      } catch (caught) {
        const error = toErrorDto(caught)
        this.error = { field, error }
        if (error.code === 'VERSION_CONFLICT') await this.load()
        return false
      } finally {
        if (this.saving === field) this.saving = null
      }
    }
    const result = this.#queue.then(run, run)
    this.#queue = result
    return result
  }

  /** Applies a result returned by a dedicated operation (API key, notification permission). */
  replace(settings: SettingsDto): void {
    this.settings = settings
  }

  errorFor(field: string): ErrorDto | null {
    return this.error?.field === field ? this.error.error : null
  }
}
