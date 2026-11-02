//! Due-queue benchmark on the plan's large collection (plan step 25: 50 000 cards, 500 000 logs,
//! target under 50 ms on the reference device). Run on a device build or locally with
//! `cargo bench -p langualinker-infrastructure --bench due_query`.

use criterion::{Criterion, criterion_group, criterion_main};
use langualinker_application::ports::Clock;
use langualinker_application::repositories::UnitOfWork;
use langualinker_domain::{DeckId, TimestampMs};
use langualinker_infrastructure::sqlite::SqliteDatabase;
use langualinker_infrastructure::sqlite::seed::{SEED_EPOCH_MS, SeedSize, seed_collection};

struct Fixed;

impl Clock for Fixed {
    fn now(&self) -> TimestampMs {
        TimestampMs(SEED_EPOCH_MS)
    }
}

fn due_queue(c: &mut Criterion) {
    let database = SqliteDatabase::open_in_memory(&Fixed).unwrap().database;
    database
        .with_connection(|conn| seed_collection(conn, SeedSize::PLAN))
        .unwrap();
    let all: Vec<DeckId> = (1..=SeedSize::PLAN.decks).map(DeckId).collect();
    let mut group = c.benchmark_group("due_queue");
    group.sample_size(30);
    group.bench_function("all_decks_200", |b| {
        b.iter(|| {
            let decks = all.clone();
            database
                .read(move |tx| tx.due_card_ids(&decks, TimestampMs(SEED_EPOCH_MS), 200))
                .unwrap()
        })
    });
    group.bench_function("one_deck_200", |b| {
        b.iter(|| {
            database
                .read(|tx| tx.due_card_ids(&[DeckId(1)], TimestampMs(SEED_EPOCH_MS), 200))
                .unwrap()
        })
    });
    group.bench_function("new_cards_20", |b| {
        b.iter(|| {
            database
                .read(|tx| tx.new_card_ids(&[DeckId(1)], 20))
                .unwrap()
        })
    });
    group.finish();
}

criterion_group!(benches, due_queue);
criterion_main!(benches);
