import { ERROR_CODES, type ErrorCode, type ErrorDto } from './types'

const RETRYABLE_BY_DEFAULT: ReadonlySet<ErrorCode> = new Set([
  'DB_BUSY',
  'NETWORK_UNAVAILABLE',
  'PROVIDER_ERROR',
  'PROVIDER_TIMEOUT',
  'INVALID_PROVIDER_OUTPUT',
  'PREVIEW_STALE',
])

export function errorDto(code: ErrorCode, details: ErrorDto['details'] = null, retryable?: boolean): ErrorDto {
  return {
    code,
    messageKey: `errors.${code}`,
    retryable: retryable ?? RETRYABLE_BY_DEFAULT.has(code),
    details,
  }
}

/** The single rejection type of every `AppApi` call. */
export class AppError extends Error {
  readonly dto: ErrorDto

  constructor(dto: ErrorDto) {
    super(dto.code)
    this.name = 'AppError'
    this.dto = dto
  }

  static of(code: ErrorCode, details: ErrorDto['details'] = null, retryable?: boolean): AppError {
    return new AppError(errorDto(code, details, retryable))
  }
}

export function isErrorDto(value: unknown): value is ErrorDto {
  if (typeof value !== 'object' || value === null) return false
  const candidate = value as Record<string, unknown>
  return (
    typeof candidate.code === 'string' &&
    (ERROR_CODES as readonly string[]).includes(candidate.code) &&
    typeof candidate.messageKey === 'string' &&
    typeof candidate.retryable === 'boolean' &&
    (candidate.details === null || typeof candidate.details === 'object')
  )
}

/** Normalizes anything thrown by an adapter into an `ErrorDto`; never leaks raw messages. */
export function toErrorDto(error: unknown): ErrorDto {
  if (error instanceof AppError) return error.dto
  if (isErrorDto(error)) return error
  return errorDto('INTERNAL')
}
