//! Unicode rules shared by the importer, the content linter and (through the fixtures in
//! `tests/fixtures/unicode/ranges.json`) the Svelte editor — plan §14.1.
//!
//! Stored text is NFC. Word-link ranges are UTF-16 code units over the NFC text, half-open
//! `[start, end)`, the same numbers JavaScript's `String.prototype.slice` takes. Rust never indexes
//! a `&str` with them directly: [`utf16_range`] maps them to UTF-8 byte offsets and refuses a
//! boundary that would split a surrogate pair.

use std::ops::Range;

use unicode_normalization::{UnicodeNormalization, is_nfc_quick};

/// The NFC form of `text` (what `text.normalize('NFC')` returns in JavaScript).
pub fn nfc(text: &str) -> String {
    if is_nfc(text) {
        text.to_owned()
    } else {
        text.nfc().collect()
    }
}

/// Whether `text` is already in NFC.
pub fn is_nfc(text: &str) -> bool {
    match is_nfc_quick(text.chars()) {
        unicode_normalization::IsNormalized::Yes => true,
        unicode_normalization::IsNormalized::No => false,
        unicode_normalization::IsNormalized::Maybe => text.nfc().eq(text.chars()),
    }
}

/// Number of UTF-16 code units of `text`, the unit of word-link ranges and of JavaScript strings.
pub fn utf16_len(text: &str) -> u32 {
    text.encode_utf16().count() as u32
}

/// Byte range of the UTF-16 range `[start, end)` in `text`, or `None` when the range is empty,
/// reversed, out of bounds or starts/ends inside a surrogate pair.
pub fn utf16_range(text: &str, start: u32, end: u32) -> Option<Range<usize>> {
    if start >= end {
        return None;
    }
    let mut units = 0u32;
    let mut start_byte = None;
    for (byte, ch) in text.char_indices() {
        if units == start {
            start_byte = Some(byte);
        }
        if units == end {
            return start_byte.map(|from| from..byte);
        }
        if units > end {
            return None;
        }
        units += ch.len_utf16() as u32;
    }
    if units == end {
        return start_byte.map(|from| from..text.len());
    }
    None
}

/// The text of the UTF-16 range `[start, end)`, if it is a valid range of `text`.
pub fn utf16_slice(text: &str, start: u32, end: u32) -> Option<&str> {
    utf16_range(text, start, end).map(|range| &text[range])
}

/// UTF-16 offset of the byte offset `byte` (which must be a char boundary).
pub fn utf16_offset(text: &str, byte: usize) -> u32 {
    utf16_len(&text[..byte])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nfc_composes_combining_marks() {
        let decomposed = "z\u{0307}a\u{0328}";
        assert!(!is_nfc(decomposed));
        assert_eq!(nfc(decomposed), "ża\u{0328}".nfc().collect::<String>());
        assert_eq!(nfc(decomposed), "żą");
        assert!(is_nfc("zażółć"));
    }

    #[test]
    fn ranges_count_utf16_units_half_open() {
        let text = "heute Abend";
        assert_eq!(utf16_slice(text, 6, 11), Some("Abend"));
        assert_eq!(utf16_slice(text, 0, 5), Some("heute"));
        assert_eq!(utf16_slice(text, 6, 12), None, "past the end");
        assert_eq!(utf16_slice(text, 5, 5), None, "empty");
        assert_eq!(utf16_slice(text, 7, 6), None, "reversed");
    }

    #[test]
    fn surrogate_pairs_cannot_be_split() {
        let text = "a👩‍💻b";
        // 👩 (2 units) + ZWJ (1) + 💻 (2) = 5 units after "a".
        assert_eq!(utf16_len(text), 7);
        assert_eq!(utf16_slice(text, 1, 6), Some("👩‍💻"));
        assert_eq!(utf16_slice(text, 2, 6), None, "starts inside 👩");
        assert_eq!(utf16_slice(text, 1, 5), None, "ends inside 💻");
        assert_eq!(utf16_slice(text, 6, 7), Some("b"));
    }

    #[test]
    fn multibyte_letters_are_one_unit() {
        let text = "Straße zażółć";
        assert_eq!(utf16_slice(text, 0, 6), Some("Straße"));
        assert_eq!(utf16_slice(text, 7, 13), Some("zażółć"));
        assert_eq!(utf16_offset(text, text.find("zażółć").unwrap()), 7);
    }

    /// The fixture the Svelte tests read too (`frontend/tests/unicodeRanges.test.ts`).
    #[test]
    fn shared_fixture_ranges() {
        let text = include_str!("../../../tests/fixtures/unicode/ranges.json");
        let fixture: serde_json::Value = serde_json::from_str(text).unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert!(cases.len() >= 10);
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let normalized = nfc(case["text"].as_str().unwrap());
            assert_eq!(normalized, case["nfc"].as_str().unwrap(), "{name}");
            let start = case["start"].as_u64().unwrap() as u32;
            let end = case["end"].as_u64().unwrap() as u32;
            assert_eq!(
                utf16_slice(&normalized, start, end),
                case["slice"].as_str(),
                "{name}"
            );
        }
    }
}
