//! Notes, note types and card templates. One note produces one card per active template.

use crate::validation::{self, ValidationResult};
use crate::{Guid, NoteId, NoteTypeId, TimestampMs, coded_enum};

coded_enum! {
    pub enum NoteTypeKind { Basic = 1, BasicAndReversed = 2, Sentence = 3, SentenceAndReversed = 4 }
}

coded_enum! {
    pub enum CardDirection { Forward = 0, Reverse = 1 }
}

impl NoteTypeKind {
    /// Notes whose front is a sentence: their words can link to word notes.
    pub fn is_sentence(self) -> bool {
        matches!(self, Self::Sentence | Self::SentenceAndReversed)
    }

    /// Templates of the built-in note type, by ordinal.
    pub fn templates(self) -> &'static [CardDirection] {
        match self {
            Self::Basic | Self::Sentence => &[CardDirection::Forward],
            Self::BasicAndReversed | Self::SentenceAndReversed => {
                &[CardDirection::Forward, CardDirection::Reverse]
            }
        }
    }

    /// Stable GUID of the built-in note type, seeded by the initial migration.
    pub fn builtin_guid(self) -> &'static str {
        match self {
            Self::Basic => "notetype-basic",
            Self::BasicAndReversed => "notetype-basic-reversed",
            Self::Sentence => "notetype-sentence",
            Self::SentenceAndReversed => "notetype-sentence-reversed",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NoteType {
    pub id: NoteTypeId,
    pub guid: Guid,
    pub kind: NoteTypeKind,
    pub name: String,
    pub version: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CardTemplate {
    pub guid: Guid,
    pub note_type_id: NoteTypeId,
    pub ordinal: u32,
    pub direction: CardDirection,
    pub name: String,
    pub version: i64,
}

/// Note fields. Stored as a JSON object so later note types can add fields without a migration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteFields {
    pub front: String,
    pub back: String,
}

impl NoteFields {
    pub fn validated(front: &str, back: &str) -> ValidationResult<Self> {
        Ok(Self {
            front: validation::required_text("front", front, validation::MAX_FIELD_CHARS)?,
            back: validation::bounded_text("back", back.trim(), validation::MAX_FIELD_CHARS)?,
        })
    }

    /// Text used for sorting, duplicate checks and word search: NFC is applied at import; here
    /// only case and surrounding whitespace are folded.
    pub fn sort_field(&self) -> String {
        self.front.trim().to_lowercase()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub id: NoteId,
    pub guid: Guid,
    pub note_type_id: NoteTypeId,
    pub fields: NoteFields,
    pub sort_field: String,
    pub tags: Vec<String>,
    pub created_at: TimestampMs,
    pub updated_at: TimestampMs,
    pub deleted_at: Option<TimestampMs>,
    pub usn: i64,
    pub version: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewNote {
    pub guid: Guid,
    pub note_type_id: NoteTypeId,
    pub fields: NoteFields,
    pub tags: Vec<String>,
}

impl NewNote {
    pub fn validated(mut self) -> ValidationResult<Self> {
        self.fields = NoteFields::validated(&self.fields.front, &self.fields.back)?;
        self.tags = validation::tags(&self.tags)?;
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reversed_types_have_two_templates() {
        assert_eq!(NoteTypeKind::Basic.templates().len(), 1);
        assert_eq!(
            NoteTypeKind::SentenceAndReversed.templates(),
            [CardDirection::Forward, CardDirection::Reverse]
        );
        assert!(NoteTypeKind::SentenceAndReversed.is_sentence());
        assert!(!NoteTypeKind::BasicAndReversed.is_sentence());
    }

    #[test]
    fn note_fields_are_trimmed_and_front_is_required() {
        let fields = NoteFields::validated(" der Hund ", " pies ").unwrap();
        assert_eq!(
            (fields.front.as_str(), fields.back.as_str()),
            ("der Hund", "pies")
        );
        assert_eq!(fields.sort_field(), "der hund");
        assert_eq!(NoteFields::validated(" ", "x").unwrap_err().field, "front");
    }
}
