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
use langualinker_domain::deck::NewCardOrder;
use langualinker_domain::deck::{Deck, DeckConfig, DeckConfigOptions, NewDeck};
use langualinker_domain::note::{CardTemplate, NewNote, Note, NoteType, NoteTypeKind};
use langualinker_domain::queue::{Candidate, DoneToday};
use langualinker_domain::review::{CardSnapshot, ReviewLog};
use langualinker_domain::{
    AssociationId, CardId, DeckConfigId, DeckId, GrammarTipId, Guid, NoteId, NoteTypeId,
    ReviewLogId, SessionId, TimestampMs,
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

/// Provenance of a note installed from bundled content (`bundled_notes`, schema v2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundledNote {
    pub note_id: NoteId,
    pub deck_guid: String,
    /// Last content version that shipped the note.
    pub content_version: String,
    /// Content version that no longer ships it.
    pub removed_in_version: Option<String>,
    /// The importer archived the note's cards when it was removed; a re-add restores them.
    pub archived_by_import: bool,
}

/// What the bundled-content importer needs beyond the entity repositories (Phase 8).
pub trait BundledContentRepository {
    /// Any deck with this GUID, including a soft-deleted one.
    fn deck_by_guid(&mut self, guid: &Guid) -> AppResult<Option<Deck>>;
    fn bundled_note(&mut self, note: NoteId) -> AppResult<Option<BundledNote>>;
    fn put_bundled_note(&mut self, note: &BundledNote) -> AppResult<()>;
    /// Bundled notes of a deck whose note is not soft-deleted, with the note GUID.
    fn bundled_notes_of_deck(&mut self, deck_guid: &str) -> AppResult<Vec<(Guid, BundledNote)>>;
    /// Fields of a note the user edited (`front`, `back`, `tags`, `noteType`, `wordLinks`,
    /// `association:<guid>`, `tip:<guid>`); a content update leaves them alone.
    fn content_overrides(&mut self, note: NoteId) -> AppResult<Vec<String>>;
    fn put_content_override(
        &mut self,
        note: NoteId,
        field: &str,
        original_content_version: &str,
        at: TimestampMs,
    ) -> AppResult<()>;
    /// Any association with this GUID, including a soft-deleted one.
    fn association_by_guid(&mut self, guid: &Guid) -> AppResult<Option<Association>>;
    fn update_association_text(
        &mut self,
        id: AssociationId,
        text: &str,
        at: TimestampMs,
    ) -> AppResult<()>;
    /// Any grammar tip with this GUID, including a soft-deleted one.
    fn grammar_tip_by_guid(&mut self, guid: &Guid) -> AppResult<Option<GrammarTip>>;
    fn update_grammar_tip_text(
        &mut self,
        id: GrammarTipId,
        text: &str,
        at: TimestampMs,
    ) -> AppResult<()>;
    /// Archives (`Some`) or restores (`None`) every active card of a note; scheduling columns
    /// are not touched. Returns the number of cards changed.
    fn set_note_cards_archived(
        &mut self,
        note: NoteId,
        archived_at: Option<TimestampMs>,
        at: TimestampMs,
    ) -> AppResult<u64>;
    /// A card of the note was ever reviewed (reps or a review log).
    fn note_has_progress(&mut self, note: NoteId) -> AppResult<bool>;
    /// Changes whenever a deck, note or card changes; a preview made against another value is
    /// stale.
    fn collection_fingerprint(&mut self) -> AppResult<String>;
}

/// Card counts of one deck (its own cards, not its subdecks).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DeckCardCounts {
    pub new: u32,
    pub learning: u32,
    pub review: u32,
    pub overdue: u32,
    pub total: u32,
    /// Cards with an interval of at least 21 days.
    pub mature: u32,
}

/// Read models of the dashboard.
pub trait DashboardQueries {
    /// Counts per deck that has live cards. Learning and review cards count when due before
    /// `day_end`; review cards due before `day_start` are also overdue. Suspended, buried,
    /// archived and deleted cards only count in `total` (suspended/buried) or not at all.
    fn deck_card_counts(
        &mut self,
        day_start: TimestampMs,
        day_end: TimestampMs,
    ) -> AppResult<Vec<(DeckId, DeckCardCounts)>>;
    /// Minutes after midnight at which a study day starts (`collection.day_cutoff_minutes`).
    fn day_cutoff_minutes(&mut self) -> AppResult<u32>;
    /// Valid (not invalidated) reviews at or after `since`.
    fn reviews_since(&mut self, since: TimestampMs) -> AppResult<u32>;
    /// Distinct study days with a valid review, newest first, as day numbers since the epoch
    /// shifted by the cutoff; at most `limit`.
    fn review_days(&mut self, cutoff_minutes: u32, limit: u32) -> AppResult<Vec<i64>>;
    /// Earliest due time after `after` of a scheduled, studyable card.
    fn next_due_after(&mut self, after: TimestampMs) -> AppResult<Option<TimestampMs>>;
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

/// Lifecycle of a study session. At most one session is not finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    Active,
    Paused,
    Finished,
}

impl SessionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Finished => "finished",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "active" => Some(Self::Active),
            "paused" => Some(Self::Paused),
            "finished" => Some(Self::Finished),
            _ => None,
        }
    }
}

/// A durable study session (`study_sessions`).
#[derive(Debug, Clone, PartialEq)]
pub struct StudySession {
    pub id: SessionId,
    /// Studied deck and its subdecks; `None` studies all decks.
    pub deck_id: Option<DeckId>,
    pub status: SessionStatus,
    /// Preset of the studied deck when the session started (audit; ratings use the current one).
    pub config_id: Option<DeckConfigId>,
    pub config_version: Option<i64>,
    /// The session studies only its own items (chosen cards), without daily limits.
    pub restricted: bool,
    pub current_card_id: Option<CardId>,
    pub started_at: TimestampMs,
    pub paused_at: Option<TimestampMs>,
    pub finished_at: Option<TimestampMs>,
    /// Answer time of valid ratings, capped per rating.
    pub active_ms: i64,
    pub updated_at: TimestampMs,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewStudySession {
    pub deck_id: Option<DeckId>,
    pub config_id: Option<DeckConfigId>,
    pub config_version: Option<i64>,
    pub restricted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionItemStatus {
    Pending,
    Answered,
    Skipped,
}

impl SessionItemStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Answered => "answered",
            Self::Skipped => "skipped",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "pending" => Some(Self::Pending),
            "answered" => Some(Self::Answered),
            "skipped" => Some(Self::Skipped),
            _ => None,
        }
    }
}

/// A card of a session, in the order the session met it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionItem {
    pub position: i64,
    pub card_id: CardId,
    pub status: SessionItemStatus,
}

/// A sibling buried by a rating: its state before and the version the rating left it at.
#[derive(Debug, Clone, PartialEq)]
pub struct SiblingChange {
    pub card_id: CardId,
    pub before: CardSnapshot,
    pub after_version: i64,
}

/// Side effects of a rating that undo must reverse besides the rated card itself.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct UndoChanges {
    pub siblings: Vec<SiblingChange>,
    /// The rating tagged the note as a leech; the note version it left.
    pub leech_tagged: Option<(NoteId, i64)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UndoEntry {
    pub id: i64,
    pub session_id: SessionId,
    pub review_log_id: ReviewLogId,
    pub operation_id: String,
    pub changes: UndoChanges,
    pub created_at: TimestampMs,
    pub consumed_at: Option<TimestampMs>,
}

/// Sessions, their items and undo entries (plan Phase 9 step 3).
pub trait StudyRepository {
    /// Fails with `Validation(sessionId, open)` when another session is not finished.
    fn insert_session(
        &mut self,
        session: &NewStudySession,
        at: TimestampMs,
    ) -> AppResult<StudySession>;
    fn session(&mut self, id: SessionId) -> AppResult<Option<StudySession>>;
    /// The session that is active or paused, if any.
    fn open_session(&mut self) -> AppResult<Option<StudySession>>;
    /// Persists status, current card, timestamps and active time.
    fn update_session(&mut self, session: &StudySession, at: TimestampMs) -> AppResult<()>;
    fn session_items(&mut self, session: SessionId) -> AppResult<Vec<SessionItem>>;
    /// Sets the status of the card's item, appending the card when the session has none.
    fn put_session_item(
        &mut self,
        session: SessionId,
        card: CardId,
        status: SessionItemStatus,
    ) -> AppResult<()>;
    fn insert_undo_entry(
        &mut self,
        session: SessionId,
        review_log: ReviewLogId,
        changes: &UndoChanges,
        at: TimestampMs,
    ) -> AppResult<i64>;
    /// The newest undo entry of the session, consumed or not.
    fn last_undo_entry(&mut self, session: SessionId) -> AppResult<Option<UndoEntry>>;
    fn consume_undo_entry(&mut self, id: i64, at: TimestampMs) -> AppResult<()>;
    /// Valid (not undone) reviews of the session, oldest first.
    fn session_reviews(&mut self, session: SessionId) -> AppResult<Vec<ReviewLog>>;
    fn session_review_count(&mut self, session: SessionId) -> AppResult<u32>;
}

/// Read models of the scheduler. Candidates are studyable cards only: not suspended, buried,
/// archived or deleted.
pub trait SchedulerQueries {
    /// IANA time zone and day cutoff (minutes) of the collection.
    fn collection_calendar(&mut self) -> AppResult<(String, u32)>;
    /// Valid ratings at or after `since`, by the deck of the rated card: introduced new cards,
    /// and reviews of review or day-learning cards (both use the review limit).
    fn done_since(&mut self, since: TimestampMs) -> AppResult<Vec<(DeckId, DoneToday)>>;
    /// Notes with a valid rating at or after `since`.
    fn notes_reviewed_since(&mut self, since: TimestampMs) -> AppResult<Vec<NoteId>>;
    /// Intraday learning cards of the decks due before `until`, by due time.
    fn learning_candidates(
        &mut self,
        decks: &[DeckId],
        until: TimestampMs,
    ) -> AppResult<Vec<Candidate>>;
    /// Review and day-learning cards of one deck due before `until`, by due time.
    fn review_candidates(
        &mut self,
        deck: DeckId,
        until: TimestampMs,
        limit: u32,
    ) -> AppResult<Vec<Candidate>>;
    /// New cards of one deck in introduction order; `Random` shuffles per `day`.
    fn new_candidates(
        &mut self,
        deck: DeckId,
        order: NewCardOrder,
        day: i64,
        limit: u32,
    ) -> AppResult<Vec<Candidate>>;
    /// Buried cards whose day has passed (`buried_until` at or before `now`, or unset).
    fn expired_buried_cards(&mut self, now: TimestampMs) -> AppResult<Vec<CardId>>;
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
    + BundledContentRepository
    + DashboardQueries
    + StudyRepository
    + SchedulerQueries
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
        + BundledContentRepository
        + DashboardQueries
        + StudyRepository
        + SchedulerQueries
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
    #[allow(dead_code, clippy::too_many_arguments)]
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
