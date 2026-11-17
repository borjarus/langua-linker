//! Performance budgets of plan Phase 16 step 3 on the plan's reference collection (50 000 cards,
//! 500 000 review logs, 10 decks): due queue p95 < 50 ms, CSV import of 5 000 rows < 5 s, and the
//! backend share of a cold start (open + migrate check + first dashboard counts). Search p95 is
//! measured in `management.rs`, 20k import/export in `transfer.rs`.
//!
//! These are measurements, not unit tests: run them on a release build of the target hardware,
//! `cargo test --release -p langualinker-infrastructure --test budgets -- --ignored --nocapture`,
//! and record the printed numbers with the hardware/build description in
//! `docs/testing/performance-budgets.md`.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, Instant};

use langualinker_application::jobs::{JobEvent, JobRecord, JobService, JobStatus};
use langualinker_application::ports::{Clock, CollectionScope, EventSink, Rng};
use langualinker_application::repositories::UnitOfWork;
use langualinker_application::study;
use langualinker_application::transfer::{CsvOptions, CsvOutcome, CsvTarget, DuplicatePolicy};
use langualinker_domain::{DeckId, TimestampMs};
use langualinker_infrastructure::sqlite::seed::{SEED_EPOCH_MS, SeedSize, seed_collection};
use langualinker_infrastructure::sqlite::{DATABASE_FILE, SqliteDatabase};
use langualinker_infrastructure::transfer::{ExportOutcome, TransferEncoders, TransferService};
use langualinker_interop::csv::Delimiter;

struct TestClock(AtomicI64);

impl Clock for TestClock {
    fn now(&self) -> TimestampMs {
        TimestampMs(self.0.load(Ordering::SeqCst))
    }
}

struct Counter(AtomicI64);

impl Rng for Counter {
    fn fill(&self, bytes: &mut [u8]) {
        let next = self.0.fetch_add(1, Ordering::SeqCst).to_le_bytes();
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = next[index % 8] ^ (index as u8).wrapping_mul(31);
        }
    }
}

struct Silent;

impl EventSink for Silent {
    fn job_progress(&self, _: &JobEvent) {}
    fn collection_changed(&self, _: &[CollectionScope], _: TimestampMs) {}
}

fn clock() -> TestClock {
    TestClock(AtomicI64::new(SEED_EPOCH_MS))
}

/// Nearest-rank percentile of sorted timings.
fn percentile(sorted: &[Duration], p: usize) -> Duration {
    sorted[((p * sorted.len()).div_ceil(100)).max(1) - 1]
}

fn report(label: &str, mut timings: Vec<Duration>) -> Duration {
    timings.sort();
    let p95 = percentile(&timings, 95);
    println!(
        "{label}: n {}, p50 {:?}, p95 {:?}, max {:?}",
        timings.len(),
        percentile(&timings, 50),
        p95,
        timings.last().unwrap()
    );
    p95
}

fn seeded_file(dir: &Path) -> std::path::PathBuf {
    let path = dir.join(DATABASE_FILE);
    let started = Instant::now();
    let db = SqliteDatabase::open(&path, &clock()).unwrap().database;
    db.with_connection(|conn| seed_collection(conn, SeedSize::PLAN))
        .unwrap();
    drop(db);
    println!("fixture 50k cards / 500k logs: {:?}", started.elapsed());
    path
}

/// Budget: due queue p95 < 50 ms (plan step 13).
#[test]
#[ignore = "budget: 50k cards, 500k logs"]
fn due_queue_p95_on_the_plan_collection() {
    let dir = tempfile::tempdir().unwrap();
    let path = seeded_file(dir.path());
    let db = SqliteDatabase::open(&path, &clock()).unwrap().database;
    let at = TimestampMs(SEED_EPOCH_MS);
    let all: Vec<DeckId> = (1..=SeedSize::PLAN.decks).map(DeckId).collect();
    // Warm-up: the first query reads pages from disk.
    db.read({
        let all = all.clone();
        move |tx| tx.due_card_ids(&all, at, 200)
    })
    .unwrap();
    let mut ids = Vec::new();
    let mut counts = Vec::new();
    for round in 0..60 {
        let decks = if round % 2 == 0 {
            all.clone()
        } else {
            vec![DeckId(1 + round % SeedSize::PLAN.decks)]
        };
        let started = Instant::now();
        db.read(move |tx| tx.due_card_ids(&decks, at, 200)).unwrap();
        ids.push(started.elapsed());
        let deck = (round % 3 == 0).then_some(DeckId(1));
        let started = Instant::now();
        db.write(move |tx| study::due_counts(tx, deck, at)).unwrap();
        counts.push(started.elapsed());
    }
    let ids = report("due card ids (200)", ids);
    let counts = report("scheduler due counts", counts);
    assert!(ids.as_millis() < 50, "due ids p95 {ids:?}");
    assert!(counts.as_millis() < 50, "due counts p95 {counts:?}");
}

/// Backend share of a cold start: open + migration check of the 50k collection and the first
/// dashboard counts. The plan budget (< 2 s) is for the whole start; this part gets 500 ms.
#[test]
#[ignore = "budget: 50k cards, 500k logs"]
fn backend_cold_open_on_the_plan_collection() {
    let dir = tempfile::tempdir().unwrap();
    let path = seeded_file(dir.path());
    let mut timings = Vec::new();
    for _ in 0..10 {
        let started = Instant::now();
        let db = SqliteDatabase::open(&path, &clock()).unwrap().database;
        db.write(|tx| study::due_counts(tx, None, TimestampMs(SEED_EPOCH_MS)))
            .unwrap();
        timings.push(started.elapsed());
        drop(db);
    }
    let p95 = report("open + first due counts", timings);
    assert!(p95.as_millis() < 500, "cold open p95 {p95:?}");
}

fn encode_import(outcome: &CsvOutcome) -> String {
    serde_json::json!({ "deckId": outcome.deck_id.0.to_string() }).to_string()
}

fn encode_export(outcome: &ExportOutcome) -> String {
    serde_json::json!({ "fileName": outcome.file_name }).to_string()
}

fn wait(jobs: &JobService<SqliteDatabase>, job: &JobRecord) -> JobRecord {
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let current = jobs.get(&job.id).unwrap();
        if matches!(
            current.status,
            JobStatus::Succeeded | JobStatus::Failed | JobStatus::Cancelled
        ) {
            return current;
        }
        assert!(Instant::now() < deadline, "job did not finish");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Budget: CSV import of 5 000 rows < 5 s, preview included (plan step 13).
#[test]
#[ignore = "budget: 5k-row import"]
fn csv_import_of_five_thousand_rows() {
    let dir = tempfile::tempdir().unwrap();
    let clock: Arc<dyn Clock> = Arc::new(clock());
    let db = Arc::new(
        SqliteDatabase::open(&dir.path().join(DATABASE_FILE), clock.as_ref())
            .unwrap()
            .database,
    );
    let events: Arc<dyn EventSink> = Arc::new(Silent);
    let jobs = JobService::new(db.clone(), events.clone(), clock.clone());
    let transfer = TransferService::new(
        db,
        jobs.clone(),
        events,
        clock,
        Arc::new(Counter(AtomicI64::new(1))),
        dir.path().join("exports"),
        TransferEncoders {
            import: encode_import,
            export: encode_export,
        },
    );
    let mut csv = String::from("front,back,tags\n");
    for index in 0..5_000 {
        csv.push_str(&format!(
            "Wort {index},słowo {index},batch t{}\n",
            index % 50
        ));
    }
    let options = CsvOptions {
        delimiter: Some(Delimiter::Comma),
        encoding: None,
        has_header: true,
        mapping: Some(vec![CsvTarget::Front, CsvTarget::Back, CsvTarget::Tags]),
        duplicate_policy: DuplicatePolicy::Skip,
        target_deck: None,
        formula_guard: false,
    };
    let started = Instant::now();
    let preview = transfer
        .preview_csv("five.csv", csv.as_bytes(), options)
        .unwrap();
    let preview_time = started.elapsed();
    let job = transfer
        .commit_csv("op-five".into(), &preview.token)
        .unwrap();
    let job = wait(&jobs, &job);
    let total = started.elapsed();
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    println!("5000 rows: preview {preview_time:?}, preview + import {total:?}");
    assert!(total.as_secs_f64() < 5.0, "import {total:?}");
}
