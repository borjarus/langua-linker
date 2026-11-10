//! HTTP adapter of the [`LlmClient`] port (plan Phase 13 step 5, ADR-18).
//!
//! - reqwest on a per-call Tokio current-thread runtime: job threads are plain threads, and a
//!   connection pool never outlives the runtime that owns its tasks.
//! - TLS: rustls with the `ring` provider and the Mozilla root store (webpki-roots). No platform
//!   verifier, so nothing has to be initialised over JNI on Android; a user-installed CA is not
//!   trusted (ADR-18).
//! - Endpoint policy: HTTPS only, no redirects (a key never follows one to another host), no
//!   system proxy, connect timeout 10 s, whole call 45 s, answers above 64 KiB are refused.
//! - Errors are classified for the outbox policy: `NotSent` only when the connection was never
//!   made; anything after that may have reached the provider.
//! - Nothing here logs prompts, answers or the key.

use std::sync::Arc;
use std::time::Duration;

use langualinker_application::ports::{
    ApiKey, LlmClient, LlmFailure, LlmProtocol, LlmRequest, LlmResponse,
};
use serde_json::{Value, json};

const ANTHROPIC_BASE: &str = "https://api.anthropic.com";
const ANTHROPIC_VERSION: &str = "2026-10-21";
const GEMINI_BASE: &str = "https://generativelanguage.googleapis.com";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const CALL_TIMEOUT: Duration = Duration::from_secs(45);
const MAX_RESPONSE_BYTES: usize = 64 * 1024;
const CANCEL_POLL: Duration = Duration::from_millis(100);

/// Calls Anthropic, Google Gemini or an OpenAI-compatible endpoint.
pub struct HttpLlmClient {
    tls: Arc<rustls::ClientConfig>,
    anthropic_base: String,
    gemini_base: String,
    /// Tests talk to a local plain-HTTP server; production never allows it.
    allow_loopback_http: bool,
    call_timeout: Duration,
}

impl Default for HttpLlmClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpLlmClient {
    pub fn new() -> Self {
        let roots = rustls::RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        };
        let mut tls = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .expect("ring supports the default TLS versions")
        .with_root_certificates(roots)
        .with_no_client_auth();
        tls.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
        Self {
            tls: Arc::new(tls),
            anthropic_base: ANTHROPIC_BASE.to_owned(),
            gemini_base: GEMINI_BASE.to_owned(),
            allow_loopback_http: false,
            call_timeout: CALL_TIMEOUT,
        }
    }

    /// A client for tests against a local server: plain HTTP to 127.0.0.1 is accepted and the
    /// Anthropic and Gemini base URLs point there.
    pub fn for_local_tests(base: &str, call_timeout: Duration) -> Self {
        Self {
            anthropic_base: base.trim_end_matches('/').to_owned(),
            gemini_base: base.trim_end_matches('/').to_owned(),
            allow_loopback_http: true,
            call_timeout,
            ..Self::new()
        }
    }

    fn allowed(&self, url: &str) -> bool {
        let host_of = |rest: &str| {
            rest.split(['/', '?', '#'])
                .next()
                .unwrap_or_default()
                .to_owned()
        };
        if let Some(rest) = url.strip_prefix("https://") {
            let host = host_of(rest);
            return !host.is_empty() && !host.contains('@');
        }
        if self.allow_loopback_http
            && let Some(rest) = url.strip_prefix("http://")
        {
            return host_of(rest).starts_with("127.0.0.1:");
        }
        false
    }

    fn url(&self, request: &LlmRequest) -> Option<String> {
        let url = match request.protocol {
            LlmProtocol::Anthropic => format!("{}/v1/messages", self.anthropic_base),
            LlmProtocol::OpenAiCompatible => {
                let base = request.endpoint.as_deref()?.trim_end_matches('/');
                if base.ends_with("/chat/completions") {
                    base.to_owned()
                } else {
                    format!("{base}/chat/completions")
                }
            }
            // The model is part of the path: only plain model ids, nothing that changes the path.
            LlmProtocol::Gemini => {
                let model = request.model.as_str();
                let plain = !model.is_empty()
                    && model.len() <= 100
                    && model
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_'))
                    && !model.contains("..");
                if !plain {
                    return None;
                }
                format!("{}/v1beta/models/{model}:generateContent", self.gemini_base)
            }
        };
        self.allowed(&url).then_some(url)
    }

    fn client(&self) -> Result<reqwest::Client, LlmFailure> {
        reqwest::Client::builder()
            .tls_backend_preconfigured((*self.tls).clone())
            .https_only(!self.allow_loopback_http)
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(self.call_timeout)
            .build()
            .map_err(|_| LlmFailure::NotSent)
    }
}

/// Request body per protocol. The answer format is enforced with a JSON schema where the API
/// supports it; the system prompt asks for the same object everywhere.
pub fn request_body(request: &LlmRequest) -> Value {
    match request.protocol {
        LlmProtocol::Anthropic => json!({
            "model": request.model,
            "max_tokens": request.max_tokens,
            "system": request.system,
            "messages": [{ "role": "user", "content": request.user }],
            "output_config": {
                "effort": "low",
                "format": { "type": "json_schema", "schema": request.schema }
            }
        }),
        LlmProtocol::OpenAiCompatible => {
            let mut body = json!({
                "model": request.model,
                "max_tokens": request.max_tokens,
                "messages": [
                    { "role": "system", "content": request.system },
                    { "role": "user", "content": request.user }
                ]
            });
            if let Some(temperature) = request.temperature {
                body["temperature"] = json!(temperature);
            }
            body
        }
        LlmProtocol::Gemini => {
            let mut config = json!({
                "maxOutputTokens": request.max_tokens,
                "responseMimeType": "application/json",
                "responseJsonSchema": request.schema,
                "thinkingConfig": { "thinkingLevel": "low" }
            });
            if let Some(temperature) = request.temperature {
                config["temperature"] = json!(temperature);
            }
            json!({
                "systemInstruction": { "parts": [{ "text": request.system }] },
                "contents": [{ "role": "user", "parts": [{ "text": request.user }] }],
                "generationConfig": config
            })
        }
    }
}

/// HTTP status → failure class.
pub fn status_failure(status: u16, body: &str) -> LlmFailure {
    match status {
        401 | 403 => LlmFailure::Unauthorized,
        // Gemini answers a wrong or expired key with 400 INVALID_ARGUMENT.
        400 if body.contains("API_KEY_INVALID") || body.contains("API key not valid") => {
            LlmFailure::Unauthorized
        }
        402 => LlmFailure::Quota,
        429 if body.contains("insufficient_quota") || body.contains("billing") => LlmFailure::Quota,
        408 | 409 | 425 | 429 | 500..=599 => LlmFailure::Unavailable { status },
        _ => LlmFailure::Rejected { status },
    }
}

/// Extracts the answer text from a successful response.
pub fn parse_response(
    protocol: LlmProtocol,
    body: &[u8],
    requested_model: &str,
) -> Result<LlmResponse, LlmFailure> {
    let value: Value = serde_json::from_slice(body).map_err(|_| LlmFailure::Malformed)?;
    let reported = match protocol {
        LlmProtocol::Gemini => &value["modelVersion"],
        LlmProtocol::Anthropic | LlmProtocol::OpenAiCompatible => &value["model"],
    };
    let model = reported
        .as_str()
        .filter(|model| !model.is_empty() && model.len() <= 100)
        .unwrap_or(requested_model)
        .to_owned();
    let text = match protocol {
        LlmProtocol::Anthropic => {
            match value["stop_reason"].as_str() {
                Some("refusal") => return Err(LlmFailure::Refused),
                Some("max_tokens") => return Err(LlmFailure::Malformed),
                _ => {}
            }
            value["content"]
                .as_array()
                .ok_or(LlmFailure::Malformed)?
                .iter()
                .filter(|block| block["type"] == "text")
                .filter_map(|block| block["text"].as_str())
                .collect::<String>()
        }
        LlmProtocol::OpenAiCompatible => {
            let choice = &value["choices"][0];
            match choice["finish_reason"].as_str() {
                Some("content_filter") => return Err(LlmFailure::Refused),
                Some("length") => return Err(LlmFailure::Malformed),
                _ => {}
            }
            if choice["message"]["refusal"]
                .as_str()
                .is_some_and(|refusal| !refusal.is_empty())
            {
                return Err(LlmFailure::Refused);
            }
            choice["message"]["content"]
                .as_str()
                .ok_or(LlmFailure::Malformed)?
                .to_owned()
        }
        LlmProtocol::Gemini => {
            let candidate = &value["candidates"][0];
            if candidate.is_null() {
                // No candidate at all: the prompt itself was blocked.
                return Err(if value["promptFeedback"]["blockReason"].is_string() {
                    LlmFailure::Refused
                } else {
                    LlmFailure::Malformed
                });
            }
            match candidate["finishReason"].as_str() {
                Some("MAX_TOKENS") => return Err(LlmFailure::Malformed),
                Some(
                    "SAFETY" | "RECITATION" | "BLOCKLIST" | "PROHIBITED_CONTENT" | "SPII"
                    | "LANGUAGE",
                ) => return Err(LlmFailure::Refused),
                _ => {}
            }
            candidate["content"]["parts"]
                .as_array()
                .ok_or(LlmFailure::Malformed)?
                .iter()
                // Thought summaries are not part of the answer.
                .filter(|part| part["thought"] != true)
                .filter_map(|part| part["text"].as_str())
                .collect::<String>()
        }
    };
    if text.trim().is_empty() {
        return Err(LlmFailure::Malformed);
    }
    Ok(LlmResponse { text, model })
}

fn send_failure(error: &reqwest::Error) -> LlmFailure {
    if error.is_connect() || error.is_builder() {
        LlmFailure::NotSent
    } else {
        // Anything after the connection was made may have reached the provider.
        LlmFailure::Timeout
    }
}

async fn call(
    client: reqwest::Client,
    url: String,
    request: &LlmRequest,
    key: Option<&ApiKey>,
) -> Result<LlmResponse, LlmFailure> {
    let mut builder = client
        .post(url)
        .header("content-type", "application/json")
        .body(request_body(request).to_string());
    builder = match (request.protocol, key) {
        (LlmProtocol::Anthropic, Some(key)) => builder
            .header("x-api-key", &key.0)
            .header("anthropic-version", ANTHROPIC_VERSION),
        (LlmProtocol::Anthropic, None) => builder.header("anthropic-version", ANTHROPIC_VERSION),
        (LlmProtocol::OpenAiCompatible, Some(key)) => builder.bearer_auth(&key.0),
        (LlmProtocol::OpenAiCompatible, None) => builder,
        // The key goes in a header, never in the URL (`?key=` would end up in logs and proxies).
        (LlmProtocol::Gemini, Some(key)) => builder.header("x-goog-api-key", &key.0),
        (LlmProtocol::Gemini, None) => builder,
    };
    let mut response = builder.send().await.map_err(|error| send_failure(&error))?;
    let status = response.status().as_u16();
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| LlmFailure::Timeout)? {
        if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(LlmFailure::Malformed);
        }
        body.extend_from_slice(&chunk);
    }
    if !(200..300).contains(&status) {
        return Err(status_failure(status, &String::from_utf8_lossy(&body)));
    }
    parse_response(request.protocol, &body, &request.model)
}

impl LlmClient for HttpLlmClient {
    fn generate(
        &self,
        request: &LlmRequest,
        key: Option<&ApiKey>,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<LlmResponse, LlmFailure> {
        let Some(url) = self.url(request) else {
            // Refused by the endpoint policy: nothing leaves the device.
            return Err(LlmFailure::Rejected { status: 0 });
        };
        let client = self.client()?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| LlmFailure::NotSent)?;
        runtime.block_on(async {
            let work = call(client, url, request, key);
            tokio::pin!(work);
            loop {
                tokio::select! {
                    result = &mut work => return result,
                    () = tokio::time::sleep(CANCEL_POLL) => {
                        if cancelled() {
                            return Err(LlmFailure::Cancelled);
                        }
                    }
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(protocol: LlmProtocol, endpoint: Option<&str>) -> LlmRequest {
        LlmRequest {
            protocol,
            endpoint: endpoint.map(str::to_owned),
            model: "model-x".into(),
            prompt_version: "word-substitution-v1".into(),
            system: "rules".into(),
            user: "data".into(),
            schema: json!({"type": "object"}),
            max_tokens: 100,
            temperature: Some(0.7),
        }
    }

    #[test]
    fn anthropic_body_uses_a_json_schema_and_no_temperature() {
        let mut anthropic = request(LlmProtocol::Anthropic, None);
        anthropic.temperature = None;
        let body = request_body(&anthropic);
        assert_eq!(body["output_config"]["format"]["type"], "json_schema");
        assert_eq!(body["system"], "rules");
        assert!(body.get("temperature").is_none());
        let openai = request_body(&request(LlmProtocol::OpenAiCompatible, Some("https://x")));
        assert_eq!(openai["messages"][0]["role"], "system");
        assert!((openai["temperature"].as_f64().unwrap() - 0.7).abs() < 1e-6);
    }

    #[test]
    fn endpoint_policy_accepts_only_https() {
        let client = HttpLlmClient::new();
        let url = |endpoint| client.url(&request(LlmProtocol::OpenAiCompatible, Some(endpoint)));
        assert_eq!(
            url("https://llm.example.com/v1/").as_deref(),
            Some("https://llm.example.com/v1/chat/completions")
        );
        assert_eq!(
            url("https://llm.example.com/v1/chat/completions").as_deref(),
            Some("https://llm.example.com/v1/chat/completions")
        );
        assert_eq!(url("http://llm.example.com/v1"), None);
        assert_eq!(url("http://127.0.0.1:8080/v1"), None);
        assert_eq!(url("https://user:pw@llm.example.com"), None);
        assert_eq!(
            client
                .url(&request(LlmProtocol::Anthropic, None))
                .as_deref(),
            Some("https://api.anthropic.com/v1/messages")
        );
        assert_eq!(
            client.url(&request(LlmProtocol::OpenAiCompatible, None)),
            None
        );
    }

    #[test]
    fn statuses_map_to_failure_classes() {
        assert_eq!(status_failure(401, ""), LlmFailure::Unauthorized);
        assert_eq!(status_failure(402, ""), LlmFailure::Quota);
        assert_eq!(
            status_failure(429, r#"{"error":{"code":"insufficient_quota"}}"#),
            LlmFailure::Quota
        );
        assert_eq!(
            status_failure(429, "rate_limit_error"),
            LlmFailure::Unavailable { status: 429 }
        );
        assert_eq!(
            status_failure(529, "overloaded_error"),
            LlmFailure::Unavailable { status: 529 }
        );
        assert_eq!(
            status_failure(400, ""),
            LlmFailure::Rejected { status: 400 }
        );
    }

    #[test]
    fn responses_parse_per_protocol() {
        let anthropic = br#"{"model":"claude-haiku-5-5","stop_reason":"end_turn",
            "content":[{"type":"thinking","thinking":""},{"type":"text","text":"{\"association\":\"x\"}"}]}"#;
        let response = parse_response(LlmProtocol::Anthropic, anthropic, "m").unwrap();
        assert_eq!(response.text, r#"{"association":"x"}"#);
        assert_eq!(response.model, "claude-haiku-5-5");
        let refusal = br#"{"stop_reason":"refusal","content":[]}"#;
        assert_eq!(
            parse_response(LlmProtocol::Anthropic, refusal, "m"),
            Err(LlmFailure::Refused)
        );
        let cut = br#"{"stop_reason":"max_tokens","content":[{"type":"text","text":"{"}]}"#;
        assert_eq!(
            parse_response(LlmProtocol::Anthropic, cut, "m"),
            Err(LlmFailure::Malformed)
        );
        let openai = br#"{"choices":[{"finish_reason":"stop","message":{"content":"{}"}}]}"#;
        let response = parse_response(LlmProtocol::OpenAiCompatible, openai, "local").unwrap();
        assert_eq!(
            (response.text.as_str(), response.model.as_str()),
            ("{}", "local")
        );
        let filtered = br#"{"choices":[{"finish_reason":"content_filter","message":{}}]}"#;
        assert_eq!(
            parse_response(LlmProtocol::OpenAiCompatible, filtered, "m"),
            Err(LlmFailure::Refused)
        );
        assert_eq!(
            parse_response(LlmProtocol::OpenAiCompatible, b"not json", "m"),
            Err(LlmFailure::Malformed)
        );
    }

    #[test]
    fn gemini_body_asks_for_json_with_the_schema_and_low_thinking() {
        let mut gemini = request(LlmProtocol::Gemini, None);
        gemini.temperature = None;
        let body = request_body(&gemini);
        assert_eq!(body["systemInstruction"]["parts"][0]["text"], "rules");
        assert_eq!(body["contents"][0]["role"], "user");
        assert_eq!(body["contents"][0]["parts"][0]["text"], "data");
        let config = &body["generationConfig"];
        assert_eq!(config["responseMimeType"], "application/json");
        assert_eq!(config["responseJsonSchema"], json!({"type": "object"}));
        assert_eq!(config["maxOutputTokens"], 100);
        assert_eq!(config["thinkingConfig"]["thinkingLevel"], "low");
        assert!(config.get("temperature").is_none());
        // Nothing about the key or the model in the body: both travel outside it.
        assert!(!body.to_string().contains("model-x"));
    }

    #[test]
    fn gemini_url_carries_only_a_plain_model_id() {
        let client = HttpLlmClient::new();
        let url = |model: &str| {
            let mut gemini = request(LlmProtocol::Gemini, Some("https://ignored.example.com"));
            gemini.model = model.into();
            client.url(&gemini)
        };
        assert_eq!(
            url("gemini-3.8-flash").as_deref(),
            Some(
                "https://generativelanguage.googleapis.com/v1beta/models/gemini-3.8-flash:generateContent"
            )
        );
        for bad in [
            "",
            "../files",
            "a/b",
            "m?key=x",
            "m#x",
            "m:streamGenerateContent",
            "m n",
        ] {
            assert_eq!(url(bad), None, "{bad}");
        }
    }

    #[test]
    fn gemini_statuses_and_responses_map_to_failure_classes() {
        assert_eq!(
            status_failure(
                400,
                r#"{"error":{"status":"INVALID_ARGUMENT","details":[{"reason":"API_KEY_INVALID"}]}}"#
            ),
            LlmFailure::Unauthorized
        );
        assert_eq!(
            status_failure(400, r#"{"error":{"status":"INVALID_ARGUMENT"}}"#),
            LlmFailure::Rejected { status: 400 }
        );
        assert_eq!(
            status_failure(429, r#"{"error":{"status":"RESOURCE_EXHAUSTED"}}"#),
            LlmFailure::Unavailable { status: 429 }
        );

        let ok = br#"{"modelVersion":"gemini-3.8-flash","candidates":[{"finishReason":"STOP",
            "content":{"role":"model","parts":[{"text":"thinking...","thought":true},{"text":"{\"association\":\"x\"}"}]}}]}"#;
        let response = parse_response(LlmProtocol::Gemini, ok, "m").unwrap();
        assert_eq!(response.text, r#"{"association":"x"}"#);
        assert_eq!(response.model, "gemini-3.8-flash");

        let blocked_prompt = br#"{"promptFeedback":{"blockReason":"SAFETY"}}"#;
        assert_eq!(
            parse_response(LlmProtocol::Gemini, blocked_prompt, "m"),
            Err(LlmFailure::Refused)
        );
        let blocked_answer =
            br#"{"candidates":[{"finishReason":"SAFETY","content":{"parts":[]}}]}"#;
        assert_eq!(
            parse_response(LlmProtocol::Gemini, blocked_answer, "m"),
            Err(LlmFailure::Refused)
        );
        let cut =
            br#"{"candidates":[{"finishReason":"MAX_TOKENS","content":{"parts":[{"text":"{"}]}}]}"#;
        assert_eq!(
            parse_response(LlmProtocol::Gemini, cut, "m"),
            Err(LlmFailure::Malformed)
        );
        assert_eq!(
            parse_response(LlmProtocol::Gemini, br#"{}"#, "m"),
            Err(LlmFailure::Malformed)
        );
    }

    #[test]
    fn the_key_is_never_printed() {
        assert_eq!(
            format!("{:?}", ApiKey("sk-secret".into())),
            "ApiKey(<redacted>)"
        );
    }
}
