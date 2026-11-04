//! Note-level learning content: associations, grammar tips, sentence word links and media.
//! Everything here belongs to a note, not a card (plan step 12).

use crate::validation::{self, ValidationError, ValidationResult};
use crate::{
    AssociationId, CardId, GrammarTipId, Guid, MediaId, NoteId, TimestampMs, WordLinkId, coded_enum,
};

coded_enum! {
    pub enum AssociationProvenance { Bundled = 1, Generated = 2, Manual = 3 }
}

coded_enum! {
    /// Moderation outcome of generated text. Bundled and manual texts are `NotRequired`.
    pub enum ModerationStatus { NotRequired = 0, Passed = 1, Blocked = 2 }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Association {
    pub id: AssociationId,
    pub guid: Guid,
    pub note_id: NoteId,
    pub text: String,
    pub provenance: AssociationProvenance,
    pub native_language: String,
    pub target_language: String,
    pub prompt_version: Option<String>,
    pub model: Option<String>,
    pub moderation: ModerationStatus,
    pub cache_key: Option<String>,
    pub sort_order: i64,
    pub is_favorite: bool,
    pub created_at: TimestampMs,
    pub updated_at: TimestampMs,
    pub deleted_at: Option<TimestampMs>,
    pub usn: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewAssociation {
    pub guid: Guid,
    pub note_id: NoteId,
    pub text: String,
    pub provenance: AssociationProvenance,
    pub native_language: String,
    pub target_language: String,
    pub prompt_version: Option<String>,
    pub model: Option<String>,
    pub moderation: ModerationStatus,
    pub cache_key: Option<String>,
}

impl NewAssociation {
    pub fn validated(mut self) -> ValidationResult<Self> {
        self.text = validation::required_text("text", &self.text, validation::MAX_SNIPPET_CHARS)?;
        if self.moderation == ModerationStatus::Blocked {
            // Blocked output is never stored as an association.
            return Err(ValidationError::new("moderation", "blocked"));
        }
        Ok(self)
    }
}

coded_enum! {
    pub enum TipProvenance { Bundled = 1, Generated = 2, Manual = 3 }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GrammarTip {
    pub id: GrammarTipId,
    pub guid: Guid,
    pub note_id: NoteId,
    pub text: String,
    pub provenance: TipProvenance,
    pub sort_order: i64,
    pub created_at: TimestampMs,
    pub updated_at: TimestampMs,
    pub deleted_at: Option<TimestampMs>,
    pub usn: i64,
}

coded_enum! {
    pub enum WordLinkResolution { Resolved = 1, Unresolved = 2, Ambiguous = 3, UserOverridden = 4 }
}

/// A phrase of a sentence note linked to a word note. Ranges are UTF-16 code units over the NFC
/// front, half-open `[start, end)`. Deleting the word leaves the link `Unresolved`, never deleted.
#[derive(Debug, Clone, PartialEq)]
pub struct SentenceWordLink {
    pub id: WordLinkId,
    pub sentence_note_id: NoteId,
    pub word_note_id: Option<NoteId>,
    pub preferred_word_card_id: Option<CardId>,
    pub surface_form: String,
    pub lemma: Option<String>,
    pub start_index: u32,
    pub end_index: u32,
    pub resolution: WordLinkResolution,
    pub created_at: TimestampMs,
    pub updated_at: TimestampMs,
}

impl SentenceWordLink {
    /// A resolved link needs a word; an unresolved one must not carry one.
    pub fn check(&self) -> ValidationResult {
        if self.start_index >= self.end_index {
            return Err(ValidationError::new("range", "empty"));
        }
        match (self.resolution, self.word_note_id) {
            (WordLinkResolution::Resolved | WordLinkResolution::UserOverridden, None) => {
                Err(ValidationError::new("wordNoteId", "required"))
            }
            (WordLinkResolution::Unresolved, Some(_)) => {
                Err(ValidationError::new("wordNoteId", "unexpected"))
            }
            _ => Ok(()),
        }
    }
}

/// Number of UTF-16 code units of `text`; kept here for existing callers (see [`crate::text`]).
pub use crate::text::utf16_len;

#[derive(Debug, Clone, PartialEq)]
pub struct Media {
    pub id: MediaId,
    pub guid: Guid,
    pub file_name: String,
    pub mime_type: String,
    pub sha256: String,
    pub size_bytes: i64,
    pub created_at: TimestampMs,
    pub deleted_at: Option<TimestampMs>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(resolution: WordLinkResolution, word: Option<i64>) -> SentenceWordLink {
        SentenceWordLink {
            id: WordLinkId(1),
            sentence_note_id: NoteId(1),
            word_note_id: word.map(NoteId),
            preferred_word_card_id: None,
            surface_form: "Hund".into(),
            lemma: None,
            start_index: 4,
            end_index: 8,
            resolution,
            created_at: TimestampMs(0),
            updated_at: TimestampMs(0),
        }
    }

    #[test]
    fn word_link_resolution_matches_word_presence() {
        assert!(link(WordLinkResolution::Resolved, Some(2)).check().is_ok());
        assert!(link(WordLinkResolution::Unresolved, None).check().is_ok());
        assert!(link(WordLinkResolution::Ambiguous, None).check().is_ok());
        assert!(link(WordLinkResolution::Resolved, None).check().is_err());
        assert!(
            link(WordLinkResolution::Unresolved, Some(2))
                .check()
                .is_err()
        );
    }

    #[test]
    fn utf16_length_counts_surrogate_pairs() {
        assert_eq!(utf16_len("Hund"), 4);
        assert_eq!(utf16_len("ż"), 1);
        assert_eq!(utf16_len("🐕"), 2);
    }

    #[test]
    fn blocked_generated_text_is_never_stored() {
        let new = NewAssociation {
            guid: Guid::from_bytes([1; 16]),
            note_id: NoteId(1),
            text: "x".into(),
            provenance: AssociationProvenance::Generated,
            native_language: "pl".into(),
            target_language: "de".into(),
            prompt_version: Some("v1".into()),
            model: Some("m".into()),
            moderation: ModerationStatus::Blocked,
            cache_key: None,
        };
        assert_eq!(new.validated().unwrap_err().reason, "blocked");
    }
}
