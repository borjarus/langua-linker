//! Decks, deck types with their capabilities, and shared presets (`deck_config`).

use crate::validation::{self, ValidationError, ValidationResult};
use crate::{DeckConfigId, DeckId, Guid, TimestampMs, coded_enum};

coded_enum! {
    /// Learner's native language (L1). Polish only in v1.
    pub enum NativeLanguage { Pl = 1 }
}

coded_enum! {
    /// Language being learned (L2); a property of the deck.
    pub enum TargetLanguage { De = 1, En = 2 }
}

impl NativeLanguage {
    pub fn code(self) -> &'static str {
        match self {
            Self::Pl => "pl",
        }
    }
}

impl TargetLanguage {
    pub fn code(self) -> &'static str {
        match self {
            Self::De => "de",
            Self::En => "en",
        }
    }
}

coded_enum! {
    /// What a deck shows besides front/back. A type change hides sections, never deletes them.
    pub enum DeckType { Simple = 1, TextWithAssociations = 2, Linguistic = 3 }
}

/// Sections a deck offers. Rust owns this map and sends it in DTOs; the UI keeps no copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeckCapabilities {
    pub associations: bool,
    pub grammar_tips: bool,
    pub word_links: bool,
    pub tts: bool,
}

impl DeckType {
    pub const fn capabilities(self) -> DeckCapabilities {
        match self {
            Self::Simple => DeckCapabilities {
                associations: false,
                grammar_tips: false,
                word_links: false,
                tts: true,
            },
            Self::TextWithAssociations => DeckCapabilities {
                associations: true,
                grammar_tips: false,
                word_links: false,
                tts: true,
            },
            Self::Linguistic => DeckCapabilities {
                associations: true,
                grammar_tips: true,
                word_links: true,
                tts: true,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Deck {
    pub id: DeckId,
    pub guid: Guid,
    pub name: String,
    pub parent_id: Option<DeckId>,
    pub description: String,
    pub native_language: NativeLanguage,
    pub target_language: TargetLanguage,
    pub deck_type: DeckType,
    pub config_id: DeckConfigId,
    /// Version of the bundled content this deck was installed from; `None` for user decks.
    pub content_version: Option<String>,
    pub bundled: bool,
    pub created_at: TimestampMs,
    pub updated_at: TimestampMs,
    pub deleted_at: Option<TimestampMs>,
    pub usn: i64,
    pub version: i64,
}

/// Validated input for a new deck.
#[derive(Debug, Clone, PartialEq)]
pub struct NewDeck {
    pub guid: Guid,
    pub name: String,
    pub parent_id: Option<DeckId>,
    pub description: String,
    pub native_language: NativeLanguage,
    pub target_language: TargetLanguage,
    pub deck_type: DeckType,
    pub config_id: DeckConfigId,
    pub content_version: Option<String>,
    pub bundled: bool,
}

impl NewDeck {
    pub fn validated(mut self) -> ValidationResult<Self> {
        self.name = validation::required_text("name", &self.name, validation::MAX_NAME_CHARS)?;
        self.description = validation::bounded_text(
            "description",
            &self.description,
            validation::MAX_DESCRIPTION_CHARS,
        )?;
        Ok(self)
    }
}

/// Rejects moving `deck` under `new_parent` when that would create a cycle or exceed the depth.
/// `parent_of` returns the parent of any deck id (`None` for roots or unknown ids).
pub fn check_reparent(
    deck: DeckId,
    new_parent: Option<DeckId>,
    parent_of: impl Fn(DeckId) -> Option<DeckId>,
) -> ValidationResult {
    let Some(mut cursor) = new_parent else {
        return Ok(());
    };
    let mut depth = 1;
    loop {
        if cursor == deck {
            return Err(ValidationError::new("parentId", "cycle"));
        }
        depth += 1;
        if depth > validation::MAX_DECK_DEPTH {
            return Err(ValidationError::new("parentId", "tooDeep"));
        }
        match parent_of(cursor) {
            Some(parent) => cursor = parent,
            None => return Ok(()),
        }
    }
}

/// Number of FSRS-6 weights w0–w20.
pub const FSRS_WEIGHT_COUNT: usize = crate::fsrs::WEIGHT_COUNT;

/// FSRS-6 default weights (see [`crate::fsrs::DEFAULT_WEIGHTS`]).
pub const DEFAULT_FSRS_WEIGHTS: [f64; FSRS_WEIGHT_COUNT] = crate::fsrs::DEFAULT_WEIGHTS;

coded_enum! {
    /// Order in which new cards are introduced.
    pub enum NewCardOrder { Position = 1, Random = 2 }
}

coded_enum! {
    /// What happens to a card that lapses `leech_threshold` times.
    pub enum LeechAction { Tag = 1, Suspend = 2 }
}

/// Scheduling options shared by every deck that references the preset.
#[derive(Debug, Clone, PartialEq)]
pub struct DeckConfigOptions {
    pub new_per_day: u32,
    pub reviews_per_day: u32,
    pub desired_retention: f64,
    pub maximum_interval_days: u32,
    pub learning_steps_minutes: Vec<u32>,
    pub relearning_steps_minutes: Vec<u32>,
    pub weights: Vec<f64>,
    pub fuzz: bool,
    pub new_card_order: NewCardOrder,
    pub bury_siblings: bool,
    pub leech_threshold: u32,
    pub leech_action: LeechAction,
}

impl Default for DeckConfigOptions {
    fn default() -> Self {
        Self {
            new_per_day: 20,
            reviews_per_day: 200,
            desired_retention: 0.9,
            maximum_interval_days: 36_500,
            learning_steps_minutes: vec![1, 10],
            relearning_steps_minutes: vec![10],
            weights: DEFAULT_FSRS_WEIGHTS.to_vec(),
            fuzz: true,
            new_card_order: NewCardOrder::Position,
            bury_siblings: true,
            leech_threshold: 8,
            leech_action: LeechAction::Tag,
        }
    }
}

impl DeckConfigOptions {
    /// Ranges from contract v1 (`savePreset`): new/day 0–9999, reviews/day 0–99 999, retention
    /// 0.70–0.99, max interval 1–36 500 days, up to 10 steps of 0–43 200 min, exactly 21 weights
    /// within the FSRS-6 reference bounds.
    pub fn validate(&self) -> ValidationResult {
        validation::in_range("newPerDay", self.new_per_day.into(), 0, 9_999)?;
        validation::in_range("reviewsPerDay", self.reviews_per_day.into(), 0, 99_999)?;
        if !(0.70..=0.99).contains(&self.desired_retention) {
            return Err(ValidationError::new("desiredRetention", "outOfRange"));
        }
        validation::in_range(
            "maxIntervalDays",
            self.maximum_interval_days.into(),
            1,
            36_500,
        )?;
        for (field, steps) in [
            ("learningStepsMinutes", &self.learning_steps_minutes),
            ("relearningStepsMinutes", &self.relearning_steps_minutes),
        ] {
            if steps.len() > 10 {
                return Err(ValidationError::new(field, "tooMany"));
            }
            for step in steps {
                validation::in_range(field, (*step).into(), 0, 43_200)?;
            }
        }
        // Count, finiteness and the reference bounds (FSRS-6 is only defined inside them).
        crate::fsrs::validate_weights(&self.weights)?;
        validation::in_range("leechThreshold", self.leech_threshold.into(), 1, 99)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeckConfig {
    pub id: DeckConfigId,
    pub guid: Guid,
    pub name: String,
    pub built_in: bool,
    pub options: DeckConfigOptions,
    pub created_at: TimestampMs,
    pub updated_at: TimestampMs,
    pub deleted_at: Option<TimestampMs>,
    pub usn: i64,
    pub version: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persistence_codes_are_explicit_and_reversible() {
        for deck_type in DeckType::ALL {
            assert_eq!(
                DeckType::from_db_code(deck_type.db_code()).unwrap(),
                *deck_type
            );
        }
        assert_eq!(DeckType::Linguistic.db_code(), 3);
        assert!(DeckType::from_db_code(0).is_err());
    }

    #[test]
    fn capabilities_follow_the_deck_type() {
        assert!(!DeckType::Simple.capabilities().associations);
        assert!(DeckType::TextWithAssociations.capabilities().associations);
        assert!(!DeckType::TextWithAssociations.capabilities().word_links);
        let linguistic = DeckType::Linguistic.capabilities();
        assert!(linguistic.grammar_tips && linguistic.word_links && linguistic.tts);
    }

    #[test]
    fn default_options_are_valid() {
        DeckConfigOptions::default().validate().unwrap();
    }

    #[test]
    fn options_outside_the_contract_ranges_are_rejected() {
        type Mutation = fn(&mut DeckConfigOptions);
        let cases: [(&str, Mutation); 7] = [
            ("newPerDay", |o| o.new_per_day = 10_000),
            ("desiredRetention", |o| o.desired_retention = 0.995),
            ("maxIntervalDays", |o| o.maximum_interval_days = 0),
            ("learningStepsMinutes", |o| {
                o.learning_steps_minutes = vec![1; 11]
            }),
            ("weights", |o| o.weights.pop().map(drop).unwrap_or_default()),
            ("weights", |o| o.weights[3] = f64::NAN),
            ("weights", |o| o.weights[20] = 0.05),
        ];
        for (field, mutate) in cases {
            let mut options = DeckConfigOptions::default();
            mutate(&mut options);
            assert_eq!(options.validate().unwrap_err().field, field);
        }
    }

    #[test]
    fn reparenting_rejects_cycles_and_depth() {
        // 1 ← 2 ← 3 (3's parent is 2, 2's parent is 1)
        let parent_of = |id: DeckId| match id.0 {
            2 => Some(DeckId(1)),
            3 => Some(DeckId(2)),
            _ => None,
        };
        assert!(check_reparent(DeckId(4), Some(DeckId(3)), parent_of).is_ok());
        assert_eq!(
            check_reparent(DeckId(1), Some(DeckId(3)), parent_of)
                .unwrap_err()
                .reason,
            "cycle"
        );
        assert!(check_reparent(DeckId(1), Some(DeckId(1)), parent_of).is_err());
        let chain = |id: DeckId| (id.0 > 1).then(|| DeckId(id.0 - 1));
        assert_eq!(
            check_reparent(DeckId(100), Some(DeckId(8)), chain)
                .unwrap_err()
                .reason,
            "tooDeep"
        );
    }
}
