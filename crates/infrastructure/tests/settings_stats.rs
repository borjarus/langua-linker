//! Settings and statistics (plan Phase 12 steps 1–7) on the real SQLite unit of work: true
//! retention follows its written definition (Hard passes, first due review per card and study
//! day, interval before the rating), the forecast's first day equals the scheduler's due count,
//! no data is "not enough data", and settings are versioned, validated, drive the calendar and
//! the default preset, and block queued AI requests when consent is withdrawn.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use langualinker_application::management::{self, CreateDeck};
use langualinker_application::ports::{Clock, Rng};
use langualinker_application::repositories::{Repositories, UnitOfWork};
use langualinker_application::settings::{self, Consent, SettingsChange};
use langualinker_application::stats::{self, Ratio, StatsRange};
use langualinker_application::{AppError, AppResult, study};
use langualinker_domain::card::{Card, CardState, NewCard, QueueState};
use langualinker_domain::deck::{
    DeckConfigOptions, DeckType, NativeLanguage, NewDeck, TargetLanguage,
};
use langualinker_domain::note::{NewNote, NoteFields, NoteTypeKind};
use langualinker_domain::review::{CardSnapshot, Rating, ReviewKind, ReviewLog};
use langualinker_domain::{CardId, DeckId, Guid, ReviewLogId, TimestampMs, ValidationError};
use langualinker_infrastructure::sqlite::SqliteDatabase;
use rusqlite::params;

/// 2026-03-02 12:00 UTC; with the default 04:00 UTC cutoff the study day began at 04:00.
const NOON: i64 = 1_772_452_800_000;
const DAY: i64 = 86_400_000;

struct Fixed;

impl Clock for Fixed {
    fn now(&self) -> TimestampMs {
        TimestampMs(NOON)
    }
}

struct Seq(AtomicU8);

impl Rng for Seq {
    fn fill(&self, bytes: &mut [u8]) {
        for byte in bytes {
            *byte = self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
}

fn db() -> Arc<SqliteDatabase> {
    Arc::new(SqliteDatabase::open_in_memory(&Fixed).unwrap().database)
}

fn guid(seed: u64) -> Guid {
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&seed.to_le_bytes());
    bytes[8] = 0x7a;
    Guid::from_bytes(bytes)
}

/// A deck with `cards` basic notes; returns the deck and its cards.
fn deck(db: &Arc<SqliteDatabase>, seed: u64, cards: u64) -> (DeckId, Vec<CardId>) {
    db.write(move |tx| {
        let config = tx.default_config()?.id;
        let deck = tx.insert_deck(
            &NewDeck {
                guid: guid(seed),
                name: format!("Deck {seed}"),
                parent_id: None,
                description: String::new(),
                native_language: NativeLanguage::Pl,
                target_language: TargetLanguage::De,
                deck_type: DeckType::Simple,
                config_id: config,
                content_version: None,
                bundled: false,
            },
            TimestampMs(0),
        )?;
        let note_type = tx.note_type(NoteTypeKind::Basic)?;
        let mut ids = Vec::new();
        for index in 0..cards {
            let note = tx.insert_note(
                &NewNote {
                    guid: guid(seed * 1_000 + index + 1),
                    note_type_id: note_type.id,
                    fields: NoteFields {
                        front: format!("Wort {seed}/{index}"),
                        back: format!("słowo {index}"),
                    },
                    tags: Vec::new(),
                },
                TimestampMs(0),
            )?;
            let position = tx.next_position()?;
            ids.push(
                tx.insert_card(
                    &NewCard {
                        guid: guid(seed * 1_000_000 + index + 1),
                        note_id: note.id,
                        deck_id: deck.id,
                        template_ordinal: 0,
                        position,
                    },
                    TimestampMs(0),
                )?
                .id,
            );
        }
        Ok((deck.id, ids))
    })
    .unwrap()
}

/// Puts a card in Review with `interval` days, due at `due`.
fn schedule(tx: &mut dyn Repositories, card: CardId, interval: i64, due: i64) -> AppResult<Card> {
    let mut stored = tx.card(card)?.expect("card");
    stored.state = CardState::Review;
    stored.queue = QueueState::Review;
    stored.due_at = Some(TimestampMs(due));
    stored.fsrs.stability = Some(interval as f64 + 0.5);
    stored.fsrs.difficulty = Some(5.0);
    stored.fsrs.scheduled_days = interval;
    stored.reps = 3;
    tx.update_card(&stored, TimestampMs(0))
}

#[allow(clippy::too_many_arguments)]
fn log(
    tx: &mut dyn Repositories,
    card: &Card,
    op: &str,
    rating: Rating,
    kind: ReviewKind,
    state_before: CardState,
    due_before: i64,
    at: i64,
) -> AppResult<ReviewLog> {
    let mut before = CardSnapshot::of(card);
    before.state = state_before;
    before.due_at = Some(TimestampMs(due_before));
    tx.insert_review_log(&ReviewLog {
        id: ReviewLogId(0),
        card_id: card.id,
        session_id: None,
        operation_id: op.into(),
        rating,
        kind,
        reviewed_at: TimestampMs(at),
        duration_ms: 5_000,
        card_before: before,
        card_after: CardSnapshot::of(card),
        fsrs_version: "fsrs-6/test".into(),
        parameters_hash: "0".repeat(32),
        invalidated_at: None,
    })
}

/// Phase 12 DoD: retention includes Hard, counts the first review per card and day, uses the
/// interval before the rating, and the fixture's tooltip numbers come out exactly.
#[test]
fn true_retention_follows_its_definition_on_a_fixture() {
    let db = db();
    let (deck_id, cards) = deck(&db, 1, 16);
    let yesterday = NOON - DAY;
    db.write(move |tx| {
        // Young (interval 5 days): 12 first reviews on their due day — 2 Again, 3 Hard, 7 Good.
        let ratings = [
            Rating::Again,
            Rating::Again,
            Rating::Hard,
            Rating::Hard,
            Rating::Hard,
            Rating::Good,
            Rating::Good,
            Rating::Good,
            Rating::Good,
            Rating::Good,
            Rating::Good,
            Rating::Good,
        ];
        for (index, rating) in ratings.iter().enumerate() {
            let card = schedule(tx, cards[index], 5, yesterday)?;
            log(
                tx,
                &card,
                &format!("y{index}"),
                *rating,
                ReviewKind::Review,
                CardState::Review,
                yesterday,
                yesterday,
            )?;
        }
        // The same card again that day does not count.
        let again = tx.card(cards[2])?.unwrap();
        log(
            tx,
            &again,
            "repeat",
            Rating::Again,
            ReviewKind::Review,
            CardState::Review,
            yesterday,
            yesterday + 60_000,
        )?;
        // Learning, early and undone reviews do not count.
        let other = schedule(tx, cards[12], 5, yesterday)?;
        log(
            tx,
            &other,
            "learning",
            Rating::Again,
            ReviewKind::Learning,
            CardState::Learning,
            yesterday,
            yesterday,
        )?;
        let early = schedule(tx, cards[13], 30, NOON + 5 * DAY)?;
        log(
            tx,
            &early,
            "early",
            Rating::Again,
            ReviewKind::Review,
            CardState::Review,
            NOON + 5 * DAY,
            yesterday,
        )?;
        let undone = schedule(tx, cards[14], 30, yesterday)?;
        log(
            tx,
            &undone,
            "undone",
            Rating::Again,
            ReviewKind::Review,
            CardState::Review,
            yesterday,
            yesterday,
        )?;
        tx.invalidate_review_log("undone", TimestampMs(yesterday + 1))?;
        Ok(())
    })
    .unwrap();

    let result = db
        .write(move |tx| stats::stats(tx, TimestampMs(NOON), Some(deck_id), StatsRange::Month))
        .unwrap();
    assert_eq!(
        result.retention_young,
        Some(Ratio {
            passed: 10,
            total: 12
        })
    );
    assert_eq!(
        result.retention_mature, None,
        "one mature rating is not enough data"
    );
    assert_eq!(result.reviews_by_day.len(), 30);
    let last_two: Vec<u32> = result.reviews_by_day[28..]
        .iter()
        .map(|day| day.count)
        .collect();
    assert_eq!(
        last_two,
        vec![15, 0],
        "valid ratings per study day, undone excluded"
    );
    assert_eq!(result.reviews_by_day[28].active_ms, 15 * 5_000);
    assert_eq!((result.streak_days, result.longest_streak_days), (1, 1));
}

/// Phase 12 step 4: the forecast's first day equals the scheduler's due count for today.
#[test]
fn forecast_day_zero_matches_the_scheduler_and_counts_overdue_cards() {
    let db = db();
    let (_, cards) = deck(&db, 2, 10);
    db.write(move |tx| {
        schedule(tx, cards[0], 3, NOON - 2 * DAY)?; // overdue
        schedule(tx, cards[1], 3, NOON - 3_600_000)?; // due today
        schedule(tx, cards[2], 3, NOON + 3_600_000)?; // later today
        schedule(tx, cards[3], 3, NOON + DAY)?; // tomorrow
        schedule(tx, cards[4], 40, NOON + 40 * DAY)?; // beyond the forecast
        let mut suspended = schedule(tx, cards[5], 3, NOON)?;
        suspended.queue = QueueState::Suspended;
        tx.update_card(&suspended, TimestampMs(0))?;
        Ok(())
    })
    .unwrap();
    let (result, due) = db
        .write(move |tx| {
            let result = stats::stats(tx, TimestampMs(NOON), None, StatsRange::Month)?;
            let moment = study::study_moment(tx, TimestampMs(NOON))?;
            let due = study::scope_counts(tx, None, &moment)?;
            Ok((result, due))
        })
        .unwrap();
    assert_eq!(result.forecast[0].stored, due.review + due.learning);
    assert_eq!(result.forecast[0].stored, 3);
    assert_eq!(result.overdue, 1);
    assert_eq!(result.forecast[1].stored, 1);
    assert_eq!(result.forecast.iter().map(|day| day.stored).sum::<u32>(), 4);
    assert_eq!(result.states.suspended, 1);
    assert_eq!(
        (result.states.review, result.young, result.mature),
        (5, 4, 1)
    );
    assert_eq!(result.states.new, 4);
    assert_eq!(
        result.reviews_per_day,
        DeckConfigOptions::default().reviews_per_day
    );
}

#[test]
fn an_empty_collection_has_no_retention_and_unknown_decks_are_not_found() {
    let db = db();
    let result = db
        .write(|tx| stats::stats(tx, TimestampMs(NOON), None, StatsRange::All))
        .unwrap();
    assert_eq!(
        (result.retention_young, result.retention_mature),
        (None, None)
    );
    assert_eq!(
        result.reviews_by_day.len(),
        30,
        "'all' shows at least a month"
    );
    assert!(result.reviews_by_day.iter().all(|day| day.count == 0));
    assert_eq!(result.streak_days, 0);
    let missing = db
        .write(|tx| stats::stats(tx, TimestampMs(NOON), Some(DeckId(999)), StatsRange::Month))
        .unwrap_err();
    assert_eq!(missing, AppError::NotFound { entity: "deck" });
}

fn update(
    db: &Arc<SqliteDatabase>,
    version: u32,
    change: SettingsChange,
) -> AppResult<settings::SettingsEffects> {
    db.write(move |tx| settings::update(tx, version, change, TimestampMs(NOON)))
}

/// Phase 12 step 6: versioned settings, validation, calendar in `collection`, default preset.
#[test]
fn settings_are_versioned_validated_and_drive_the_calendar_and_new_decks() {
    let db = db();
    assert!(
        db.write(|tx| settings::initialize(tx, "Europe/Warsaw", TimestampMs(NOON)))
            .unwrap()
    );
    assert!(
        !db.write(|tx| settings::initialize(tx, "Asia/Tokyo", TimestampMs(NOON)))
            .unwrap(),
        "once"
    );
    let first = db.read(|tx| settings::read(tx)).unwrap();
    assert_eq!(first.time_zone, "Europe/Warsaw");
    assert_eq!(
        (first.day_cutoff_hour, first.daily_goal, first.doc.version),
        (4, 20, 1)
    );
    assert_eq!(first.default_target_language, TargetLanguage::De);
    assert_eq!(first.ai_consent, Consent::Unset);

    let effects = update(
        &db,
        1,
        SettingsChange {
            daily_goal: Some(35),
            time_zone: Some("Asia/Tokyo".into()),
            day_cutoff_hour: Some(6),
            reminder_time: Some(Some("07:30".into())),
            ..SettingsChange::default()
        },
    )
    .unwrap();
    assert!(effects.calendar_changed);
    let read = db.read(|tx| settings::read(tx)).unwrap();
    assert_eq!(
        (read.daily_goal, read.day_cutoff_hour, read.doc.version),
        (35, 6, 2)
    );
    assert_eq!(
        db.write(|tx| tx.collection_calendar()).unwrap(),
        ("Asia/Tokyo".to_owned(), 360)
    );
    assert_eq!(read.doc.reminder_time.as_deref(), Some("07:30"));

    // A stale form conflicts instead of overwriting.
    assert_eq!(
        update(
            &db,
            1,
            SettingsChange {
                daily_goal: Some(10),
                ..SettingsChange::default()
            }
        )
        .unwrap_err(),
        AppError::VersionConflict {
            entity: "settings",
            current: 2
        }
    );
    for (change, field) in [
        (
            SettingsChange {
                daily_goal: Some(0),
                ..SettingsChange::default()
            },
            "dailyGoal",
        ),
        (
            SettingsChange {
                time_zone: Some("Mars/Base".into()),
                ..SettingsChange::default()
            },
            "timeZone",
        ),
        (
            SettingsChange {
                day_cutoff_hour: Some(24),
                ..SettingsChange::default()
            },
            "dayCutoffHour",
        ),
        (
            SettingsChange {
                tts_rate: Some(2.0),
                ..SettingsChange::default()
            },
            "ttsRate",
        ),
        (
            SettingsChange {
                reminder_time: Some(Some("7:30".into())),
                ..SettingsChange::default()
            },
            "reminderTime",
        ),
        (
            SettingsChange {
                ai_endpoint: Some(Some("http://x.example".into())),
                ..SettingsChange::default()
            },
            "aiEndpoint",
        ),
        (
            SettingsChange {
                ai_provider: Some("nope".into()),
                ..SettingsChange::default()
            },
            "aiProvider",
        ),
        (
            SettingsChange {
                backup_retention: Some(0),
                ..SettingsChange::default()
            },
            "backupRetention",
        ),
        (
            SettingsChange {
                backup_target: Some(settings::BackupTarget::External),
                ..SettingsChange::default()
            },
            "backupTarget",
        ),
        (
            SettingsChange {
                default_preset_id: Some(langualinker_domain::DeckConfigId(99)),
                ..SettingsChange::default()
            },
            "defaultPresetId",
        ),
    ] {
        match update(&db, 2, change).unwrap_err() {
            AppError::Validation(ValidationError { field: actual, .. }) => {
                assert_eq!(actual, field)
            }
            other => panic!("{field}: {other:?}"),
        }
    }
    assert_eq!(
        db.read(|tx| settings::read(tx)).unwrap().doc.version,
        2,
        "nothing stored"
    );

    // New decks get the chosen preset.
    let preset = db
        .write(|tx| {
            tx.insert_config(
                &guid(77),
                "Powoli",
                &DeckConfigOptions::default(),
                TimestampMs(NOON),
            )
            .map(|config| config.id)
        })
        .unwrap();
    update(
        &db,
        2,
        SettingsChange {
            default_preset_id: Some(preset),
            ..SettingsChange::default()
        },
    )
    .unwrap();
    let rng = Seq(AtomicU8::new(1));
    let created = db
        .write(move |tx| {
            let input = CreateDeck {
                operation_id: "deck-1".into(),
                name: "Nowa".into(),
                parent_id: None,
                target_language: TargetLanguage::Es,
                deck_type: DeckType::Simple,
            };
            let id = management::create_deck(tx, &rng, &input, TimestampMs(NOON))?;
            Ok(tx.deck(id)?.unwrap().config_id)
        })
        .unwrap();
    assert_eq!(created, preset);
}

/// Phase 12 step 7: consent is versioned and revocable; withdrawing it blocks queued requests in
/// the same transaction, and a provider change drops the key that belonged to the old one.
#[test]
fn withdrawing_ai_consent_blocks_queued_requests_before_the_network() {
    let db = db();
    let (_, cards) = deck(&db, 3, 1);
    db.with_connection(move |conn| {
        let note: i64 = conn
            .query_row(
                "SELECT note_id FROM cards WHERE id = ?1",
                [cards[0].0],
                |row| row.get(0),
            )
            .unwrap();
        for (op, status) in [("a", "queued"), ("b", "sending"), ("c", "done")] {
            conn.execute(
                "INSERT INTO llm_requests (note_id, operation_id, variant, status, prompt_kind,
                     prompt_version, provider, model, cache_key, source_hash, created_at,
                     updated_at)
                 VALUES (?1, ?2, 0, ?3, 'wordSubstitution', 'p1', 'anthropic', 'm', 'k', 'h', 0, 0)",
                params![note, op, status],
            )
            .unwrap();
        }
        Ok(())
    })
    .unwrap();
    update(
        &db,
        1,
        SettingsChange {
            ai_consent: Some(Consent::Granted),
            ..SettingsChange::default()
        },
    )
    .unwrap();
    let granted = db.read(|tx| settings::read(tx)).unwrap();
    assert_eq!(granted.ai_consent, Consent::Granted);
    assert_eq!(
        granted.doc.ai.consent_version,
        Some(settings::CONSENT_VERSION)
    );
    assert_eq!(granted.doc.ai.consent_at, Some(NOON));

    let effects = update(
        &db,
        2,
        SettingsChange {
            ai_consent: Some(Consent::Declined),
            ai_provider: Some("anthropic".into()),
            ..SettingsChange::default()
        },
    )
    .unwrap();
    assert_eq!(effects.blocked_requests, 2);
    assert!(effects.remove_api_key);
    let statuses: Vec<String> = db
        .with_connection(|conn| {
            let mut statement = conn
                .prepare("SELECT status FROM llm_requests ORDER BY operation_id")
                .unwrap();
            Ok(statement
                .query_map([], |row| row.get(0))
                .unwrap()
                .map(Result::unwrap)
                .collect())
        })
        .unwrap();
    assert_eq!(statuses, vec!["blocked", "blocked", "done"]);
    let read = db.read(|tx| settings::read(tx)).unwrap();
    assert_eq!(read.ai_consent, Consent::Declined);
    assert_eq!(read.doc.ai.model.as_deref(), Some("claude-haiku-5-5"));

    // An answer given to an older consent text counts as unset.
    db.write(|tx| {
        let mut doc = settings::load_doc(tx)?;
        doc.ai.consent_version = Some(0);
        tx.put_setting(
            settings::SETTINGS_KEY,
            &serde_json::to_string(&doc).unwrap(),
            TimestampMs(NOON),
        )
    })
    .unwrap();
    assert_eq!(
        db.read(|tx| settings::read(tx)).unwrap().ai_consent,
        Consent::Unset
    );
}

/// Gemini (BYOK): choosing it picks its first model, needs no endpoint and maps to its protocol.
#[test]
fn gemini_is_a_provider_with_its_own_protocol_and_models() {
    use langualinker_application::llm;
    use langualinker_application::ports::LlmProtocol;

    let db = db();
    update(
        &db,
        1,
        SettingsChange {
            ai_consent: Some(Consent::Granted),
            ai_provider: Some("gemini".into()),
            ..SettingsChange::default()
        },
    )
    .unwrap();
    let read = db.read(|tx| settings::read(tx)).unwrap();
    assert_eq!(read.doc.ai.provider.as_deref(), Some("gemini"));
    assert_eq!(read.doc.ai.model.as_deref(), Some("gemini-3.8-flash"));
    assert_eq!(read.doc.ai.endpoint, None);

    let config = db.read(llm::provider_config).unwrap();
    assert_eq!(config.protocol, LlmProtocol::Gemini);
    assert_eq!(config.model, "gemini-3.8-flash");
    assert_eq!(config.endpoint, None);
    assert!(config.requires_key);

    let provider = settings::AI_PROVIDERS
        .iter()
        .find(|provider| provider.id == "gemini")
        .unwrap();
    assert!(!provider.custom_endpoint);
    assert!(provider.models.contains(&"gemini-3.7-flash"));
}
