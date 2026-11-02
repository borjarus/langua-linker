//! Repository ports and the unit of work. One transaction exposes every repository, so a use case
//! that touches several entities commits or rolls back as a whole.
//!
//! Conventions: `*_at` arguments are the caller's clock; versioned updates take the version the
//! caller read and fail with `VersionConflict` when it is stale; "soft delete" sets `deleted_at`
//! (and bumps `usn`) so synchronization can later see the tombstone.

use langualinker_domain::card::{Card, NewCard};
use langualinker_domain::content::{
    Association, GrammarTip, NewAssociation, SentenceWordLink, TipProvenance,
};
use langualinker_domain::deck::{Deck, DeckConfig, DeckConfigOptions, NewDeck};
use langualinker_domain::note::{CardTemplate, NewNote, Note, NoteType, NoteTypeKind};
use langualinker_domain::review::ReviewLog;
use langualinker_domain::{
    AssociationId, CardId, DeckConfigId, DeckId, Guid, NoteId, NoteTypeId, TimestampMs,
};

use crate::AppResult;
use crate::jobs::{JobId, JobRecord, NewJob};

pub trait DeckRepository {
    fn insert_deck(&mut self, deck: &NewDeck, at: TimestampMs) -> AppResult<Deck>;
    /// Any deck, including soft-deleted ones.
    fn deck(&mut self, id: DeckId) -> AppResult<Option<Deck>>;
    fn active_decks(&mut self) -> AppResult<Vec<Deck>>;
    fn update_deck(&mut self, deck: &Deck, at: TimestampMs) -> AppResult<Deck>;
    fn soft_delete_deck(
        &mut self,
        id: DeckId,
        expected_version: i64,
        at: TimestampMs,
    ) -> AppResult<()>;
    /// Newest bundled-content version among active decks (`None` before content is installed).
    fn installed_content_version(&mut self) -> AppResult<Option<String>>;
}

pub trait DeckConfigRepository {
    fn insert_config(
        &mut self,
        guid: &Guid,
        name: &str,
        options: &DeckConfigOptions,
        at: TimestampMs,
    ) -> AppResult<DeckConfig>;
    fn config(&mut self, id: DeckConfigId) -> AppResult<Option<DeckConfig>>;
    fn active_configs(&mut self) -> AppResult<Vec<DeckConfig>>;
    /// The built-in preset seeded by the first migration.
    fn default_config(&mut self) -> AppResult<DeckConfig>;
    fn update_config(&mut self, config: &DeckConfig, at: TimestampMs) -> AppResult<DeckConfig>;
}

pub trait NoteTypeRepository {
    fn note_type(&mut self, kind: NoteTypeKind) -> AppResult<NoteType>;
    fn note_type_by_id(&mut self, id: NoteTypeId) -> AppResult<Option<NoteType>>;
    fn templates(&mut self, note_type: NoteTypeId) -> AppResult<Vec<CardTemplate>>;
}

pub trait NoteRepository {
    fn insert_note(&mut self, note: &NewNote, at: TimestampMs) -> AppResult<Note>;
    fn note(&mut self, id: NoteId) -> AppResult<Option<Note>>;
    fn note_by_guid(&mut self, guid: &Guid) -> AppResult<Option<Note>>;
    fn update_note(&mut self, note: &Note, at: TimestampMs) -> AppResult<Note>;
    /// Soft-deletes the note and all its cards.
    fn soft_delete_note(
        &mut self,
        id: NoteId,
        expected_version: i64,
        at: TimestampMs,
    ) -> AppResult<()>;
}

pub trait CardRepository {
    fn insert_card(&mut self, card: &NewCard, at: TimestampMs) -> AppResult<Card>;
    fn card(&mut self, id: CardId) -> AppResult<Option<Card>>;
    /// Active (not deleted) cards of a note, archived ones included, by template ordinal.
    fn cards_of_note(&mut self, note: NoteId) -> AppResult<Vec<Card>>;
    fn update_card(&mut self, card: &Card, at: TimestampMs) -> AppResult<Card>;
    /// Review and day-learning cards of the decks due before `until`, oldest first.
    fn due_card_ids(
        &mut self,
        decks: &[DeckId],
        until: TimestampMs,
        limit: u32,
    ) -> AppResult<Vec<CardId>>;
    /// New cards of the decks in introduction order.
    fn new_card_ids(&mut self, decks: &[DeckId], limit: u32) -> AppResult<Vec<CardId>>;
    fn next_position(&mut self) -> AppResult<i64>;
}

pub trait ReviewLogRepository {
    /// Fails with `Validation(operationId, duplicate)` when the operation was already logged.
    fn insert_review_log(&mut self, log: &ReviewLog) -> AppResult<ReviewLog>;
    /// Logs of a card, newest first; invalidated ones included.
    fn review_logs(&mut self, card: CardId) -> AppResult<Vec<ReviewLog>>;
    fn review_log_by_operation(&mut self, operation_id: &str) -> AppResult<Option<ReviewLog>>;
    fn invalidate_review_log(&mut self, operation_id: &str, at: TimestampMs) -> AppResult<()>;
}

pub trait ContentRepository {
    fn insert_association(
        &mut self,
        association: &NewAssociation,
        at: TimestampMs,
    ) -> AppResult<Association>;
    /// Active associations of a note in display order.
    fn associations(&mut self, note: NoteId) -> AppResult<Vec<Association>>;
    /// Makes `association` the single favorite of its note (`None` clears the favorite).
    fn set_favorite_association(
        &mut self,
        note: NoteId,
        association: Option<AssociationId>,
        at: TimestampMs,
    ) -> AppResult<()>;
    fn soft_delete_association(&mut self, id: AssociationId, at: TimestampMs) -> AppResult<()>;

    fn insert_grammar_tip(
        &mut self,
        note: NoteId,
        guid: &Guid,
        text: &str,
        provenance: TipProvenance,
        at: TimestampMs,
    ) -> AppResult<GrammarTip>;
    fn grammar_tips(&mut self, note: NoteId) -> AppResult<Vec<GrammarTip>>;

    /// Replaces every link of a sentence note.
    fn replace_word_links(
        &mut self,
        sentence: NoteId,
        links: &[SentenceWordLink],
        at: TimestampMs,
    ) -> AppResult<Vec<SentenceWordLink>>;
    fn word_links(&mut self, sentence: NoteId) -> AppResult<Vec<SentenceWordLink>>;
    /// Links pointing at a deleted word become unresolved; the links themselves stay.
    fn unresolve_links_to(&mut self, word: NoteId, at: TimestampMs) -> AppResult<u64>;
}

pub trait SettingsRepository {
    fn setting(&mut self, key: &str) -> AppResult<Option<String>>;
    fn put_setting(&mut self, key: &str, value: &str, at: TimestampMs) -> AppResult<()>;
}

pub trait JobRepository {
    fn insert_job(&mut self, job: &NewJob, at: TimestampMs) -> AppResult<JobRecord>;
    fn job(&mut self, id: &JobId) -> AppResult<Option<JobRecord>>;
    /// Persists status, progress, result and error; `sequence` must be greater than the stored one.
    fn update_job(&mut self, job: &JobRecord) -> AppResult<()>;
    fn unfinished_jobs(&mut self) -> AppResult<Vec<JobRecord>>;
}

/// Result of a mutation recorded under its `operationId`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedOperation {
    pub operation: String,
    pub result_json: String,
}

/// Idempotency store: a repeated `operationId` returns the first result (contract rule 4).
pub trait IdempotencyRepository {
    fn recorded_operation(&mut self, operation_id: &str) -> AppResult<Option<RecordedOperation>>;
    fn record_operation(
        &mut self,
        operation_id: &str,
        operation: &RecordedOperation,
        at: TimestampMs,
    ) -> AppResult<()>;
}

/// Every repository inside one transaction.
pub trait Repositories:
    DeckRepository
    + DeckConfigRepository
    + NoteTypeRepository
    + NoteRepository
    + CardRepository
    + ReviewLogRepository
    + ContentRepository
    + SettingsRepository
    + JobRepository
    + IdempotencyRepository
{
}

impl<T> Repositories for T where
    T: DeckRepository
        + DeckConfigRepository
        + NoteTypeRepository
        + NoteRepository
        + CardRepository
        + ReviewLogRepository
        + ContentRepository
        + SettingsRepository
        + JobRepository
        + IdempotencyRepository
{
}

/// Runs work in a database transaction off the caller's thread.
pub trait UnitOfWork: Send + Sync + 'static {
    /// One read transaction (consistent snapshot).
    fn read<T, F>(&self, work: F) -> AppResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut dyn Repositories) -> AppResult<T> + Send + 'static;

    /// One write transaction: committed when `work` returns `Ok`, rolled back otherwise.
    fn write<T, F>(&self, work: F) -> AppResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut dyn Repositories) -> AppResult<T> + Send + 'static;

    /// Schema version of the open database.
    fn schema_version(&self) -> i64;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compile-time check: the unit of work hands out `&mut dyn Repositories`, so every port must
    /// stay object safe, and the blanket impl must cover a `dyn Repositories` itself.
    #[allow(dead_code)]
    fn ports_are_object_safe(
        decks: &mut dyn DeckRepository,
        configs: &mut dyn DeckConfigRepository,
        note_types: &mut dyn NoteTypeRepository,
        notes: &mut dyn NoteRepository,
        cards: &mut dyn CardRepository,
        reviews: &mut dyn ReviewLogRepository,
        content: &mut dyn ContentRepository,
        settings: &mut dyn SettingsRepository,
        jobs: &mut dyn JobRepository,
        idempotency: &mut dyn IdempotencyRepository,
        all: &mut dyn Repositories,
    ) {
        fn needs_repositories(_: &mut (impl Repositories + ?Sized)) {}
        let _ = (
            decks,
            configs,
            note_types,
            notes,
            cards,
            reviews,
            content,
            settings,
            jobs,
            idempotency,
        );
        needs_repositories(all);
    }

    #[test]
    fn recorded_operations_compare_by_value() {
        let first = RecordedOperation {
            operation: "job:import".into(),
            result_json: "42".into(),
        };
        assert_eq!(first.clone(), first);
        assert_ne!(
            first,
            RecordedOperation {
                operation: "job:export".into(),
                result_json: "42".into(),
            }
        );
    }
}
