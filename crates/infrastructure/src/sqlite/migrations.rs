//! Numbered, checksummed migrations (plan step 21, 23).
//!
//! - Each migration runs in its own `IMMEDIATE` transaction together with its bookkeeping row, so
//!   a failure (constraint, disk full, process kill) leaves the database at the previous version.
//! - Applied migrations are checked against their SHA-256 on every open; an edited migration is
//!   refused instead of silently diverging.
//! - Downgrade policy: none. A database newer than this build is never touched
//!   (`SchemaTooNew`); the user restores a backup or updates the app.
//! - Before upgrading an existing database a copy is written next to it with `VACUUM INTO`
//!   (`<name>.pre-v<N>.sqlite`). The copy doubles as the disk-space check: if it cannot be
//!   written, nothing is migrated. It is kept until the next upgrade replaces it.

use std::path::{Path, PathBuf};

use langualinker_application::{AppError, AppResult};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use sha2::{Digest, Sha256};

use super::map_sqlite;

#[derive(Debug, Clone, Copy)]
pub struct Migration {
    pub version: i64,
    pub name: &'static str,
    pub sql: &'static str,
}

impl Migration {
    pub fn checksum(&self) -> String {
        Sha256::digest(self.sql.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}

/// The production migration list; versions start at 1 and have no gaps.
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "initial",
        sql: include_str!("../../migrations/0001_initial.sql"),
    },
    Migration {
        version: 2,
        name: "bundled_content",
        sql: include_str!("../../migrations/0002_bundled_content.sql"),
    },
    Migration {
        version: 3,
        name: "scheduler",
        sql: include_str!("../../migrations/0003_scheduler.sql"),
    },
];

/// Highest schema version this build understands.
pub fn latest_version(migrations: &[Migration]) -> i64 {
    migrations.last().map_or(0, |migration| migration.version)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationReport {
    pub from_version: i64,
    pub to_version: i64,
    /// Copy written before an upgrade of an existing database.
    pub safety_copy: Option<PathBuf>,
}

const BOOKKEEPING: &str = "CREATE TABLE IF NOT EXISTS schema_migrations (
    version     INTEGER PRIMARY KEY,
    name        TEXT    NOT NULL,
    checksum    TEXT    NOT NULL,
    applied_at  INTEGER NOT NULL
)";

/// Reads the applied version without changing anything (backups use this too).
pub fn applied_version(conn: &Connection) -> AppResult<i64> {
    let has_table: bool = conn
        .query_row(
            "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'schema_migrations')",
            [],
            |row| row.get(0),
        )
        .map_err(map_sqlite)?;
    if !has_table {
        return Ok(0);
    }
    conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get(0),
    )
    .map_err(map_sqlite)
}

/// Brings the database to the latest version. `path` is the database file (`None` in memory).
pub fn migrate(
    conn: &mut Connection,
    path: Option<&Path>,
    migrations: &[Migration],
    now_ms: i64,
) -> AppResult<MigrationReport> {
    check_sequence(migrations)?;
    let latest = latest_version(migrations);
    let from_version = applied_version(conn)?;
    if from_version > latest {
        return Err(AppError::SchemaTooNew {
            found: from_version,
            supported: latest,
        });
    }
    verify_checksums(conn, migrations, from_version)?;
    if from_version == latest {
        return Ok(MigrationReport {
            from_version,
            to_version: latest,
            safety_copy: None,
        });
    }

    let safety_copy = match path {
        Some(path) if from_version > 0 => Some(write_safety_copy(conn, path, latest)?),
        _ => None,
    };
    conn.execute(BOOKKEEPING, []).map_err(map_sqlite)?;
    for migration in migrations.iter().filter(|m| m.version > from_version) {
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite)?;
        tx.execute_batch(migration.sql).map_err(map_sqlite)?;
        tx.execute(
            "INSERT INTO schema_migrations (version, name, checksum, applied_at) VALUES (?1, ?2, ?3, ?4)",
            params![migration.version, migration.name, migration.checksum(), now_ms],
        )
        .map_err(map_sqlite)?;
        tx.pragma_update(None, "user_version", migration.version)
            .map_err(map_sqlite)?;
        tx.commit().map_err(map_sqlite)?;
        tracing::info!(schema_version = migration.version, "migration applied");
    }
    Ok(MigrationReport {
        from_version,
        to_version: latest,
        safety_copy,
    })
}

fn check_sequence(migrations: &[Migration]) -> AppResult<()> {
    let ordered = migrations
        .iter()
        .enumerate()
        .all(|(index, migration)| migration.version == index as i64 + 1);
    if ordered {
        Ok(())
    } else {
        Err(AppError::Internal(
            "migration versions must be 1..N without gaps".into(),
        ))
    }
}

fn verify_checksums(conn: &Connection, migrations: &[Migration], applied: i64) -> AppResult<()> {
    for migration in migrations.iter().filter(|m| m.version <= applied) {
        let stored: Option<String> = conn
            .query_row(
                "SELECT checksum FROM schema_migrations WHERE version = ?1",
                [migration.version],
                |row| row.get(0),
            )
            .optional()
            .map_err(map_sqlite)?;
        if stored.as_deref() != Some(migration.checksum().as_str()) {
            return Err(AppError::Internal(format!(
                "migration {} checksum mismatch",
                migration.version
            )));
        }
    }
    Ok(())
}

fn write_safety_copy(conn: &Connection, path: &Path, target: i64) -> AppResult<PathBuf> {
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("collection");
    let copy = path.with_file_name(format!("{stem}.pre-v{target}.sqlite"));
    if copy.exists() {
        std::fs::remove_file(&copy).map_err(|error| AppError::Internal(error.to_string()))?;
    }
    let copy_text = copy
        .to_str()
        .ok_or_else(|| AppError::Internal("database path is not UTF-8".into()))?;
    conn.execute("VACUUM INTO ?1", [copy_text])
        .map_err(|error| {
            // A partial copy is useless and wastes the space that was missing.
            let _ = std::fs::remove_file(&copy);
            map_sqlite(error)
        })?;
    Ok(copy)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Schema v1 only; the synthetic v2 migrations below build on it.
    const BASE: &[Migration] = &[MIGRATIONS[0]];

    fn memory() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        conn
    }

    const V2_OK: Migration = Migration {
        version: 2,
        name: "add_column",
        sql: "ALTER TABLE decks ADD COLUMN color TEXT;",
    };

    /// Fails half-way: the first statement succeeds, the second violates a constraint.
    const V2_FAILS: Migration = Migration {
        version: 2,
        name: "broken",
        sql: "UPDATE decks SET name = name || ' (v2)';
              INSERT INTO decks (guid, name, native_language, target_language, config_id, created_at, updated_at)
              VALUES ('dup', 'x', 1, 1, 999, 0, 0);",
    };

    fn insert_deck(conn: &Connection) {
        conn.execute(
            "INSERT INTO decks (guid, name, native_language, target_language, config_id, created_at, updated_at)
             VALUES ('d1', 'Deutsch', 1, 1, 1, 0, 0)",
            [],
        )
        .unwrap();
    }

    #[test]
    fn fresh_database_reaches_the_latest_version() {
        let mut conn = memory();
        let latest = latest_version(MIGRATIONS);
        let report = migrate(&mut conn, None, MIGRATIONS, 1).unwrap();
        assert_eq!((report.from_version, report.to_version), (0, latest));
        assert_eq!(applied_version(&conn).unwrap(), latest);
        let user_version: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(user_version, latest);
        // Idempotent: a second run changes nothing.
        let again = migrate(&mut conn, None, MIGRATIONS, 2).unwrap();
        assert_eq!((again.from_version, again.to_version), (latest, latest));
    }

    #[test]
    fn upgrade_keeps_existing_rows() {
        let mut conn = memory();
        migrate(&mut conn, None, BASE, 1).unwrap();
        insert_deck(&conn);
        migrate(&mut conn, None, &[MIGRATIONS[0], V2_OK], 2).unwrap();
        let (name, color): (String, Option<String>) = conn
            .query_row("SELECT name, color FROM decks WHERE guid = 'd1'", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!((name.as_str(), color), ("Deutsch", None));
    }

    #[test]
    fn failed_migration_rolls_back_to_the_previous_version() {
        let mut conn = memory();
        migrate(&mut conn, None, BASE, 1).unwrap();
        insert_deck(&conn);
        let error = migrate(&mut conn, None, &[MIGRATIONS[0], V2_FAILS], 2).unwrap_err();
        assert!(matches!(error, AppError::Internal(_)), "{error:?}");
        assert_eq!(applied_version(&conn).unwrap(), 1);
        let name: String = conn
            .query_row("SELECT name FROM decks WHERE guid = 'd1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            name, "Deutsch",
            "the first statement of v2 must be rolled back"
        );
    }

    #[test]
    fn newer_database_is_refused_and_untouched() {
        let mut conn = memory();
        migrate(&mut conn, None, &[MIGRATIONS[0], V2_OK], 1).unwrap();
        let error = migrate(&mut conn, None, BASE, 2).unwrap_err();
        assert_eq!(
            error,
            AppError::SchemaTooNew {
                found: 2,
                supported: 1
            }
        );
        assert_eq!(applied_version(&conn).unwrap(), 2);
    }

    #[test]
    fn edited_migration_is_detected() {
        let mut conn = memory();
        migrate(&mut conn, None, BASE, 1).unwrap();
        let edited = Migration {
            sql: "-- edited\nSELECT 1;",
            ..MIGRATIONS[0]
        };
        let error = migrate(&mut conn, None, &[edited], 2).unwrap_err();
        assert!(
            matches!(error, AppError::Internal(ref m) if m.contains("checksum")),
            "{error:?}"
        );
    }

    #[test]
    fn gaps_in_the_migration_list_are_rejected() {
        let mut conn = memory();
        let gap = Migration {
            version: 3,
            ..V2_OK
        };
        assert!(migrate(&mut conn, None, &[MIGRATIONS[0], gap], 1).is_err());
        assert_eq!(applied_version(&conn).unwrap(), 0);
    }

    #[test]
    fn upgrade_of_a_file_writes_a_safety_copy_first() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("collection.sqlite");
        let mut conn = Connection::open(&path).unwrap();
        migrate(&mut conn, Some(&path), BASE, 1).unwrap();
        insert_deck(&conn);
        // A failing upgrade still leaves the original database and the copy intact.
        assert!(migrate(&mut conn, Some(&path), &[MIGRATIONS[0], V2_FAILS], 2).is_err());
        let copy = dir.path().join("collection.pre-v2.sqlite");
        let copied = Connection::open(&copy).unwrap();
        assert_eq!(applied_version(&copied).unwrap(), 1);
        let decks: i64 = copied
            .query_row("SELECT COUNT(*) FROM decks", [], |r| r.get(0))
            .unwrap();
        assert_eq!(decks, 1);
        drop(copied);
        // Retrying with a fixed migration succeeds and replaces the copy.
        let report = migrate(&mut conn, Some(&path), &[MIGRATIONS[0], V2_OK], 3).unwrap();
        assert_eq!(report.safety_copy.as_deref(), Some(copy.as_path()));
        assert_eq!(applied_version(&conn).unwrap(), 2);
    }

    /// Fixture: a v1 collection with a deck, note and card upgrades to v2 without losing rows;
    /// the new provenance table follows its note on delete.
    #[test]
    fn v1_collection_upgrades_to_bundled_content_provenance() {
        let mut conn = memory();
        migrate(&mut conn, None, BASE, 1).unwrap();
        insert_deck(&conn);
        conn.execute_batch(
            "INSERT INTO notes (id, guid, note_type_id, fields, sort_field, created_at, updated_at)
                 VALUES (7, 'n7', 1, '{\"front\":\"Hund\",\"back\":\"pies\"}', 'hund', 0, 0);
             INSERT INTO cards (guid, note_id, deck_id, template_ordinal, created_at, updated_at)
                 SELECT 'c7', 7, id, 0, 0, 0 FROM decks WHERE guid = 'd1';",
        )
        .unwrap();
        let report = migrate(&mut conn, None, &MIGRATIONS[..2], 2).unwrap();
        assert_eq!((report.from_version, report.to_version), (1, 2));
        let cards: i64 = conn
            .query_row("SELECT COUNT(*) FROM cards WHERE note_id = 7", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(cards, 1);
        conn.execute(
            "INSERT INTO bundled_notes (note_id, deck_guid, content_version) VALUES (7, 'd1', '2026.11.04')",
            [],
        )
        .unwrap();
        // Archiving by import requires a removal version.
        assert!(
            conn.execute(
                "UPDATE bundled_notes SET archived_by_import = 1 WHERE note_id = 7",
                []
            )
            .is_err()
        );
        conn.execute("DELETE FROM cards WHERE note_id = 7", [])
            .unwrap();
        conn.execute("DELETE FROM notes WHERE id = 7", []).unwrap();
        let left: i64 = conn
            .query_row("SELECT COUNT(*) FROM bundled_notes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 0);
    }

    /// Fixture: a v2 collection with a finished session, a rating and an undo entry in the old
    /// bare-array form upgrades to v3; sessions default to unrestricted and the entry still reads.
    #[test]
    fn v2_collection_upgrades_to_scheduler_sessions() {
        let mut conn = memory();
        migrate(&mut conn, None, &MIGRATIONS[..2], 1).unwrap();
        insert_deck(&conn);
        conn.execute_batch(
            "INSERT INTO notes (id, guid, note_type_id, fields, sort_field, created_at, updated_at)
                 VALUES (7, 'n7', 1, '{\"front\":\"Hund\",\"back\":\"pies\"}', 'hund', 0, 0);
             INSERT INTO cards (id, guid, note_id, deck_id, template_ordinal, created_at, updated_at)
                 SELECT 9, 'c9', 7, id, 0, 0, 0 FROM decks WHERE guid = 'd1';
             INSERT INTO study_sessions (id, status, started_at, created_at, updated_at)
                 VALUES (3, 'finished', 0, 0, 0);
             INSERT INTO review_logs (id, card_id, session_id, operation_id, rating, kind,
                     reviewed_at, duration_ms, card_before, card_after, fsrs_version, parameters_hash)
                 VALUES (5, 9, 3, 'op', 3, 0, 0, 0, '{}', '{}', 'fsrs-6', 'h');
             INSERT INTO review_undo_entries (session_id, review_log_id, created_at)
                 VALUES (3, 5, 0);",
        )
        .unwrap();
        let report = migrate(&mut conn, None, MIGRATIONS, 2).unwrap();
        assert_eq!((report.from_version, report.to_version), (2, 3));
        let (restricted, changes): (bool, String) = conn
            .query_row(
                "SELECT s.restricted, u.sibling_changes FROM study_sessions s
                 JOIN review_undo_entries u ON u.session_id = s.id WHERE s.id = 3",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert!(!restricted);
        let decoded = crate::sqlite::codec::undo_changes_from_json(&changes).unwrap();
        assert!(decoded.siblings.is_empty() && decoded.leech_tagged.is_none());
        assert!(
            conn.execute("UPDATE study_sessions SET restricted = 2 WHERE id = 3", [])
                .is_err()
        );
    }
}
