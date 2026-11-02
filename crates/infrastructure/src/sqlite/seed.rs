//! Deterministic large collection for performance checks (plan step 25): 50 000 cards with
//! 500 000 review logs by default. Written with raw SQL in one transaction, so it takes seconds,
//! not minutes; the data is plausible, not scheduled by FSRS.

use langualinker_application::AppResult;
use rusqlite::{Connection, params};

use super::map_sqlite;

#[derive(Debug, Clone, Copy)]
pub struct SeedSize {
    pub decks: i64,
    pub cards: i64,
    pub logs_per_card: i64,
}

impl SeedSize {
    pub const PLAN: Self = Self {
        decks: 10,
        cards: 50_000,
        logs_per_card: 10,
    };
}

/// Day 0 of the seeded collection: 2026-03-02T00:00:00Z.
pub const SEED_EPOCH_MS: i64 = 1_772_409_600_000;
const DAY_MS: i64 = 86_400_000;

/// Fills an empty, migrated collection. Card `i` is New when `i % 5 == 0`, otherwise Review with a
/// due date spread over ±60 days around day 0 (deterministic, no RNG).
pub fn seed_collection(conn: &mut Connection, size: SeedSize) -> AppResult<()> {
    let tx = conn.transaction().map_err(map_sqlite)?;
    for deck in 1..=size.decks {
        tx.execute(
            "INSERT INTO decks (id, guid, name, native_language, target_language, config_id,
                 created_at, updated_at)
             VALUES (?1, 'seed-deck-' || ?1, 'Deck ' || ?1, 1, 1, 1, ?2, ?2)",
            params![deck, SEED_EPOCH_MS],
        )
        .map_err(map_sqlite)?;
        tx.execute(
            "INSERT INTO deck_settings (deck_id, deck_type, updated_at) VALUES (?1, 3, ?2)",
            params![deck, SEED_EPOCH_MS],
        )
        .map_err(map_sqlite)?;
    }
    {
        let mut note = tx
            .prepare(
                "INSERT INTO notes (id, guid, note_type_id, fields, sort_field, created_at, updated_at)
                 VALUES (?1, 'seed-note-' || ?1, 1, json_object('front', 'Wort ' || ?1, 'back', 'słowo ' || ?1),
                     'wort ' || ?1, ?2, ?2)",
            )
            .map_err(map_sqlite)?;
        let mut card = tx
            .prepare(
                "INSERT INTO cards (id, guid, note_id, deck_id, template_ordinal, position, state,
                     queue, due_at, stability, difficulty, last_review_at, reps, lapses,
                     created_at, updated_at)
                 VALUES (?1, 'seed-card-' || ?1, ?1, ?2, 0, ?1, ?3, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)",
            )
            .map_err(map_sqlite)?;
        let mut log = tx
            .prepare(
                "INSERT INTO review_logs (card_id, operation_id, rating, kind, reviewed_at,
                     duration_ms, card_before, card_after, fsrs_version, parameters_hash)
                 VALUES (?1, ?2, ?3, 1, ?4, 4000, '{}', '{}', 'FSRS-6', 'seed')",
            )
            .map_err(map_sqlite)?;
        for id in 1..=size.cards {
            note.execute(params![id, SEED_EPOCH_MS])
                .map_err(map_sqlite)?;
            let deck = (id - 1) % size.decks + 1;
            let review = id % 5 != 0;
            let (state, due, stability, difficulty, last, reps) = if review {
                let offset_days = (id * 7919) % 121 - 60;
                (
                    2,
                    Some(SEED_EPOCH_MS + offset_days * DAY_MS),
                    Some(1.0 + (id % 300) as f64),
                    Some(1.0 + (id % 9) as f64),
                    Some(SEED_EPOCH_MS - DAY_MS),
                    size.logs_per_card,
                )
            } else {
                (0, None, None, None, None, 0)
            };
            card.execute(params![
                id,
                deck,
                state,
                due,
                stability,
                difficulty,
                last,
                reps,
                id % 3,
                SEED_EPOCH_MS
            ])
            .map_err(map_sqlite)?;
            for n in 0..size.logs_per_card {
                let at = SEED_EPOCH_MS - (n + 1) * DAY_MS - (id % 1000) * 1000;
                log.execute(params![id, format!("seed-{id}-{n}"), 1 + (id + n) % 4, at])
                    .map_err(map_sqlite)?;
            }
        }
    }
    tx.commit().map_err(map_sqlite)?;
    conn.execute_batch("ANALYZE").map_err(map_sqlite)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::SqliteDatabase;
    use crate::sqlite::tests::FixedClock;

    const SMALL: SeedSize = SeedSize {
        decks: 3,
        cards: 50,
        logs_per_card: 4,
    };

    fn database() -> SqliteDatabase {
        SqliteDatabase::open_in_memory(&FixedClock(SEED_EPOCH_MS))
            .unwrap()
            .database
    }

    fn seeded(size: SeedSize) -> SqliteDatabase {
        let db = database();
        db.with_connection(move |conn| seed_collection(conn, size))
            .unwrap();
        db
    }

    fn count(db: &SqliteDatabase, sql: &'static str) -> i64 {
        db.with_connection(move |conn| {
            conn.query_row(sql, [], |row| row.get(0))
                .map_err(map_sqlite)
        })
        .unwrap()
    }

    #[test]
    fn plan_size_matches_the_documented_collection() {
        let plan = SeedSize::PLAN;
        assert_eq!(
            (plan.decks, plan.cards, plan.cards * plan.logs_per_card),
            (10, 50_000, 500_000)
        );
    }

    #[test]
    fn seeds_the_requested_number_of_rows() {
        let db = seeded(SMALL);
        // Built-in note types and the default preset exist already; the seed adds only its own.
        assert_eq!(
            count(
                &db,
                "SELECT COUNT(*) FROM decks WHERE guid LIKE 'seed-deck-%'"
            ),
            3
        );
        assert_eq!(count(&db, "SELECT COUNT(*) FROM deck_settings"), 3);
        assert_eq!(count(&db, "SELECT COUNT(*) FROM notes"), 50);
        assert_eq!(count(&db, "SELECT COUNT(*) FROM cards"), 50);
        assert_eq!(count(&db, "SELECT COUNT(*) FROM review_logs"), 200);
    }

    #[test]
    fn cards_are_spread_round_robin_over_the_decks() {
        let db = seeded(SMALL);
        assert_eq!(count(&db, "SELECT deck_id FROM cards WHERE id = 1"), 1);
        assert_eq!(count(&db, "SELECT deck_id FROM cards WHERE id = 3"), 3);
        assert_eq!(count(&db, "SELECT deck_id FROM cards WHERE id = 4"), 1);
        assert_eq!(count(&db, "SELECT COUNT(DISTINCT deck_id) FROM cards"), 3);
    }

    #[test]
    fn every_fifth_card_is_new_and_the_rest_are_reviews() {
        let db = seeded(SMALL);
        assert_eq!(count(&db, "SELECT COUNT(*) FROM cards WHERE state = 0"), 10);
        assert_eq!(
            count(
                &db,
                "SELECT COUNT(*) FROM cards WHERE state = 0
                 AND id % 5 = 0 AND due_at IS NULL AND reps = 0 AND last_review_at IS NULL"
            ),
            10
        );
        assert_eq!(
            count(
                &db,
                "SELECT COUNT(*) FROM cards WHERE state = 2
                 AND id % 5 != 0 AND due_at IS NOT NULL AND reps = 4
                 AND stability IS NOT NULL AND difficulty IS NOT NULL"
            ),
            40
        );
    }

    #[test]
    fn review_due_dates_stay_within_sixty_days_of_the_epoch() {
        let db = seeded(SeedSize {
            decks: 2,
            cards: 400,
            logs_per_card: 1,
        });
        let window = 60 * DAY_MS;
        let outside = count(
            &db,
            "SELECT COUNT(*) FROM cards WHERE state = 2 AND ABS(due_at - 1772409600000) > 5184000000",
        );
        assert_eq!(outside, 0, "window is ±{window} ms");
        assert!(
            count(
                &db,
                "SELECT COUNT(*) FROM cards WHERE due_at < 1772409600000"
            ) > 0
        );
        assert!(
            count(
                &db,
                "SELECT COUNT(*) FROM cards WHERE due_at > 1772409600000"
            ) > 0
        );
    }

    #[test]
    fn review_logs_precede_the_epoch_and_have_unique_operation_ids() {
        let db = seeded(SMALL);
        assert_eq!(
            count(
                &db,
                "SELECT COUNT(*) FROM review_logs WHERE reviewed_at >= 1772409600000"
            ),
            0
        );
        assert_eq!(
            count(&db, "SELECT COUNT(DISTINCT operation_id) FROM review_logs"),
            200
        );
        assert_eq!(
            count(
                &db,
                "SELECT COUNT(*) FROM review_logs WHERE rating NOT BETWEEN 1 AND 4"
            ),
            0
        );
    }

    #[test]
    fn seeding_is_deterministic() {
        let sql = "SELECT COALESCE(SUM(due_at), 0) + SUM(reps) * 7 + SUM(lapses) * 13 FROM cards";
        assert_eq!(count(&seeded(SMALL), sql), count(&seeded(SMALL), sql));
    }

    #[test]
    fn a_collection_with_no_cards_still_gets_its_decks() {
        let db = seeded(SeedSize {
            decks: 2,
            cards: 0,
            logs_per_card: 3,
        });
        assert_eq!(count(&db, "SELECT COUNT(*) FROM cards"), 0);
        assert_eq!(count(&db, "SELECT COUNT(*) FROM review_logs"), 0);
        assert_eq!(
            count(
                &db,
                "SELECT COUNT(*) FROM decks WHERE guid LIKE 'seed-deck-%'"
            ),
            2
        );
    }

    #[test]
    fn seeding_twice_fails_and_leaves_the_first_seed_intact() {
        let db = seeded(SMALL);
        let again = db.with_connection(|conn| seed_collection(conn, SMALL));
        assert!(again.is_err());
        assert_eq!(count(&db, "SELECT COUNT(*) FROM cards"), 50);
        assert_eq!(count(&db, "SELECT COUNT(*) FROM review_logs"), 200);
    }
}
