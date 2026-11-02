//! Input limits and validation errors. Rust validates even when the form already did (plan
//! Phase 7 step 5): payload and field sizes are bounded and ranges are checked here.

/// A rejected input: `field` names the input field, `reason` a stable machine-readable cause.
/// Both cross IPC as `ErrorDto.details`; neither ever contains the rejected value.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{field}: {reason}")]
pub struct ValidationError {
    pub field: &'static str,
    pub reason: &'static str,
}

impl ValidationError {
    pub const fn new(field: &'static str, reason: &'static str) -> Self {
        Self { field, reason }
    }
}

pub type ValidationResult<T = ()> = Result<T, ValidationError>;

/// Maximum characters of a deck or preset name.
pub const MAX_NAME_CHARS: usize = 120;
/// Maximum characters of a deck description.
pub const MAX_DESCRIPTION_CHARS: usize = 2_000;
/// Maximum characters of one note field (front, back).
pub const MAX_FIELD_CHARS: usize = 10_000;
/// Maximum characters of an association or grammar tip.
pub const MAX_SNIPPET_CHARS: usize = 2_000;
/// Maximum characters of one tag; tags hold no whitespace.
pub const MAX_TAG_CHARS: usize = 64;
/// Maximum tags on one note.
pub const MAX_TAGS: usize = 50;
/// Maximum serialized size of one IPC payload accepted by a command.
pub const MAX_PAYLOAD_BYTES: usize = 1_048_576;
/// Maximum deck nesting depth (a root deck has depth 1).
pub const MAX_DECK_DEPTH: usize = 8;

/// Trims and checks a required single-line text.
pub fn required_text(
    field: &'static str,
    value: &str,
    max_chars: usize,
) -> ValidationResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ValidationError::new(field, "required"));
    }
    bounded_text(field, trimmed, max_chars)
}

/// Checks an optional text (may be empty) against its length limit; control characters other
/// than newline and tab are rejected.
pub fn bounded_text(
    field: &'static str,
    value: &str,
    max_chars: usize,
) -> ValidationResult<String> {
    if value.chars().count() > max_chars {
        return Err(ValidationError::new(field, "tooLong"));
    }
    if value
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err(ValidationError::new(field, "controlCharacter"));
    }
    Ok(value.to_owned())
}

/// Normalizes tags: trimmed, no inner whitespace, de-duplicated case-insensitively, bounded.
pub fn tags(values: &[String]) -> ValidationResult<Vec<String>> {
    let mut result: Vec<String> = Vec::new();
    for value in values {
        let tag = value.trim();
        if tag.is_empty() {
            continue;
        }
        if tag.chars().any(char::is_whitespace) {
            return Err(ValidationError::new("tags", "whitespace"));
        }
        let tag = bounded_text("tags", tag, MAX_TAG_CHARS)?;
        if !result.iter().any(|known| known.eq_ignore_ascii_case(&tag)) {
            result.push(tag);
        }
    }
    if result.len() > MAX_TAGS {
        return Err(ValidationError::new("tags", "tooMany"));
    }
    Ok(result)
}

/// Checks an integer range (inclusive).
pub fn in_range(field: &'static str, value: i64, min: i64, max: i64) -> ValidationResult<i64> {
    if (min..=max).contains(&value) {
        Ok(value)
    } else {
        Err(ValidationError::new(field, "outOfRange"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_text_trims_and_rejects_blank() {
        assert_eq!(required_text("name", "  Deutsch ", 10).unwrap(), "Deutsch");
        assert_eq!(
            required_text("name", "   ", 10).unwrap_err(),
            ValidationError::new("name", "required")
        );
    }

    #[test]
    fn length_counts_characters_not_bytes() {
        assert!(bounded_text("front", "żółć", 4).is_ok());
        assert_eq!(
            bounded_text("front", "żółćx", 4).unwrap_err().reason,
            "tooLong"
        );
    }

    #[test]
    fn control_characters_are_rejected_but_newlines_kept() {
        assert!(bounded_text("back", "line\nnext\tcol", 50).is_ok());
        assert!(bounded_text("back", "nul\0", 50).is_err());
    }

    #[test]
    fn tags_are_normalized() {
        let input = [
            "A1".to_owned(),
            " verb ".to_owned(),
            "a1".to_owned(),
            String::new(),
        ];
        assert_eq!(tags(&input).unwrap(), ["A1", "verb"]);
        assert!(tags(&["two words".to_owned()]).is_err());
        let many: Vec<String> = (0..=MAX_TAGS).map(|i| format!("t{i}")).collect();
        assert_eq!(tags(&many).unwrap_err().reason, "tooMany");
    }
}
