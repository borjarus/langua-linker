//! Mnemonic associations from an AI provider (plan Phase 13, ADR-18).
//!
//! Flow: `generate` checks consent, provider and key, then writes one outbox row per requested
//! variant (`llm_requests`, status `queued`) and starts a durable `llm` job. The rows show on the
//! card at once as placeholders; studying never waits for them. The job takes each row through:
//!
//! 1. *Prepare* (one write transaction): consent still granted, note still there with the same
//!    text, input moderation, then the cache — a hit attaches the stored answer with zero network
//!    calls. Otherwise the local daily call limit is checked, the row becomes `sending` and its
//!    attempt counter goes up *before* the call (a kill while sending counts as an attempt: the
//!    provider may already have charged it).
//! 2. *Call* outside any transaction, cancellable, bounded by the adapter's timeouts.
//! 3. *Finish* (one write transaction): the call is logged without content; a valid, moderated
//!    answer goes to the cache and — only if the row is still `sending` and the note text is
//!    unchanged — becomes a `generated` association. Failures follow an explicit policy:
//!    - not sent (offline): back to `queued` with a growing retry time, attempt not counted;
//!      the outbox is resumed on start, on return to the foreground and by a periodic check
//!      while the app runs — never after the app was killed (no native background work);
//!    - timeout, rate limit/overload/5xx, invalid output, moderation or a refusal: retried in the
//!      same job up to [`MAX_ATTEMPTS`] provider calls in total, then `failed` (or `blocked`);
//!    - rejected key, provider quota or another 4xx: `failed` at once.
//!
//!    There is no exactly-once promise: a provider without idempotency keys may charge a call
//!    whose answer never arrived, and the attempt limit bounds that cost.

pub mod moderation;
pub mod prompt;

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use langualinker_domain::content::{AssociationProvenance, ModerationStatus, NewAssociation};
use langualinker_domain::deck::Deck;
use langualinker_domain::note::{Note, NoteTypeKind};
use langualinker_domain::{NoteId, TimestampMs, ValidationError};

use crate::jobs::{JobContext, JobKind, JobRecord, JobService, NewJob};
use crate::learning;
use crate::ports::{
    ApiKey, Clock, CollectionScope, EventSink, LlmClient, LlmFailure, LlmProtocol, LlmRequest, Rng,
    SecretStore,
};
use crate::repositories::{
    CachedAssociation, LlmCall, LlmRequestRow, LlmRequestStatus, NewLlmRequest, Repositories,
    UnitOfWork,
};
use crate::settings::{self, AI_KEY_SECRET, AI_PROVIDERS, Consent};
use crate::{AppError, AppResult};

use prompt::{AnswerError, CacheInput, Params, PromptKind};

/// Provider calls per outbox row, shared by timeout, overload, parse and moderation retries.
pub const MAX_ATTEMPTS: u32 = 3;
/// Variants one request may ask for.
pub const MAX_VARIANTS: u32 = 3;
/// Local cost guard: provider calls per UTC day.
pub const DAILY_CALL_LIMIT: u32 = 100;
/// Offline rows give up after this many attempts to leave the device.
pub const MAX_OFFLINE_RETRIES: u32 = 200;
const OFFLINE_BACKOFF_MS: i64 = 30_000;
const OFFLINE_BACKOFF_MAX_MS: i64 = 15 * 60_000;
const DAY_MS: i64 = 86_400_000;
/// Rows a resumed outbox job takes at once.
const RESUME_BATCH: u32 = 20;

/// Prefix of the id a placeholder carries in `AssociationDto.id` (`llm-<request id>`), so it can
/// never be mistaken for a stored association.
pub const PLACEHOLDER_PREFIX: &str = "llm-";

/// Stored error codes that are not contract codes.
const CODE_CONSENT: &str = "CONSENT_WITHDRAWN";
const CODE_STALE: &str = "STALE";
const CODE_DISMISSED: &str = "DISMISSED";

// ---------------------------------------------------------------- read model

/// What a placeholder shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenerationState {
    Pending,
    OfflineQueued,
    Error,
    Blocked,
}

/// An outbox row as a card shows it, next to the stored associations.
#[derive(Debug, Clone, PartialEq)]
pub struct Generation {
    pub request_id: i64,
    pub note_id: NoteId,
    pub state: GenerationState,
    pub model: String,
    pub error: Option<AppError>,
}

impl Generation {
    pub fn placeholder_id(&self) -> String {
        format!("{PLACEHOLDER_PREFIX}{}", self.request_id)
    }
}

/// Parses `llm-<id>`; `None` for any other id.
pub fn parse_placeholder_id(value: &str) -> Option<i64> {
    value
        .strip_prefix(PLACEHOLDER_PREFIX)
        .filter(|digits| !digits.is_empty() && digits.len() <= 19)
        .and_then(|digits| digits.parse::<i64>().ok())
        .filter(|id| *id > 0)
}

fn stored_code(error: &AppError) -> String {
    match error {
        AppError::PermissionDenied { reason } => format!("PERMISSION_DENIED:{reason}"),
        other => other.code().to_owned(),
    }
}

/// The error a stored code stands for (details that are not stored are dropped).
fn error_from_code(code: &str) -> AppError {
    let (code, reason) = code.split_once(':').unwrap_or((code, ""));
    match code {
        "NETWORK_UNAVAILABLE" => AppError::NetworkUnavailable,
        "PROVIDER_TIMEOUT" => AppError::ProviderTimeout,
        "INVALID_PROVIDER_OUTPUT" => AppError::InvalidProviderOutput,
        "MODERATION_BLOCKED" => AppError::ModerationBlocked,
        "QUOTA_EXCEEDED" => AppError::QuotaExceeded { resets_at: 0 },
        "CANCELLED" => AppError::Cancelled,
        CODE_CONSENT => AppError::PermissionDenied {
            reason: "aiConsent",
        },
        "PERMISSION_DENIED" => AppError::PermissionDenied {
            reason: match reason {
                "aiConsent" => "aiConsent",
                "apiKey" => "apiKey",
                "apiKeyRejected" => "apiKeyRejected",
                "aiProvider" => "aiProvider",
                "aiModel" => "aiModel",
                "aiEndpoint" => "aiEndpoint",
                _ => "ai",
            },
        },
        _ => AppError::ProviderError { status: 0 },
    }
}

/// Placeholders of a note: requests still in flight, waiting offline, failed or blocked.
pub fn generations(tx: &mut dyn Repositories, note: NoteId) -> AppResult<Vec<Generation>> {
    Ok(tx
        .visible_llm_requests(note)?
        .into_iter()
        .map(|row| {
            let code = row.error_code.as_deref();
            let (state, error) = match row.status {
                LlmRequestStatus::Queued if code == Some("NETWORK_UNAVAILABLE") => {
                    (GenerationState::OfflineQueued, None)
                }
                LlmRequestStatus::Blocked if code == Some("MODERATION_BLOCKED") => {
                    (GenerationState::Blocked, Some(AppError::ModerationBlocked))
                }
                LlmRequestStatus::Blocked | LlmRequestStatus::Failed => (
                    GenerationState::Error,
                    Some(error_from_code(code.unwrap_or("PROVIDER_ERROR"))),
                ),
                _ => (GenerationState::Pending, None),
            };
            Generation {
                request_id: row.id,
                note_id: row.note_id,
                state,
                model: row.model,
                error,
            }
        })
        .collect())
}

/// Removes a placeholder (the delete button on the card). An in-flight call is not aborted, but
/// its answer is no longer attached; it still goes to the cache.
pub fn dismiss(
    tx: &mut dyn Repositories,
    note: NoteId,
    request_id: i64,
    now: TimestampMs,
) -> AppResult<()> {
    let mut row = tx
        .llm_request(request_id)?
        .filter(|row| row.note_id == note)
        .filter(|row| {
            !matches!(
                row.status,
                LlmRequestStatus::Done | LlmRequestStatus::Dismissed
            )
        })
        .ok_or(AppError::NotFound {
            entity: "association",
        })?;
    row.status = LlmRequestStatus::Dismissed;
    row.error_code = Some(CODE_DISMISSED.into());
    row.next_attempt_at = None;
    tx.update_llm_request(&row, now)
}

// ---------------------------------------------------------------- provider and quota

/// The provider configuration a request uses.
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderConfig {
    pub provider: String,
    pub protocol: LlmProtocol,
    pub model: String,
    pub endpoint: Option<String>,
    pub requires_key: bool,
}

/// Consent and provider settings, or the reason generation is not possible.
pub fn provider_config(tx: &mut dyn Repositories) -> AppResult<ProviderConfig> {
    let current = settings::read(tx)?;
    if current.ai_consent != Consent::Granted {
        return Err(AppError::PermissionDenied {
            reason: "aiConsent",
        });
    }
    let ai = current.doc.ai;
    let provider = ai
        .provider
        .as_deref()
        .and_then(|id| AI_PROVIDERS.iter().find(|provider| provider.id == id))
        .ok_or(AppError::PermissionDenied {
            reason: "aiProvider",
        })?;
    let model = ai
        .model
        .ok_or(AppError::PermissionDenied { reason: "aiModel" })?;
    let protocol = match provider.id {
        "anthropic" => LlmProtocol::Anthropic,
        _ => LlmProtocol::OpenAiCompatible,
    };
    if provider.custom_endpoint && ai.endpoint.is_none() {
        return Err(AppError::PermissionDenied {
            reason: "aiEndpoint",
        });
    }
    Ok(ProviderConfig {
        provider: provider.id.to_owned(),
        protocol,
        model,
        endpoint: ai.endpoint.filter(|_| provider.custom_endpoint),
        requires_key: provider.requires_key,
    })
}

/// Local daily call limit (UTC day).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quota {
    pub used: u32,
    pub limit: u32,
    pub resets_at: TimestampMs,
}

pub fn quota(tx: &mut dyn Repositories, now: TimestampMs) -> AppResult<Quota> {
    let day_start = now.0 - now.0.rem_euclid(DAY_MS);
    Ok(Quota {
        used: tx.llm_calls_since(TimestampMs(day_start))?,
        limit: DAILY_CALL_LIMIT,
        resets_at: TimestampMs(day_start + DAY_MS),
    })
}

/// 90th percentile of successful call durations since `since` (plan step 18); `None` without
/// calls. Only times are measured, never content.
pub fn latency_p90(tx: &mut dyn Repositories, since: TimestampMs) -> AppResult<Option<i64>> {
    let mut durations = tx.llm_call_durations(since)?;
    if durations.is_empty() {
        return Ok(None);
    }
    durations.sort_unstable();
    let rank = (durations.len() * 9).div_ceil(10).max(1) - 1;
    Ok(Some(durations[rank]))
}

// ---------------------------------------------------------------- note context

struct NoteContext {
    note: Note,
    deck: Deck,
    kind: NoteTypeKind,
}

fn note_context(tx: &mut dyn Repositories, note_id: NoteId) -> AppResult<NoteContext> {
    let note = learning::live_note(tx, note_id)?;
    let card = tx
        .cards_of_note(note.id)?
        .into_iter()
        .next()
        .ok_or(AppError::NotFound { entity: "card" })?;
    let deck = learning::live_deck(tx, card.deck_id)?;
    let kind = tx
        .note_type_by_id(note.note_type_id)?
        .ok_or(AppError::NotFound { entity: "noteType" })?
        .kind;
    Ok(NoteContext { note, deck, kind })
}

impl NoteContext {
    fn prompt_kind(&self) -> PromptKind {
        PromptKind::for_note(self.kind, &self.note.fields.front)
    }

    fn source_hash(&self) -> String {
        prompt::source_hash(
            &self.note.fields.front,
            &self.note.fields.back,
            self.deck.native_language,
            self.deck.target_language,
        )
    }

    fn cache_input<'a>(&'a self, config: &'a ProviderConfig) -> CacheInput<'a> {
        CacheInput {
            kind: self.prompt_kind(),
            provider: &config.provider,
            model: &config.model,
            params: Params::for_protocol(config.protocol),
            native: self.deck.native_language,
            target: self.deck.target_language,
            front: &self.note.fields.front,
            back: &self.note.fields.back,
        }
    }

    fn request(&self, config: &ProviderConfig, variant: u8) -> LlmRequest {
        let kind = self.prompt_kind();
        let params = Params::for_protocol(config.protocol);
        LlmRequest {
            protocol: config.protocol,
            endpoint: config.endpoint.clone(),
            model: config.model.clone(),
            prompt_version: kind.version().to_owned(),
            system: prompt::system_prompt(
                kind,
                self.deck.native_language,
                self.deck.target_language,
            ),
            user: prompt::user_prompt(&self.note.fields.front, &self.note.fields.back, variant),
            schema: prompt::answer_schema(),
            max_tokens: params.max_tokens,
            temperature: params.temperature,
        }
    }
}

/// The first `count` cache slots this note has not used yet: a new request asks for a new
/// association, while the same text in another note (or after the note was deleted and
/// re-created) reuses the cache.
fn free_slots(used: &HashSet<String>, input: &CacheInput<'_>, count: usize) -> Vec<String> {
    (0u32..)
        .map(|slot| input.key(slot))
        .filter(|key| !used.contains(key))
        .take(count)
        .collect()
}

// ---------------------------------------------------------------- service

/// Generation use cases and the outbox runner.
pub struct LlmService<U: UnitOfWork> {
    db: Arc<U>,
    jobs: JobService<U>,
    events: Arc<dyn EventSink>,
    clock: Arc<dyn Clock>,
    rng: Arc<dyn Rng>,
    client: Arc<dyn LlmClient>,
    secrets: Option<Arc<dyn SecretStore>>,
    /// Rows a job of this process is working on; a resume never takes them twice.
    active: Arc<Mutex<HashSet<i64>>>,
    /// Pause before retrying a timeout or an overload (multiplied by the attempt number).
    retry_delay: Duration,
}

impl<U: UnitOfWork> Clone for LlmService<U> {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            jobs: self.jobs.clone(),
            events: self.events.clone(),
            clock: self.clock.clone(),
            rng: self.rng.clone(),
            client: self.client.clone(),
            secrets: self.secrets.clone(),
            active: self.active.clone(),
            retry_delay: self.retry_delay,
        }
    }
}

/// What preparing a row decided.
enum Prepared {
    /// Nothing to do (already handled, or another job has it).
    Skip,
    /// Finished without a network call (cache hit, stale note, blocked input, limits).
    Finished(AppResult<bool>),
    Send {
        request: LlmRequest,
        provider: String,
        model: String,
        requires_key: bool,
    },
}

/// What finishing a call decided.
enum Finished {
    Done,
    Retry(Duration),
    Stop(AppResult<bool>),
}

/// Outcome of one provider call after parsing and moderation.
enum CallOutcome {
    Answer { text: String, model: String },
    Failure(LlmFailure),
    Invalid,
    Blocked,
}

impl CallOutcome {
    fn label(&self) -> &'static str {
        match self {
            Self::Answer { .. } => "ok",
            Self::Invalid => "invalidOutput",
            Self::Blocked => "moderationBlocked",
            Self::Failure(failure) => match failure {
                LlmFailure::NotSent => "notSent",
                LlmFailure::Timeout => "timeout",
                LlmFailure::Unauthorized => "unauthorized",
                LlmFailure::Quota => "quota",
                LlmFailure::Unavailable { .. } => "unavailable",
                LlmFailure::Rejected { .. } => "rejected",
                LlmFailure::Refused => "refused",
                LlmFailure::Malformed => "malformed",
                LlmFailure::Cancelled => "cancelled",
            },
        }
    }
}

impl<U: UnitOfWork + 'static> LlmService<U> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        db: Arc<U>,
        jobs: JobService<U>,
        events: Arc<dyn EventSink>,
        clock: Arc<dyn Clock>,
        rng: Arc<dyn Rng>,
        client: Arc<dyn LlmClient>,
        secrets: Option<Arc<dyn SecretStore>>,
    ) -> Self {
        Self {
            db,
            jobs,
            events,
            clock,
            rng,
            client,
            secrets,
            active: Arc::default(),
            retry_delay: Duration::from_millis(1_500),
        }
    }

    /// Pause before timeout/overload retries (tests use zero).
    pub fn with_retry_delay(mut self, delay: Duration) -> Self {
        self.retry_delay = delay;
        self
    }

    fn has_key(&self) -> bool {
        self.secrets
            .as_ref()
            .is_some_and(|store| store.has(AI_KEY_SECRET).unwrap_or(false))
    }

    fn key(&self) -> AppResult<Option<ApiKey>> {
        match &self.secrets {
            Some(store) => Ok(store.read(AI_KEY_SECRET)?.map(ApiKey)),
            None => Ok(None),
        }
    }

    /// GenerateAssociation: queues `variants` new associations of a note and starts a job.
    /// A repeated `operation_id` returns the same job and queues nothing again.
    pub fn generate(
        &self,
        operation_id: &str,
        note_id: NoteId,
        variants: u32,
    ) -> AppResult<JobRecord> {
        if !(1..=MAX_VARIANTS).contains(&variants) {
            return Err(ValidationError::new("variants", "range").into());
        }
        let config = self.db.read(provider_config)?;
        if config.requires_key && !self.has_key() {
            return Err(AppError::PermissionDenied { reason: "apiKey" });
        }
        let now = self.clock.now();
        let operation = operation_id.to_owned();
        self.db.write(move |tx| {
            if tx.recorded_operation(&operation)?.is_some() {
                return Ok(());
            }
            let context = note_context(tx, note_id)?;
            if !context.deck.deck_type.capabilities().associations {
                return Err(ValidationError::new("noteId", "associationsHidden").into());
            }
            let quota = quota(tx, now)?;
            if quota.used + variants > quota.limit {
                return Err(AppError::QuotaExceeded {
                    resets_at: quota.resets_at.0,
                });
            }
            let used: HashSet<String> = tx.used_cache_keys(note_id)?.into_iter().collect();
            let input = context.cache_input(&config);
            let keys = free_slots(&used, &input, variants as usize);
            let kind = context.prompt_kind();
            let source_hash = context.source_hash();
            for (variant, cache_key) in keys.into_iter().enumerate() {
                tx.insert_llm_request(
                    &NewLlmRequest {
                        note_id,
                        operation_id: operation.clone(),
                        variant: u8::try_from(variant).unwrap_or(0),
                        prompt_kind: kind.as_str().to_owned(),
                        prompt_version: kind.version().to_owned(),
                        provider: config.provider.clone(),
                        model: config.model.clone(),
                        cache_key,
                        source_hash: source_hash.clone(),
                    },
                    now,
                )?;
            }
            Ok(())
        })?;
        self.events
            .collection_changed(&[CollectionScope::Notes], self.clock.now());

        let service = self.clone();
        let operation = operation_id.to_owned();
        self.jobs.start(
            NewJob {
                kind: JobKind::Llm,
                operation_id: Some(operation_id.to_owned()),
                total: Some(u64::from(variants)),
            },
            Box::new(move |context| {
                let rows = service
                    .db
                    .read(move |tx| tx.llm_requests_of_operation(&operation))?;
                let job_id = context.id().0.parse::<i64>().ok();
                let ids: Vec<i64> = rows.iter().map(|row| row.id).collect();
                let at = service.clock.now();
                service.db.write(move |tx| {
                    for mut row in rows {
                        row.job_id = job_id;
                        tx.update_llm_request(&row, at)?;
                    }
                    Ok(())
                })?;
                service.run(context, &ids)
            }),
        )
    }

    /// Requests left `sending` by a killed process go back to the queue; one that already used
    /// its attempts fails as a timeout (its answer is lost and may have been charged).
    pub fn recover_interrupted(&self) -> AppResult<usize> {
        let now = self.clock.now();
        self.db.write(move |tx| {
            let rows = tx.llm_requests_with_status(LlmRequestStatus::Sending)?;
            for mut row in rows.iter().cloned() {
                if row.attempts >= MAX_ATTEMPTS {
                    row.status = LlmRequestStatus::Failed;
                    row.error_code = Some("PROVIDER_TIMEOUT".into());
                    row.next_attempt_at = None;
                } else {
                    row.status = LlmRequestStatus::Queued;
                    row.error_code = Some("PROVIDER_TIMEOUT".into());
                    row.next_attempt_at = Some(now);
                }
                tx.update_llm_request(&row, now)?;
            }
            Ok(rows.len())
        })
    }

    /// Runs queued rows whose retry time has come (start, foreground, periodic check). Returns
    /// the job, or `None` when nothing is due.
    pub fn resume(&self) -> AppResult<Option<JobRecord>> {
        let now = self.clock.now();
        let due = self
            .db
            .read(move |tx| tx.due_llm_requests(now, RESUME_BATCH))?;
        let ids: Vec<i64> = {
            let active = self.active.lock().expect("llm registry");
            due.into_iter().filter(|id| !active.contains(id)).collect()
        };
        if ids.is_empty() {
            return Ok(None);
        }
        let service = self.clone();
        let total = ids.len() as u64;
        self.jobs
            .start(
                NewJob {
                    kind: JobKind::Llm,
                    operation_id: None,
                    total: Some(total),
                },
                Box::new(move |context| service.run(context, &ids)),
            )
            .map(Some)
    }

    /// Works through rows; succeeds when at least one association was attached (or nothing
    /// failed), otherwise fails with the first row's error.
    fn run(&self, context: &JobContext<U>, ids: &[i64]) -> AppResult<Option<String>> {
        let claimed: Vec<i64> = {
            let mut active = self.active.lock().expect("llm registry");
            ids.iter()
                .copied()
                .filter(|id| active.insert(*id))
                .collect()
        };
        let total = claimed.len() as u64;
        let mut attached = 0;
        let mut first_error = None;
        let mut outcome = Ok(());
        for (index, id) in claimed.iter().enumerate() {
            if let Err(error) = context.check_cancelled() {
                outcome = Err(error);
                break;
            }
            match self.process(context, *id) {
                Ok(true) => attached += 1,
                Ok(false) => {}
                Err(AppError::Cancelled) => {
                    outcome = Err(AppError::Cancelled);
                    break;
                }
                Err(error) => {
                    first_error.get_or_insert(error);
                }
            }
            if let Err(error) = context.progress(index as u64 + 1, total) {
                outcome = Err(error);
                break;
            }
        }
        if outcome == Err(AppError::Cancelled) {
            // Cancel policy: rows of a cancelled job are dismissed, never retried later.
            let at = self.clock.now();
            let rest = claimed.clone();
            let dismissed = self.db.write(move |tx| {
                for id in rest {
                    if let Some(mut row) = tx.llm_request(id)?
                        && matches!(
                            row.status,
                            LlmRequestStatus::Queued | LlmRequestStatus::Sending
                        )
                    {
                        row.status = LlmRequestStatus::Dismissed;
                        row.error_code = Some("CANCELLED".into());
                        row.next_attempt_at = None;
                        tx.update_llm_request(&row, at)?;
                    }
                }
                Ok(())
            });
            if dismissed.is_err() {
                tracing::warn!("cancelled AI requests not dismissed");
            }
            self.events
                .collection_changed(&[CollectionScope::Notes], self.clock.now());
        }
        {
            let mut active = self.active.lock().expect("llm registry");
            for id in &claimed {
                active.remove(id);
            }
        }
        outcome?;
        match first_error {
            Some(error) if attached == 0 => Err(error),
            _ => Ok(None),
        }
    }

    /// One row: prepare, call, finish — repeated while the policy allows a retry.
    fn process(&self, context: &JobContext<U>, id: i64) -> AppResult<bool> {
        loop {
            let now = self.clock.now();
            let rng = self.rng.clone();
            let prepared = self
                .db
                .write(move |tx| prepare(tx, rng.as_ref(), id, now))?;
            let (request, provider, model, requires_key) = match prepared {
                Prepared::Skip => return Ok(false),
                Prepared::Finished(result) => {
                    self.changed();
                    return result;
                }
                Prepared::Send {
                    request,
                    provider,
                    model,
                    requires_key,
                } => (request, provider, model, requires_key),
            };
            let key = self.key()?;
            if requires_key && key.is_none() {
                let at = self.clock.now();
                let error = AppError::PermissionDenied { reason: "apiKey" };
                let stored = stored_code(&error);
                self.db.write(move |tx| {
                    if let Some(mut row) = tx.llm_request(id)? {
                        row.status = LlmRequestStatus::Failed;
                        row.attempts = row.attempts.saturating_sub(1);
                        row.error_code = Some(stored);
                        tx.update_llm_request(&row, at)?;
                    }
                    Ok(())
                })?;
                self.changed();
                return Err(error);
            }

            let started_at = self.clock.now();
            let started = Instant::now();
            let cancelled = || context.check_cancelled().is_err();
            let outcome = match self.client.generate(&request, key.as_ref(), &cancelled) {
                Ok(response) => match prompt::parse_answer(&response.text) {
                    Ok(text) => {
                        if moderation::check(&[&text]) == moderation::Verdict::Blocked {
                            CallOutcome::Blocked
                        } else {
                            CallOutcome::Answer {
                                text,
                                model: response.model,
                            }
                        }
                    }
                    Err(AnswerError::Declined) => CallOutcome::Blocked,
                    Err(AnswerError::Invalid) => CallOutcome::Invalid,
                },
                Err(LlmFailure::Refused) => CallOutcome::Blocked,
                Err(LlmFailure::Malformed) => CallOutcome::Invalid,
                Err(failure) => CallOutcome::Failure(failure),
            };
            let duration_ms = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
            tracing::info!(
                outcome = outcome.label(),
                duration_ms,
                "AI provider call finished"
            );
            let call = LlmCall {
                request_id: Some(id),
                started_at,
                duration_ms,
                provider,
                model,
                outcome: outcome.label().to_owned(),
            };
            let now = self.clock.now();
            let delay = self.retry_delay;
            let rng = self.rng.clone();
            let finished = self
                .db
                .write(move |tx| finish(tx, rng.as_ref(), id, call, outcome, delay, now))?;
            match finished {
                Finished::Done => {
                    self.changed();
                    return Ok(true);
                }
                Finished::Retry(wait) => {
                    self.wait(context, wait)?;
                }
                Finished::Stop(result) => {
                    self.changed();
                    return result;
                }
            }
        }
    }

    /// Sleeps in short steps so a cancel is noticed.
    fn wait(&self, context: &JobContext<U>, total: Duration) -> AppResult<()> {
        let step = Duration::from_millis(100);
        let deadline = Instant::now() + total;
        while Instant::now() < deadline {
            context.check_cancelled()?;
            std::thread::sleep(step.min(deadline - Instant::now()));
        }
        context.check_cancelled()
    }

    fn changed(&self) {
        self.events
            .collection_changed(&[CollectionScope::Notes], self.clock.now());
    }
}

fn mark(
    tx: &mut dyn Repositories,
    mut row: LlmRequestRow,
    status: LlmRequestStatus,
    code: Option<String>,
    now: TimestampMs,
) -> AppResult<()> {
    row.status = status;
    row.error_code = code;
    row.next_attempt_at = None;
    tx.update_llm_request(&row, now)
}

fn attach(
    tx: &mut dyn Repositories,
    rng: &dyn Rng,
    mut row: LlmRequestRow,
    context: &NoteContext,
    text: String,
    model: String,
    now: TimestampMs,
) -> AppResult<()> {
    let association = tx.insert_association(
        &NewAssociation {
            guid: rng.guid(),
            note_id: row.note_id,
            text,
            provenance: AssociationProvenance::Generated,
            native_language: context.deck.native_language.code().to_owned(),
            target_language: context.deck.target_language.code().to_owned(),
            prompt_version: Some(row.prompt_version.clone()),
            model: Some(model),
            moderation: ModerationStatus::Passed,
            cache_key: Some(row.cache_key.clone()),
        },
        now,
    )?;
    row.association_id = Some(association.id);
    row.status = LlmRequestStatus::Done;
    row.error_code = None;
    row.next_attempt_at = None;
    tx.update_llm_request(&row, now)
}

/// Step 1 of a row (see the module docs).
fn prepare(
    tx: &mut dyn Repositories,
    rng: &dyn Rng,
    id: i64,
    now: TimestampMs,
) -> AppResult<Prepared> {
    let Some(mut row) = tx.llm_request(id)? else {
        return Ok(Prepared::Skip);
    };
    if row.status != LlmRequestStatus::Queued {
        return Ok(Prepared::Skip);
    }
    let config = match provider_config(tx) {
        Ok(config) => config,
        Err(error) => {
            let status = if matches!(
                error,
                AppError::PermissionDenied {
                    reason: "aiConsent"
                }
            ) {
                LlmRequestStatus::Blocked
            } else {
                LlmRequestStatus::Failed
            };
            mark(tx, row, status, Some(stored_code(&error)), now)?;
            return Ok(Prepared::Finished(Err(error)));
        }
    };
    let context = match note_context(tx, row.note_id) {
        Ok(context) => context,
        Err(AppError::NotFound { .. } | AppError::Deleted { .. }) => {
            mark(
                tx,
                row,
                LlmRequestStatus::Dismissed,
                Some(CODE_STALE.into()),
                now,
            )?;
            return Ok(Prepared::Finished(Ok(false)));
        }
        Err(error) => return Err(error),
    };
    if context.source_hash() != row.source_hash
        || !context.deck.deck_type.capabilities().associations
    {
        // The note text changed while the request waited: the old prompt is not sent.
        mark(
            tx,
            row,
            LlmRequestStatus::Dismissed,
            Some(CODE_STALE.into()),
            now,
        )?;
        return Ok(Prepared::Finished(Ok(false)));
    }
    if moderation::check(&[&context.note.fields.front, &context.note.fields.back])
        == moderation::Verdict::Blocked
    {
        mark(
            tx,
            row,
            LlmRequestStatus::Blocked,
            Some("MODERATION_BLOCKED".into()),
            now,
        )?;
        return Ok(Prepared::Finished(Err(AppError::ModerationBlocked)));
    }
    if row.provider != config.provider || row.model != config.model {
        // The learner switched provider or model while the row waited: use the current one.
        let used: HashSet<String> = tx
            .used_cache_keys(row.note_id)?
            .into_iter()
            .filter(|key| *key != row.cache_key)
            .collect();
        let input = context.cache_input(&config);
        row.cache_key = free_slots(&used, &input, 1)
            .pop()
            .unwrap_or_else(|| input.key(0));
        row.provider = config.provider.clone();
        row.model = config.model.clone();
        row.prompt_kind = context.prompt_kind().as_str().to_owned();
        row.prompt_version = context.prompt_kind().version().to_owned();
    }
    if let Some(cached) = tx.cached_association(&row.cache_key)? {
        attach(tx, rng, row, &context, cached.text, cached.model, now)?;
        return Ok(Prepared::Finished(Ok(true)));
    }
    let quota = quota(tx, now)?;
    if quota.used >= quota.limit {
        mark(
            tx,
            row,
            LlmRequestStatus::Failed,
            Some("QUOTA_EXCEEDED".into()),
            now,
        )?;
        return Ok(Prepared::Finished(Err(AppError::QuotaExceeded {
            resets_at: quota.resets_at.0,
        })));
    }
    let request = context.request(&config, row.variant);
    row.status = LlmRequestStatus::Sending;
    row.attempts += 1;
    row.next_attempt_at = None;
    let (provider, model) = (row.provider.clone(), row.model.clone());
    tx.update_llm_request(&row, now)?;
    Ok(Prepared::Send {
        request,
        provider,
        model,
        requires_key: config.requires_key,
    })
}

/// Step 3 of a row (see the module docs).
fn finish(
    tx: &mut dyn Repositories,
    rng: &dyn Rng,
    id: i64,
    call: LlmCall,
    outcome: CallOutcome,
    retry_delay: Duration,
    now: TimestampMs,
) -> AppResult<Finished> {
    tx.insert_llm_call(&call)?;
    let Some(mut row) = tx.llm_request(id)? else {
        // The note was deleted (its rows cascade).
        return Ok(Finished::Stop(Ok(false)));
    };
    let sending = row.status == LlmRequestStatus::Sending;
    let retry_left = row.attempts < MAX_ATTEMPTS;
    let retry = |mut row: LlmRequestRow,
                 tx: &mut dyn Repositories,
                 code: &str,
                 wait: Duration|
     -> AppResult<Finished> {
        row.status = LlmRequestStatus::Queued;
        row.error_code = Some(code.to_owned());
        tx.update_llm_request(&row, now)?;
        Ok(Finished::Retry(wait))
    };
    match outcome {
        CallOutcome::Answer { text, model } => {
            // A valid answer is kept for reuse even when it is no longer attached here.
            tx.put_cached_association(
                &row.cache_key,
                &CachedAssociation {
                    text: text.clone(),
                    model: model.clone(),
                    prompt_version: row.prompt_version.clone(),
                },
                now,
            )?;
            if !sending {
                return Ok(Finished::Stop(Ok(false)));
            }
            match note_context(tx, row.note_id) {
                Ok(context) if context.source_hash() == row.source_hash => {
                    attach(tx, rng, row, &context, text, model, now)?;
                    Ok(Finished::Done)
                }
                Ok(_) | Err(AppError::NotFound { .. } | AppError::Deleted { .. }) => {
                    // Changed or deleted note: the answer is for a text that is gone.
                    mark(
                        tx,
                        row,
                        LlmRequestStatus::Dismissed,
                        Some(CODE_STALE.into()),
                        now,
                    )?;
                    Ok(Finished::Stop(Ok(false)))
                }
                Err(error) => Err(error),
            }
        }
        _ if !sending => Ok(Finished::Stop(Ok(false))),
        CallOutcome::Failure(LlmFailure::NotSent) => {
            // Never reached the provider: not an attempt; wait for the connection.
            row.attempts = row.attempts.saturating_sub(1);
            row.offline_retries += 1;
            if row.offline_retries >= MAX_OFFLINE_RETRIES {
                mark(
                    tx,
                    row,
                    LlmRequestStatus::Failed,
                    Some("NETWORK_UNAVAILABLE".into()),
                    now,
                )?;
                return Ok(Finished::Stop(Err(AppError::NetworkUnavailable)));
            }
            let exponent = row.offline_retries.saturating_sub(1).min(10);
            let backoff = (OFFLINE_BACKOFF_MS << exponent).min(OFFLINE_BACKOFF_MAX_MS);
            row.status = LlmRequestStatus::Queued;
            row.error_code = Some("NETWORK_UNAVAILABLE".into());
            row.next_attempt_at = Some(TimestampMs(now.0 + backoff));
            tx.update_llm_request(&row, now)?;
            Ok(Finished::Stop(Err(AppError::NetworkUnavailable)))
        }
        CallOutcome::Failure(LlmFailure::Cancelled) => {
            mark(
                tx,
                row,
                LlmRequestStatus::Dismissed,
                Some("CANCELLED".into()),
                now,
            )?;
            Ok(Finished::Stop(Err(AppError::Cancelled)))
        }
        CallOutcome::Failure(LlmFailure::Timeout) => {
            if retry_left {
                let wait = retry_delay * row.attempts;
                return retry(row, tx, "PROVIDER_TIMEOUT", wait);
            }
            mark(
                tx,
                row,
                LlmRequestStatus::Failed,
                Some("PROVIDER_TIMEOUT".into()),
                now,
            )?;
            Ok(Finished::Stop(Err(AppError::ProviderTimeout)))
        }
        CallOutcome::Failure(LlmFailure::Unavailable { status }) => {
            if retry_left {
                let wait = retry_delay * row.attempts;
                return retry(row, tx, "PROVIDER_ERROR", wait);
            }
            mark(
                tx,
                row,
                LlmRequestStatus::Failed,
                Some("PROVIDER_ERROR".into()),
                now,
            )?;
            Ok(Finished::Stop(Err(AppError::ProviderError {
                status: i64::from(status),
            })))
        }
        CallOutcome::Invalid => {
            if retry_left {
                return retry(row, tx, "INVALID_PROVIDER_OUTPUT", Duration::ZERO);
            }
            mark(
                tx,
                row,
                LlmRequestStatus::Failed,
                Some("INVALID_PROVIDER_OUTPUT".into()),
                now,
            )?;
            Ok(Finished::Stop(Err(AppError::InvalidProviderOutput)))
        }
        CallOutcome::Blocked => {
            if retry_left {
                return retry(row, tx, "MODERATION_BLOCKED", Duration::ZERO);
            }
            mark(
                tx,
                row,
                LlmRequestStatus::Blocked,
                Some("MODERATION_BLOCKED".into()),
                now,
            )?;
            Ok(Finished::Stop(Err(AppError::ModerationBlocked)))
        }
        CallOutcome::Failure(LlmFailure::Unauthorized) => {
            let error = AppError::PermissionDenied {
                reason: "apiKeyRejected",
            };
            mark(
                tx,
                row,
                LlmRequestStatus::Failed,
                Some(stored_code(&error)),
                now,
            )?;
            Ok(Finished::Stop(Err(error)))
        }
        CallOutcome::Failure(LlmFailure::Quota) => {
            mark(
                tx,
                row,
                LlmRequestStatus::Failed,
                Some("QUOTA_EXCEEDED".into()),
                now,
            )?;
            Ok(Finished::Stop(Err(AppError::QuotaExceeded {
                resets_at: 0,
            })))
        }
        CallOutcome::Failure(LlmFailure::Rejected { status }) => {
            mark(
                tx,
                row,
                LlmRequestStatus::Failed,
                Some("PROVIDER_ERROR".into()),
                now,
            )?;
            Ok(Finished::Stop(Err(AppError::ProviderError {
                status: i64::from(status),
            })))
        }
        CallOutcome::Failure(LlmFailure::Refused | LlmFailure::Malformed) => {
            unreachable!("mapped to Blocked and Invalid before finish")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_ids_round_trip_and_reject_other_ids() {
        assert_eq!(parse_placeholder_id("llm-42"), Some(42));
        for bad in ["42", "llm-", "llm--1", "llm-0", "llm-x", "LLM-4"] {
            assert_eq!(parse_placeholder_id(bad), None, "{bad}");
        }
        let generation = Generation {
            request_id: 7,
            note_id: NoteId(1),
            state: GenerationState::Pending,
            model: "m".into(),
            error: None,
        };
        assert_eq!(generation.placeholder_id(), "llm-7");
    }

    #[test]
    fn stored_codes_round_trip_to_errors() {
        for error in [
            AppError::NetworkUnavailable,
            AppError::ProviderTimeout,
            AppError::InvalidProviderOutput,
            AppError::ModerationBlocked,
            AppError::PermissionDenied {
                reason: "apiKeyRejected",
            },
            AppError::PermissionDenied {
                reason: "aiConsent",
            },
        ] {
            assert_eq!(error_from_code(&stored_code(&error)), error);
        }
        assert_eq!(
            error_from_code(CODE_CONSENT),
            AppError::PermissionDenied {
                reason: "aiConsent"
            }
        );
        assert_eq!(error_from_code("PROVIDER_ERROR").code(), "PROVIDER_ERROR");
    }
}
