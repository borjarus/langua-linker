/**
 * Word-link ranges (plan §14.1): UTF-16 code units over NFC text, half-open `[start, end)`.
 * Same rule as `crates/domain/src/text.rs` (`utf16_slice`); both are checked against
 * `tests/fixtures/unicode/ranges.json`.
 */

const isHighSurrogate = (code: number): boolean => code >= 0xd800 && code <= 0xdbff
const isLowSurrogate = (code: number): boolean => code >= 0xdc00 && code <= 0xdfff

/** Text of the range in already-NFC `text`, or `null` when the range is empty, reversed, out of bounds or splits a surrogate pair. */
export function utf16Slice(text: string, start: number, end: number): string | null {
  if (!Number.isInteger(start) || !Number.isInteger(end) || start < 0 || start >= end || end > text.length) return null
  if (isLowSurrogate(text.charCodeAt(start))) return null
  if (end < text.length && isLowSurrogate(text.charCodeAt(end)) && isHighSurrogate(text.charCodeAt(end - 1))) return null
  return text.slice(start, end)
}
