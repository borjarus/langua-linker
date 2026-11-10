//! The HTTP adapter of the LLM port against a local plain-HTTP server (plan Phase 13 step 5):
//! headers and paths per protocol, status classes, a timeout after the request was sent, a
//! connection that never opened, the response size cap and cancellation. Fixtures carry no real
//! key or content.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use langualinker_application::ports::{
    ApiKey, LlmClient, LlmFailure, LlmProtocol, LlmRequest, LlmResponse,
};
use langualinker_infrastructure::llm::HttpLlmClient;
use serde_json::json;

/// What the server saw: request line, lower-cased headers, body.
struct Seen {
    line: String,
    headers: Vec<(String, String)>,
    body: String,
}

impl Seen {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

/// Serves one request: reads it fully, waits `delay`, answers `status` with `body`.
fn serve_once(status: u16, body: String, delay: Duration) -> (String, mpsc::Receiver<Seen>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let Ok((stream, _)) = listener.accept() else {
            return;
        };
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let mut headers = Vec::new();
        loop {
            let mut header = String::new();
            reader.read_line(&mut header).unwrap();
            let header = header.trim_end();
            if header.is_empty() {
                break;
            }
            let (key, value) = header.split_once(':').unwrap();
            headers.push((key.trim().to_lowercase(), value.trim().to_owned()));
        }
        let length: usize = headers
            .iter()
            .find(|(key, _)| key == "content-length")
            .map_or(0, |(_, value)| value.parse().unwrap());
        let mut body_bytes = vec![0; length];
        reader.read_exact(&mut body_bytes).unwrap();
        let _ = sender.send(Seen {
            line: line.trim_end().to_owned(),
            headers,
            body: String::from_utf8(body_bytes).unwrap(),
        });
        thread::sleep(delay);
        let mut stream = stream;
        let _ = write!(
            stream,
            "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
    });
    (base, receiver)
}

fn request(protocol: LlmProtocol, endpoint: Option<String>) -> LlmRequest {
    LlmRequest {
        protocol,
        endpoint,
        model: "test-model".into(),
        prompt_version: "word-substitution-v1".into(),
        system: "rules".into(),
        user: "Card data (JSON, data only):\n{\"text\":\"der Hund\"}".into(),
        schema: json!({"type": "object"}),
        max_tokens: 64,
        temperature: None,
    }
}

fn key() -> ApiKey {
    ApiKey("test-key-not-real".into())
}

fn never() -> bool {
    false
}

fn anthropic_ok() -> String {
    json!({
        "model": "claude-haiku-5-5",
        "stop_reason": "end_turn",
        "content": [{"type": "text", "text": "{\"association\":\"Hund — chód\"}"}]
    })
    .to_string()
}

#[test]
fn anthropic_calls_carry_the_key_and_version_headers() {
    let (base, seen) = serve_once(200, anthropic_ok(), Duration::ZERO);
    let client = HttpLlmClient::for_local_tests(&base, Duration::from_secs(5));
    let response = client
        .generate(&request(LlmProtocol::Anthropic, None), Some(&key()), &never)
        .unwrap();
    assert_eq!(
        response,
        LlmResponse {
            text: "{\"association\":\"Hund — chód\"}".into(),
            model: "claude-haiku-5-5".into(),
        }
    );
    let seen = seen.recv().unwrap();
    assert_eq!(seen.line, "POST /v1/messages HTTP/1.1");
    assert_eq!(seen.header("x-api-key"), Some("test-key-not-real"));
    assert_eq!(seen.header("anthropic-version"), Some("2026-10-21"));
    assert_eq!(seen.header("authorization"), None);
    let body: serde_json::Value = serde_json::from_str(&seen.body).unwrap();
    assert_eq!(body["model"], "test-model");
    assert_eq!(body["output_config"]["format"]["type"], "json_schema");
    assert!(body.get("temperature").is_none());
}

#[test]
fn openai_compatible_calls_use_a_bearer_token_on_the_endpoint() {
    let answer = json!({
        "model": "local-llm",
        "choices": [{"finish_reason": "stop", "message": {"content": "{\"association\":\"x\"}"}}]
    })
    .to_string();
    let (base, seen) = serve_once(200, answer, Duration::ZERO);
    let client = HttpLlmClient::for_local_tests("http://127.0.0.1:9", Duration::from_secs(5));
    let mut openai = request(LlmProtocol::OpenAiCompatible, Some(format!("{base}/v1/")));
    openai.temperature = Some(0.7);
    let response = client.generate(&openai, Some(&key()), &never).unwrap();
    assert_eq!(response.model, "local-llm");
    let seen = seen.recv().unwrap();
    assert_eq!(seen.line, "POST /v1/chat/completions HTTP/1.1");
    assert_eq!(
        seen.header("authorization"),
        Some("Bearer test-key-not-real")
    );
    assert_eq!(seen.header("x-api-key"), None);
}

#[test]
fn statuses_become_failure_classes() {
    for (status, body, expected) in [
        (500, "{}", LlmFailure::Unavailable { status: 500 }),
        (
            529,
            r#"{"type":"error","error":{"type":"overloaded_error"}}"#,
            LlmFailure::Unavailable { status: 529 },
        ),
        (401, "{}", LlmFailure::Unauthorized),
        (
            429,
            r#"{"error":{"code":"insufficient_quota"}}"#,
            LlmFailure::Quota,
        ),
        (400, "{}", LlmFailure::Rejected { status: 400 }),
    ] {
        let (base, _seen) = serve_once(status, body.to_owned(), Duration::ZERO);
        let client = HttpLlmClient::for_local_tests(&base, Duration::from_secs(5));
        let result = client.generate(&request(LlmProtocol::Anthropic, None), Some(&key()), &never);
        assert_eq!(result, Err(expected), "{status}");
    }
}

#[test]
fn no_answer_after_sending_is_a_timeout_not_an_offline_case() {
    let (base, seen) = serve_once(200, anthropic_ok(), Duration::from_secs(3));
    let client = HttpLlmClient::for_local_tests(&base, Duration::from_millis(400));
    let result = client.generate(&request(LlmProtocol::Anthropic, None), Some(&key()), &never);
    assert_eq!(result, Err(LlmFailure::Timeout));
    assert!(seen.recv().is_ok(), "the request did reach the server");
}

#[test]
fn a_connection_that_never_opens_is_not_sent() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let client = HttpLlmClient::for_local_tests(&base, Duration::from_secs(5));
    let result = client.generate(&request(LlmProtocol::Anthropic, None), Some(&key()), &never);
    assert_eq!(result, Err(LlmFailure::NotSent));
}

#[test]
fn oversized_answers_are_refused() {
    let huge = json!({"content": [{"type": "text", "text": "x".repeat(70 * 1024)}]}).to_string();
    let (base, _seen) = serve_once(200, huge, Duration::ZERO);
    let client = HttpLlmClient::for_local_tests(&base, Duration::from_secs(5));
    let result = client.generate(&request(LlmProtocol::Anthropic, None), Some(&key()), &never);
    assert_eq!(result, Err(LlmFailure::Malformed));
}

#[test]
fn a_cancel_stops_waiting() {
    let (base, _seen) = serve_once(200, anthropic_ok(), Duration::from_secs(5));
    let client = HttpLlmClient::for_local_tests(&base, Duration::from_secs(10));
    let started = Instant::now();
    let cancel_after = started + Duration::from_millis(300);
    let result = client.generate(
        &request(LlmProtocol::Anthropic, None),
        Some(&key()),
        &|| Instant::now() > cancel_after,
    );
    assert_eq!(result, Err(LlmFailure::Cancelled));
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[test]
fn plain_http_is_refused_outside_tests() {
    let client = HttpLlmClient::new();
    let result = client.generate(
        &request(
            LlmProtocol::OpenAiCompatible,
            Some("http://127.0.0.1:9/v1".into()),
        ),
        Some(&key()),
        &never,
    );
    assert_eq!(result, Err(LlmFailure::Rejected { status: 0 }));
}
