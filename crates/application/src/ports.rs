//! Ports: everything the application needs from the outside world. Infrastructure implements
//! them; tests use fakes. Platform ports (LLM, notifications, TTS, files, telemetry) are declared
//! now so use cases can depend on them; their adapters arrive in Phases 12–16.

use langualinker_domain::{Guid, TimestampMs};

use crate::AppResult;
use crate::jobs::JobEvent;

/// Source of the current time. Injected so the scheduler and tests are deterministic.
pub trait Clock: Send + Sync {
    fn now(&self) -> TimestampMs;
}

/// Source of randomness (GUIDs, fuzz). Injected for deterministic tests.
pub trait Rng: Send + Sync {
    fn fill(&self, bytes: &mut [u8]);

    fn guid(&self) -> Guid {
        let mut bytes = [0u8; 16];
        self.fill(&mut bytes);
        Guid::from_bytes(bytes)
    }
}

/// OS secret storage (ADR-07). There is deliberately no way to list secrets, and no IPC command
/// returns a stored value: `read` exists for Rust-side consumers only (the LLM client, Phase 13).
pub trait SecretStore: Send + Sync {
    fn save(&self, name: &str, value: &str) -> AppResult<()>;
    /// Returns whether a secret was removed.
    fn remove(&self, name: &str) -> AppResult<bool>;
    fn has(&self, name: &str) -> AppResult<bool>;
    fn read(&self, name: &str) -> AppResult<Option<String>>;
}

/// Data areas a committed mutation touched; the UI refetches the matching queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CollectionScope {
    Decks,
    Cards,
    Notes,
    Settings,
    Stats,
    Backups,
}

/// Outgoing signals to the UI. Signals only: the UI reads the truth through queries.
pub trait EventSink: Send + Sync {
    fn job_progress(&self, event: &JobEvent);
    /// Emitted after a mutation committed, never before.
    fn collection_changed(&self, scopes: &[CollectionScope], at: TimestampMs);
}

/// Wire protocol of an association provider (settings `AI_PROVIDERS`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmProtocol {
    /// Anthropic Messages API (`api.anthropic.com`).
    Anthropic,
    /// OpenAI-compatible chat completions at a user-supplied HTTPS endpoint.
    OpenAiCompatible,
    /// Google Gemini API `generateContent` (`generativelanguage.googleapis.com`).
    Gemini,
}

/// A request to the association provider (Phase 13). Prompts are built in Rust; the card text is
/// only inside `user`, as data. The key travels separately as [`ApiKey`].
#[derive(Debug, Clone, PartialEq)]
pub struct LlmRequest {
    pub protocol: LlmProtocol,
    /// HTTPS base URL for [`LlmProtocol::OpenAiCompatible`]; ignored otherwise.
    pub endpoint: Option<String>,
    pub model: String,
    pub prompt_version: String,
    pub system: String,
    pub user: String,
    /// JSON Schema of the expected answer (sent where the protocol supports it).
    pub schema: serde_json::Value,
    /// Output token cap sent to the provider (pinned per prompt version).
    pub max_tokens: u32,
    /// Sampling temperature where the protocol accepts one (`None` = provider default).
    pub temperature: Option<f32>,
}

/// The provider key, read from the [`SecretStore`] by the service. `Debug` never shows it.
#[derive(Clone, PartialEq, Eq)]
pub struct ApiKey(pub String);

impl std::fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ApiKey(<redacted>)")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LlmResponse {
    /// The answer text (the JSON the prompt asked for, not yet parsed).
    pub text: String,
    /// Model that answered, as the provider reports it.
    pub model: String,
}

/// Failure classes of a provider call. `NotSent` is the only one where the provider certainly
/// did not receive (and cannot charge) the request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LlmFailure {
    /// No connection, DNS or TLS failure before the request was written.
    NotSent,
    /// Sent, but no complete answer in time.
    Timeout,
    /// The key was rejected (401/403).
    Unauthorized,
    /// The provider's quota or billing limit (402, `insufficient_quota`).
    Quota,
    /// Retryable provider failure: rate limit, overload, 5xx (`status` = HTTP status).
    Unavailable { status: u16 },
    /// Non-retryable provider error (other 4xx).
    Rejected { status: u16 },
    /// The provider declined to answer for safety reasons.
    Refused,
    /// The answer was cut off, too large or not in the expected envelope.
    Malformed,
    /// The caller cancelled while waiting.
    Cancelled,
}

pub trait LlmClient: Send + Sync {
    /// One provider call. `cancelled` is polled while waiting; returning `true` aborts the call.
    fn generate(
        &self,
        request: &LlmRequest,
        key: Option<&ApiKey>,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<LlmResponse, LlmFailure>;
}

/// Local notifications (Phase 15).
pub trait Notifications: Send + Sync {
    fn schedule_daily_reminder(&self, minutes_after_midnight: u32) -> AppResult<()>;
    fn cancel_daily_reminder(&self) -> AppResult<()>;
}

/// Text to speech (Phase 15); speech runs on the platform, Rust only reports availability.
pub trait Tts: Send + Sync {
    fn has_voice(&self, language: &str) -> AppResult<bool>;
}

/// Opaque token for a file the user picked. The UI never sends a path (plan step 29).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FileToken(pub String);

/// Access to user-picked files through tokens issued by the picker (Phases 12–14).
pub trait FileAccess: Send + Sync {
    fn read_all(&self, token: &FileToken) -> AppResult<Vec<u8>>;
    fn write_all(&self, token: &FileToken, bytes: &[u8]) -> AppResult<()>;
}

/// Opt-in usage telemetry (Phase 16). Off by default; events carry no content.
pub trait Telemetry: Send + Sync {
    fn record(&self, event: &'static str, fields: &[(&'static str, i64)]);
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::Mutex;

    use super::*;

    /// Counts up from a seed, so generated GUIDs are predictable.
    struct Counting(Mutex<u8>);

    impl Rng for Counting {
        fn fill(&self, bytes: &mut [u8]) {
            let mut next = self.0.lock().unwrap();
            for byte in bytes {
                *byte = *next;
                *next = next.wrapping_add(1);
            }
        }
    }

    struct Fixed(i64);

    impl Clock for Fixed {
        fn now(&self) -> TimestampMs {
            TimestampMs(self.0)
        }
    }

    #[test]
    fn default_guid_is_built_from_sixteen_random_bytes() {
        let rng = Counting(Mutex::new(0));
        assert_eq!(rng.guid().as_str(), "000102030405060708090a0b0c0d0e0f");
    }

    #[test]
    fn consecutive_guids_differ() {
        let rng = Counting(Mutex::new(0));
        assert_ne!(rng.guid(), rng.guid());
    }

    #[test]
    fn generated_guids_are_valid_stored_guids() {
        let guid = Counting(Mutex::new(250)).guid();
        assert_eq!(Guid::parse(guid.as_str()).as_ref(), Ok(&guid));
    }

    #[test]
    fn ports_are_usable_as_shared_trait_objects() {
        let clock: std::sync::Arc<dyn Clock> = std::sync::Arc::new(Fixed(42));
        assert_eq!(clock.now(), TimestampMs(42));
        let rng: std::sync::Arc<dyn Rng> = std::sync::Arc::new(Counting(Mutex::new(1)));
        assert_eq!(rng.guid().as_str().len(), 32);
    }

    #[test]
    fn file_tokens_compare_by_value() {
        let tokens: HashSet<FileToken> = ["a", "b", "a"].map(|t| FileToken(t.into())).into();
        assert_eq!(tokens.len(), 2);
    }

    #[test]
    fn collection_scopes_are_distinct_set_members() {
        let scopes: HashSet<CollectionScope> = [
            CollectionScope::Decks,
            CollectionScope::Cards,
            CollectionScope::Notes,
            CollectionScope::Settings,
            CollectionScope::Stats,
            CollectionScope::Backups,
            CollectionScope::Decks,
        ]
        .into();
        assert_eq!(scopes.len(), 6);
    }
}
