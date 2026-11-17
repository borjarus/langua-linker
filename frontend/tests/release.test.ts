import { describe, expect, it } from 'vitest'
import { policyUrl } from '../src/app/profile'

describe('privacy policy link (Phase 17)', () => {
  it('accepts only an absolute https URL', () => {
    expect(policyUrl('https://example.org/privacy')).toBe('https://example.org/privacy')
    expect(policyUrl(undefined)).toBeNull()
    expect(policyUrl('')).toBeNull()
    expect(policyUrl('http://example.org/privacy')).toBeNull()
    expect(policyUrl('javascript:alert(1)')).toBeNull()
    expect(policyUrl('/privacy')).toBeNull()
  })
})
