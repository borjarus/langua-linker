//! Training examples for the on-device association model (ADR-23).
//!
//! An example is exactly what the app sends a provider for a note — the same system prompt
//! (method, L1, L2) and user message as [`prompt::system_prompt`] and [`prompt::user_prompt`] —
//! followed by the answer in the form [`prompt::parse_answer`] accepts. So the fine-tuned model
//! learns the app's frame, and its output goes through the same parser and moderation.

use langualinker_domain::deck::{NativeLanguage, TargetLanguage};
use langualinker_domain::note::NoteTypeKind;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::moderation::{self, Verdict};
use super::prompt::{self, MAX_ASSOCIATION_CHARS, PromptKind};

/// One supervised example: a conversation in ChatML roles plus where it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Example {
    /// Note GUID; the train/eval split is decided on it, so both directions of a note and all
    /// its associations land on the same side.
    pub note_guid: String,
    pub target: TargetLanguage,
    pub kind: PromptKind,
    pub system: String,
    pub user: String,
    /// `{"association": "..."}`.
    pub answer: String,
}

/// Why a note's association is left out of the data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    Empty,
    TooLong,
    Moderation,
}

/// The example for one association of a note, or why it is skipped. The association is the
/// first variant (variant 1 in the user message), as when a learner asks for the first time.
pub fn example(
    note_guid: &str,
    note_type: NoteTypeKind,
    native: NativeLanguage,
    target: TargetLanguage,
    front: &str,
    back: &str,
    association: &str,
) -> Result<Example, Skip> {
    let association = association.trim();
    if association.is_empty() {
        return Err(Skip::Empty);
    }
    if association.chars().count() > MAX_ASSOCIATION_CHARS {
        return Err(Skip::TooLong);
    }
    if moderation::check(&[front, back, association]) == Verdict::Blocked {
        return Err(Skip::Moderation);
    }
    let kind = PromptKind::for_note(note_type, front);
    let answer = json!({ "association": association }).to_string();
    Ok(Example {
        note_guid: note_guid.to_owned(),
        target,
        kind,
        system: prompt::system_prompt(kind, native, target),
        user: prompt::user_prompt(front, back, 0),
        answer,
    })
}

/// `true` for the notes held out for evaluation: `percent` of the notes, chosen by a hash of
/// the GUID, so the split is stable across exports.
pub fn is_eval(note_guid: &str, percent: u8) -> bool {
    let digest = Sha256::digest(note_guid.as_bytes());
    let bucket = u16::from_be_bytes([digest[0], digest[1]]) % 100;
    bucket < u16::from(percent.min(100))
}

/// One JSONL line: `messages` in the chat format the training libraries read, plus metadata
/// that the training ignores and the evaluation groups by.
pub fn to_json(example: &Example) -> Value {
    json!({
        "messages": [
            { "role": "system", "content": example.system },
            { "role": "user", "content": example.user },
            { "role": "assistant", "content": example.answer },
        ],
        "meta": {
            "noteGuid": example.note_guid,
            "target": example.target.code(),
            "method": example.kind.as_str(),
            "promptVersion": example.kind.version(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(association: &str) -> Result<Example, Skip> {
        example(
            "enw-1",
            NoteTypeKind::Basic,
            NativeLanguage::Pl,
            TargetLanguage::En,
            "pretty",
            "ładny",
            association,
        )
    }

    #[test]
    fn an_example_is_the_apps_own_prompt_and_a_parseable_answer() {
        let example = word("„Pretty” brzmi jak „pryty” – ładne pryty na balu.").unwrap();
        assert_eq!(example.kind, PromptKind::WordSubstitution);
        assert_eq!(
            example.system,
            prompt::system_prompt(
                PromptKind::WordSubstitution,
                NativeLanguage::Pl,
                TargetLanguage::En
            )
        );
        assert_eq!(example.user, prompt::user_prompt("pretty", "ładny", 0));
        assert_eq!(
            prompt::parse_answer(&example.answer).unwrap(),
            "„Pretty” brzmi jak „pryty” – ładne pryty na balu."
        );
        let line = to_json(&example);
        assert_eq!(line["messages"][2]["role"], "assistant");
        assert_eq!(line["meta"]["promptVersion"], "word-substitution-v1");
    }

    #[test]
    fn sentences_use_their_methods() {
        let sentence = example(
            "ens-1",
            NoteTypeKind::SentenceAndReversed,
            NativeLanguage::Pl,
            TargetLanguage::En,
            "She is tall and very pretty.",
            "Ona jest wysoka i bardzo ładna.",
            "Wysoka pani z obrazkiem.",
        )
        .unwrap();
        assert_eq!(sentence.kind, PromptKind::SentenceStoryChain);
    }

    #[test]
    fn empty_long_and_blocked_associations_are_skipped() {
        assert_eq!(word("  "), Err(Skip::Empty));
        assert_eq!(word(&"a".repeat(401)), Err(Skip::TooLong));
        assert_eq!(word("kurwa mać"), Err(Skip::Moderation));
    }

    #[test]
    fn the_split_is_stable_and_near_the_requested_share() {
        assert_eq!(is_eval("ens-abc", 5), is_eval("ens-abc", 5));
        assert!(!is_eval("ens-abc", 0));
        assert!(is_eval("ens-abc", 100));
        let held = (0..10_000)
            .filter(|n| is_eval(&format!("note-{n}"), 5))
            .count();
        assert!((400..600).contains(&held), "{held}");
    }
}
