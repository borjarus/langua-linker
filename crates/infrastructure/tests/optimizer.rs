//! FSRS optimizer on the real SQLite unit of work (plan Phase 18 DoD): the dataset of a preset's
//! cards with its exclusions, a cancellable job that stores a proposed run, apply and revert at
//! the shown preset version, stale proposals, idempotent retries, and history left untouched.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use langualinker_application::jobs::{JobEvent, JobId, JobRecord, JobService, JobStatus};
use langualinker_application::optimizer::{
    self, OptimizerService, OptimizerView, RunAction, RunStatus,
};
use langualinker_application::ports::{Clock, CollectionScope, EventSink};
use langualinker_application::repositories::{Repositories, UnitOfWork};
use langualinker_application::{AppError, AppResult};
use langualinker_domain::card::NewCard;
use langualinker_domain::deck::{
    DeckConfigOptions, DeckType, NativeLanguage, NewDeck, TargetLanguage,
};
use langualinker_domain::fsrs::{
    DEFAULT_WEIGHTS, FsrsParameters, MAX_INTERVAL_DAYS, MemoryState, Phase, ReviewInput,
    SchedulerConfig,
};
use langualinker_domain::note::{NewNote, NoteFields, NoteTypeKind};
use langualinker_domain::review::{CardSnapshot, Rating, ReviewKind, ReviewLog};
use langualinker_domain::{CardId, DeckConfigId, Guid, ReviewLogId, TimestampMs};
use langualinker_infrastructure::sqlite::SqliteDatabase;

/// 2026-03-02 12:00 UTC.
const NOON: i64 = 1_772_452_800_000;
const DAY: i64 = 86_400_000;

struct Fixed;

impl Clock for Fixed {
    fn now(&self) -> TimestampMs {
        TimestampMs(NOON)
    }
}

#[derive(Default)]
struct Recorder {
    jobs: Mutex<Vec<JobEvent>>,
}

impl EventSink for Recorder {
    fn job_progress(&self, event: &JobEvent) {
        self.jobs.lock().unwrap().push(event.clone());
    }

    fn collection_changed(&self, _: &[CollectionScope], _: TimestampMs) {}
}

struct Setup {
    db: Arc<SqliteDatabase>,
    jobs: JobService<SqliteDatabase>,
    optimizer: OptimizerService<SqliteDatabase>,
    events: Arc<Recorder>,
}

fn setup() -> Setup {
    let db = Arc::new(SqliteDatabase::open_in_memory(&Fixed).unwrap().database);
    let events = Arc::new(Recorder::default());
    let jobs = JobService::new(db.clone(), events.clone(), Arc::new(Fixed));
    let optimizer = OptimizerService::new(db.clone(), jobs.clone(), Arc::new(Fixed), |id| {
        format!(r#"{{"type":"optimize","runId":"{id}"}}"#)
    });
    Setup {
        db,
        jobs,
        optimizer,
        events,
    }
}

fn guid(seed: u64) -> Guid {
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&seed.to_le_bytes());
    bytes[8] = 0x0f;
    Guid::from_bytes(bytes)
}

fn write<T: Send + 'static>(
    db: &Arc<SqliteDatabase>,
    work: impl FnOnce(&mut dyn Repositories) -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    db.write(work)
}

/// A preset with one deck and `cards` cards, each with a learning answer, a same-day step and
/// `reviews` answers on later days that follow the weights `truth`. Returns the preset.
fn collection(db: &Arc<SqliteDatabase>, seed: u64, cards: u64, reviews: usize) -> DeckConfigId {
    write(db, move |tx| {
        let preset = tx
            .insert_config(
                &guid(seed),
                "Optimized",
                &DeckConfigOptions::default(),
                TimestampMs(0),
            )?
            .id;
        let deck = tx
            .insert_deck(
                &NewDeck {
                    guid: guid(seed + 1),
                    name: format!("Deck {seed}"),
                    parent_id: None,
                    description: String::new(),
                    native_language: NativeLanguage::Pl,
                    target_language: TargetLanguage::De,
                    deck_type: DeckType::Linguistic,
                    config_id: preset,
                    content_version: None,
                    bundled: false,
                },
                TimestampMs(0),
            )?
            .id;
        let note_type = tx.note_type(NoteTypeKind::Basic)?;
        let mut truth = DEFAULT_WEIGHTS;
        truth[2] = 5.5;
        truth[8] = 1.3;
        truth[20] = 0.3;
        let scheduler = SchedulerConfig {
            parameters: FsrsParameters::new(&truth, 0.9, MAX_INTERVAL_DAYS)?,
            learning_steps_seconds: vec![600],
            relearning_steps_seconds: vec![600],
            fuzz: false,
        };
        let mut sample = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        let mut uniform = move || {
            sample ^= sample << 13;
            sample ^= sample >> 7;
            sample ^= sample << 17;
            (sample >> 11) as f64 / (1u64 << 53) as f64
        };
        for index in 0..cards {
            let note = tx.insert_note(
                &NewNote {
                    guid: guid(seed * 10_000 + index + 10),
                    note_type_id: note_type.id,
                    fields: NoteFields {
                        front: format!("Wort {seed} {index}"),
                        back: format!("słowo {index}"),
                    },
                    tags: Vec::new(),
                },
                TimestampMs(0),
            )?;
            let position = tx.next_position()?;
            let card = tx.insert_card(
                &NewCard {
                    guid: guid(seed * 10_000 + index + 5_000),
                    note_id: note.id,
                    deck_id: deck,
                    template_ordinal: 0,
                    position,
                },
                TimestampMs(0),
            )?;
            let snapshot = CardSnapshot::of(&card);
            let mut at = NOON - 400 * DAY + index as i64 * 1_000;
            let mut input = ReviewInput {
                phase: Phase::Learning { step: 0 },
                memory: None,
                elapsed_days: None,
            };
            let mut answers = vec![(Rating::Good, ReviewKind::Learning, 0)];
            answers.push((Rating::Good, ReviewKind::Learning, 0));
            let mut memory = MemoryState {
                stability: 0.0,
                difficulty: 0.0,
            };
            for (step, (rating, kind, gap)) in answers.clone().into_iter().enumerate() {
                if step > 0 {
                    input.elapsed_days = Some(gap);
                }
                let out = scheduler.review(&input, rating, 0.0);
                input.phase = out.phase;
                input.memory = Some(out.memory);
                memory = out.memory;
                insert(tx, card.id, &snapshot, rating, kind, at, index, step)?;
            }
            for review in 0..reviews {
                let interval = scheduler.parameters.next_interval_days(memory.stability);
                let gap = ((f64::from(interval) * (0.5 + uniform())).round() as i64).max(1);
                let recall = scheduler
                    .parameters
                    .retrievability(gap as f64, memory.stability);
                let rating = if uniform() < recall {
                    Rating::Good
                } else {
                    Rating::Again
                };
                input.elapsed_days = Some(gap);
                let out = scheduler.review(&input, rating, 0.0);
                input.phase = out.phase;
                input.memory = Some(out.memory);
                memory = out.memory;
                at += gap * DAY;
                insert(
                    tx,
                    card.id,
                    &snapshot,
                    rating,
                    ReviewKind::Review,
                    at,
                    index,
                    review + 2,
                )?;
            }
        }
        Ok(preset)
    })
    .unwrap()
}

#[allow(clippy::too_many_arguments)]
fn insert(
    tx: &mut dyn Repositories,
    card: CardId,
    snapshot: &CardSnapshot,
    rating: Rating,
    kind: ReviewKind,
    at: i64,
    index: u64,
    step: usize,
) -> AppResult<()> {
    tx.insert_review_log(&ReviewLog {
        id: ReviewLogId(0),
        card_id: card,
        session_id: None,
        operation_id: format!("op-{}-{index}-{step}", card.0),
        rating,
        kind,
        reviewed_at: TimestampMs(at),
        duration_ms: 3_000,
        card_before: snapshot.clone(),
        card_after: snapshot.clone(),
        fsrs_version: "fsrs-6/py-fsrs-6.3.0".into(),
        parameters_hash: "test".into(),
        invalidated_at: None,
    })?;
    Ok(())
}

fn wait(jobs: &JobService<SqliteDatabase>, id: &JobId) -> JobRecord {
    for _ in 0..6_000 {
        let job = jobs.get(id).unwrap();
        if job.status.is_finished() {
            return job;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("job {id} did not finish");
}

fn view(setup: &Setup, preset: DeckConfigId) -> OptimizerView {
    setup.optimizer.view(preset).unwrap()
}

fn action(op: &str, run: i64, version: i64) -> RunAction {
    RunAction {
        operation_id: op.into(),
        run_id: run,
        expected_version: version,
    }
}

fn run_once(setup: &Setup, preset: DeckConfigId, op: &str) -> i64 {
    let job = setup.optimizer.start(op.into(), preset).unwrap();
    let finished = wait(&setup.jobs, &job.id);
    assert_eq!(finished.status, JobStatus::Succeeded, "{finished:?}");
    let result = finished.result_json.unwrap();
    let id: String = result
        .trim_start_matches(r#"{"type":"optimize","runId":""#)
        .trim_end_matches(r#""}"#)
        .into();
    id.parse().unwrap()
}

#[test]
fn a_run_is_proposed_applied_and_reverted_without_touching_the_history() {
    let setup = setup();
    let preset = collection(&setup.db, 1, 160, 3);
    // An undone answer is flagged, kept and excluded.
    setup
        .db
        .write(|tx| tx.invalidate_review_log("op-1-0-2", TimestampMs(NOON)))
        .unwrap();
    let before = view(&setup, preset);
    assert_eq!(before.dataset.reviews, 160 * 5);
    assert_eq!(before.dataset.excluded_undone, 1);
    assert_eq!(before.dataset.usable, 160 * 3 - 1);
    assert!(before.runs.is_empty());
    assert_eq!(before.running_job, None);
    let logs_before = setup.db.read(move |tx| tx.optimizer_logs(preset)).unwrap();

    let run = run_once(&setup, preset, "op-start-1");
    let progress: Vec<_> = setup
        .events
        .jobs
        .lock()
        .unwrap()
        .iter()
        .filter_map(|event| event.progress)
        .collect();
    assert!(progress.len() >= 2, "{progress:?}");
    assert!(progress.windows(2).all(|pair| pair[0].done <= pair[1].done));

    let proposed = view(&setup, preset);
    let first = &proposed.runs[0];
    assert_eq!(first.run.id, run);
    assert_eq!(first.run.status, RunStatus::Proposed);
    assert!(first.can_apply && !first.can_revert);
    assert_eq!(first.run.weights_before, DEFAULT_WEIGHTS.to_vec());
    assert_eq!(first.report.dataset.usable, 160 * 3 - 1);
    assert_eq!(
        first.report.training_reviews + first.report.validation_reviews,
        160 * 3 - 1
    );
    assert!(first.report.few_reviews);
    assert!(first.report.proposed.daily_reviews > 0.0);
    // The answers follow other weights than the defaults: the proposal predicts them better.
    assert!(first.report.recommended, "{:?}", first.report);
    assert_eq!(first.run.dataset_hash.len(), 32);

    let version = proposed.preset.version;
    // A stale screen is refused.
    assert!(matches!(
        setup.db.write(move |tx| optimizer::apply(
            tx,
            &action("op-apply", run, version - 1),
            TimestampMs(NOON)
        )),
        Err(AppError::VersionConflict {
            entity: "preset",
            ..
        })
    ));
    let (applied, _) = setup
        .db
        .write(move |tx| optimizer::apply(tx, &action("op-apply", run, version), TimestampMs(NOON)))
        .unwrap();
    assert_eq!(applied.options.weights, first.run.weights_proposed);
    assert_eq!(applied.version, version + 1);
    // A retry answers the same; a second apply is refused.
    let (replayed, _) = setup
        .db
        .write(move |tx| optimizer::apply(tx, &action("op-apply", run, version), TimestampMs(NOON)))
        .unwrap();
    assert_eq!(replayed.version, version + 1);
    let again = setup.db.write(move |tx| {
        optimizer::apply(
            tx,
            &action("op-apply-2", run, version + 1),
            TimestampMs(NOON),
        )
    });
    assert!(matches!(again, Err(AppError::Validation(error)) if error.reason == "status"));

    let after_apply = view(&setup, preset);
    assert_eq!(after_apply.runs[0].run.status, RunStatus::Applied);
    assert_eq!(after_apply.runs[0].run.applied_at, Some(TimestampMs(NOON)));
    assert!(after_apply.runs[0].can_revert && !after_apply.runs[0].can_apply);

    let (reverted, _) = setup
        .db
        .write(move |tx| {
            optimizer::revert(
                tx,
                &action("op-revert", run, version + 1),
                TimestampMs(NOON),
            )
        })
        .unwrap();
    assert_eq!(reverted.options.weights, DEFAULT_WEIGHTS.to_vec());
    let after_revert = view(&setup, preset);
    assert_eq!(after_revert.runs[0].run.status, RunStatus::Reverted);
    assert!(!after_revert.runs[0].can_revert && !after_revert.runs[0].can_apply);

    // Nothing in the history changed.
    let logs_after = setup.db.read(move |tx| tx.optimizer_logs(preset)).unwrap();
    assert_eq!(logs_before, logs_after);
}

#[test]
fn a_proposal_goes_stale_when_the_weights_change() {
    let setup = setup();
    let preset = collection(&setup.db, 2, 120, 2);
    let run = run_once(&setup, preset, "op-start-2");
    let version = setup
        .db
        .write(move |tx| {
            let mut config = tx.config(preset)?.unwrap();
            config.options.weights[0] = 0.3;
            Ok(tx.update_config(&config, TimestampMs(NOON))?.version)
        })
        .unwrap();
    let state = view(&setup, preset);
    assert!(!state.runs[0].can_apply);
    let stale = setup.db.write(move |tx| {
        optimizer::apply(tx, &action("op-stale", run, version), TimestampMs(NOON))
    });
    assert!(matches!(stale, Err(AppError::Validation(error)) if error.reason == "stale"));
    let missing = setup.db.write(move |tx| {
        optimizer::apply(
            tx,
            &action("op-missing", run + 99, version),
            TimestampMs(NOON),
        )
    });
    assert!(matches!(
        missing,
        Err(AppError::NotFound {
            entity: "optimizerRun"
        })
    ));
}

#[test]
fn too_little_history_is_refused_before_a_job_starts() {
    let setup = setup();
    let preset = collection(&setup.db, 3, 30, 2);
    assert_eq!(view(&setup, preset).dataset.usable, 60);
    let refused = setup.optimizer.start("op-few".into(), preset);
    assert!(
        matches!(refused, Err(AppError::Validation(error)) if error.field == "presetId" && error.reason == "notEnoughReviews")
    );
    assert!(matches!(
        setup.optimizer.view(DeckConfigId(9_999)),
        Err(AppError::NotFound { entity: "preset" })
    ));
}

#[test]
fn a_cancelled_run_stores_nothing_and_a_running_one_is_shared() {
    let setup = setup();
    let preset = collection(&setup.db, 4, 400, 4);
    let job = setup.optimizer.start("op-cancel".into(), preset).unwrap();
    // A second start while it runs returns the same job.
    let same = setup.optimizer.start("op-cancel-2".into(), preset).unwrap();
    assert_eq!(same.id, job.id);
    assert_eq!(view(&setup, preset).running_job, Some(job.id.clone()));
    setup.jobs.cancel(&job.id).unwrap();
    let finished = wait(&setup.jobs, &job.id);
    assert_eq!(finished.status, JobStatus::Cancelled);
    let state = view(&setup, preset);
    assert!(state.runs.is_empty());
    assert_eq!(state.running_job, None);
}
