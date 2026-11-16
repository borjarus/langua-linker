//! Anki interoperability: the card codec ([`codec`]) and the schema 11 `.apkg` writer
//! ([`schema11`]).
//!
//! The schema 11 wiki is not a complete specification of newer formats (plan Phase 14 step 8):
//! the writer produces the legacy layout every Anki version since 2.1 imports, and the codec
//! decodes the version differences we have fixtures for (scheduler v1 relearning, queue −2/−3,
//! filtered `odue`/`odid`, epoch-second learning due values, FSRS memory in `cards.data`).

pub mod codec;
#[cfg(feature = "apkg")]
pub mod schema11;

/// A part of the collection a format cannot carry, with how many items it concerns. Codes are
/// stable (contract `ExportLossDto.code`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Loss {
    /// Associations (mnemonics) and their provenance.
    Associations,
    GrammarTips,
    WordLinks,
    /// Review history (CSV has none).
    ReviewHistory,
    /// The FSRS card snapshot before and after each review (Anki keeps a reduced revlog row).
    ReviewSnapshots,
    /// FSRS stability/difficulty (CSV has none).
    FsrsMemory,
    /// FSRS stability/difficulty written as Anki card data (`s`/`d`): read by Anki 23.10+, not
    /// verified against every version — the own backup is the lossless form.
    FsrsMemoryUnverified,
    /// Position inside the learning steps; Anki restarts the remaining steps.
    LearningSteps,
    /// When a bury ends; Anki unburies on the next day.
    BuryExpiry,
    /// The day cutoff is not a whole hour; Anki's rollover is in hours.
    CutoffRounded,
    /// Archived cards (templates the note type no longer has) are not exported.
    ArchivedCards,
    /// Tags with whitespace written with `_` (Anki separates tags with spaces).
    TagWhitespace,
    /// Scheduling of reverse cards (CSV rows carry the forward card only).
    ReverseScheduling,
    /// Card flags (CSV has none).
    Flags,
}

impl Loss {
    pub fn code(self) -> &'static str {
        match self {
            Self::Associations => "associations",
            Self::GrammarTips => "grammarTips",
            Self::WordLinks => "wordLinks",
            Self::ReviewHistory => "reviewHistory",
            Self::ReviewSnapshots => "reviewSnapshots",
            Self::FsrsMemory => "fsrsMemory",
            Self::FsrsMemoryUnverified => "fsrsMemoryUnverified",
            Self::LearningSteps => "learningSteps",
            Self::BuryExpiry => "buryExpiry",
            Self::CutoffRounded => "cutoffRounded",
            Self::ArchivedCards => "archivedCards",
            Self::TagWhitespace => "tagWhitespace",
            Self::ReverseScheduling => "reverseScheduling",
            Self::Flags => "flags",
        }
    }
}

/// Counted losses, in a stable order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Losses(pub std::collections::BTreeMap<Loss, u32>);

impl Losses {
    pub fn add(&mut self, loss: Loss, count: u32) {
        if count > 0 {
            *self.0.entry(loss).or_default() += count;
        }
    }

    pub fn count(&self, loss: Loss) -> u32 {
        self.0.get(&loss).copied().unwrap_or(0)
    }

    pub fn merge(&mut self, other: &Losses) {
        for (loss, count) in &other.0 {
            self.add(*loss, *count);
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (Loss, u32)> + '_ {
        self.0.iter().map(|(loss, count)| (*loss, *count))
    }
}
