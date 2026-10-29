import { expect } from 'vitest'

const ISO_UTC = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/
const STUDY_DAY = /^\d{4}-\d{2}-\d{2}$/

/** Walks a DTO and enforces transport rules from plan §4.1 (mirrored in `crates/contracts/tests`). */
export function assertTransportRules(value: unknown, path = '$'): void {
  if (value === undefined) throw new Error(`${path}: undefined is not JSON-safe (use null)`)
  if (typeof value === 'bigint' || typeof value === 'function' || value instanceof Date) throw new Error(`${path}: not JSON-safe`)
  if (typeof value === 'number' && !Number.isFinite(value)) throw new Error(`${path}: non-finite number`)
  if (Array.isArray(value)) return value.forEach((item, index) => assertTransportRules(item, `${path}[${index}]`))
  if (value === null || typeof value !== 'object') return
  for (const [key, field] of Object.entries(value)) {
    const fieldPath = `${path}.${key}`
    assertTransportRules(field, fieldPath)
    if ((key === 'id' || key.endsWith('Id')) && field !== null && key !== 'operationId') {
      expect(typeof field, `${fieldPath} must be a string id`).toBe('string')
    }
    if (key.endsWith('Ids') && Array.isArray(field)) field.forEach((id) => expect(typeof id, fieldPath).toBe('string'))
    if (key.endsWith('At') && field !== null) expect(field, `${fieldPath} must be UTC ISO-8601`).toMatch(ISO_UTC)
    if (key === 'day') expect(field, fieldPath).toMatch(STUDY_DAY)
  }
}
