//! Repository implementations over one SQLite transaction. All SQL is parameterized; table and
//! column names are constants.

use langualinker_application::jobs::{
    JobFailure, JobId, JobKind, JobProgress, JobRecord, JobStatus, NewJob,
};
use langualinker_application::repositories::{
    CardRepository, ContentRepository, DeckConfigRepository, DeckRepository, IdempotencyRepository,
    JobRepository, NoteRepository, NoteTypeRepository, RecordedOperation, ReviewLogRepository,
    SettingsRepository,
};
use langualinker_application::{AppError, AppResult};
use langualinker_domain::card::{Card, CardFlag, CardFsrsState, CardState, NewCard, QueueState};
use langualinker_domain::content::{
    Association, AssociationProvenance, GrammarTip, ModerationStatus, NewAssociation,
    SentenceWordLink, TipProvenance, WordLinkResolution,
};
use langualinker_domain::deck::{
    Deck, DeckConfig, DeckConfigOptions, DeckType, LeechAction, NativeLanguage, NewCardOrder,
    NewDeck, TargetLanguage,
};
use langualinker_domain::note::{
    CardDirection, CardTemplate, NewNote, Note, NoteType, NoteTypeKind,
};
use langualinker_domain::review::{Rating, ReviewKind, ReviewLog};
use langualinker_domain::{
    AssociationId, CardId, DeckConfigId, DeckId, GrammarTipId, Guid, NoteId, NoteTypeId,
    ReviewLogId, SessionId, TimestampMs, UnknownCode, ValidationError, WordLinkId,
};
use rusqlite::types::Type;
use rusqlite::{OptionalExtension, Row, Transaction, params, params_from_iter};

use super::{codec, map_sqlite};

mod bundled;
mod management;
mod stats;
mod study;

pub(crate) use stats::collection_counts;

/// Update sequence number for local changes not yet synchronized (Anki convention).
const LOCAL_USN: i64 = -1;

pub struct SqliteRepos<'a> {
    tx: &'a Transaction<'a>,
}

impl<'a> SqliteRepos<'a> {
    pub fn new(tx: &'a Transaction<'a>) -> Self {
        Self { tx }
    }
}

fn code<T>(index: usize, value: Result<T, UnknownCode>) -> rusqlite::Result<T> {
    value.map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(index, Type::Integer, Box::new(error))
    })
}

fn guid(row: &Row<'_>, index: usize) -> rusqlite::Result<Guid> {
    let text: String = row.get(index)?;
    Guid::parse(&text).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
    })
}

fn ts(row: &Row<'_>, index: usize) -> rusqlite::Result<TimestampMs> {
    row.get(index).map(TimestampMs)
}

fn opt_ts(row: &Row<'_>, index: usize) -> rusqlite::Result<Option<TimestampMs>> {
    row.get::<_, Option<i64>>(index)
        .map(|value| value.map(TimestampMs))
}

fn is_unique_violation(error: &rusqlite::Error, column: &str) -> bool {
    matches!(error, rusqlite::Error::SqliteFailure(failure, Some(message))
        if matches!(failure.extended_code, rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE | rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY)
            && message.contains(column))
}

fn placeholders(count: usize) -> String {
    vec!["?"; count].join(", ")
}

impl SqliteRepos<'_> {
    /// Explains why a versioned update matched no row.
    fn version_failure(&self, table: &'static str, entity: &'static str, id: i64) -> AppError {
        let sql = format!("SELECT version, deleted_at FROM {table} WHERE id = ?1");
        let row: Option<(i64, Option<i64>)> = match self
            .tx
            .query_row(&sql, [id], |row| Ok((row.get(0)?, row.get(1)?)))
            .optional()
        {
            Ok(row) => row,
            Err(error) => return map_sqlite(error),
        };
        match row {
            None => AppError::NotFound { entity },
            Some((_, Some(_))) => AppError::Deleted { entity },
            Some((current, None)) => AppError::VersionConflict { entity, current },
        }
    }

    fn note_tags(&self, note: NoteId) -> AppResult<Vec<String>> {
        let mut statement = self
            .tx
            .prepare_cached(
                "SELECT t.name FROM note_tags nt JOIN tags t ON t.id = nt.tag_id
                 WHERE nt.note_id = ?1 ORDER BY nt.position",
            )
            .map_err(map_sqlite)?;
        statement
            .query_map([note.0], |row| row.get(0))
            .and_then(Iterator::collect)
            .map_err(map_sqlite)
    }

    fn replace_note_tags(&self, note: NoteId, tags: &[String], at: TimestampMs) -> AppResult<()> {
        self.tx
            .execute("DELETE FROM note_tags WHERE note_id = ?1", [note.0])
            .map_err(map_sqlite)?;
        for (position, tag) in tags.iter().enumerate() {
            self.tx
                .execute(
                    "INSERT INTO tags (name, created_at, usn) VALUES (?1, ?2, ?3)
                     ON CONFLICT (name) DO NOTHING",
                    params![tag, at.0, LOCAL_USN],
                )
                .map_err(map_sqlite)?;
            self.tx
                .execute(
                    "INSERT OR IGNORE INTO note_tags (note_id, tag_id, position)
                     SELECT ?1, id, ?2 FROM tags WHERE name = ?3",
                    params![note.0, position as i64, tag],
                )
                .map_err(map_sqlite)?;
        }
        Ok(())
    }
}

// ------------------------------------------------------------------ decks

const DECK_COLUMNS: &str = "d.id, d.guid, d.name, d.parent_id, d.description, d.native_language,
    d.target_language, s.deck_type, d.config_id, d.content_version, d.bundled, d.created_at,
    d.updated_at, d.deleted_at, d.usn, d.version";

fn deck_row(row: &Row<'_>) -> rusqlite::Result<Deck> {
    Ok(Deck {
        id: DeckId(row.get(0)?),
        guid: guid(row, 1)?,
        name: row.get(2)?,
        parent_id: row.get::<_, Option<i64>>(3)?.map(DeckId),
        description: row.get(4)?,
        native_language: code(5, NativeLanguage::from_db_code(row.get(5)?))?,
        target_language: code(6, TargetLanguage::from_db_code(row.get(6)?))?,
        deck_type: code(7, DeckType::from_db_code(row.get(7)?))?,
        config_id: DeckConfigId(row.get(8)?),
        content_version: row.get(9)?,
        bundled: row.get(10)?,
        created_at: ts(row, 11)?,
        updated_at: ts(row, 12)?,
        deleted_at: opt_ts(row, 13)?,
        usn: row.get(14)?,
        version: row.get(15)?,
    })
}

impl DeckRepository for SqliteRepos<'_> {
    fn insert_deck(&mut self, deck: &NewDeck, at: TimestampMs) -> AppResult<Deck> {
        let deck = deck.clone().validated()?;
        if let Some(parent) = deck.parent_id {
            match self.deck(parent)? {
                Some(parent) if parent.deleted_at.is_none() => {}
                _ => return Err(ValidationError::new("parentId", "notFound").into()),
            }
        }
        self.tx
            .execute(
                "INSERT INTO decks (guid, name, parent_id, description, native_language,
                     target_language, config_id, content_version, bundled, created_at, updated_at, usn)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10, ?11)",
                params![
                    deck.guid.as_str(),
                    deck.name,
                    deck.parent_id.map(|id| id.0),
                    deck.description,
                    deck.native_language.db_code(),
                    deck.target_language.db_code(),
                    deck.config_id.0,
                    deck.content_version,
                    deck.bundled,
                    at.0,
                    LOCAL_USN,
                ],
            )
            .map_err(map_sqlite)?;
        let id = DeckId(self.tx.last_insert_rowid());
        self.tx
            .execute(
                "INSERT INTO deck_settings (deck_id, deck_type, updated_at, usn) VALUES (?1, ?2, ?3, ?4)",
                params![id.0, deck.deck_type.db_code(), at.0, LOCAL_USN],
            )
            .map_err(map_sqlite)?;
        self.deck(id)?
            .ok_or_else(|| AppError::Internal("inserted deck vanished".into()))
    }

    fn deck(&mut self, id: DeckId) -> AppResult<Option<Deck>> {
        let sql = format!(
            "SELECT {DECK_COLUMNS} FROM decks d JOIN deck_settings s ON s.deck_id = d.id WHERE d.id = ?1"
        );
        self.tx
            .query_row(&sql, [id.0], deck_row)
            .optional()
            .map_err(map_sqlite)
    }

    fn active_decks(&mut self) -> AppResult<Vec<Deck>> {
        let sql = format!(
            "SELECT {DECK_COLUMNS} FROM decks d JOIN deck_settings s ON s.deck_id = d.id
             WHERE d.deleted_at IS NULL ORDER BY d.name COLLATE NOCASE, d.id"
        );
        let mut statement = self.tx.prepare_cached(&sql).map_err(map_sqlite)?;
        statement
            .query_map([], deck_row)
            .and_then(Iterator::collect)
            .map_err(map_sqlite)
    }

    fn update_deck(&mut self, deck: &Deck, at: TimestampMs) -> AppResult<Deck> {
        let name = langualinker_domain::validation::required_text(
            "name",
            &deck.name,
            langualinker_domain::validation::MAX_NAME_CHARS,
        )?;
        let description = langualinker_domain::validation::bounded_text(
            "description",
            &deck.description,
            langualinker_domain::validation::MAX_DESCRIPTION_CHARS,
        )?;
        let tx = self.tx;
        langualinker_domain::deck::check_reparent(deck.id, deck.parent_id, |id| {
            tx.query_row("SELECT parent_id FROM decks WHERE id = ?1", [id.0], |row| {
                row.get::<_, Option<i64>>(0)
            })
            .ok()
            .flatten()
            .map(DeckId)
        })?;
        let changed = self
            .tx
            .execute(
                "UPDATE decks SET name = ?1, parent_id = ?2, description = ?3, target_language = ?4,
                     config_id = ?5, content_version = ?6, updated_at = ?7, usn = ?8,
                     version = version + 1
                 WHERE id = ?9 AND version = ?10 AND deleted_at IS NULL",
                params![
                    name,
                    deck.parent_id.map(|id| id.0),
                    description,
                    deck.target_language.db_code(),
                    deck.config_id.0,
                    deck.content_version,
                    at.0,
                    LOCAL_USN,
                    deck.id.0,
                    deck.version,
                ],
            )
            .map_err(map_sqlite)?;
        if changed == 0 {
            return Err(self.version_failure("decks", "deck", deck.id.0));
        }
        self.tx
            .execute(
                "UPDATE deck_settings SET deck_type = ?1, updated_at = ?2, usn = ?3 WHERE deck_id = ?4",
                params![deck.deck_type.db_code(), at.0, LOCAL_USN, deck.id.0],
            )
            .map_err(map_sqlite)?;
        self.deck(deck.id)?
            .ok_or(AppError::NotFound { entity: "deck" })
    }

    fn soft_delete_deck(
        &mut self,
        id: DeckId,
        expected_version: i64,
        at: TimestampMs,
    ) -> AppResult<()> {
        let changed = self
            .tx
            .execute(
                "UPDATE decks SET deleted_at = ?1, updated_at = ?1, usn = ?2, version = version + 1
                 WHERE id = ?3 AND version = ?4 AND deleted_at IS NULL",
                params![at.0, LOCAL_USN, id.0, expected_version],
            )
            .map_err(map_sqlite)?;
        if changed == 0 {
            return Err(self.version_failure("decks", "deck", id.0));
        }
        Ok(())
    }

    fn installed_content_version(&mut self) -> AppResult<Option<String>> {
        self.tx
            .query_row(
                "SELECT MAX(content_version) FROM decks WHERE bundled = 1 AND deleted_at IS NULL",
                [],
                |row| row.get(0),
            )
            .map_err(map_sqlite)
    }
}

// ------------------------------------------------------------------ presets

const CONFIG_COLUMNS: &str = "id, guid, name, built_in, new_per_day, reviews_per_day,
    desired_retention, maximum_interval_days, learning_steps_minutes, relearning_steps_minutes,
    fsrs_weights, fuzz, new_card_order, bury_siblings, leech_threshold, leech_action, created_at,
    updated_at, deleted_at, usn, version";

struct ConfigRow {
    config: DeckConfig,
    learning: String,
    relearning: String,
    weights: String,
}

fn config_row(row: &Row<'_>) -> rusqlite::Result<ConfigRow> {
    Ok(ConfigRow {
        config: DeckConfig {
            id: DeckConfigId(row.get(0)?),
            guid: guid(row, 1)?,
            name: row.get(2)?,
            built_in: row.get(3)?,
            options: DeckConfigOptions {
                new_per_day: row.get(4)?,
                reviews_per_day: row.get(5)?,
                desired_retention: row.get(6)?,
                maximum_interval_days: row.get(7)?,
                learning_steps_minutes: Vec::new(),
                relearning_steps_minutes: Vec::new(),
                weights: Vec::new(),
                fuzz: row.get(11)?,
                new_card_order: code(12, NewCardOrder::from_db_code(row.get(12)?))?,
                bury_siblings: row.get(13)?,
                leech_threshold: row.get(14)?,
                leech_action: code(15, LeechAction::from_db_code(row.get(15)?))?,
            },
            created_at: ts(row, 16)?,
            updated_at: ts(row, 17)?,
            deleted_at: opt_ts(row, 18)?,
            usn: row.get(19)?,
            version: row.get(20)?,
        },
        learning: row.get(8)?,
        relearning: row.get(9)?,
        weights: row.get(10)?,
    })
}

impl ConfigRow {
    fn decode(mut self) -> AppResult<DeckConfig> {
        let options = &mut self.config.options;
        options.learning_steps_minutes =
            codec::u32_list_from_json(&self.learning, "learning_steps_minutes")?;
        options.relearning_steps_minutes =
            codec::u32_list_from_json(&self.relearning, "relearning_steps_minutes")?;
        options.weights = codec::f64_list_from_json(&self.weights, "fsrs_weights")?;
        Ok(self.config)
    }
}

impl SqliteRepos<'_> {
    fn configs_where(&self, condition: &str, args: &[i64]) -> AppResult<Vec<DeckConfig>> {
        let sql = format!("SELECT {CONFIG_COLUMNS} FROM deck_config WHERE {condition} ORDER BY id");
        let mut statement = self.tx.prepare_cached(&sql).map_err(map_sqlite)?;
        let rows: Vec<ConfigRow> = statement
            .query_map(params_from_iter(args), config_row)
            .and_then(Iterator::collect)
            .map_err(map_sqlite)?;
        rows.into_iter().map(ConfigRow::decode).collect()
    }
}

impl DeckConfigRepository for SqliteRepos<'_> {
    fn insert_config(
        &mut self,
        guid: &Guid,
        name: &str,
        options: &DeckConfigOptions,
        at: TimestampMs,
    ) -> AppResult<DeckConfig> {
        let name = langualinker_domain::validation::required_text(
            "name",
            name,
            langualinker_domain::validation::MAX_NAME_CHARS,
        )?;
        options.validate()?;
        self.tx
            .execute(
                "INSERT INTO deck_config (guid, name, new_per_day, reviews_per_day,
                     desired_retention, maximum_interval_days, learning_steps_minutes,
                     relearning_steps_minutes, fsrs_weights, fuzz, new_card_order, bury_siblings,
                     leech_threshold, leech_action, created_at, updated_at, usn)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15, ?16)",
                params![
                    guid.as_str(),
                    name,
                    options.new_per_day,
                    options.reviews_per_day,
                    options.desired_retention,
                    options.maximum_interval_days,
                    codec::u32_list_to_json(&options.learning_steps_minutes),
                    codec::u32_list_to_json(&options.relearning_steps_minutes),
                    codec::f64_list_to_json(&options.weights),
                    options.fuzz,
                    options.new_card_order.db_code(),
                    options.bury_siblings,
                    options.leech_threshold,
                    options.leech_action.db_code(),
                    at.0,
                    LOCAL_USN,
                ],
            )
            .map_err(map_sqlite)?;
        let id = DeckConfigId(self.tx.last_insert_rowid());
        self.config(id)?
            .ok_or_else(|| AppError::Internal("inserted preset vanished".into()))
    }

    fn config(&mut self, id: DeckConfigId) -> AppResult<Option<DeckConfig>> {
        Ok(self.configs_where("id = ?1", &[id.0])?.pop())
    }

    fn active_configs(&mut self) -> AppResult<Vec<DeckConfig>> {
        self.configs_where("deleted_at IS NULL", &[])
    }

    fn default_config(&mut self) -> AppResult<DeckConfig> {
        self.configs_where("built_in = 1 AND deleted_at IS NULL", &[])?
            .into_iter()
            .next()
            .ok_or(AppError::NotFound { entity: "preset" })
    }

    fn update_config(&mut self, config: &DeckConfig, at: TimestampMs) -> AppResult<DeckConfig> {
        let name = langualinker_domain::validation::required_text(
            "name",
            &config.name,
            langualinker_domain::validation::MAX_NAME_CHARS,
        )?;
        let options = &config.options;
        options.validate()?;
        let changed = self
            .tx
            .execute(
                "UPDATE deck_config SET name = ?1, new_per_day = ?2, reviews_per_day = ?3,
                     desired_retention = ?4, maximum_interval_days = ?5,
                     learning_steps_minutes = ?6, relearning_steps_minutes = ?7, fsrs_weights = ?8,
                     fuzz = ?9, new_card_order = ?10, bury_siblings = ?11, leech_threshold = ?12,
                     leech_action = ?13, updated_at = ?14, usn = ?15, version = version + 1
                 WHERE id = ?16 AND version = ?17 AND deleted_at IS NULL",
                params![
                    name,
                    options.new_per_day,
                    options.reviews_per_day,
                    options.desired_retention,
                    options.maximum_interval_days,
                    codec::u32_list_to_json(&options.learning_steps_minutes),
                    codec::u32_list_to_json(&options.relearning_steps_minutes),
                    codec::f64_list_to_json(&options.weights),
                    options.fuzz,
                    options.new_card_order.db_code(),
                    options.bury_siblings,
                    options.leech_threshold,
                    options.leech_action.db_code(),
                    at.0,
                    LOCAL_USN,
                    config.id.0,
                    config.version,
                ],
            )
            .map_err(map_sqlite)?;
        if changed == 0 {
            return Err(self.version_failure("deck_config", "preset", config.id.0));
        }
        self.config(config.id)?
            .ok_or(AppError::NotFound { entity: "preset" })
    }
}

// ------------------------------------------------------------------ note types

fn note_type_row(row: &Row<'_>) -> rusqlite::Result<NoteType> {
    Ok(NoteType {
        id: NoteTypeId(row.get(0)?),
        guid: guid(row, 1)?,
        kind: code(2, NoteTypeKind::from_db_code(row.get(2)?))?,
        name: row.get(3)?,
        version: row.get(4)?,
    })
}

impl NoteTypeRepository for SqliteRepos<'_> {
    fn note_type(&mut self, kind: NoteTypeKind) -> AppResult<NoteType> {
        self.tx
            .query_row(
                "SELECT id, guid, kind, name, version FROM note_types
                 WHERE kind = ?1 AND deleted_at IS NULL",
                [kind.db_code()],
                note_type_row,
            )
            .optional()
            .map_err(map_sqlite)?
            .ok_or(AppError::NotFound { entity: "noteType" })
    }

    fn note_type_by_id(&mut self, id: NoteTypeId) -> AppResult<Option<NoteType>> {
        self.tx
            .query_row(
                "SELECT id, guid, kind, name, version FROM note_types WHERE id = ?1",
                [id.0],
                note_type_row,
            )
            .optional()
            .map_err(map_sqlite)
    }

    fn templates(&mut self, note_type: NoteTypeId) -> AppResult<Vec<CardTemplate>> {
        let mut statement = self
            .tx
            .prepare_cached(
                "SELECT guid, note_type_id, ordinal, direction, name, version FROM card_templates
                 WHERE note_type_id = ?1 ORDER BY ordinal",
            )
            .map_err(map_sqlite)?;
        statement
            .query_map([note_type.0], |row| {
                Ok(CardTemplate {
                    guid: guid(row, 0)?,
                    note_type_id: NoteTypeId(row.get(1)?),
                    ordinal: row.get(2)?,
                    direction: code(3, CardDirection::from_db_code(row.get(3)?))?,
                    name: row.get(4)?,
                    version: row.get(5)?,
                })
            })
            .and_then(Iterator::collect)
            .map_err(map_sqlite)
    }
}

// ------------------------------------------------------------------ notes

const NOTE_COLUMNS: &str =
    "id, guid, note_type_id, fields, sort_field, created_at, updated_at, deleted_at, usn, version";

fn note_row(row: &Row<'_>) -> rusqlite::Result<(Note, String)> {
    Ok((
        Note {
            id: NoteId(row.get(0)?),
            guid: guid(row, 1)?,
            note_type_id: NoteTypeId(row.get(2)?),
            fields: langualinker_domain::note::NoteFields {
                front: String::new(),
                back: String::new(),
            },
            sort_field: row.get(4)?,
            tags: Vec::new(),
            created_at: ts(row, 5)?,
            updated_at: ts(row, 6)?,
            deleted_at: opt_ts(row, 7)?,
            usn: row.get(8)?,
            version: row.get(9)?,
        },
        row.get(3)?,
    ))
}

impl SqliteRepos<'_> {
    fn note_where(&self, condition: &str, arg: &dyn rusqlite::ToSql) -> AppResult<Option<Note>> {
        let sql = format!("SELECT {NOTE_COLUMNS} FROM notes WHERE {condition}");
        let Some((mut note, fields)) = self
            .tx
            .query_row(&sql, [arg], note_row)
            .optional()
            .map_err(map_sqlite)?
        else {
            return Ok(None);
        };
        note.fields = codec::note_fields_from_json(&fields)?;
        note.tags = self.note_tags(note.id)?;
        Ok(Some(note))
    }
}

impl NoteRepository for SqliteRepos<'_> {
    fn insert_note(&mut self, note: &NewNote, at: TimestampMs) -> AppResult<Note> {
        let note = note.clone().validated()?;
        self.tx
            .execute(
                "INSERT INTO notes (guid, note_type_id, fields, sort_field, created_at, updated_at, usn)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6)",
                params![
                    note.guid.as_str(),
                    note.note_type_id.0,
                    codec::note_fields_to_json(&note.fields),
                    note.fields.sort_field(),
                    at.0,
                    LOCAL_USN,
                ],
            )
            .map_err(|error| {
                if is_unique_violation(&error, "notes.guid") {
                    ValidationError::new("guid", "duplicate").into()
                } else {
                    map_sqlite(error)
                }
            })?;
        let id = NoteId(self.tx.last_insert_rowid());
        self.replace_note_tags(id, &note.tags, at)?;
        self.note(id)?
            .ok_or_else(|| AppError::Internal("inserted note vanished".into()))
    }

    fn note(&mut self, id: NoteId) -> AppResult<Option<Note>> {
        self.note_where("id = ?1", &id.0)
    }

    fn note_by_guid(&mut self, guid: &Guid) -> AppResult<Option<Note>> {
        self.note_where("guid = ?1", &guid.as_str())
    }

    fn update_note(&mut self, note: &Note, at: TimestampMs) -> AppResult<Note> {
        let fields = langualinker_domain::note::NoteFields::validated(
            &note.fields.front,
            &note.fields.back,
        )?;
        let tags = langualinker_domain::validation::tags(&note.tags)?;
        let changed = self
            .tx
            .execute(
                "UPDATE notes SET note_type_id = ?1, fields = ?2, sort_field = ?3, updated_at = ?4,
                     usn = ?5, version = version + 1
                 WHERE id = ?6 AND version = ?7 AND deleted_at IS NULL",
                params![
                    note.note_type_id.0,
                    codec::note_fields_to_json(&fields),
                    fields.sort_field(),
                    at.0,
                    LOCAL_USN,
                    note.id.0,
                    note.version,
                ],
            )
            .map_err(map_sqlite)?;
        if changed == 0 {
            return Err(self.version_failure("notes", "note", note.id.0));
        }
        self.replace_note_tags(note.id, &tags, at)?;
        self.note(note.id)?
            .ok_or(AppError::NotFound { entity: "note" })
    }

    fn soft_delete_note(
        &mut self,
        id: NoteId,
        expected_version: i64,
        at: TimestampMs,
    ) -> AppResult<()> {
        let changed = self
            .tx
            .execute(
                "UPDATE notes SET deleted_at = ?1, updated_at = ?1, usn = ?2, version = version + 1
                 WHERE id = ?3 AND version = ?4 AND deleted_at IS NULL",
                params![at.0, LOCAL_USN, id.0, expected_version],
            )
            .map_err(map_sqlite)?;
        if changed == 0 {
            return Err(self.version_failure("notes", "note", id.0));
        }
        self.tx
            .execute(
                "UPDATE cards SET deleted_at = ?1, updated_at = ?1, usn = ?2, version = version + 1
                 WHERE note_id = ?3 AND deleted_at IS NULL",
                params![at.0, LOCAL_USN, id.0],
            )
            .map_err(map_sqlite)?;
        Ok(())
    }
}

// ------------------------------------------------------------------ cards

const CARD_COLUMNS: &str = "id, guid, note_id, deck_id, template_ordinal, position, flags, state,
    queue, due_at, stability, difficulty, legacy_ease, last_review_at, scheduled_days,
    elapsed_days, reps, lapses, learning_step, buried_until, archived_at, created_at, updated_at,
    deleted_at, usn, version";

fn card_row(row: &Row<'_>) -> rusqlite::Result<Card> {
    Ok(Card {
        id: CardId(row.get(0)?),
        guid: guid(row, 1)?,
        note_id: NoteId(row.get(2)?),
        deck_id: DeckId(row.get(3)?),
        template_ordinal: row.get(4)?,
        position: row.get(5)?,
        flag: code(6, CardFlag::from_db_code(row.get(6)?))?,
        state: code(7, CardState::from_db_code(row.get(7)?))?,
        queue: code(8, QueueState::from_db_code(row.get(8)?))?,
        due_at: opt_ts(row, 9)?,
        fsrs: CardFsrsState {
            stability: row.get(10)?,
            difficulty: row.get(11)?,
            last_review_at: opt_ts(row, 13)?,
            scheduled_days: row.get(14)?,
            elapsed_days: row.get(15)?,
        },
        legacy_ease: row.get(12)?,
        reps: row.get(16)?,
        lapses: row.get(17)?,
        learning_step: row.get(18)?,
        buried_until: opt_ts(row, 19)?,
        archived_at: opt_ts(row, 20)?,
        created_at: ts(row, 21)?,
        updated_at: ts(row, 22)?,
        deleted_at: opt_ts(row, 23)?,
        usn: row.get(24)?,
        version: row.get(25)?,
    })
}

impl CardRepository for SqliteRepos<'_> {
    fn insert_card(&mut self, card: &NewCard, at: TimestampMs) -> AppResult<Card> {
        self.tx
            .execute(
                "INSERT INTO cards (guid, note_id, deck_id, template_ordinal, position, created_at,
                     updated_at, usn)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?7)",
                params![
                    card.guid.as_str(),
                    card.note_id.0,
                    card.deck_id.0,
                    card.template_ordinal,
                    card.position,
                    at.0,
                    LOCAL_USN,
                ],
            )
            .map_err(|error| {
                if is_unique_violation(&error, "cards.note_id") {
                    ValidationError::new("templateOrdinal", "duplicate").into()
                } else {
                    map_sqlite(error)
                }
            })?;
        let id = CardId(self.tx.last_insert_rowid());
        self.card(id)?
            .ok_or_else(|| AppError::Internal("inserted card vanished".into()))
    }

    fn card(&mut self, id: CardId) -> AppResult<Option<Card>> {
        let sql = format!("SELECT {CARD_COLUMNS} FROM cards WHERE id = ?1");
        self.tx
            .query_row(&sql, [id.0], card_row)
            .optional()
            .map_err(map_sqlite)
    }

    fn cards_of_note(&mut self, note: NoteId) -> AppResult<Vec<Card>> {
        let sql = format!(
            "SELECT {CARD_COLUMNS} FROM cards WHERE note_id = ?1 AND deleted_at IS NULL
             ORDER BY template_ordinal"
        );
        let mut statement = self.tx.prepare_cached(&sql).map_err(map_sqlite)?;
        statement
            .query_map([note.0], card_row)
            .and_then(Iterator::collect)
            .map_err(map_sqlite)
    }

    fn soft_delete_card(
        &mut self,
        id: CardId,
        expected_version: i64,
        at: TimestampMs,
    ) -> AppResult<Card> {
        let changed = self
            .tx
            .execute(
                "UPDATE cards SET deleted_at = ?1, updated_at = ?1, usn = ?2, version = version + 1
                 WHERE id = ?3 AND version = ?4 AND deleted_at IS NULL",
                params![at.0, LOCAL_USN, id.0, expected_version],
            )
            .map_err(map_sqlite)?;
        if changed == 0 {
            return Err(self.version_failure("cards", "card", id.0));
        }
        self.card(id)?.ok_or(AppError::NotFound { entity: "card" })
    }

    fn update_card(&mut self, card: &Card, at: TimestampMs) -> AppResult<Card> {
        let changed = self
            .tx
            .execute(
                "UPDATE cards SET deck_id = ?1, position = ?2, flags = ?3, state = ?4, queue = ?5,
                     due_at = ?6, stability = ?7, difficulty = ?8, legacy_ease = ?9,
                     last_review_at = ?10, scheduled_days = ?11, elapsed_days = ?12, reps = ?13,
                     lapses = ?14, learning_step = ?15, buried_until = ?16, archived_at = ?17,
                     updated_at = ?18, usn = ?19, version = version + 1
                 WHERE id = ?20 AND version = ?21 AND deleted_at IS NULL",
                params![
                    card.deck_id.0,
                    card.position,
                    card.flag.db_code(),
                    card.state.db_code(),
                    card.queue.db_code(),
                    card.due_at.map(|at| at.0),
                    card.fsrs.stability,
                    card.fsrs.difficulty,
                    card.legacy_ease,
                    card.fsrs.last_review_at.map(|at| at.0),
                    card.fsrs.scheduled_days,
                    card.fsrs.elapsed_days,
                    card.reps,
                    card.lapses,
                    card.learning_step,
                    card.buried_until.map(|at| at.0),
                    card.archived_at.map(|at| at.0),
                    at.0,
                    LOCAL_USN,
                    card.id.0,
                    card.version,
                ],
            )
            .map_err(map_sqlite)?;
        if changed == 0 {
            return Err(self.version_failure("cards", "card", card.id.0));
        }
        self.card(card.id)?
            .ok_or(AppError::NotFound { entity: "card" })
    }

    fn due_card_ids(
        &mut self,
        decks: &[DeckId],
        until: TimestampMs,
        limit: u32,
    ) -> AppResult<Vec<CardId>> {
        if decks.is_empty() {
            return Ok(Vec::new());
        }
        // The deck/queue/due partial index serves each (deck, queue) range; the IN lists keep
        // the planner on it (see the query-plan test).
        let sql = format!(
            "SELECT id FROM cards
             WHERE deck_id IN ({}) AND queue IN (2, 3) AND due_at <= ?
               AND deleted_at IS NULL AND archived_at IS NULL
             ORDER BY due_at, id LIMIT ?",
            placeholders(decks.len())
        );
        let mut args: Vec<i64> = decks.iter().map(|deck| deck.0).collect();
        args.extend([until.0, i64::from(limit)]);
        let mut statement = self.tx.prepare_cached(&sql).map_err(map_sqlite)?;
        statement
            .query_map(params_from_iter(args), |row| row.get(0).map(CardId))
            .and_then(Iterator::collect)
            .map_err(map_sqlite)
    }

    fn new_card_ids(&mut self, decks: &[DeckId], limit: u32) -> AppResult<Vec<CardId>> {
        if decks.is_empty() {
            return Ok(Vec::new());
        }
        let sql = format!(
            "SELECT id FROM cards
             WHERE deck_id IN ({}) AND queue = 0 AND deleted_at IS NULL AND archived_at IS NULL
             ORDER BY position, id LIMIT ?",
            placeholders(decks.len())
        );
        let mut args: Vec<i64> = decks.iter().map(|deck| deck.0).collect();
        args.push(i64::from(limit));
        let mut statement = self.tx.prepare_cached(&sql).map_err(map_sqlite)?;
        statement
            .query_map(params_from_iter(args), |row| row.get(0).map(CardId))
            .and_then(Iterator::collect)
            .map_err(map_sqlite)
    }

    fn next_position(&mut self) -> AppResult<i64> {
        self.tx
            .query_row(
                "SELECT COALESCE(MAX(position), 0) + 1 FROM cards",
                [],
                |row| row.get(0),
            )
            .map_err(map_sqlite)
    }
}

// ------------------------------------------------------------------ review logs

const LOG_COLUMNS: &str = "id, card_id, session_id, operation_id, rating, kind, reviewed_at,
    duration_ms, card_before, card_after, fsrs_version, parameters_hash, invalidated_at";

fn log_row(row: &Row<'_>) -> rusqlite::Result<(ReviewLog, String, String)> {
    let empty = langualinker_domain::review::CardSnapshot {
        state: CardState::New,
        queue: QueueState::New,
        due_at: None,
        position: 0,
        fsrs: CardFsrsState::NEW,
        legacy_ease: None,
        reps: 0,
        lapses: 0,
        learning_step: None,
        buried_until: None,
        flag: CardFlag::None,
        version: 0,
    };
    Ok((
        ReviewLog {
            id: ReviewLogId(row.get(0)?),
            card_id: CardId(row.get(1)?),
            session_id: row.get::<_, Option<i64>>(2)?.map(SessionId),
            operation_id: row.get(3)?,
            rating: code(4, Rating::from_db_code(row.get(4)?))?,
            kind: code(5, ReviewKind::from_db_code(row.get(5)?))?,
            reviewed_at: ts(row, 6)?,
            duration_ms: row.get(7)?,
            card_before: empty.clone(),
            card_after: empty,
            fsrs_version: row.get(10)?,
            parameters_hash: row.get(11)?,
            invalidated_at: opt_ts(row, 12)?,
        },
        row.get(8)?,
        row.get(9)?,
    ))
}

fn decode_log((mut log, before, after): (ReviewLog, String, String)) -> AppResult<ReviewLog> {
    log.card_before = codec::snapshot_from_json(&before)?;
    log.card_after = codec::snapshot_from_json(&after)?;
    Ok(log)
}

impl ReviewLogRepository for SqliteRepos<'_> {
    fn insert_review_log(&mut self, log: &ReviewLog) -> AppResult<ReviewLog> {
        self.tx
            .execute(
                "INSERT INTO review_logs (card_id, session_id, operation_id, rating, kind,
                     reviewed_at, duration_ms, card_before, card_after, fsrs_version,
                     parameters_hash, invalidated_at, usn)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    log.card_id.0,
                    log.session_id.map(|id| id.0),
                    log.operation_id,
                    log.rating.db_code(),
                    log.kind.db_code(),
                    log.reviewed_at.0,
                    log.duration_ms,
                    codec::snapshot_to_json(&log.card_before),
                    codec::snapshot_to_json(&log.card_after),
                    log.fsrs_version,
                    log.parameters_hash,
                    log.invalidated_at.map(|at| at.0),
                    LOCAL_USN,
                ],
            )
            .map_err(|error| {
                if is_unique_violation(&error, "review_logs.operation_id") {
                    ValidationError::new("operationId", "duplicate").into()
                } else {
                    map_sqlite(error)
                }
            })?;
        let mut stored = log.clone();
        stored.id = ReviewLogId(self.tx.last_insert_rowid());
        Ok(stored)
    }

    fn review_logs(&mut self, card: CardId) -> AppResult<Vec<ReviewLog>> {
        let sql = format!(
            "SELECT {LOG_COLUMNS} FROM review_logs WHERE card_id = ?1 ORDER BY reviewed_at DESC, id DESC"
        );
        let mut statement = self.tx.prepare_cached(&sql).map_err(map_sqlite)?;
        let rows: Vec<_> = statement
            .query_map([card.0], log_row)
            .and_then(Iterator::collect)
            .map_err(map_sqlite)?;
        rows.into_iter().map(decode_log).collect()
    }

    fn review_log_by_operation(&mut self, operation_id: &str) -> AppResult<Option<ReviewLog>> {
        let sql = format!("SELECT {LOG_COLUMNS} FROM review_logs WHERE operation_id = ?1");
        self.tx
            .query_row(&sql, [operation_id], log_row)
            .optional()
            .map_err(map_sqlite)?
            .map(decode_log)
            .transpose()
    }

    fn invalidate_review_log(&mut self, operation_id: &str, at: TimestampMs) -> AppResult<()> {
        let changed = self
            .tx
            .execute(
                "UPDATE review_logs SET invalidated_at = ?1, usn = ?2
                 WHERE operation_id = ?3 AND invalidated_at IS NULL",
                params![at.0, LOCAL_USN, operation_id],
            )
            .map_err(map_sqlite)?;
        if changed == 0 {
            return Err(AppError::NotFound { entity: "review" });
        }
        Ok(())
    }
}

// ------------------------------------------------------------------ content

const ASSOCIATION_COLUMNS: &str = "id, guid, note_id, text, provenance, native_language,
    target_language, prompt_version, model, moderation, cache_key, sort_order, is_favorite,
    created_at, updated_at, deleted_at, usn";

fn association_row(row: &Row<'_>) -> rusqlite::Result<Association> {
    Ok(Association {
        id: AssociationId(row.get(0)?),
        guid: guid(row, 1)?,
        note_id: NoteId(row.get(2)?),
        text: row.get(3)?,
        provenance: code(4, AssociationProvenance::from_db_code(row.get(4)?))?,
        native_language: row.get(5)?,
        target_language: row.get(6)?,
        prompt_version: row.get(7)?,
        model: row.get(8)?,
        moderation: code(9, ModerationStatus::from_db_code(row.get(9)?))?,
        cache_key: row.get(10)?,
        sort_order: row.get(11)?,
        is_favorite: row.get(12)?,
        created_at: ts(row, 13)?,
        updated_at: ts(row, 14)?,
        deleted_at: opt_ts(row, 15)?,
        usn: row.get(16)?,
    })
}

const LINK_COLUMNS: &str = "id, sentence_note_id, word_note_id, preferred_word_card_id,
    surface_form, lemma, start_index, end_index, resolution, created_at, updated_at";

fn link_row(row: &Row<'_>) -> rusqlite::Result<SentenceWordLink> {
    Ok(SentenceWordLink {
        id: WordLinkId(row.get(0)?),
        sentence_note_id: NoteId(row.get(1)?),
        word_note_id: row.get::<_, Option<i64>>(2)?.map(NoteId),
        preferred_word_card_id: row.get::<_, Option<i64>>(3)?.map(CardId),
        surface_form: row.get(4)?,
        lemma: row.get(5)?,
        start_index: row.get(6)?,
        end_index: row.get(7)?,
        resolution: code(8, WordLinkResolution::from_db_code(row.get(8)?))?,
        created_at: ts(row, 9)?,
        updated_at: ts(row, 10)?,
    })
}

impl ContentRepository for SqliteRepos<'_> {
    fn insert_association(
        &mut self,
        association: &NewAssociation,
        at: TimestampMs,
    ) -> AppResult<Association> {
        let association = association.clone().validated()?;
        self.tx
            .execute(
                "INSERT INTO associations (guid, note_id, text, provenance, native_language,
                     target_language, prompt_version, model, moderation, cache_key, sort_order,
                     created_at, updated_at, usn)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                     (SELECT COALESCE(MAX(sort_order), -1) + 1 FROM associations WHERE note_id = ?2),
                     ?11, ?11, ?12)",
                params![
                    association.guid.as_str(),
                    association.note_id.0,
                    association.text,
                    association.provenance.db_code(),
                    association.native_language,
                    association.target_language,
                    association.prompt_version,
                    association.model,
                    association.moderation.db_code(),
                    association.cache_key,
                    at.0,
                    LOCAL_USN,
                ],
            )
            .map_err(map_sqlite)?;
        let id = self.tx.last_insert_rowid();
        let sql = format!("SELECT {ASSOCIATION_COLUMNS} FROM associations WHERE id = ?1");
        self.tx
            .query_row(&sql, [id], association_row)
            .map_err(map_sqlite)
    }

    fn associations(&mut self, note: NoteId) -> AppResult<Vec<Association>> {
        let sql = format!(
            "SELECT {ASSOCIATION_COLUMNS} FROM associations
             WHERE note_id = ?1 AND deleted_at IS NULL ORDER BY sort_order, id"
        );
        let mut statement = self.tx.prepare_cached(&sql).map_err(map_sqlite)?;
        statement
            .query_map([note.0], association_row)
            .and_then(Iterator::collect)
            .map_err(map_sqlite)
    }

    fn set_favorite_association(
        &mut self,
        note: NoteId,
        association: Option<AssociationId>,
        at: TimestampMs,
    ) -> AppResult<()> {
        self.tx
            .execute(
                "UPDATE associations SET is_favorite = 0, updated_at = ?1, usn = ?2
                 WHERE note_id = ?3 AND is_favorite = 1 AND deleted_at IS NULL",
                params![at.0, LOCAL_USN, note.0],
            )
            .map_err(map_sqlite)?;
        let Some(association) = association else {
            return Ok(());
        };
        let changed = self
            .tx
            .execute(
                "UPDATE associations SET is_favorite = 1, updated_at = ?1, usn = ?2
                 WHERE id = ?3 AND note_id = ?4 AND deleted_at IS NULL",
                params![at.0, LOCAL_USN, association.0, note.0],
            )
            .map_err(map_sqlite)?;
        if changed == 0 {
            return Err(AppError::NotFound {
                entity: "association",
            });
        }
        Ok(())
    }

    fn soft_delete_association(&mut self, id: AssociationId, at: TimestampMs) -> AppResult<()> {
        let changed = self
            .tx
            .execute(
                "UPDATE associations SET deleted_at = ?1, updated_at = ?1, usn = ?2
                 WHERE id = ?3 AND deleted_at IS NULL",
                params![at.0, LOCAL_USN, id.0],
            )
            .map_err(map_sqlite)?;
        if changed == 0 {
            return Err(AppError::NotFound {
                entity: "association",
            });
        }
        Ok(())
    }

    fn insert_grammar_tip(
        &mut self,
        note: NoteId,
        guid: &Guid,
        text: &str,
        provenance: TipProvenance,
        at: TimestampMs,
    ) -> AppResult<GrammarTip> {
        let text = langualinker_domain::validation::required_text(
            "text",
            text,
            langualinker_domain::validation::MAX_SNIPPET_CHARS,
        )?;
        self.tx
            .execute(
                "INSERT INTO grammar_tips (guid, note_id, text, provenance, sort_order, created_at,
                     updated_at, usn)
                 VALUES (?1, ?2, ?3, ?4,
                     (SELECT COALESCE(MAX(sort_order), -1) + 1 FROM grammar_tips WHERE note_id = ?2),
                     ?5, ?5, ?6)",
                params![guid.as_str(), note.0, text, provenance.db_code(), at.0, LOCAL_USN],
            )
            .map_err(map_sqlite)?;
        let id = self.tx.last_insert_rowid();
        self.grammar_tips(note)?
            .into_iter()
            .find(|tip| tip.id.0 == id)
            .ok_or_else(|| AppError::Internal("inserted tip vanished".into()))
    }

    fn grammar_tips(&mut self, note: NoteId) -> AppResult<Vec<GrammarTip>> {
        let mut statement = self
            .tx
            .prepare_cached(
                "SELECT id, guid, note_id, text, provenance, sort_order, created_at, updated_at,
                     deleted_at, usn
                 FROM grammar_tips WHERE note_id = ?1 AND deleted_at IS NULL ORDER BY sort_order, id",
            )
            .map_err(map_sqlite)?;
        statement
            .query_map([note.0], |row| {
                Ok(GrammarTip {
                    id: GrammarTipId(row.get(0)?),
                    guid: guid(row, 1)?,
                    note_id: NoteId(row.get(2)?),
                    text: row.get(3)?,
                    provenance: code(4, TipProvenance::from_db_code(row.get(4)?))?,
                    sort_order: row.get(5)?,
                    created_at: ts(row, 6)?,
                    updated_at: ts(row, 7)?,
                    deleted_at: opt_ts(row, 8)?,
                    usn: row.get(9)?,
                })
            })
            .and_then(Iterator::collect)
            .map_err(map_sqlite)
    }

    fn replace_word_links(
        &mut self,
        sentence: NoteId,
        links: &[SentenceWordLink],
        at: TimestampMs,
    ) -> AppResult<Vec<SentenceWordLink>> {
        for link in links {
            link.check()?;
        }
        self.tx
            .execute(
                "DELETE FROM sentence_word_links WHERE sentence_note_id = ?1",
                [sentence.0],
            )
            .map_err(map_sqlite)?;
        for link in links {
            self.tx
                .execute(
                    "INSERT INTO sentence_word_links (sentence_note_id, word_note_id,
                         preferred_word_card_id, surface_form, lemma, start_index, end_index,
                         resolution, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
                    params![
                        sentence.0,
                        link.word_note_id.map(|id| id.0),
                        link.preferred_word_card_id.map(|id| id.0),
                        link.surface_form,
                        link.lemma,
                        link.start_index,
                        link.end_index,
                        link.resolution.db_code(),
                        at.0,
                    ],
                )
                .map_err(map_sqlite)?;
        }
        self.word_links(sentence)
    }

    fn word_links(&mut self, sentence: NoteId) -> AppResult<Vec<SentenceWordLink>> {
        let sql = format!(
            "SELECT {LINK_COLUMNS} FROM sentence_word_links WHERE sentence_note_id = ?1
             ORDER BY start_index, id"
        );
        let mut statement = self.tx.prepare_cached(&sql).map_err(map_sqlite)?;
        statement
            .query_map([sentence.0], link_row)
            .and_then(Iterator::collect)
            .map_err(map_sqlite)
    }

    fn word_link(&mut self, id: WordLinkId) -> AppResult<Option<SentenceWordLink>> {
        let sql = format!("SELECT {LINK_COLUMNS} FROM sentence_word_links WHERE id = ?1");
        self.tx
            .query_row(&sql, [id.0], link_row)
            .optional()
            .map_err(map_sqlite)
    }

    fn update_word_link(
        &mut self,
        link: &SentenceWordLink,
        at: TimestampMs,
    ) -> AppResult<SentenceWordLink> {
        link.check()?;
        let changed = self
            .tx
            .execute(
                "UPDATE sentence_word_links
                 SET word_note_id = ?1, preferred_word_card_id = ?2, resolution = ?3, updated_at = ?4
                 WHERE id = ?5",
                params![
                    link.word_note_id.map(|id| id.0),
                    link.preferred_word_card_id.map(|id| id.0),
                    link.resolution.db_code(),
                    at.0,
                    link.id.0,
                ],
            )
            .map_err(map_sqlite)?;
        if changed == 0 {
            return Err(AppError::NotFound { entity: "wordLink" });
        }
        self.word_link(link.id)?
            .ok_or(AppError::NotFound { entity: "wordLink" })
    }

    fn unresolve_links_to(&mut self, word: NoteId, at: TimestampMs) -> AppResult<u64> {
        let changed = self
            .tx
            .execute(
                "UPDATE sentence_word_links
                 SET word_note_id = NULL, preferred_word_card_id = NULL, resolution = ?1, updated_at = ?2
                 WHERE word_note_id = ?3",
                params![WordLinkResolution::Unresolved.db_code(), at.0, word.0],
            )
            .map_err(map_sqlite)?;
        Ok(changed as u64)
    }
}

// ------------------------------------------------------------------ settings

impl SettingsRepository for SqliteRepos<'_> {
    fn setting(&mut self, key: &str) -> AppResult<Option<String>> {
        self.tx
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()
            .map_err(map_sqlite)
    }

    fn put_setting(&mut self, key: &str, value: &str, at: TimestampMs) -> AppResult<()> {
        if serde_json::from_str::<serde_json::Value>(value).is_err() {
            return Err(ValidationError::new("value", "json").into());
        }
        self.tx
            .execute(
                "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value,
                     updated_at = excluded.updated_at, version = version + 1",
                params![key, value, at.0],
            )
            .map_err(map_sqlite)?;
        Ok(())
    }
}

// ------------------------------------------------------------------ jobs

const JOB_COLUMNS: &str = "id, kind, status, sequence, progress_done, progress_total, operation_id,
    started_at, finished_at, error_code, error_details, result";

/// A job row with its kind, status and error columns still undecoded.
type RawJob = (JobRecord, String, String, Option<String>, Option<String>);

fn job_row(row: &Row<'_>) -> rusqlite::Result<RawJob> {
    let done: Option<i64> = row.get(4)?;
    let total: Option<i64> = row.get(5)?;
    Ok((
        JobRecord {
            id: JobId(row.get::<_, i64>(0)?.to_string()),
            kind: JobKind::PrepareContent,
            status: JobStatus::Queued,
            sequence: row.get(3)?,
            progress: done.zip(total).map(|(done, total)| JobProgress {
                done: done.max(0) as u64,
                total: total.max(0) as u64,
            }),
            operation_id: row.get(6)?,
            started_at: ts(row, 7)?,
            finished_at: opt_ts(row, 8)?,
            error: None,
            result_json: row.get(11)?,
        },
        row.get(1)?,
        row.get(2)?,
        row.get(9)?,
        row.get(10)?,
    ))
}

fn decode_job((mut job, kind, status, error_code, error_details): RawJob) -> AppResult<JobRecord> {
    job.kind = JobKind::parse(&kind)?;
    job.status = JobStatus::parse(&status)?;
    job.error = match error_code {
        Some(code) => Some(JobFailure {
            code,
            details: match error_details {
                Some(details) => codec::job_details_from_json(&details)?,
                None => Vec::new(),
            },
        }),
        None => None,
    };
    Ok(job)
}

fn parse_job_id(id: &JobId) -> Option<i64> {
    id.0.parse().ok()
}

impl JobRepository for SqliteRepos<'_> {
    fn insert_job(&mut self, job: &NewJob, at: TimestampMs) -> AppResult<JobRecord> {
        let total = job.total.map(|total| total as i64);
        self.tx
            .execute(
                "INSERT INTO jobs (kind, status, sequence, progress_done, progress_total,
                     operation_id, started_at)
                 VALUES (?1, 'queued', 0, ?2, ?3, ?4, ?5)",
                params![
                    job.kind.as_str(),
                    total.map(|_| 0),
                    total,
                    job.operation_id,
                    at.0
                ],
            )
            .map_err(map_sqlite)?;
        let id = JobId(self.tx.last_insert_rowid().to_string());
        self.job(&id)?
            .ok_or_else(|| AppError::Internal("inserted job vanished".into()))
    }

    fn job(&mut self, id: &JobId) -> AppResult<Option<JobRecord>> {
        let Some(id) = parse_job_id(id) else {
            return Ok(None);
        };
        let sql = format!("SELECT {JOB_COLUMNS} FROM jobs WHERE id = ?1");
        self.tx
            .query_row(&sql, [id], job_row)
            .optional()
            .map_err(map_sqlite)?
            .map(decode_job)
            .transpose()
    }

    fn update_job(&mut self, job: &JobRecord) -> AppResult<()> {
        let id = parse_job_id(&job.id).ok_or(AppError::NotFound { entity: "job" })?;
        let changed = self
            .tx
            .execute(
                "UPDATE jobs SET status = ?1, sequence = ?2, progress_done = ?3,
                     progress_total = ?4, finished_at = ?5, error_code = ?6, error_details = ?7,
                     result = ?8
                 WHERE id = ?9 AND sequence < ?2",
                params![
                    job.status.as_str(),
                    job.sequence,
                    job.progress.map(|progress| progress.done as i64),
                    job.progress.map(|progress| progress.total as i64),
                    job.finished_at.map(|at| at.0),
                    job.error.as_ref().map(|error| error.code.as_str()),
                    job.error
                        .as_ref()
                        .map(|error| codec::job_details_to_json(&error.details)),
                    job.result_json,
                    id,
                ],
            )
            .map_err(map_sqlite)?;
        if changed == 0 {
            return Err(AppError::Internal(format!("stale sequence for job {id}")));
        }
        Ok(())
    }

    fn unfinished_jobs(&mut self) -> AppResult<Vec<JobRecord>> {
        let sql = format!(
            "SELECT {JOB_COLUMNS} FROM jobs WHERE status IN ('queued', 'running') ORDER BY id"
        );
        let mut statement = self.tx.prepare_cached(&sql).map_err(map_sqlite)?;
        let rows: Vec<RawJob> = statement
            .query_map([], job_row)
            .and_then(Iterator::collect)
            .map_err(map_sqlite)?;
        rows.into_iter().map(decode_job).collect()
    }
}

// ------------------------------------------------------------------ idempotency

impl IdempotencyRepository for SqliteRepos<'_> {
    fn recorded_operation(&mut self, operation_id: &str) -> AppResult<Option<RecordedOperation>> {
        self.tx
            .query_row(
                "SELECT operation, result FROM operations WHERE operation_id = ?1",
                [operation_id],
                |row| {
                    Ok(RecordedOperation {
                        operation: row.get(0)?,
                        result_json: row.get(1)?,
                    })
                },
            )
            .optional()
            .map_err(map_sqlite)
    }

    fn record_operation(
        &mut self,
        operation_id: &str,
        operation: &RecordedOperation,
        at: TimestampMs,
    ) -> AppResult<()> {
        if operation_id.is_empty() || operation_id.len() > 128 {
            return Err(ValidationError::new("operationId", "format").into());
        }
        self.tx
            .execute(
                "INSERT INTO operations (operation_id, operation, result, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    operation_id,
                    operation.operation,
                    operation.result_json,
                    at.0
                ],
            )
            .map_err(|error| {
                if is_unique_violation(&error, "operations.operation_id") {
                    ValidationError::new("operationId", "duplicate").into()
                } else {
                    map_sqlite(error)
                }
            })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use langualinker_application::repositories::{Repositories, UnitOfWork};
    use langualinker_domain::deck::DeckType;

    use super::*;
    use crate::sqlite::SqliteDatabase;
    use crate::sqlite::tests::FixedClock;

    const T0: TimestampMs = TimestampMs(1_772_438_400_000);
    const T1: TimestampMs = TimestampMs(1_772_438_500_000);

    fn db() -> SqliteDatabase {
        SqliteDatabase::open_in_memory(&FixedClock(T0.0))
            .unwrap()
            .database
    }

    fn guid_of(n: u8) -> Guid {
        Guid::from_bytes([n; 16])
    }

    fn new_deck(n: u8, content_version: Option<&str>, bundled: bool) -> NewDeck {
        NewDeck {
            guid: guid_of(n),
            name: format!("Deck {n}"),
            parent_id: None,
            description: String::new(),
            native_language: NativeLanguage::Pl,
            target_language: TargetLanguage::De,
            deck_type: DeckType::Linguistic,
            config_id: DeckConfigId(1),
            content_version: content_version.map(str::to_owned),
            bundled,
        }
    }

    fn new_note(tx: &mut dyn Repositories, n: u8, front: &str, tags: &[&str]) -> Note {
        let note_type = tx.note_type(NoteTypeKind::Basic).unwrap();
        tx.insert_note(
            &NewNote {
                guid: guid_of(n),
                note_type_id: note_type.id,
                fields: langualinker_domain::note::NoteFields {
                    front: front.into(),
                    back: "back".into(),
                },
                tags: tags.iter().map(|tag| (*tag).to_owned()).collect(),
            },
            T0,
        )
        .unwrap()
    }

    /// A card of its own note in `deck`, at `position`.
    fn new_card(tx: &mut dyn Repositories, n: u8, deck: DeckId, position: i64) -> Card {
        let note = new_note(tx, n, &format!("front {n}"), &[]);
        tx.insert_card(
            &NewCard {
                guid: guid_of(n.wrapping_add(100)),
                note_id: note.id,
                deck_id: deck,
                template_ordinal: 0,
                position,
            },
            T0,
        )
        .unwrap()
    }

    /// Makes a new card a scheduled review card due at `due`.
    fn schedule(tx: &mut dyn Repositories, mut card: Card, due: i64) -> Card {
        card.state = CardState::Review;
        card.queue = QueueState::Review;
        card.due_at = Some(TimestampMs(due));
        tx.update_card(&card, T1).unwrap()
    }

    // ---------------------------------------------------------------- helpers

    #[test]
    fn placeholders_join_question_marks() {
        assert_eq!(placeholders(0), "");
        assert_eq!(placeholders(1), "?");
        assert_eq!(placeholders(3), "?, ?, ?");
    }

    #[test]
    fn unique_violations_are_matched_by_column() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE t (a TEXT UNIQUE); INSERT INTO t VALUES ('x');")
            .unwrap();
        let error = conn.execute("INSERT INTO t VALUES ('x')", []).unwrap_err();
        assert!(is_unique_violation(&error, "t.a"));
        assert!(!is_unique_violation(&error, "t.b"));
        assert!(!is_unique_violation(
            &rusqlite::Error::QueryReturnedNoRows,
            "t.a"
        ));
    }

    #[test]
    fn unknown_codes_become_conversion_errors_naming_the_column() {
        let error = code(5, NativeLanguage::from_db_code(9_999)).unwrap_err();
        assert!(matches!(
            error,
            rusqlite::Error::FromSqlConversionFailure(5, Type::Integer, _)
        ));
    }

    // ---------------------------------------------------------------- presets

    #[test]
    fn default_preset_is_the_built_in_one() {
        let db = db();
        let preset = db.read(|tx| tx.default_config()).unwrap();
        assert!(preset.built_in);
        assert_eq!(preset.options, DeckConfigOptions::default());
        let active = db.read(|tx| tx.active_configs()).unwrap();
        assert_eq!(active.first().map(|c| c.id), Some(preset.id));
    }

    #[test]
    fn inserted_preset_keeps_every_option() {
        let db = db();
        let options = DeckConfigOptions {
            new_per_day: 5,
            reviews_per_day: 50,
            desired_retention: 0.85,
            maximum_interval_days: 365,
            learning_steps_minutes: vec![2, 20, 200],
            relearning_steps_minutes: Vec::new(),
            fuzz: false,
            bury_siblings: false,
            leech_threshold: 3,
            ..DeckConfigOptions::default()
        };
        let stored = {
            let options = options.clone();
            db.write(move |tx| tx.insert_config(&guid_of(1), "  Heavy  ", &options, T0))
                .unwrap()
        };
        assert_eq!(stored.name, "Heavy", "name is trimmed");
        assert!(!stored.built_in);
        assert_eq!(stored.options, options);
        assert_eq!((stored.version, stored.created_at), (1, T0));
        let id = stored.id;
        assert_eq!(db.read(move |tx| tx.config(id)).unwrap(), Some(stored));
        assert_eq!(db.read(|tx| tx.active_configs()).unwrap().len(), 2);
    }

    #[test]
    fn invalid_presets_are_rejected_before_touching_the_table() {
        let db = db();
        let bad = DeckConfigOptions {
            desired_retention: 0.2,
            ..DeckConfigOptions::default()
        };
        let error = db
            .write(move |tx| tx.insert_config(&guid_of(1), "Preset", &bad, T0))
            .unwrap_err();
        assert_eq!(error.code(), "VALIDATION_FAILED");
        let blank = db
            .write(|tx| tx.insert_config(&guid_of(2), "   ", &DeckConfigOptions::default(), T0))
            .unwrap_err();
        assert_eq!(blank.code(), "VALIDATION_FAILED");
        assert_eq!(db.read(|tx| tx.active_configs()).unwrap().len(), 1);
    }

    #[test]
    fn unknown_preset_reads_as_none() {
        assert_eq!(
            db().read(|tx| tx.config(DeckConfigId(9_999))).unwrap(),
            None
        );
    }

    #[test]
    fn preset_updates_are_versioned() {
        let db = db();
        let preset = db.read(|tx| tx.default_config()).unwrap();
        let mut edited = preset.clone();
        edited.name = "Renamed".into();
        edited.options.new_per_day = 7;
        let updated = {
            let edited = edited.clone();
            db.write(move |tx| tx.update_config(&edited, T1)).unwrap()
        };
        assert_eq!(updated.version, preset.version + 1);
        assert_eq!(
            (updated.name.as_str(), updated.options.new_per_day),
            ("Renamed", 7)
        );
        assert_eq!(updated.updated_at, T1);

        let stale = db
            .write(move |tx| tx.update_config(&edited, T1))
            .unwrap_err();
        assert_eq!(
            stale,
            AppError::VersionConflict {
                entity: "preset",
                current: updated.version
            }
        );
    }

    #[test]
    fn updating_a_missing_or_invalid_preset_fails_cleanly() {
        let db = db();
        let mut preset = db.read(|tx| tx.default_config()).unwrap();
        let mut invalid = preset.clone();
        invalid.options.weights.pop();
        let error = db
            .write(move |tx| tx.update_config(&invalid, T1))
            .unwrap_err();
        assert_eq!(error.code(), "VALIDATION_FAILED");
        preset.id = DeckConfigId(9_999);
        let missing = db
            .write(move |tx| tx.update_config(&preset, T1))
            .unwrap_err();
        assert_eq!(missing, AppError::NotFound { entity: "preset" });
    }

    // ---------------------------------------------------------------- decks

    #[test]
    fn soft_deleted_decks_leave_the_active_list_but_stay_readable() {
        let db = db();
        let deck = db
            .write(|tx| tx.insert_deck(&new_deck(1, None, false), T0))
            .unwrap();
        let keep = db
            .write(|tx| tx.insert_deck(&new_deck(2, None, false), T0))
            .unwrap();
        let (id, version) = (deck.id, deck.version);
        db.write(move |tx| tx.soft_delete_deck(id, version, T1))
            .unwrap();

        let active = db.read(|tx| tx.active_decks()).unwrap();
        assert_eq!(active.iter().map(|d| d.id).collect::<Vec<_>>(), [keep.id]);
        let stored = db.read(move |tx| tx.deck(id)).unwrap().unwrap();
        assert_eq!(stored.deleted_at, Some(T1));
        assert_eq!(stored.version, version + 1);
    }

    #[test]
    fn deleting_a_deck_twice_or_with_a_stale_version_fails() {
        let db = db();
        let deck = db
            .write(|tx| tx.insert_deck(&new_deck(1, None, false), T0))
            .unwrap();
        let id = deck.id;
        let stale = db
            .write(move |tx| tx.soft_delete_deck(id, deck.version + 5, T1))
            .unwrap_err();
        assert_eq!(
            stale,
            AppError::VersionConflict {
                entity: "deck",
                current: deck.version
            }
        );
        db.write(move |tx| tx.soft_delete_deck(id, deck.version, T1))
            .unwrap();
        let again = db
            .write(move |tx| tx.soft_delete_deck(id, deck.version + 1, T1))
            .unwrap_err();
        assert_eq!(again, AppError::Deleted { entity: "deck" });
        let missing = db
            .write(|tx| tx.soft_delete_deck(DeckId(9_999), 1, T1))
            .unwrap_err();
        assert_eq!(missing, AppError::NotFound { entity: "deck" });
    }

    #[test]
    fn deck_update_trims_text_and_bumps_the_version() {
        let db = db();
        let deck = db
            .write(|tx| tx.insert_deck(&new_deck(1, None, false), T0))
            .unwrap();
        let mut edited = deck.clone();
        edited.name = "  Renamed ".into();
        edited.description = "notes".into();
        let updated = db.write(move |tx| tx.update_deck(&edited, T1)).unwrap();
        assert_eq!(updated.name, "Renamed");
        assert_eq!(updated.description, "notes");
        assert_eq!(
            (updated.version, updated.updated_at),
            (deck.version + 1, T1)
        );
        assert_eq!(updated.created_at, T0);
    }

    #[test]
    fn installed_content_version_ignores_user_and_deleted_decks() {
        let db = db();
        assert_eq!(db.read(|tx| tx.installed_content_version()).unwrap(), None);
        db.write(|tx| {
            tx.insert_deck(&new_deck(1, Some("2026.01"), true), T0)?;
            tx.insert_deck(&new_deck(2, Some("2026.03"), false), T0)?;
            Ok(())
        })
        .unwrap();
        assert_eq!(
            db.read(|tx| tx.installed_content_version())
                .unwrap()
                .as_deref(),
            Some("2026.01"),
            "a user deck never counts"
        );
        let newest = db
            .write(|tx| tx.insert_deck(&new_deck(3, Some("2026.02"), true), T0))
            .unwrap();
        assert_eq!(
            db.read(|tx| tx.installed_content_version())
                .unwrap()
                .as_deref(),
            Some("2026.02")
        );
        db.write(move |tx| tx.soft_delete_deck(newest.id, newest.version, T1))
            .unwrap();
        assert_eq!(
            db.read(|tx| tx.installed_content_version())
                .unwrap()
                .as_deref(),
            Some("2026.01")
        );
    }

    // ---------------------------------------------------------------- note types

    #[test]
    fn every_note_type_kind_is_seeded_with_ordered_templates() {
        let db = db();
        for kind in NoteTypeKind::ALL.iter().copied() {
            let (note_type, templates) = db
                .read(move |tx| {
                    let note_type = tx.note_type(kind)?;
                    let templates = tx.templates(note_type.id)?;
                    Ok((note_type, templates))
                })
                .unwrap();
            assert_eq!(note_type.kind, kind);
            assert!(!templates.is_empty(), "{kind:?}");
            let ordinals: Vec<i64> = templates.iter().map(|t| i64::from(t.ordinal)).collect();
            assert_eq!(ordinals, (0..templates.len() as i64).collect::<Vec<_>>());
            assert!(templates.iter().all(|t| t.note_type_id == note_type.id));
            let id = note_type.id;
            assert_eq!(
                db.read(move |tx| tx.note_type_by_id(id)).unwrap(),
                Some(note_type)
            );
        }
    }

    #[test]
    fn unknown_note_type_id_reads_as_none_with_no_templates() {
        let db = db();
        assert_eq!(
            db.read(|tx| tx.note_type_by_id(NoteTypeId(9_999))).unwrap(),
            None
        );
        assert!(
            db.read(|tx| tx.templates(NoteTypeId(9_999)))
                .unwrap()
                .is_empty()
        );
    }

    // ---------------------------------------------------------------- notes

    #[test]
    fn notes_are_found_by_guid_and_duplicates_are_rejected() {
        let db = db();
        let note = db.write(|tx| Ok(new_note(tx, 1, "Hund", &["A1"]))).unwrap();
        let found = db.read(|tx| tx.note_by_guid(&guid_of(1))).unwrap();
        assert_eq!(found, Some(note.clone()));
        assert_eq!(db.read(|tx| tx.note_by_guid(&guid_of(2))).unwrap(), None);

        let duplicate = db
            .write(move |tx| {
                tx.insert_note(
                    &NewNote {
                        guid: guid_of(1),
                        note_type_id: note.note_type_id,
                        fields: note.fields.clone(),
                        tags: Vec::new(),
                    },
                    T0,
                )
            })
            .unwrap_err();
        assert_eq!(
            duplicate,
            AppError::Validation(ValidationError::new("guid", "duplicate"))
        );
    }

    #[test]
    fn note_update_rewrites_fields_sort_key_and_tag_order() {
        let db = db();
        let note = db
            .write(|tx| Ok(new_note(tx, 1, "Hund", &["A1", "Tiere"])))
            .unwrap();
        assert_eq!(note.tags, ["A1", "Tiere"]);
        let mut edited = note.clone();
        edited.fields.front = " Katze ".into();
        edited.tags = vec!["Tiere".into(), "B2".into()];
        let updated = db.write(move |tx| tx.update_note(&edited, T1)).unwrap();
        assert_eq!(updated.fields.front, "Katze");
        assert_eq!(updated.sort_field, "katze");
        assert_eq!(updated.tags, ["Tiere", "B2"]);
        assert_eq!(
            (updated.version, updated.updated_at),
            (note.version + 1, T1)
        );
    }

    #[test]
    fn note_update_conflicts_on_stale_version_and_reports_deleted() {
        let db = db();
        let note = db.write(|tx| Ok(new_note(tx, 1, "Hund", &[]))).unwrap();
        {
            let note = note.clone();
            db.write(move |tx| tx.update_note(&note, T1)).unwrap();
        }
        let stale = {
            let note = note.clone();
            db.write(move |tx| tx.update_note(&note, T1)).unwrap_err()
        };
        assert_eq!(
            stale,
            AppError::VersionConflict {
                entity: "note",
                current: note.version + 1
            }
        );
        let id = note.id;
        db.write(move |tx| tx.soft_delete_note(id, note.version + 1, T1))
            .unwrap();
        let deleted = db.write(move |tx| tx.update_note(&note, T1)).unwrap_err();
        assert_eq!(deleted, AppError::Deleted { entity: "note" });
    }

    #[test]
    fn invalid_note_update_changes_nothing() {
        let db = db();
        let note = db.write(|tx| Ok(new_note(tx, 1, "Hund", &["A1"]))).unwrap();
        let mut blank = note.clone();
        blank.fields.front = "   ".into();
        let error = db.write(move |tx| tx.update_note(&blank, T1)).unwrap_err();
        assert_eq!(error.code(), "VALIDATION_FAILED");
        let id = note.id;
        assert_eq!(db.read(move |tx| tx.note(id)).unwrap(), Some(note));
    }

    // ---------------------------------------------------------------- cards

    #[test]
    fn next_position_follows_the_highest_card_position() {
        let db = db();
        assert_eq!(db.read(|tx| tx.next_position()).unwrap(), 1);
        db.write(|tx| {
            let deck = tx.insert_deck(&new_deck(1, None, false), T0)?;
            new_card(tx, 1, deck.id, 41);
            Ok(())
        })
        .unwrap();
        assert_eq!(db.read(|tx| tx.next_position()).unwrap(), 42);
    }

    #[test]
    fn card_update_persists_scheduling_state_and_bumps_the_version() {
        let db = db();
        let (card, updated) = db
            .write(|tx| {
                let deck = tx.insert_deck(&new_deck(1, None, false), T0)?;
                let card = new_card(tx, 1, deck.id, 1);
                let mut edited = card.clone();
                edited.state = CardState::Review;
                edited.queue = QueueState::Review;
                edited.due_at = Some(T1);
                edited.fsrs.stability = Some(3.5);
                edited.fsrs.difficulty = Some(6.25);
                edited.fsrs.last_review_at = Some(T0);
                edited.reps = 2;
                edited.lapses = 1;
                let updated = tx.update_card(&edited, T1)?;
                Ok((card, updated))
            })
            .unwrap();
        assert_eq!(updated.version, card.version + 1);
        assert_eq!(updated.due_at, Some(T1));
        assert_eq!(
            (updated.fsrs.stability, updated.fsrs.difficulty),
            (Some(3.5), Some(6.25))
        );
        assert_eq!((updated.reps, updated.lapses), (2, 1));
        assert_eq!(updated.fsrs.last_review_at, Some(T0));
        assert_eq!(updated.updated_at, T1);
    }

    #[test]
    fn card_update_conflicts_when_stale_and_reports_deleted_cards() {
        let db = db();
        let card = db
            .write(|tx| {
                let deck = tx.insert_deck(&new_deck(1, None, false), T0)?;
                Ok(new_card(tx, 1, deck.id, 1))
            })
            .unwrap();
        let mut moved = card.clone();
        moved.position = 9;
        {
            let moved = moved.clone();
            db.write(move |tx| tx.update_card(&moved, T1)).unwrap();
        }
        let stale = {
            let moved = moved.clone();
            db.write(move |tx| tx.update_card(&moved, T1)).unwrap_err()
        };
        assert_eq!(
            stale,
            AppError::VersionConflict {
                entity: "card",
                current: card.version + 1
            }
        );
        let note = card.note_id;
        let note_version = db.read(move |tx| tx.note(note)).unwrap().unwrap().version;
        db.write(move |tx| tx.soft_delete_note(note, note_version, T1))
            .unwrap();
        let deleted = db.write(move |tx| tx.update_card(&moved, T1)).unwrap_err();
        assert_eq!(deleted, AppError::Deleted { entity: "card" });
    }

    #[test]
    fn due_cards_come_oldest_first_and_respect_deck_cutoff_and_limit() {
        let db = db();
        let (deck_a, deck_b, ids) = db
            .write(|tx| {
                let a = tx.insert_deck(&new_deck(1, None, false), T0)?;
                let b = tx.insert_deck(&new_deck(2, None, false), T0)?;
                let later = new_card(tx, 10, a.id, 1);
                let later = schedule(tx, later, 3_000);
                let earlier = new_card(tx, 11, a.id, 2);
                let earlier = schedule(tx, earlier, 1_000);
                let other_deck = new_card(tx, 12, b.id, 3);
                let other_deck = schedule(tx, other_deck, 2_000);
                let _new = new_card(tx, 13, a.id, 4);
                Ok((a.id, b.id, [later.id, earlier.id, other_deck.id]))
            })
            .unwrap();
        let [later, earlier, other] = ids;
        let due = |decks: Vec<DeckId>, until: i64, limit: u32| {
            db.read(move |tx| tx.due_card_ids(&decks, TimestampMs(until), limit))
                .unwrap()
        };
        assert_eq!(
            due(vec![deck_a, deck_b], 10_000, 10),
            [earlier, other, later]
        );
        assert_eq!(due(vec![deck_a], 10_000, 10), [earlier, later]);
        assert_eq!(due(vec![deck_a, deck_b], 2_000, 10), [earlier, other]);
        assert_eq!(due(vec![deck_a, deck_b], 10_000, 1), [earlier]);
        assert!(due(vec![deck_a], 999, 10).is_empty());
        assert!(due(Vec::new(), 10_000, 10).is_empty());
    }

    #[test]
    fn due_cards_skip_archived_and_deleted_ones() {
        let db = db();
        let (deck, kept) = db
            .write(|tx| {
                let deck = tx.insert_deck(&new_deck(1, None, false), T0)?;
                let kept = new_card(tx, 10, deck.id, 1);
                let kept = schedule(tx, kept, 1_000);
                let archived = new_card(tx, 11, deck.id, 2);
                let mut archived = schedule(tx, archived, 1_000);
                archived.archived_at = Some(T1);
                tx.update_card(&archived, T1)?;
                let doomed = new_card(tx, 12, deck.id, 3);
                let doomed = schedule(tx, doomed, 1_000);
                let note = tx.note(doomed.note_id)?.unwrap();
                tx.soft_delete_note(note.id, note.version, T1)?;
                Ok((deck.id, kept.id))
            })
            .unwrap();
        let ids = db
            .read(move |tx| tx.due_card_ids(&[deck], TimestampMs(10_000), 10))
            .unwrap();
        assert_eq!(ids, [kept]);
    }

    #[test]
    fn new_cards_follow_their_position_and_exclude_scheduled_ones() {
        let db = db();
        let (deck, ids) = db
            .write(|tx| {
                let deck = tx.insert_deck(&new_deck(1, None, false), T0)?;
                let third = new_card(tx, 10, deck.id, 30);
                let first = new_card(tx, 11, deck.id, 10);
                let second = new_card(tx, 12, deck.id, 20);
                let scheduled = new_card(tx, 13, deck.id, 5);
                schedule(tx, scheduled, 1_000);
                Ok((deck.id, [first.id, second.id, third.id]))
            })
            .unwrap();
        let new = |limit: u32, decks: Vec<DeckId>| {
            db.read(move |tx| tx.new_card_ids(&decks, limit)).unwrap()
        };
        assert_eq!(new(10, vec![deck]), ids);
        assert_eq!(new(2, vec![deck]), [ids[0], ids[1]]);
        assert!(new(10, Vec::new()).is_empty());
        assert!(new(10, vec![DeckId(9_999)]).is_empty());
    }

    // ---------------------------------------------------------------- settings

    #[test]
    fn settings_are_upserted_and_must_be_json() {
        let db = db();
        assert_eq!(db.read(|tx| tx.setting("theme")).unwrap(), None);
        db.write(|tx| tx.put_setting("theme", "\"dark\"", T0))
            .unwrap();
        db.write(|tx| tx.put_setting("theme", "\"light\"", T1))
            .unwrap();
        assert_eq!(
            db.read(|tx| tx.setting("theme")).unwrap().as_deref(),
            Some("\"light\"")
        );
        let invalid = db
            .write(|tx| tx.put_setting("theme", "dark", T1))
            .unwrap_err();
        assert_eq!(
            invalid,
            AppError::Validation(ValidationError::new("value", "json"))
        );
        assert_eq!(
            db.read(|tx| tx.setting("theme")).unwrap().as_deref(),
            Some("\"light\"")
        );
    }

    // ---------------------------------------------------------------- jobs

    fn new_job(kind: JobKind, total: Option<u64>) -> NewJob {
        NewJob {
            kind,
            operation_id: None,
            total,
        }
    }

    #[test]
    fn job_without_a_total_has_no_progress() {
        let db = db();
        let job = db
            .write(|tx| tx.insert_job(&new_job(JobKind::Backup, None), T0))
            .unwrap();
        assert_eq!(job.progress, None);
        assert_eq!(job.operation_id, None);
        assert_eq!(
            (job.kind, job.status, job.started_at),
            (JobKind::Backup, JobStatus::Queued, T0)
        );
    }

    #[test]
    fn job_result_and_progress_survive_an_update() {
        let db = db();
        let mut job = db
            .write(|tx| tx.insert_job(&new_job(JobKind::Export, Some(8)), T0))
            .unwrap();
        job.status = JobStatus::Succeeded;
        job.sequence = 3;
        job.progress = Some(JobProgress { done: 8, total: 8 });
        job.finished_at = Some(T1);
        job.result_json = Some(r#"{"type":"export"}"#.into());
        {
            let job = job.clone();
            db.write(move |tx| tx.update_job(&job)).unwrap();
        }
        let id = job.id.clone();
        assert_eq!(db.read(move |tx| tx.job(&id)).unwrap(), Some(job));
    }

    #[test]
    fn updating_a_job_with_a_non_numeric_id_is_not_found() {
        let db = db();
        let mut job = db
            .write(|tx| tx.insert_job(&new_job(JobKind::Import, None), T0))
            .unwrap();
        job.id = JobId("abc".into());
        job.sequence = 1;
        let error = db.write(move |tx| tx.update_job(&job)).unwrap_err();
        assert_eq!(error, AppError::NotFound { entity: "job" });
    }

    #[test]
    fn only_queued_and_running_jobs_are_unfinished_in_id_order() {
        let db = db();
        let ids = db
            .write(|tx| {
                let mut ids = Vec::new();
                for status in [
                    JobStatus::Running,
                    JobStatus::Succeeded,
                    JobStatus::Queued,
                    JobStatus::Cancelled,
                    JobStatus::Failed,
                ] {
                    let mut job = tx.insert_job(&new_job(JobKind::Import, None), T0)?;
                    if status != JobStatus::Queued {
                        job.status = status;
                        job.sequence = 1;
                        tx.update_job(&job)?;
                    }
                    ids.push((job.id, status));
                }
                Ok(ids)
            })
            .unwrap();
        let unfinished = db.read(|tx| tx.unfinished_jobs()).unwrap();
        let expected: Vec<_> = ids
            .into_iter()
            .filter(|(_, status)| !status.is_finished())
            .map(|(id, _)| id)
            .collect();
        assert_eq!(
            unfinished.iter().map(|j| j.id.clone()).collect::<Vec<_>>(),
            expected
        );
        assert_eq!(unfinished.len(), 2);
    }

    // ---------------------------------------------------------------- idempotency

    #[test]
    fn operation_ids_must_be_between_one_and_128_bytes() {
        let db = db();
        let recorded = RecordedOperation {
            operation: "x".into(),
            result_json: "1".into(),
        };
        for bad in [String::new(), "a".repeat(129)] {
            let recorded = recorded.clone();
            let error = db
                .write(move |tx| tx.record_operation(&bad, &recorded, T0))
                .unwrap_err();
            assert_eq!(
                error,
                AppError::Validation(ValidationError::new("operationId", "format"))
            );
        }
        let longest = "a".repeat(128);
        {
            let (longest, recorded) = (longest.clone(), recorded.clone());
            db.write(move |tx| tx.record_operation(&longest, &recorded, T0))
                .unwrap();
        }
        assert_eq!(
            db.read(move |tx| tx.recorded_operation(&longest)).unwrap(),
            Some(recorded)
        );
        assert_eq!(
            db.read(|tx| tx.recorded_operation("unknown")).unwrap(),
            None
        );
    }
}
