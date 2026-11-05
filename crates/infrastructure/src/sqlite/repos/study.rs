//! Study sessions, undo entries and the scheduler's read models (schema v3, plan Phase 9).

use langualinker_application::content::IMPORTED_SCHEDULER;
use langualinker_application::repositories::{
    NewStudySession, SchedulerQueries, SessionItem, SessionItemStatus, SessionStatus,
    StudyRepository, StudySession, UndoChanges, UndoEntry,
};
use langualinker_application::{AppError, AppResult};
use langualinker_domain::card::{CardState, QueueState};
use langualinker_domain::deck::NewCardOrder;
use langualinker_domain::queue::{Candidate, DoneToday};
use langualinker_domain::review::{ReviewKind, ReviewLog};
use langualinker_domain::{
    CardId, DeckConfigId, DeckId, NoteId, ReviewLogId, SessionId, TimestampMs, ValidationError,
};
use rusqlite::types::Type;
use rusqlite::{OptionalExtension, Row, params, params_from_iter};

use super::{LOG_COLUMNS, SqliteRepos, decode_log, log_row, opt_ts, placeholders, ts};
use crate::sqlite::{codec, map_sqlite};

const SESSION_COLUMNS: &str = "id, deck_id, status, config_id, config_version, restricted,
    current_card_id, started_at, paused_at, finished_at, active_ms, updated_at";

fn text_code<T>(index: usize, text: String, parse: fn(&str) -> Option<T>) -> rusqlite::Result<T> {
    parse(&text).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            Type::Text,
            format!("unknown status {text}").into(),
        )
    })
}

fn session_row(row: &Row<'_>) -> rusqlite::Result<StudySession> {
    Ok(StudySession {
        id: SessionId(row.get(0)?),
        deck_id: row.get::<_, Option<i64>>(1)?.map(DeckId),
        status: text_code(2, row.get(2)?, SessionStatus::parse)?,
        config_id: row.get::<_, Option<i64>>(3)?.map(DeckConfigId),
        config_version: row.get(4)?,
        restricted: row.get(5)?,
        current_card_id: row.get::<_, Option<i64>>(6)?.map(CardId),
        started_at: ts(row, 7)?,
        paused_at: opt_ts(row, 8)?,
        finished_at: opt_ts(row, 9)?,
        active_ms: row.get(10)?,
        updated_at: ts(row, 11)?,
    })
}

fn candidate_row(row: &Row<'_>) -> rusqlite::Result<Candidate> {
    Ok(Candidate {
        card_id: CardId(row.get(0)?),
        note_id: NoteId(row.get(1)?),
        deck_id: DeckId(row.get(2)?),
        due_at: opt_ts(row, 3)?,
    })
}

/// Valid ratings made in this app (imported history does not count against today's limits).
fn counted_reviews() -> String {
    format!("l.invalidated_at IS NULL AND l.fsrs_version <> '{IMPORTED_SCHEDULER}'")
}

impl SqliteRepos<'_> {
    fn session_where(&self, condition: &str, args: &[i64]) -> AppResult<Option<StudySession>> {
        let sql = format!("SELECT {SESSION_COLUMNS} FROM study_sessions WHERE {condition}");
        self.tx
            .query_row(&sql, params_from_iter(args), session_row)
            .optional()
            .map_err(map_sqlite)
    }

    fn candidates(&self, sql: &str, args: Vec<i64>) -> AppResult<Vec<Candidate>> {
        let mut statement = self.tx.prepare_cached(sql).map_err(map_sqlite)?;
        statement
            .query_map(params_from_iter(args), candidate_row)
            .and_then(Iterator::collect)
            .map_err(map_sqlite)
    }
}

impl StudyRepository for SqliteRepos<'_> {
    fn insert_session(
        &mut self,
        session: &NewStudySession,
        at: TimestampMs,
    ) -> AppResult<StudySession> {
        self.tx
            .execute(
                "INSERT INTO study_sessions (deck_id, status, config_id, config_version, restricted,
                     started_at, created_at, updated_at)
                 VALUES (?1, 'active', ?2, ?3, ?4, ?5, ?5, ?5)",
                params![
                    session.deck_id.map(|id| id.0),
                    session.config_id.map(|id| id.0),
                    session.config_version,
                    session.restricted,
                    at.0,
                ],
            )
            .map_err(|error| match &error {
                rusqlite::Error::SqliteFailure(_, Some(message))
                    if message.contains("idx_study_sessions_open") =>
                {
                    ValidationError::new("sessionId", "open").into()
                }
                _ => map_sqlite(error),
            })?;
        let id = self.tx.last_insert_rowid();
        self.session_where("id = ?1", &[id])?
            .ok_or_else(|| AppError::Internal("inserted session vanished".into()))
    }

    fn session(&mut self, id: SessionId) -> AppResult<Option<StudySession>> {
        self.session_where("id = ?1", &[id.0])
    }

    fn open_session(&mut self) -> AppResult<Option<StudySession>> {
        self.session_where("status <> 'finished' ORDER BY id DESC LIMIT 1", &[])
    }

    fn update_session(&mut self, session: &StudySession, at: TimestampMs) -> AppResult<()> {
        let changed = self
            .tx
            .execute(
                "UPDATE study_sessions SET status = ?1, current_card_id = ?2, paused_at = ?3,
                     finished_at = ?4, active_ms = ?5, updated_at = ?6
                 WHERE id = ?7",
                params![
                    session.status.as_str(),
                    session.current_card_id.map(|id| id.0),
                    session.paused_at.map(|at| at.0),
                    session.finished_at.map(|at| at.0),
                    session.active_ms,
                    at.0,
                    session.id.0,
                ],
            )
            .map_err(|error| match &error {
                rusqlite::Error::SqliteFailure(_, Some(message))
                    if message.contains("idx_study_sessions_open") =>
                {
                    ValidationError::new("sessionId", "open").into()
                }
                _ => map_sqlite(error),
            })?;
        if changed == 0 {
            return Err(AppError::NotFound { entity: "session" });
        }
        Ok(())
    }

    fn session_items(&mut self, session: SessionId) -> AppResult<Vec<SessionItem>> {
        let mut statement = self
            .tx
            .prepare_cached(
                "SELECT position, card_id, status FROM session_items WHERE session_id = ?1
                 ORDER BY position",
            )
            .map_err(map_sqlite)?;
        statement
            .query_map([session.0], |row| {
                Ok(SessionItem {
                    position: row.get(0)?,
                    card_id: CardId(row.get(1)?),
                    status: text_code(2, row.get(2)?, SessionItemStatus::parse)?,
                })
            })
            .and_then(Iterator::collect)
            .map_err(map_sqlite)
    }

    fn put_session_item(
        &mut self,
        session: SessionId,
        card: CardId,
        status: SessionItemStatus,
    ) -> AppResult<()> {
        let changed = self
            .tx
            .execute(
                "UPDATE session_items SET status = ?1 WHERE session_id = ?2 AND card_id = ?3",
                params![status.as_str(), session.0, card.0],
            )
            .map_err(map_sqlite)?;
        if changed == 0 {
            self.tx
                .execute(
                    "INSERT INTO session_items (session_id, position, card_id, status)
                     SELECT ?1, COALESCE(MAX(position), -1) + 1, ?2, ?3
                     FROM session_items WHERE session_id = ?1",
                    params![session.0, card.0, status.as_str()],
                )
                .map_err(map_sqlite)?;
        }
        Ok(())
    }

    fn insert_undo_entry(
        &mut self,
        session: SessionId,
        review_log: ReviewLogId,
        changes: &UndoChanges,
        at: TimestampMs,
    ) -> AppResult<i64> {
        self.tx
            .execute(
                "INSERT INTO review_undo_entries (session_id, review_log_id, sibling_changes,
                     created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    session.0,
                    review_log.0,
                    codec::undo_changes_to_json(changes),
                    at.0
                ],
            )
            .map_err(map_sqlite)?;
        Ok(self.tx.last_insert_rowid())
    }

    fn last_undo_entry(&mut self, session: SessionId) -> AppResult<Option<UndoEntry>> {
        let row: Option<(UndoEntry, String)> = self
            .tx
            .query_row(
                "SELECT u.id, u.session_id, u.review_log_id, l.operation_id, u.sibling_changes,
                     u.created_at, u.consumed_at
                 FROM review_undo_entries u JOIN review_logs l ON l.id = u.review_log_id
                 WHERE u.session_id = ?1 ORDER BY u.id DESC LIMIT 1",
                [session.0],
                |row| {
                    Ok((
                        UndoEntry {
                            id: row.get(0)?,
                            session_id: SessionId(row.get(1)?),
                            review_log_id: ReviewLogId(row.get(2)?),
                            operation_id: row.get(3)?,
                            changes: UndoChanges::default(),
                            created_at: ts(row, 5)?,
                            consumed_at: opt_ts(row, 6)?,
                        },
                        row.get(4)?,
                    ))
                },
            )
            .optional()
            .map_err(map_sqlite)?;
        row.map(|(mut entry, json)| {
            entry.changes = codec::undo_changes_from_json(&json)?;
            Ok(entry)
        })
        .transpose()
    }

    fn consume_undo_entry(&mut self, id: i64, at: TimestampMs) -> AppResult<()> {
        let changed = self
            .tx
            .execute(
                "UPDATE review_undo_entries SET consumed_at = ?1
                 WHERE id = ?2 AND consumed_at IS NULL",
                params![at.0, id],
            )
            .map_err(map_sqlite)?;
        if changed == 0 {
            return Err(AppError::NotFound { entity: "undo" });
        }
        Ok(())
    }

    fn session_reviews(&mut self, session: SessionId) -> AppResult<Vec<ReviewLog>> {
        let sql = format!(
            "SELECT {LOG_COLUMNS} FROM review_logs
             WHERE session_id = ?1 AND invalidated_at IS NULL ORDER BY reviewed_at, id"
        );
        let mut statement = self.tx.prepare_cached(&sql).map_err(map_sqlite)?;
        let rows: Vec<_> = statement
            .query_map([session.0], log_row)
            .and_then(Iterator::collect)
            .map_err(map_sqlite)?;
        rows.into_iter().map(decode_log).collect()
    }

    fn session_review_count(&mut self, session: SessionId) -> AppResult<u32> {
        self.tx
            .query_row(
                "SELECT COUNT(*) FROM review_logs WHERE session_id = ?1 AND invalidated_at IS NULL",
                [session.0],
                |row| row.get::<_, i64>(0),
            )
            .map(|count| count.clamp(0, u32::MAX.into()) as u32)
            .map_err(map_sqlite)
    }
}

impl SchedulerQueries for SqliteRepos<'_> {
    fn collection_calendar(&mut self) -> AppResult<(String, u32)> {
        self.tx
            .query_row(
                "SELECT time_zone, day_cutoff_minutes FROM collection WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map(|row| row.unwrap_or_else(|| ("UTC".to_owned(), 240)))
            .map_err(map_sqlite)
    }

    fn done_since(&mut self, since: TimestampMs) -> AppResult<Vec<(DeckId, DoneToday)>> {
        let sql = format!(
            "SELECT c.deck_id,
                 SUM(json_extract(l.card_before, '$.state') = {new}),
                 SUM(l.kind = {review} OR json_extract(l.card_before, '$.queue') = {day_learn})
             FROM review_logs l JOIN cards c ON c.id = l.card_id
             WHERE l.reviewed_at >= ?1 AND {counted}
             GROUP BY c.deck_id",
            new = CardState::New.db_code(),
            review = ReviewKind::Review.db_code(),
            day_learn = QueueState::DayLearn.db_code(),
            counted = counted_reviews(),
        );
        let mut statement = self.tx.prepare_cached(&sql).map_err(map_sqlite)?;
        let count = |row: &Row<'_>, index: usize| -> rusqlite::Result<u32> {
            Ok(row
                .get::<_, Option<i64>>(index)?
                .unwrap_or(0)
                .clamp(0, u32::MAX.into()) as u32)
        };
        statement
            .query_map([since.0], |row| {
                Ok((
                    DeckId(row.get(0)?),
                    DoneToday {
                        new: count(row, 1)?,
                        review: count(row, 2)?,
                    },
                ))
            })
            .and_then(Iterator::collect)
            .map_err(map_sqlite)
    }

    fn notes_reviewed_since(&mut self, since: TimestampMs) -> AppResult<Vec<NoteId>> {
        let sql = format!(
            "SELECT DISTINCT c.note_id FROM review_logs l JOIN cards c ON c.id = l.card_id
             WHERE l.reviewed_at >= ?1 AND {}",
            counted_reviews()
        );
        let mut statement = self.tx.prepare_cached(&sql).map_err(map_sqlite)?;
        statement
            .query_map([since.0], |row| row.get(0).map(NoteId))
            .and_then(Iterator::collect)
            .map_err(map_sqlite)
    }

    fn learning_candidates(
        &mut self,
        decks: &[DeckId],
        until: TimestampMs,
    ) -> AppResult<Vec<Candidate>> {
        if decks.is_empty() {
            return Ok(Vec::new());
        }
        let sql = format!(
            "SELECT id, note_id, deck_id, due_at FROM cards
             WHERE deck_id IN ({}) AND queue = {} AND due_at < ?
               AND deleted_at IS NULL AND archived_at IS NULL
             ORDER BY due_at, id",
            placeholders(decks.len()),
            QueueState::Learning.db_code(),
        );
        let mut args: Vec<i64> = decks.iter().map(|deck| deck.0).collect();
        args.push(until.0);
        self.candidates(&sql, args)
    }

    fn review_candidates(
        &mut self,
        deck: DeckId,
        until: TimestampMs,
        limit: u32,
    ) -> AppResult<Vec<Candidate>> {
        let sql = format!(
            "SELECT id, note_id, deck_id, due_at FROM cards
             WHERE deck_id = ?1 AND queue IN ({}, {}) AND due_at < ?2
               AND deleted_at IS NULL AND archived_at IS NULL
             ORDER BY due_at, id LIMIT ?3",
            QueueState::Review.db_code(),
            QueueState::DayLearn.db_code(),
        );
        self.candidates(&sql, vec![deck.0, until.0, i64::from(limit)])
    }

    fn new_candidates(
        &mut self,
        deck: DeckId,
        order: NewCardOrder,
        day: i64,
        limit: u32,
    ) -> AppResult<Vec<Candidate>> {
        // Random order is a per-day permutation (multiplicative hash of the id), stable all day.
        let (order_by, args) = match order {
            NewCardOrder::Position => ("position, id LIMIT ?2", vec![deck.0, i64::from(limit)]),
            NewCardOrder::Random => (
                "(id * 2654435761 + ?2 * 40503) % 4294967311, id LIMIT ?3",
                vec![deck.0, day, i64::from(limit)],
            ),
        };
        let sql = format!(
            "SELECT id, note_id, deck_id, due_at FROM cards
             WHERE deck_id = ?1 AND queue = {} AND deleted_at IS NULL AND archived_at IS NULL
             ORDER BY {order_by}",
            QueueState::New.db_code(),
        );
        self.candidates(&sql, args)
    }

    fn expired_buried_cards(&mut self, now: TimestampMs) -> AppResult<Vec<CardId>> {
        let mut statement = self
            .tx
            .prepare_cached(&format!(
                "SELECT id FROM cards
                 WHERE queue IN ({}, {}) AND (buried_until IS NULL OR buried_until <= ?1)
                   AND deleted_at IS NULL
                 ORDER BY id",
                QueueState::SchedBuried.db_code(),
                QueueState::UserBuried.db_code(),
            ))
            .map_err(map_sqlite)?;
        statement
            .query_map([now.0], |row| row.get(0).map(CardId))
            .and_then(Iterator::collect)
            .map_err(map_sqlite)
    }
}

#[cfg(test)]
mod tests {
    //! Repository tests of study sessions, session items, undo entries and the scheduler's read
    //! models on an in-memory database.

    use langualinker_application::ports::Clock;
    use langualinker_application::repositories::{Repositories, SiblingChange, UnitOfWork};
    use langualinker_domain::card::{Card, CardState, NewCard, QueueState};
    use langualinker_domain::deck::{
        DeckType, NativeLanguage, NewCardOrder, NewDeck, TargetLanguage,
    };
    use langualinker_domain::note::{NewNote, NoteFields, NoteTypeKind};
    use langualinker_domain::review::{CardSnapshot, Rating, ReviewKind, ReviewLog};
    use langualinker_domain::{DeckConfigId, Guid, NoteId, ReviewLogId};
    use rusqlite::params;

    use super::*;
    use crate::sqlite::SqliteDatabase;

    struct FixedClock;

    impl Clock for FixedClock {
        fn now(&self) -> TimestampMs {
            T0
        }
    }

    const T0: TimestampMs = TimestampMs(1_772_438_400_000);
    const DAY: i64 = TimestampMs::MS_PER_DAY;

    fn db() -> SqliteDatabase {
        SqliteDatabase::open_in_memory(&FixedClock)
            .unwrap()
            .database
    }

    fn guid(n: u32) -> Guid {
        let mut bytes = [0u8; 16];
        bytes[..4].copy_from_slice(&n.to_be_bytes());
        bytes[15] = 0x77;
        Guid::from_bytes(bytes)
    }

    /// One deck with `count` basic notes, each with its cards. Returns the deck id and the cards.
    fn setup(tx: &mut dyn Repositories, count: u32) -> AppResult<(DeckId, Vec<Card>)> {
        let deck = tx.insert_deck(
            &NewDeck {
                guid: guid(0),
                name: "Deutsch".into(),
                parent_id: None,
                description: String::new(),
                native_language: NativeLanguage::Pl,
                target_language: TargetLanguage::De,
                deck_type: DeckType::Linguistic,
                config_id: DeckConfigId(1),
                content_version: None,
                bundled: false,
            },
            T0,
        )?;
        let note_type = tx.note_type(NoteTypeKind::Basic)?;
        let templates = tx.templates(note_type.id)?;
        let mut cards = Vec::new();
        for n in 1..=count {
            let note = tx.insert_note(
                &NewNote {
                    guid: guid(n),
                    note_type_id: note_type.id,
                    fields: NoteFields {
                        front: format!("Wort {n}"),
                        back: "słowo".into(),
                    },
                    tags: Vec::new(),
                },
                T0,
            )?;
            for template in &templates {
                let position = tx.next_position()?;
                let card = tx.insert_card(
                    &NewCard {
                        guid: guid(1_000 + n * 10 + template.ordinal),
                        note_id: note.id,
                        deck_id: deck.id,
                        template_ordinal: template.ordinal,
                        position,
                    },
                    T0,
                )?;
                cards.push(card);
            }
        }
        Ok((deck.id, cards))
    }

    fn reload(tx: &mut dyn Repositories, card: CardId) -> AppResult<Card> {
        Ok(tx.card(card)?.expect("card exists"))
    }

    /// Puts a card into a queue with the given due and buried times; returns the stored card.
    fn place(
        tx: &mut dyn Repositories,
        card: CardId,
        queue: QueueState,
        due: Option<i64>,
        buried_until: Option<i64>,
    ) -> AppResult<Card> {
        let mut current = reload(tx, card)?;
        current.queue = queue;
        current.state = match queue {
            QueueState::Review | QueueState::SchedBuried | QueueState::UserBuried => {
                CardState::Review
            }
            QueueState::Learning | QueueState::DayLearn => CardState::Learning,
            _ => CardState::New,
        };
        current.due_at = due.map(TimestampMs);
        current.buried_until = buried_until.map(TimestampMs);
        tx.update_card(&current, T0)
    }

    fn session(tx: &mut dyn Repositories, deck: DeckId) -> AppResult<StudySession> {
        tx.insert_session(
            &NewStudySession {
                deck_id: Some(deck),
                config_id: Some(DeckConfigId(1)),
                config_version: Some(1),
                restricted: false,
            },
            T0,
        )
    }

    fn log(
        card: &Card,
        operation: &str,
        session: Option<SessionId>,
        rating: Rating,
        kind: ReviewKind,
        reviewed_at: TimestampMs,
        before: CardSnapshot,
    ) -> ReviewLog {
        ReviewLog {
            id: ReviewLogId(0),
            card_id: card.id,
            session_id: session,
            operation_id: operation.into(),
            rating,
            kind,
            reviewed_at,
            duration_ms: 1_000,
            card_before: before,
            card_after: CardSnapshot::of(card),
            fsrs_version: "FSRS-6".into(),
            parameters_hash: "hash".into(),
            invalidated_at: None,
        }
    }

    fn ids(cards: &[Candidate]) -> Vec<CardId> {
        cards.iter().map(|candidate| candidate.card_id).collect()
    }

    #[test]
    fn a_session_is_stored_and_read_back_with_its_defaults() {
        let db = db();
        db.write(|tx| {
            let (deck, _) = setup(tx, 1)?;
            let stored = session(tx, deck)?;
            assert_eq!(stored.status, SessionStatus::Active);
            assert_eq!(stored.deck_id, Some(deck));
            assert_eq!(stored.config_id, Some(DeckConfigId(1)));
            assert_eq!(stored.config_version, Some(1));
            assert!(!stored.restricted);
            assert_eq!(stored.current_card_id, None);
            assert_eq!(stored.started_at, T0);
            assert_eq!(stored.active_ms, 0);
            assert_eq!(tx.session(stored.id)?, Some(stored.clone()));
            assert_eq!(tx.open_session()?, Some(stored));
            assert_eq!(tx.session(SessionId(9_999))?, None);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn only_one_session_may_be_open_until_it_is_finished() {
        let db = db();
        db.write(|tx| {
            let (deck, _) = setup(tx, 1)?;
            let first = session(tx, deck)?;
            let error = tx
                .insert_session(
                    &NewStudySession {
                        deck_id: Some(deck),
                        config_id: None,
                        config_version: None,
                        restricted: false,
                    },
                    T0,
                )
                .unwrap_err();
            assert_eq!(
                error,
                AppError::Validation(ValidationError::new("sessionId", "open"))
            );

            let mut finished = first.clone();
            finished.status = SessionStatus::Finished;
            finished.finished_at = Some(T0);
            tx.update_session(&finished, T0)?;
            assert_eq!(tx.open_session()?, None);
            assert_eq!(session(tx, deck)?.status, SessionStatus::Active);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn a_paused_session_is_still_the_open_one() {
        let db = db();
        db.write(|tx| {
            let (deck, cards) = setup(tx, 1)?;
            let mut stored = session(tx, deck)?;
            stored.status = SessionStatus::Paused;
            stored.paused_at = Some(T0.plus_ms(60_000));
            stored.current_card_id = Some(cards[0].id);
            stored.active_ms = 4_500;
            tx.update_session(&stored, T0.plus_ms(60_000))?;

            let open = tx.open_session()?.expect("paused session is open");
            assert_eq!(open.status, SessionStatus::Paused);
            assert_eq!(open.paused_at, Some(T0.plus_ms(60_000)));
            assert_eq!(open.current_card_id, Some(cards[0].id));
            assert_eq!(open.active_ms, 4_500);
            assert_eq!(open.updated_at, T0.plus_ms(60_000));
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn updating_a_missing_session_is_not_found() {
        let db = db();
        db.write(|tx| {
            let (deck, _) = setup(tx, 1)?;
            let mut stored = session(tx, deck)?;
            stored.id = SessionId(9_999);
            assert_eq!(
                tx.update_session(&stored, T0).unwrap_err(),
                AppError::NotFound { entity: "session" }
            );
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn session_items_keep_the_order_they_were_met_and_update_in_place() {
        let db = db();
        db.write(|tx| {
            let (deck, cards) = setup(tx, 3)?;
            let stored = session(tx, deck)?;
            tx.put_session_item(stored.id, cards[2].id, SessionItemStatus::Pending)?;
            tx.put_session_item(stored.id, cards[0].id, SessionItemStatus::Pending)?;
            tx.put_session_item(stored.id, cards[2].id, SessionItemStatus::Answered)?;

            let items = tx.session_items(stored.id)?;
            assert_eq!(
                items,
                vec![
                    SessionItem {
                        position: 0,
                        card_id: cards[2].id,
                        status: SessionItemStatus::Answered,
                    },
                    SessionItem {
                        position: 1,
                        card_id: cards[0].id,
                        status: SessionItemStatus::Pending,
                    },
                ]
            );
            assert!(tx.session_items(SessionId(9_999))?.is_empty());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn items_of_one_session_do_not_leak_into_another() {
        let db = db();
        db.write(|tx| {
            let (deck, cards) = setup(tx, 2)?;
            let first = session(tx, deck)?;
            let mut finished = first.clone();
            finished.status = SessionStatus::Finished;
            finished.finished_at = Some(T0);
            tx.update_session(&finished, T0)?;
            let second = session(tx, deck)?;
            tx.put_session_item(first.id, cards[0].id, SessionItemStatus::Pending)?;
            tx.put_session_item(second.id, cards[0].id, SessionItemStatus::Skipped)?;

            assert_eq!(tx.session_items(first.id)?.len(), 1);
            assert_eq!(
                tx.session_items(first.id)?[0].status,
                SessionItemStatus::Pending
            );
            assert_eq!(
                tx.session_items(second.id)?[0].status,
                SessionItemStatus::Skipped
            );
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn the_latest_undo_entry_is_returned_with_its_changes_and_is_consumed_once() {
        let db = db();
        db.write(|tx| {
            let (deck, cards) = setup(tx, 2)?;
            let stored = session(tx, deck)?;
            let before = CardSnapshot::of(&cards[0]);
            let first = tx.insert_review_log(&log(
                &cards[0],
                "op-1",
                Some(stored.id),
                Rating::Good,
                ReviewKind::Learning,
                T0,
                before.clone(),
            ))?;
            let second = tx.insert_review_log(&log(
                &cards[1],
                "op-2",
                Some(stored.id),
                Rating::Again,
                ReviewKind::Learning,
                T0,
                CardSnapshot::of(&cards[1]),
            ))?;
            let changes = UndoChanges {
                siblings: vec![SiblingChange {
                    card_id: cards[1].id,
                    before: CardSnapshot::of(&cards[1]),
                    after_version: 3,
                }],
                leech_tagged: Some((NoteId(1), 5)),
            };
            let first_entry = tx.insert_undo_entry(stored.id, first.id, &changes, T0)?;
            let second_entry =
                tx.insert_undo_entry(stored.id, second.id, &UndoChanges::default(), T0.plus_ms(1))?;
            assert!(second_entry > first_entry);

            let last = tx.last_undo_entry(stored.id)?.expect("an entry exists");
            assert_eq!(last.id, second_entry);
            assert_eq!(last.operation_id, "op-2");
            assert_eq!(last.review_log_id, second.id);
            assert_eq!(last.changes, UndoChanges::default());
            assert_eq!(last.consumed_at, None);

            tx.consume_undo_entry(second_entry, T0.plus_ms(2))?;
            let consumed = tx.last_undo_entry(stored.id)?.expect("entry kept");
            assert_eq!(consumed.consumed_at, Some(T0.plus_ms(2)));
            assert_eq!(
                tx.consume_undo_entry(second_entry, T0.plus_ms(3))
                    .unwrap_err(),
                AppError::NotFound { entity: "undo" }
            );

            let older = tx.last_undo_entry(stored.id)?;
            assert_eq!(older.map(|entry| entry.id), Some(second_entry));
            assert!(tx.last_undo_entry(SessionId(9_999))?.is_none());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn an_undo_entry_round_trips_sibling_and_leech_changes() {
        let db = db();
        db.write(|tx| {
            let (deck, cards) = setup(tx, 1)?;
            let stored = session(tx, deck)?;
            let review = tx.insert_review_log(&log(
                &cards[0],
                "op-1",
                Some(stored.id),
                Rating::Again,
                ReviewKind::Review,
                T0,
                CardSnapshot::of(&cards[0]),
            ))?;
            let changes = UndoChanges {
                siblings: vec![SiblingChange {
                    card_id: cards[0].id,
                    before: CardSnapshot::of(&cards[0]),
                    after_version: 2,
                }],
                leech_tagged: Some((NoteId(1), 4)),
            };
            tx.insert_undo_entry(stored.id, review.id, &changes, T0)?;
            let entry = tx.last_undo_entry(stored.id)?.expect("entry");
            assert_eq!(entry.changes, changes);
            assert_eq!(entry.created_at, T0);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn session_reviews_and_count_leave_out_undone_ratings() {
        let db = db();
        db.write(|tx| {
            let (deck, cards) = setup(tx, 3)?;
            let stored = session(tx, deck)?;
            assert_eq!(tx.session_review_count(stored.id)?, 0);
            for (index, card) in cards.iter().enumerate() {
                tx.insert_review_log(&log(
                    card,
                    &format!("op-{index}"),
                    Some(stored.id),
                    Rating::Good,
                    ReviewKind::Learning,
                    T0.plus_ms(index as i64 * 1_000),
                    CardSnapshot::of(card),
                ))?;
            }
            assert_eq!(tx.session_review_count(stored.id)?, 3);

            tx.invalidate_review_log("op-1", T0.plus_ms(10_000))?;
            assert_eq!(tx.session_review_count(stored.id)?, 2);
            let reviews = tx.session_reviews(stored.id)?;
            let operations: Vec<&str> = reviews.iter().map(|r| r.operation_id.as_str()).collect();
            assert_eq!(operations, vec!["op-0", "op-2"]);
            assert_eq!(tx.session_review_count(SessionId(9_999))?, 0);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn the_collection_calendar_defaults_to_utc_and_follows_the_stored_zone() {
        let db = db();
        let default = db.write(|tx| tx.collection_calendar()).unwrap();
        assert_eq!(default.0, "UTC");
        assert_eq!(default.1, 240);

        db.with_connection(|conn| {
            conn.execute(
                "UPDATE collection SET time_zone = ?1, day_cutoff_minutes = ?2 WHERE id = 1",
                params!["Europe/Warsaw", 180],
            )
            .map_err(map_sqlite)?;
            Ok(())
        })
        .unwrap();
        let stored = db.write(|tx| tx.collection_calendar()).unwrap();
        assert_eq!(stored, ("Europe/Warsaw".to_owned(), 180));
    }

    #[test]
    fn learning_candidates_are_due_learning_cards_of_the_decks_by_due_time() {
        let db = db();
        db.write(|tx| {
            let (deck, cards) = setup(tx, 4)?;
            place(
                tx,
                cards[0].id,
                QueueState::Learning,
                Some(T0.0 + 300),
                None,
            )?;
            place(
                tx,
                cards[1].id,
                QueueState::Learning,
                Some(T0.0 + 100),
                None,
            )?;
            // Not yet due before `until`.
            place(
                tx,
                cards[2].id,
                QueueState::Learning,
                Some(T0.0 + 900),
                None,
            )?;
            // Review queue never counts as learning.
            place(tx, cards[3].id, QueueState::Review, Some(T0.0 + 50), None)?;

            let until = TimestampMs(T0.0 + 500);
            let found = tx.learning_candidates(&[deck], until)?;
            assert_eq!(ids(&found), vec![cards[1].id, cards[0].id]);
            assert_eq!(found[0].deck_id, deck);
            assert_eq!(found[0].note_id, cards[1].note_id);

            assert!(tx.learning_candidates(&[], until)?.is_empty());
            assert!(tx.learning_candidates(&[DeckId(9_999)], until)?.is_empty());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn review_candidates_take_review_and_day_learning_cards_up_to_the_limit() {
        let db = db();
        db.write(|tx| {
            let (deck, cards) = setup(tx, 5)?;
            place(tx, cards[0].id, QueueState::Review, Some(T0.0 + 400), None)?;
            place(
                tx,
                cards[1].id,
                QueueState::DayLearn,
                Some(T0.0 + 200),
                None,
            )?;
            place(tx, cards[2].id, QueueState::Review, Some(T0.0 + 100), None)?;
            place(
                tx,
                cards[3].id,
                QueueState::Review,
                Some(T0.0 + 9_000),
                None,
            )?;
            place(tx, cards[4].id, QueueState::Learning, Some(T0.0 + 50), None)?;

            let until = TimestampMs(T0.0 + 1_000);
            let all = tx.review_candidates(deck, until, 10)?;
            assert_eq!(ids(&all), vec![cards[2].id, cards[1].id, cards[0].id]);

            let limited = tx.review_candidates(deck, until, 2)?;
            assert_eq!(ids(&limited), vec![cards[2].id, cards[1].id]);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn new_candidates_follow_the_position_or_a_stable_daily_shuffle() {
        let db = db();
        db.write(|tx| {
            let (deck, cards) = setup(tx, 6)?;
            // Cards created in order have ascending positions, so position order is creation order.
            let by_position = tx.new_candidates(deck, NewCardOrder::Position, 1, 4)?;
            assert_eq!(ids(&by_position), ids_of(&cards[..4]));

            let monday = tx.new_candidates(deck, NewCardOrder::Random, 20_000, 6)?;
            let again = tx.new_candidates(deck, NewCardOrder::Random, 20_000, 6)?;
            assert_eq!(ids(&monday), ids(&again), "a day's order does not change");
            let mut sorted = ids(&monday);
            sorted.sort();
            assert_eq!(sorted, ids_of(&cards), "shuffle keeps every card once");

            let limited = tx.new_candidates(deck, NewCardOrder::Random, 20_000, 2)?;
            assert_eq!(ids(&limited), ids(&monday)[..2].to_vec());
            Ok(())
        })
        .unwrap();
    }

    fn ids_of(cards: &[Card]) -> Vec<CardId> {
        cards.iter().map(|card| card.id).collect()
    }

    #[test]
    fn new_candidates_skip_cards_that_already_left_the_new_queue() {
        let db = db();
        db.write(|tx| {
            let (deck, cards) = setup(tx, 3)?;
            place(tx, cards[0].id, QueueState::Suspended, None, None)?;
            let mut archived = reload(tx, cards[1].id)?;
            archived.archived_at = Some(T0);
            tx.update_card(&archived, T0)?;

            let found = tx.new_candidates(deck, NewCardOrder::Position, 1, 10)?;
            assert_eq!(ids(&found), vec![cards[2].id]);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn expired_buried_cards_are_those_whose_burial_has_passed_or_has_no_end() {
        let db = db();
        db.write(|tx| {
            let (_, cards) = setup(tx, 4)?;
            let past = T0.0 - 1;
            place(
                tx,
                cards[0].id,
                QueueState::SchedBuried,
                Some(T0.0),
                Some(past),
            )?;
            place(tx, cards[1].id, QueueState::UserBuried, Some(T0.0), None)?;
            place(
                tx,
                cards[2].id,
                QueueState::SchedBuried,
                Some(T0.0),
                Some(T0.0 + DAY),
            )?;
            // Not buried at all.
            place(tx, cards[3].id, QueueState::Review, Some(T0.0), None)?;

            let expired = tx.expired_buried_cards(T0)?;
            assert_eq!(expired, vec![cards[0].id, cards[1].id]);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn done_since_counts_new_and_review_answers_by_deck_without_invalid_or_imported_ones() {
        let db = db();
        db.write(|tx| {
            let (deck, cards) = setup(tx, 4)?;
            let session_id = session(tx, deck)?.id;
            let new_card = reload(tx, cards[0].id)?;
            let review_card = place(tx, cards[1].id, QueueState::Review, Some(T0.0), None)?;
            let day_learn_card = place(tx, cards[2].id, QueueState::DayLearn, Some(T0.0), None)?;
            let imported = place(tx, cards[3].id, QueueState::Review, Some(T0.0), None)?;

            let new_before = CardSnapshot::of(&new_card);
            let mut review_before_snapshot = CardSnapshot::of(&review_card);
            review_before_snapshot.queue = QueueState::Review;
            let mut day_before = CardSnapshot::of(&day_learn_card);
            day_before.queue = QueueState::DayLearn;

            tx.insert_review_log(&log(
                &new_card,
                "new",
                Some(session_id),
                Rating::Good,
                ReviewKind::Learning,
                T0,
                new_before,
            ))?;
            tx.insert_review_log(&log(
                &review_card,
                "review",
                Some(session_id),
                Rating::Good,
                ReviewKind::Review,
                T0,
                review_before_snapshot,
            ))?;
            tx.insert_review_log(&log(
                &day_learn_card,
                "day",
                Some(session_id),
                Rating::Hard,
                ReviewKind::Learning,
                T0,
                day_before,
            ))?;
            let mut imported_log = log(
                &imported,
                "imported",
                None,
                Rating::Good,
                ReviewKind::Review,
                T0,
                CardSnapshot::of(&imported),
            );
            imported_log.fsrs_version = IMPORTED_SCHEDULER.into();
            tx.insert_review_log(&imported_log)?;
            // Invalidated before the window end: not counted.
            tx.insert_review_log(&log(
                &new_card,
                "undone",
                Some(session_id),
                Rating::Easy,
                ReviewKind::Learning,
                T0,
                CardSnapshot::of(&new_card),
            ))?;
            tx.invalidate_review_log("undone", T0)?;
            // Before `since`: not counted.
            tx.insert_review_log(&log(
                &new_card,
                "old",
                Some(session_id),
                Rating::Good,
                ReviewKind::Learning,
                T0.plus_ms(-DAY),
                CardSnapshot::of(&new_card),
            ))?;

            let counts = tx.done_since(T0)?;
            assert_eq!(counts.len(), 1);
            assert_eq!(counts[0].0, deck);
            assert_eq!(counts[0].1.new, 1);
            assert_eq!(
                counts[0].1.review, 2,
                "review kind and day-learning both count"
            );
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn notes_reviewed_since_lists_each_note_once() {
        let db = db();
        db.write(|tx| {
            let (deck, cards) = setup(tx, 2)?;
            let session_id = session(tx, deck)?.id;
            for (operation, card) in [("a", &cards[0]), ("b", &cards[0]), ("c", &cards[1])] {
                tx.insert_review_log(&log(
                    card,
                    operation,
                    Some(session_id),
                    Rating::Good,
                    ReviewKind::Learning,
                    T0,
                    CardSnapshot::of(card),
                ))?;
            }
            tx.invalidate_review_log("c", T0)?;

            assert_eq!(tx.notes_reviewed_since(T0)?, vec![cards[0].note_id]);
            assert!(tx.notes_reviewed_since(T0.plus_ms(1))?.is_empty());
            Ok(())
        })
        .unwrap();
    }
}
