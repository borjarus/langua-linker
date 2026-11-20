//! On-device model store and client (ADR-23): download with resume and verification against
//! a local HTTP server, and routing of `OnDevice` requests to a stand-in engine.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use langualinker_application::llm::local::LocalModel;
use langualinker_application::ports::{LlmClient, LlmFailure, LlmProtocol, LlmRequest};
use langualinker_infrastructure::llm::HttpLlmClient;
use langualinker_infrastructure::ondevice::{
    LocalEngine, LocalModelState, ModelStore, RoutingLlmClient,
};
use sha2::{Digest, Sha256};

/// Serves `body` at any path; honours `Range: bytes=N-`; can cut the body short or stall.
struct Server {
    url: String,
    requests: Arc<Mutex<Vec<String>>>,
}

#[derive(Clone, Copy)]
enum Behaviour {
    Whole,
    /// Sends only this many bytes of the first response, then closes.
    CutFirstAfter(usize),
    /// Waits this long between chunks.
    Slow(Duration),
}

fn serve(body: Vec<u8>, behaviour: Behaviour) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/model.gguf", listener.local_addr().unwrap());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let seen = requests.clone();
    let served = Arc::new(AtomicUsize::new(0));
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut range_start = 0usize;
            let mut head = String::new();
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("range: bytes=") {
                    range_start = value.trim().trim_end_matches('-').parse().unwrap_or(0);
                }
                head.push_str(&line);
            }
            seen.lock().unwrap().push(head);
            let index = served.fetch_add(1, Ordering::SeqCst);
            let rest = &body[range_start.min(body.len())..];
            let status = if range_start > 0 {
                "206 Partial Content"
            } else {
                "200 OK"
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                rest.len()
            );
            let send = match behaviour {
                Behaviour::CutFirstAfter(bytes) if index == 0 => &rest[..bytes.min(rest.len())],
                _ => rest,
            };
            for chunk in send.chunks(4_096) {
                if let Behaviour::Slow(pause) = behaviour {
                    std::thread::sleep(pause);
                }
                if stream.write_all(chunk).is_err() {
                    break;
                }
            }
            let _ = stream.flush();
        }
    });
    Server { url, requests }
}

fn sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn model(url: &str, body: &[u8]) -> LocalModel {
    LocalModel {
        id: "test-model".into(),
        name: "Test".into(),
        file: "test-model.Q4_K_M.gguf".into(),
        url: url.into(),
        sha256: sha(body),
        bytes: body.len() as u64,
        context_tokens: 2_048,
        prompt_versions: Vec::new(),
    }
}

fn body() -> Vec<u8> {
    (0..300_000u32).map(|n| (n % 251) as u8).collect()
}

fn store(dir: &Path, model: LocalModel) -> Arc<ModelStore> {
    Arc::new(ModelStore::with_catalog(
        dir.join("models"),
        vec![model],
        true,
    ))
}

fn wait_until(store: &ModelStore, done: impl Fn(LocalModelState) -> bool) -> LocalModelState {
    let start = Instant::now();
    loop {
        let state = store.state("test-model").unwrap();
        if done(state) {
            return state;
        }
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "stuck in {state:?}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn idle(state: LocalModelState) -> bool {
    !matches!(state, LocalModelState::Downloading { .. })
}

#[test]
fn a_model_downloads_verifies_and_reports() {
    let dir = tempfile::tempdir().unwrap();
    let body = body();
    let server = serve(body.clone(), Behaviour::Whole);
    let store = store(dir.path(), model(&server.url, &body));
    assert_eq!(
        store.state("test-model").unwrap(),
        LocalModelState::NotDownloaded { done: 0 }
    );
    let reports = Arc::new(AtomicUsize::new(0));
    let counter = reports.clone();
    store
        .start(
            "test-model",
            Arc::new(move || {
                counter.fetch_add(1, Ordering::SeqCst);
            }),
        )
        .unwrap();
    assert_eq!(wait_until(&store, idle), LocalModelState::Ready);
    assert!(reports.load(Ordering::SeqCst) >= 1, "the end is reported");
    let (path, context) = store.ready_path("test-model").unwrap();
    assert_eq!(std::fs::read(path).unwrap(), body);
    assert_eq!(context, 2_048);

    store.delete("test-model").unwrap();
    assert_eq!(
        store.state("test-model").unwrap(),
        LocalModelState::NotDownloaded { done: 0 }
    );
}

#[test]
fn a_broken_transfer_resumes_with_a_range_request() {
    let dir = tempfile::tempdir().unwrap();
    let body = body();
    let server = serve(body.clone(), Behaviour::CutFirstAfter(100_000));
    let store = store(dir.path(), model(&server.url, &body));
    store.start("test-model", Arc::new(|| {})).unwrap();
    let state = wait_until(&store, idle);
    assert_eq!(
        state,
        LocalModelState::Failed {
            done: 100_000,
            code: "network"
        }
    );

    store.start("test-model", Arc::new(|| {})).unwrap();
    assert_eq!(wait_until(&store, idle), LocalModelState::Ready);
    let requests = server.requests.lock().unwrap();
    assert!(
        requests[1]
            .to_ascii_lowercase()
            .contains("range: bytes=100000-"),
        "{requests:?}"
    );
}

#[test]
fn a_file_that_is_not_the_catalog_file_is_deleted() {
    let dir = tempfile::tempdir().unwrap();
    let body = body();
    let server = serve(body.clone(), Behaviour::Whole);
    let mut entry = model(&server.url, &body);
    entry.sha256 = sha(b"another file");
    let store = store(dir.path(), entry);
    store.start("test-model", Arc::new(|| {})).unwrap();
    assert_eq!(
        wait_until(&store, idle),
        LocalModelState::Failed {
            done: 0,
            code: "checksum"
        }
    );
    assert!(store.ready_path("test-model").is_none());
    assert_eq!(
        std::fs::read_dir(dir.path().join("models"))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn urls_outside_the_release_hosts_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let body = body();
    // A store without the test exception refuses plain HTTP to 127.0.0.1 before connecting.
    let server = serve(body.clone(), Behaviour::Whole);
    let store = Arc::new(ModelStore::with_catalog(
        dir.path().join("models"),
        vec![model(&server.url, &body)],
        false,
    ));
    store.start("test-model", Arc::new(|| {})).unwrap();
    assert_eq!(
        wait_until(&store, idle),
        LocalModelState::Failed {
            done: 0,
            code: "refused"
        }
    );
    assert!(server.requests.lock().unwrap().is_empty());
}

#[test]
fn deleting_cancels_a_running_download() {
    let dir = tempfile::tempdir().unwrap();
    let body = body();
    let server = serve(body.clone(), Behaviour::Slow(Duration::from_millis(30)));
    let store = store(dir.path(), model(&server.url, &body));
    store.start("test-model", Arc::new(|| {})).unwrap();
    wait_until(
        &store,
        |state| matches!(state, LocalModelState::Downloading { done } if done > 0),
    );
    store.delete("test-model").unwrap();
    let state = wait_until(&store, idle);
    assert!(
        matches!(state, LocalModelState::NotDownloaded { .. }),
        "{state:?}"
    );
    assert!(store.ready_path("test-model").is_none());
}

#[test]
fn unknown_models_are_not_in_the_store() {
    let dir = tempfile::tempdir().unwrap();
    let store = store(dir.path(), model("https://github.com/x/y.gguf", b"x"));
    assert!(store.state("other").is_err());
    assert!(store.start("other", Arc::new(|| {})).is_err());
}

struct Echo;

impl LocalEngine for Echo {
    fn generate(
        &self,
        _model: &Path,
        _context_tokens: u32,
        prompt: &str,
        _max_tokens: u32,
        _temperature: f32,
        _cancelled: &dyn Fn() -> bool,
    ) -> Result<String, LlmFailure> {
        assert!(prompt.ends_with("<|im_start|>assistant\n"));
        Ok("{\"association\": \"Pryty — ładne pryty.\"}<|im_end|>".into())
    }
}

fn request(protocol: LlmProtocol, model: &str) -> LlmRequest {
    LlmRequest {
        protocol,
        endpoint: None,
        model: model.into(),
        prompt_version: "word-substitution-v1".into(),
        system: "rules".into(),
        user: "data".into(),
        schema: serde_json::json!({}),
        max_tokens: 256,
        temperature: Some(0.7),
    }
}

#[test]
fn on_device_requests_run_locally_once_the_model_is_ready() {
    let dir = tempfile::tempdir().unwrap();
    let body = body();
    let server = serve(body.clone(), Behaviour::Whole);
    let store = store(dir.path(), model(&server.url, &body));
    let client = RoutingLlmClient::new(HttpLlmClient::new(), store.clone(), Some(Arc::new(Echo)));
    assert!(client.offers_on_device());
    assert!(!client.local_model_ready("test-model"));
    assert_eq!(
        client.generate(&request(LlmProtocol::OnDevice, "test-model"), None, &|| {
            false
        }),
        Err(LlmFailure::Rejected { status: 0 })
    );

    store.start("test-model", Arc::new(|| {})).unwrap();
    assert_eq!(wait_until(&store, idle), LocalModelState::Ready);
    assert!(client.local_model_ready("test-model"));
    let answer = client
        .generate(&request(LlmProtocol::OnDevice, "test-model"), None, &|| {
            false
        })
        .unwrap();
    assert_eq!(answer.text, "{\"association\": \"Pryty — ładne pryty.\"}");
    assert_eq!(answer.model, "test-model");
    // Nothing went over the network for it.
    assert_eq!(server.requests.lock().unwrap().len(), 1);

    let without_engine = RoutingLlmClient::new(HttpLlmClient::new(), store, None);
    assert!(!without_engine.offers_on_device());
    assert!(!without_engine.local_model_ready("test-model"));
}

#[test]
fn a_part_file_survives_for_the_next_start() {
    let dir = tempfile::tempdir().unwrap();
    let body = body();
    let models = dir.path().join("models");
    std::fs::create_dir_all(&models).unwrap();
    std::fs::write(models.join("test-model.Q4_K_M.gguf.part"), &body[..50_000]).unwrap();
    let mut file = std::fs::File::open(models.join("test-model.Q4_K_M.gguf.part")).unwrap();
    let mut kept = Vec::new();
    file.read_to_end(&mut kept).unwrap();
    let store = store(dir.path(), model("https://github.com/x/y.gguf", &body));
    assert_eq!(
        store.state("test-model").unwrap(),
        LocalModelState::NotDownloaded { done: 50_000 }
    );
}

/// The llama.cpp engine on a real GGUF file: `LL_TEST_GGUF=/path/model.gguf cargo test -p
/// langualinker-infrastructure --features on-device --test ondevice -- --ignored`
/// (any small model works, e.g. `ggml-org/models` `tinyllamas/stories260K.gguf`).
#[cfg(feature = "on-device")]
mod llama_engine {
    use super::*;
    use langualinker_infrastructure::ondevice::llama::LlamaEngine;

    fn model_file() -> std::path::PathBuf {
        std::env::var_os("LL_TEST_GGUF")
            .map(Into::into)
            .expect("LL_TEST_GGUF points to a GGUF model")
    }

    #[test]
    #[ignore = "needs a GGUF model in LL_TEST_GGUF"]
    fn generates_stops_and_cancels() {
        let engine = LlamaEngine::new();
        let path = model_file();
        let prompt =
            langualinker_infrastructure::ondevice::chatml_prompt("rules", "Once upon a time");
        let text = engine
            .generate(&path, 512, &prompt, 24, 0.7, &|| false)
            .unwrap();
        assert!(!text.is_empty());
        // The model stays loaded: a second generation needs no reload and still works.
        let again = engine
            .generate(&path, 512, &prompt, 8, 0.7, &|| false)
            .unwrap();
        assert!(!again.is_empty());
        assert_eq!(
            engine.generate(&path, 512, &prompt, 24, 0.7, &|| true),
            Err(LlmFailure::Cancelled)
        );
        // A prompt that leaves no room for the answer is refused before decoding.
        assert_eq!(
            engine.generate(&path, 256, &prompt.repeat(40), 24, 0.7, &|| false),
            Err(LlmFailure::Malformed)
        );
        // A file that is not a model fails for good, without a retry.
        let dir = tempfile::tempdir().unwrap();
        let junk = dir.path().join("junk.gguf");
        std::fs::write(&junk, b"not a model").unwrap();
        assert_eq!(
            engine.generate(&junk, 512, &prompt, 8, 0.7, &|| false),
            Err(LlmFailure::Rejected { status: 0 })
        );
    }

    #[test]
    #[ignore = "needs a GGUF model in LL_TEST_GGUF"]
    fn the_routing_client_runs_a_downloaded_model() {
        let dir = tempfile::tempdir().unwrap();
        let body = std::fs::read(model_file()).unwrap();
        let server = serve(body.clone(), Behaviour::Whole);
        let store = store(dir.path(), model(&server.url, &body));
        let client = RoutingLlmClient::new(
            HttpLlmClient::new(),
            store.clone(),
            langualinker_infrastructure::ondevice::default_engine(),
        );
        store.start("test-model", Arc::new(|| {})).unwrap();
        assert_eq!(wait_until(&store, idle), LocalModelState::Ready);
        let mut request = request(LlmProtocol::OnDevice, "test-model");
        request.max_tokens = 16;
        let started = Instant::now();
        let answer = client.generate(&request, None, &|| false).unwrap();
        assert_eq!(answer.model, "test-model");
        eprintln!(
            "generated {} chars in {:?}",
            answer.text.len(),
            started.elapsed()
        );
    }
}
