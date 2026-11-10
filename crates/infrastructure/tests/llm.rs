//! AI associations (plan Phase 13) on the real SQLite unit of work with a scripted fake provider:
//! the five failure classes plus a timeout after sending, the cache (zero network calls on a
//! repeat; text, prompt and model invalidate it), the outbox across "restarts" and reconnects,
//! consent withdrawal, stale answers, moderation, the daily limit, and what is sent.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use langualinker_application::jobs::{JobEvent, JobId, JobRecord, JobService, JobStatus};
use langualinker_application::llm::{self, GenerationState, LlmService};
use langualinker_application::ports::{
    ApiKey, Clock, CollectionScope, EventSink, LlmClient, LlmFailure, LlmProtocol, LlmRequest,
    LlmResponse, Rng, SecretStore,
};
use langualinker_application::repositories::{LlmRequestStatus, UnitOfWork};
use langualinker_application::settings::{self, AI_KEY_SECRET, Consent, SettingsChange};
use langualinker_application::{AppError, AppResult, learning, management};
use langualinker_domain::content::AssociationProvenance;
use langualinker_domain::deck::{DeckType, NativeLanguage, NewDeck, TargetLanguage};
use langualinker_domain::note::{NewNote, NoteFields, NoteTypeKind};
use langualinker_domain::{DeckId, Guid, NoteId, TimestampMs};
use langualinker_infrastructure::sqlite::SqliteDatabase;

/// 2026-03-02 12:00 UTC.
const NOON: i64 = 1_772_452_800_000;

// ---------------------------------------------------------------- fakes

struct TestClock(AtomicI64);

impl Clock for TestClock {
    fn now(&self) -> TimestampMs {
        TimestampMs(self.0.load(Ordering::SeqCst))
    }
}

struct CountingRng(Mutex<u64>);

impl Rng for CountingRng {
    fn fill(&self, bytes: &mut [u8]) {
        let mut next = self.0.lock().unwrap();
        *next += 1;
        let seed = next.to_le_bytes();
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = seed[index % 8] ^ (index as u8).wrapping_mul(31);
        }
    }
}

#[derive(Default)]
struct Events(Mutex<Vec<CollectionScope>>);

impl EventSink for Events {
    fn job_progress(&self, _: &JobEvent) {}
    fn collection_changed(&self, scopes: &[CollectionScope], _: TimestampMs) {
        self.0.lock().unwrap().extend_from_slice(scopes);
    }
}

#[derive(Default)]
struct Secrets(Mutex<HashMap<String, String>>);

impl SecretStore for Secrets {
    fn save(&self, name: &str, value: &str) -> AppResult<()> {
        self.0.lock().unwrap().insert(name.into(), value.into());
        Ok(())
    }
    fn remove(&self, name: &str) -> AppResult<bool> {
        Ok(self.0.lock().unwrap().remove(name).is_some())
    }
    fn has(&self, name: &str) -> AppResult<bool> {
        Ok(self.0.lock().unwrap().contains_key(name))
    }
    fn read(&self, name: &str) -> AppResult<Option<String>> {
        Ok(self.0.lock().unwrap().get(name).cloned())
    }
}

type Hook = Box<dyn Fn() + Send + Sync>;

/// Answers from a script; an empty script answers with a valid association.
#[derive(Default)]
struct FakeLlm {
    script: Mutex<VecDeque<Result<LlmResponse, LlmFailure>>>,
    calls: Mutex<Vec<(LlmRequest, Option<String>)>>,
    /// Runs inside the next call (e.g. the learner edits the note meanwhile).
    during_call: Mutex<Option<Hook>>,
}

impl FakeLlm {
    fn answer(text: &str) -> Result<LlmResponse, LlmFailure> {
        Ok(LlmResponse {
            text: serde_json::json!({ "association": text }).to_string(),
            model: "claude-haiku-5-5".into(),
        })
    }

    fn push(&self, outcomes: impl IntoIterator<Item = Result<LlmResponse, LlmFailure>>) {
        self.script.lock().unwrap().extend(outcomes);
    }

    fn count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}

impl LlmClient for FakeLlm {
    fn generate(
        &self,
        request: &LlmRequest,
        key: Option<&ApiKey>,
        _: &dyn Fn() -> bool,
    ) -> Result<LlmResponse, LlmFailure> {
        self.calls
            .lock()
            .unwrap()
            .push((request.clone(), key.map(|key| key.0.clone())));
        if let Some(hook) = self.during_call.lock().unwrap().take() {
            hook();
        }
        let next = self.script.lock().unwrap().pop_front();
        next.unwrap_or_else(|| {
            let n = self.count();
            Self::answer(&format!(
                "„Hund” brzmi jak „chód” — pies idzie w chodzie nr {n}."
            ))
        })
    }
}

// ---------------------------------------------------------------- fixture

struct Fixture {
    db: Arc<SqliteDatabase>,
    clock: Arc<TestClock>,
    llm: Arc<FakeLlm>,
    secrets: Arc<Secrets>,
    service: LlmService<SqliteDatabase>,
    jobs: JobService<SqliteDatabase>,
    deck: DeckId,
    next_seed: Mutex<u64>,
}

fn guid(seed: u64) -> Guid {
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&seed.to_le_bytes());
    bytes[8] = 0x7c;
    Guid::from_bytes(bytes)
}

impl Fixture {
    fn new() -> Self {
        Self::with_db(None)
    }

    fn with_db(db: Option<Arc<SqliteDatabase>>) -> Self {
        let clock = Arc::new(TestClock(AtomicI64::new(NOON)));
        let db = db.unwrap_or_else(|| {
            Arc::new(
                SqliteDatabase::open_in_memory(clock.as_ref())
                    .unwrap()
                    .database,
            )
        });
        let events: Arc<dyn EventSink> = Arc::new(Events::default());
        let jobs = JobService::new(db.clone(), events.clone(), clock.clone());
        let llm = Arc::new(FakeLlm::default());
        let secrets = Arc::new(Secrets::default());
        let service = LlmService::new(
            db.clone(),
            jobs.clone(),
            events,
            clock.clone(),
            Arc::new(CountingRng(Mutex::new(0))),
            llm.clone(),
            Some(secrets.clone()),
        )
        .with_retry_delay(Duration::ZERO);
        let deck = db
            .write(|tx| {
                let config = tx.default_config()?.id;
                Ok(tx
                    .insert_deck(
                        &NewDeck {
                            guid: guid(1),
                            name: "Niemiecki".into(),
                            parent_id: None,
                            description: String::new(),
                            native_language: NativeLanguage::Pl,
                            target_language: TargetLanguage::De,
                            deck_type: DeckType::TextWithAssociations,
                            config_id: config,
                            content_version: None,
                            bundled: false,
                        },
                        TimestampMs(0),
                    )?
                    .id)
            })
            .unwrap_or_else(|_| {
                // A reopened collection already has the deck.
                db.read(|tx| Ok(tx.active_decks()?[0].id)).unwrap()
            });
        Self {
            db,
            clock,
            llm,
            secrets,
            service,
            jobs,
            deck,
            next_seed: Mutex::new(100),
        }
    }

    /// Consent granted, Anthropic with its first model, a key in the store.
    fn enable_ai(&self) {
        let now = self.clock.now();
        self.db
            .write(move |tx| {
                let version = settings::read(tx)?.doc.version;
                settings::update(
                    tx,
                    version,
                    SettingsChange {
                        ai_consent: Some(Consent::Granted),
                        ai_provider: Some("anthropic".into()),
                        ..SettingsChange::default()
                    },
                    now,
                )
                .map(drop)
            })
            .unwrap();
        self.secrets.save(AI_KEY_SECRET, "sk-test-key-123").unwrap();
    }

    fn change(&self, change: SettingsChange) {
        let now = self.clock.now();
        self.db
            .write(move |tx| {
                let version = settings::read(tx)?.doc.version;
                settings::update(tx, version, change, now).map(drop)
            })
            .unwrap();
    }

    fn note(&self, kind: NoteTypeKind, front: &str, back: &str) -> NoteId {
        let seed = {
            let mut next = self.next_seed.lock().unwrap();
            *next += 1;
            *next
        };
        let (front, back, deck) = (front.to_owned(), back.to_owned(), self.deck);
        self.db
            .write(move |tx| {
                let note_type = tx.note_type(kind)?;
                let note = tx.insert_note(
                    &NewNote {
                        guid: guid(seed),
                        note_type_id: note_type.id,
                        fields: NoteFields { front, back },
                        tags: Vec::new(),
                    },
                    TimestampMs(0),
                )?;
                for template in tx.templates(note_type.id)? {
                    let position = tx.next_position()?;
                    tx.insert_card(
                        &langualinker_domain::card::NewCard {
                            guid: guid(seed * 100 + u64::from(template.ordinal) + 1),
                            note_id: note.id,
                            deck_id: deck,
                            template_ordinal: template.ordinal,
                            position,
                        },
                        TimestampMs(0),
                    )?;
                }
                Ok(note.id)
            })
            .unwrap()
    }

    fn word(&self, front: &str, back: &str) -> NoteId {
        self.note(NoteTypeKind::BasicAndReversed, front, back)
    }

    fn wait(&self, job: &JobRecord) -> JobRecord {
        wait_for(&self.jobs, &job.id)
    }

    fn generate(&self, operation: &str, note: NoteId, variants: u32) -> JobRecord {
        let job = self.service.generate(operation, note, variants).unwrap();
        self.wait(&job)
    }

    fn content(&self, note: NoteId) -> learning::NoteContent {
        self.db
            .read(move |tx| learning::note_content(tx, note))
            .unwrap()
    }

    fn requests(&self, note: NoteId) -> Vec<langualinker_application::repositories::LlmRequestRow> {
        self.db
            .read(move |tx| {
                let mut rows = Vec::new();
                for status in LlmRequestStatus::ALL {
                    rows.extend(
                        tx.llm_requests_with_status(status)?
                            .into_iter()
                            .filter(|row| row.note_id == note),
                    );
                }
                rows.sort_by_key(|row| row.id);
                Ok(rows)
            })
            .unwrap()
    }

    fn calls_logged(&self) -> u32 {
        self.db
            .read(|tx| tx.llm_calls_since(TimestampMs(0)))
            .unwrap()
    }

    fn advance(&self, ms: i64) {
        self.clock.0.fetch_add(ms, Ordering::SeqCst);
    }
}

fn wait_for(jobs: &JobService<SqliteDatabase>, id: &JobId) -> JobRecord {
    for _ in 0..2_000 {
        let job = jobs.get(id).unwrap();
        if job.status.is_finished() {
            return job;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("job {id} did not finish");
}

fn failure_code(job: &JobRecord) -> &str {
    job.error.as_ref().map_or("", |error| error.code.as_str())
}

// ---------------------------------------------------------------- tests

#[test]
fn a_generated_association_is_attached_with_its_provenance() {
    let f = Fixture::new();
    f.enable_ai();
    let note = f.word("der Hund", "pies");

    let job = f.generate("op-1", note, 1);
    assert_eq!(job.status, JobStatus::Succeeded);

    let content = f.content(note);
    assert_eq!(content.associations.len(), 1);
    let association = &content.associations[0];
    assert_eq!(association.provenance, AssociationProvenance::Generated);
    assert_eq!(association.model.as_deref(), Some("claude-haiku-5-5"));
    assert_eq!(
        association.prompt_version.as_deref(),
        Some("word-substitution-v1")
    );
    assert!(content.generations.is_empty(), "nothing pending any more");

    // The request carried the key, the pinned parameters and only the card's text.
    let calls = f.llm.calls.lock().unwrap();
    let (request, key) = &calls[0];
    assert_eq!(key.as_deref(), Some("sk-test-key-123"));
    assert_eq!(request.protocol, LlmProtocol::Anthropic);
    assert_eq!(request.model, "claude-haiku-5-5");
    assert_eq!(request.temperature, None);
    assert!(request.user.contains("\"text\":\"der Hund\""));
    assert!(request.user.contains("\"meaning\":\"pies\""));
    assert!(!request.user.contains("Niemiecki"), "no deck name is sent");
    assert!(
        !request.system.contains("Hund"),
        "no card text in the rules"
    );
    assert!(request.system.contains("Polish-speaking learner of German"));
}

#[test]
fn a_retried_operation_queues_nothing_twice() {
    let f = Fixture::new();
    f.enable_ai();
    let note = f.word("die Katze", "kot");
    let first = f.generate("op-same", note, 2);
    let again = f.service.generate("op-same", note, 2).unwrap();
    assert_eq!(first.id, again.id);
    assert_eq!(f.requests(note).len(), 2);
    assert_eq!(f.llm.count(), 2);
    assert_eq!(f.content(note).associations.len(), 2);
}

#[test]
fn generation_needs_consent_a_provider_and_a_key() {
    let f = Fixture::new();
    let note = f.word("der Tisch", "stół");
    let denied = f.service.generate("op-a", note, 1).unwrap_err();
    assert_eq!(
        denied,
        AppError::PermissionDenied {
            reason: "aiConsent"
        }
    );

    f.enable_ai();
    f.secrets.remove(AI_KEY_SECRET).unwrap();
    let no_key = f.service.generate("op-b", note, 1).unwrap_err();
    assert_eq!(no_key, AppError::PermissionDenied { reason: "apiKey" });

    let variants = f.service.generate("op-c", note, 4).unwrap_err();
    assert_eq!(variants.code(), "VALIDATION_FAILED");
    assert_eq!(f.llm.count(), 0, "nothing reached the provider");
}

#[test]
fn a_repeat_hits_the_cache_without_any_network_call() {
    let f = Fixture::new();
    f.enable_ai();
    let first = f.word("das Haus", "dom");
    f.generate("op-1", first, 1);
    assert_eq!(f.llm.count(), 1);

    // The same word in another note: the cached answer, zero calls.
    let second = f.word(" das  Haus ", "dom");
    let job = f.generate("op-2", second, 1);
    assert_eq!(job.status, JobStatus::Succeeded);
    assert_eq!(f.llm.count(), 1, "cache hit performs zero network calls");
    assert_eq!(
        f.content(second).associations[0].text,
        f.content(first).associations[0].text
    );

    // Asking again on the same note wants a new association: a new slot, a new call.
    f.generate("op-3", first, 1);
    assert_eq!(f.llm.count(), 2);
    assert_eq!(f.content(first).associations.len(), 2);
}

#[test]
fn text_meaning_and_model_changes_invalidate_the_cache() {
    let f = Fixture::new();
    f.enable_ai();
    let base = f.word("die Bank", "ławka");
    f.generate("op-1", base, 1);
    // Homograph with another meaning.
    let homograph = f.word("die Bank", "bank (instytucja)");
    f.generate("op-2", homograph, 1);
    assert_eq!(f.llm.count(), 2, "different meaning, no reuse");
    // A sentence of the same text uses another prompt.
    let sentence = f.note(NoteTypeKind::Sentence, "die Bank", "ławka");
    f.generate("op-3", sentence, 1);
    assert_eq!(f.llm.count(), 3, "different prompt, no reuse");
    // Another model.
    f.change(SettingsChange {
        ai_model: Some(Some("claude-sonnet-5-5".into())),
        ..SettingsChange::default()
    });
    let same_text = f.word("die Bank", "ławka");
    f.generate("op-4", same_text, 1);
    assert_eq!(f.llm.count(), 4, "different model, no reuse");
    assert_eq!(f.llm.calls.lock().unwrap()[3].0.model, "claude-sonnet-5-5");
}

#[test]
fn offline_requests_wait_in_the_outbox_and_run_after_reconnect() {
    let f = Fixture::new();
    f.enable_ai();
    let note = f.word("der Zug", "pociąg");
    f.llm.push([Err(LlmFailure::NotSent)]);

    let job = f.generate("op-offline", note, 1);
    assert_eq!(failure_code(&job), "NETWORK_UNAVAILABLE");
    let content = f.content(note);
    assert_eq!(content.generations.len(), 1);
    assert_eq!(content.generations[0].state, GenerationState::OfflineQueued);
    let row = &f.requests(note)[0];
    assert_eq!(
        (row.attempts, row.offline_retries),
        (0, 1),
        "not an attempt"
    );
    assert_eq!(row.next_attempt_at, Some(TimestampMs(NOON + 30_000)));
    assert_eq!(f.calls_logged(), 0, "a call that never left is not counted");

    // Before the retry time nothing runs.
    assert!(f.service.resume().unwrap().is_none());
    // Reconnected, app in the foreground after the back-off.
    f.advance(30_000);
    let resumed = f.service.resume().unwrap().expect("due request");
    assert_eq!(f.wait(&resumed).status, JobStatus::Succeeded);
    assert_eq!(f.content(note).associations.len(), 1);
    assert!(f.content(note).generations.is_empty());
}

#[test]
fn the_outbox_survives_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("collection.sqlite");
    let clock = TestClock(AtomicI64::new(NOON));
    let note;
    {
        let db = Arc::new(SqliteDatabase::open(&path, &clock).unwrap().database);
        let f = Fixture::with_db(Some(db));
        f.enable_ai();
        note = f.word("der Apfel", "jabłko");
        f.llm.push([Err(LlmFailure::NotSent)]);
        f.generate("op-before-restart", note, 1);
        // A second row was "sending" when the process died.
        let sending = f.word("die Birne", "gruszka");
        f.llm.push([Err(LlmFailure::NotSent)]);
        f.generate("op-killed", sending, 1);
        f.db.write(move |tx| {
            let mut row = tx.llm_requests_of_operation("op-killed")?.remove(0);
            row.status = LlmRequestStatus::Sending;
            row.attempts = 1;
            tx.update_llm_request(&row, TimestampMs(NOON))
        })
        .unwrap();
    }
    let db = Arc::new(SqliteDatabase::open(&path, &clock).unwrap().database);
    let f = Fixture::with_db(Some(db));
    f.secrets.save(AI_KEY_SECRET, "sk-test-key-123").unwrap();
    assert_eq!(f.service.recover_interrupted().unwrap(), 1);
    let killed =
        f.db.read(|tx| tx.llm_requests_of_operation("op-killed"))
            .unwrap()
            .remove(0);
    assert_eq!(killed.status, LlmRequestStatus::Queued);
    assert_eq!(
        killed.attempts, 1,
        "a kill while sending counts as an attempt"
    );

    f.advance(60_000);
    let job = f.service.resume().unwrap().expect("due requests");
    assert_eq!(f.wait(&job).status, JobStatus::Succeeded);
    assert_eq!(f.content(note).associations.len(), 1);
    assert_eq!(f.content(killed.note_id).associations.len(), 1);
}

#[test]
fn a_timeout_after_sending_is_retried_then_fails_within_the_attempt_limit() {
    let f = Fixture::new();
    f.enable_ai();
    let note = f.word("die Uhr", "zegar");
    f.llm.push([
        Err(LlmFailure::Timeout),
        Err(LlmFailure::Timeout),
        Err(LlmFailure::Timeout),
    ]);
    let job = f.generate("op-timeout", note, 1);
    assert_eq!(failure_code(&job), "PROVIDER_TIMEOUT");
    assert_eq!(f.llm.count(), llm::MAX_ATTEMPTS as usize);
    assert_eq!(
        f.calls_logged(),
        llm::MAX_ATTEMPTS,
        "every attempt may be charged"
    );
    let row = &f.requests(note)[0];
    assert_eq!((row.status, row.attempts), (LlmRequestStatus::Failed, 3));
    let generation = &f.content(note).generations[0];
    assert_eq!(generation.state, GenerationState::Error);
    assert_eq!(generation.error, Some(AppError::ProviderTimeout));
    // Not retried later: no automatic charge beyond the limit.
    f.advance(3_600_000);
    assert!(f.service.resume().unwrap().is_none());
}

#[test]
fn overload_and_invalid_output_share_one_attempt_budget() {
    let f = Fixture::new();
    f.enable_ai();
    let note = f.word("der Mond", "księżyc");
    f.llm.push([
        Err(LlmFailure::Unavailable { status: 529 }),
        Ok(LlmResponse {
            text: "Sure! Here is an idea".into(),
            model: "m".into(),
        }),
        FakeLlm::answer("„Mond” jak „mąd-ry” księżyc."),
    ]);
    let job = f.generate("op-mixed", note, 1);
    assert_eq!(job.status, JobStatus::Succeeded);
    assert_eq!(f.llm.count(), 3);
    assert_eq!(f.requests(note)[0].attempts, 3);

    let broken = f.word("der Stern", "gwiazda");
    f.llm.push([
        Err(LlmFailure::Malformed),
        Err(LlmFailure::Malformed),
        Err(LlmFailure::Malformed),
    ]);
    let job = f.generate("op-invalid", broken, 1);
    assert_eq!(failure_code(&job), "INVALID_PROVIDER_OUTPUT");
    assert_eq!(f.llm.count(), 6);
}

#[test]
fn unsafe_output_is_blocked_and_never_stored_as_visible() {
    let f = Fixture::new();
    f.enable_ai();
    let note = f.word("der Hase", "zając");
    f.llm.push([
        FakeLlm::answer("Ten kurwa zając…"),
        Err(LlmFailure::Refused),
        FakeLlm::answer(""),
    ]);
    let job = f.generate("op-unsafe", note, 1);
    assert_eq!(failure_code(&job), "MODERATION_BLOCKED");
    let content = f.content(note);
    assert!(
        content.associations.is_empty(),
        "blocked output is not stored"
    );
    assert_eq!(content.generations[0].state, GenerationState::Blocked);
    let key = f.requests(note)[0].cache_key.clone();
    let cached = f.db.read(move |tx| tx.cached_association(&key)).unwrap();
    assert!(cached.is_none(), "blocked output is not cached either");

    // A retry that passes moderation is attached.
    let retried = f.word("der Igel", "jeż");
    f.llm.push([
        FakeLlm::answer("Ten kurwa jeż"),
        FakeLlm::answer("Igel — „igła”: jeż z igłami."),
    ]);
    f.generate("op-retry", retried, 1);
    assert_eq!(
        f.content(retried).associations[0].text,
        "Igel — „igła”: jeż z igłami."
    );
}

#[test]
fn unsafe_card_text_is_never_sent() {
    let f = Fixture::new();
    f.enable_ai();
    let note = f.word("die Hure", "—");
    let job = f.generate("op-input", note, 1);
    assert_eq!(failure_code(&job), "MODERATION_BLOCKED");
    assert_eq!(f.llm.count(), 0);
    assert_eq!(
        f.content(note).generations[0].state,
        GenerationState::Blocked
    );
}

#[test]
fn rejected_keys_and_provider_quota_fail_at_once() {
    let f = Fixture::new();
    f.enable_ai();
    let note = f.word("das Brot", "chleb");
    f.llm.push([Err(LlmFailure::Unauthorized)]);
    let job = f.generate("op-401", note, 1);
    assert_eq!(failure_code(&job), "PERMISSION_DENIED");
    assert_eq!(f.llm.count(), 1);
    assert_eq!(
        f.content(note).generations[0].error,
        Some(AppError::PermissionDenied {
            reason: "apiKeyRejected"
        })
    );

    let other = f.word("die Milch", "mleko");
    f.llm.push([Err(LlmFailure::Quota)]);
    let job = f.generate("op-402", other, 1);
    assert_eq!(failure_code(&job), "QUOTA_EXCEEDED");
    assert_eq!(f.llm.count(), 2);

    let third = f.word("der Käse", "ser");
    f.llm.push([Err(LlmFailure::Rejected { status: 400 })]);
    let job = f.generate("op-400", third, 1);
    assert_eq!(failure_code(&job), "PROVIDER_ERROR");
    assert_eq!(f.llm.count(), 3);
}

#[test]
fn withdrawing_consent_blocks_queued_requests_before_the_network() {
    let f = Fixture::new();
    f.enable_ai();
    let note = f.word("der Fisch", "ryba");
    f.llm.push([Err(LlmFailure::NotSent)]);
    f.generate("op-queued", note, 1);
    f.change(SettingsChange {
        ai_consent: Some(Consent::Declined),
        ..SettingsChange::default()
    });
    f.advance(3_600_000);
    assert!(
        f.service.resume().unwrap().is_none(),
        "nothing queued any more"
    );
    assert_eq!(f.llm.count(), 1, "only the first offline try");
    let generation = &f.content(note).generations[0];
    assert_eq!(generation.state, GenerationState::Error);
    assert_eq!(
        generation.error,
        Some(AppError::PermissionDenied {
            reason: "aiConsent"
        })
    );
}

#[test]
fn an_answer_for_a_changed_note_is_cached_but_not_attached() {
    let f = Fixture::new();
    f.enable_ai();
    let note = f.word("der Vogel", "ptak");
    let db = f.db.clone();
    *f.llm.during_call.lock().unwrap() = Some(Box::new(move || {
        db.write(move |tx| {
            let mut stored = tx.note(note)?.unwrap();
            stored.fields.back = "ptak (zwierzę)".into();
            tx.update_note(&stored, TimestampMs(NOON)).map(drop)
        })
        .unwrap();
    }));
    let job = f.generate("op-stale", note, 1);
    assert_eq!(job.status, JobStatus::Succeeded);
    let content = f.content(note);
    assert!(content.associations.is_empty(), "no stale attach");
    assert!(
        content.generations.is_empty(),
        "the placeholder is dismissed"
    );
    assert_eq!(f.requests(note)[0].status, LlmRequestStatus::Dismissed);

    // A queued request whose note changed is not sent at all.
    f.llm.push([Err(LlmFailure::NotSent)]);
    f.generate("op-queued", note, 1);
    f.db.write(move |tx| {
        let mut stored = tx.note(note)?.unwrap();
        stored.fields.back = "ptaszek".into();
        tx.update_note(&stored, TimestampMs(NOON)).map(drop)
    })
    .unwrap();
    let calls = f.llm.count();
    f.advance(60_000);
    let job = f.service.resume().unwrap().unwrap();
    f.wait(&job);
    assert_eq!(f.llm.count(), calls);
}

#[test]
fn a_dismissed_placeholder_disappears_and_is_not_retried() {
    let f = Fixture::new();
    f.enable_ai();
    let note = f.word("die Sonne", "słońce");
    f.llm.push([Err(LlmFailure::NotSent)]);
    f.generate("op-1", note, 1);
    let request = f.content(note).generations[0].request_id;
    let now = f.clock.now();
    f.db.write(move |tx| {
        management::update_note_content(
            tx,
            &CountingRng(Mutex::new(9)),
            "op-dismiss",
            note,
            &management::NoteContentChange::DismissGeneration(request),
            now,
        )
        .map(drop)
    })
    .unwrap();
    assert!(f.content(note).generations.is_empty());
    f.advance(3_600_000);
    assert!(f.service.resume().unwrap().is_none());
    // Another note's placeholder cannot be dismissed through this note.
    let other = f.word("der Regen", "deszcz");
    let missing =
        f.db.write(move |tx| llm::dismiss(tx, other, request, now))
            .unwrap_err();
    assert_eq!(missing.code(), "NOT_FOUND");
}

#[test]
fn the_daily_call_limit_stops_new_requests() {
    let f = Fixture::new();
    f.enable_ai();
    let now = f.clock.now();
    f.db.write(move |tx| {
        for _ in 0..llm::DAILY_CALL_LIMIT {
            tx.insert_llm_call(&langualinker_application::repositories::LlmCall {
                request_id: None,
                started_at: now,
                duration_ms: 900,
                provider: "anthropic".into(),
                model: "m".into(),
                outcome: "ok".into(),
            })?;
        }
        Ok(())
    })
    .unwrap();
    let note = f.word("der Wind", "wiatr");
    let error = f.service.generate("op-limit", note, 1).unwrap_err();
    assert!(matches!(error, AppError::QuotaExceeded { .. }));
    let quota = f.db.read(move |tx| llm::quota(tx, now)).unwrap();
    assert_eq!((quota.used, quota.limit), (100, 100));
    assert_eq!(
        f.db.read(|tx| llm::latency_p90(tx, TimestampMs(0)))
            .unwrap(),
        Some(900)
    );
}

#[test]
fn study_content_shows_placeholders_and_rating_does_not_wait() {
    let f = Fixture::new();
    f.enable_ai();
    let note = f.word("der Baum", "drzewo");
    f.llm
        .push([Err(LlmFailure::NotSent), Err(LlmFailure::NotSent)]);
    f.generate("op-1", note, 2);
    let content = f.content(note);
    assert_eq!(content.generations.len(), 2);
    assert!(
        content
            .generations
            .iter()
            .all(|generation| generation.placeholder_id().starts_with("llm-"))
    );
    // A card of the note can still be rated while its requests wait.
    let now = f.clock.now();
    let rated = f.db.write(move |tx| {
        let card = tx.cards_of_note(note)?[0].clone();
        let deck = card.deck_id;
        let view = langualinker_application::study::start_study(
            tx,
            &langualinker_application::study::StartStudy {
                operation_id: "op-study".into(),
                deck_id: Some(deck),
                card_ids: None,
            },
            now,
        )?;
        Ok(view.current.is_some())
    });
    assert_eq!(rated, Ok(true));
}

#[test]
fn sentences_and_texts_use_their_prompts() {
    let f = Fixture::new();
    f.enable_ai();
    let sentence = f.note(NoteTypeKind::Sentence, "Ich trinke Wasser.", "Piję wodę.");
    f.generate("op-1", sentence, 1);
    let text = f.note(
        NoteTypeKind::Sentence,
        "Am Wochenende fahren wir mit dem Zug zu meiner Oma, weil sie im Garten Hilfe braucht.",
        "W weekend jedziemy pociągiem do babci.",
    );
    f.generate("op-2", text, 1);
    let calls = f.llm.calls.lock().unwrap();
    assert_eq!(calls[0].0.prompt_version, "sentence-story-chain-v1");
    assert_eq!(calls[1].0.prompt_version, "text-key-term-chain-v1");
}

#[test]
fn hidden_sections_cannot_be_generated() {
    let f = Fixture::new();
    f.enable_ai();
    let note = f.word("die Tür", "drzwi");
    let deck = f.deck;
    f.db.write(move |tx| {
        let mut stored = tx.deck(deck)?.unwrap();
        stored.deck_type = DeckType::Simple;
        tx.update_deck(&stored, TimestampMs(NOON)).map(drop)
    })
    .unwrap();
    let error = f.service.generate("op-hidden", note, 1).unwrap_err();
    assert_eq!(error.code(), "VALIDATION_FAILED");
}
