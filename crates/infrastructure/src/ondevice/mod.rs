//! On-device association model (ADR-23): the downloaded GGUF files, their download, and the
//! [`LlmClient`] that sends [`LlmProtocol::OnDevice`] requests to the local engine and every
//! other request to the HTTP client.
//!
//! - Models live in `<data dir>/models/`; only catalog files (`llm::local`) are downloaded, and
//!   only a file whose size and SHA-256 match is renamed from `<file>.part` into place. A file
//!   in place is therefore a verified model.
//! - The engine is a port ([`LocalEngine`]); llama.cpp implements it with the `on-device`
//!   feature (mobile builds). Without the feature [`default_engine`] is `None`, the provider is not
//!   offered and `local_model_ready` is `false`.
//! - Prompts and answers are never logged, as with the HTTP providers.

pub mod download;
#[cfg(feature = "on-device")]
pub mod llama;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use langualinker_application::llm::local::{self, LocalModel};
use langualinker_application::ports::{
    ApiKey, LlmClient, LlmFailure, LlmProtocol, LlmRequest, LlmResponse,
};

use crate::llm::HttpLlmClient;
use download::{DownloadError, Progress};

/// Directory under the app data directory.
pub const MODELS_DIR: &str = "models";
/// Least time between two progress reports of a download.
const REPORT_EVERY: Duration = Duration::from_millis(750);

/// Where a catalog model stands on this device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalModelState {
    /// Not on the device; `done` bytes of an interrupted download are kept for a resume.
    NotDownloaded {
        done: u64,
    },
    Downloading {
        done: u64,
    },
    /// Downloaded and verified.
    Ready,
    /// The last download failed (`code`: see [`DownloadError::code`]).
    Failed {
        done: u64,
        code: &'static str,
    },
}

/// Runs a model file (llama.cpp on the device; a stand-in in tests).
pub trait LocalEngine: Send + Sync {
    /// Generates the assistant answer for a ChatML prompt, stopping at the end-of-turn token,
    /// after `max_tokens`, or when `cancelled` answers `true`.
    fn generate(
        &self,
        model: &Path,
        context_tokens: u32,
        prompt: &str,
        max_tokens: u32,
        temperature: f32,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<String, LlmFailure>;
}

/// The engine this build has: llama.cpp with the `on-device` feature, none without it.
pub fn default_engine() -> Option<Arc<dyn LocalEngine>> {
    #[cfg(feature = "on-device")]
    {
        Some(Arc::new(llama::LlamaEngine::new()))
    }
    #[cfg(not(feature = "on-device"))]
    {
        None
    }
}

/// The ChatML prompt of a request (Bielik's chat template), ending where the answer starts.
pub fn chatml_prompt(system: &str, user: &str) -> String {
    format!(
        "<|im_start|>system\n{system}<|im_end|>\n<|im_start|>user\n{user}<|im_end|>\n<|im_start|>assistant\n"
    )
}

/// The answer up to the end-of-turn marker, trimmed.
pub fn answer_text(generated: &str) -> &str {
    generated
        .split("<|im_end|>")
        .next()
        .unwrap_or_default()
        .trim()
}

struct Running {
    progress: Arc<Progress>,
}

/// Downloaded models and running downloads.
pub struct ModelStore {
    dir: PathBuf,
    catalog: Vec<LocalModel>,
    running: Mutex<HashMap<String, Running>>,
    failed: Mutex<HashMap<String, &'static str>>,
    allow_loopback_http: bool,
}

/// A store answer that is not a state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreError {
    UnknownModel,
}

impl ModelStore {
    /// The app's store over the compiled-in catalog.
    pub fn new(data_dir: &Path) -> Self {
        Self::with_catalog(data_dir.join(MODELS_DIR), local::catalog().to_vec(), false)
    }

    /// A store over another catalog; tests download from 127.0.0.1 over plain HTTP.
    pub fn with_catalog(dir: PathBuf, catalog: Vec<LocalModel>, allow_loopback_http: bool) -> Self {
        Self {
            dir,
            catalog,
            running: Mutex::default(),
            failed: Mutex::default(),
            allow_loopback_http,
        }
    }

    pub fn catalog(&self) -> &[LocalModel] {
        &self.catalog
    }

    fn model(&self, id: &str) -> Result<&LocalModel, StoreError> {
        self.catalog
            .iter()
            .find(|model| model.id == id)
            .ok_or(StoreError::UnknownModel)
    }

    fn target(&self, model: &LocalModel) -> PathBuf {
        self.dir.join(&model.file)
    }

    fn part(&self, model: &LocalModel) -> PathBuf {
        self.dir.join(format!("{}.part", model.file))
    }

    pub fn state(&self, id: &str) -> Result<LocalModelState, StoreError> {
        let model = self.model(id)?;
        if let Some(running) = self
            .running
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(id)
        {
            return Ok(LocalModelState::Downloading {
                done: running.progress.done.load(Ordering::Relaxed),
            });
        }
        if std::fs::metadata(self.target(model)).is_ok_and(|meta| meta.len() == model.bytes) {
            return Ok(LocalModelState::Ready);
        }
        let done = std::fs::metadata(self.part(model)).map_or(0, |meta| meta.len());
        Ok(
            match self
                .failed
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(id)
            {
                Some(code) => LocalModelState::Failed { done, code },
                None => LocalModelState::NotDownloaded { done },
            },
        )
    }

    /// The verified file of a model, if it is on the device.
    pub fn ready_path(&self, id: &str) -> Option<(PathBuf, u32)> {
        let model = self.model(id).ok()?;
        (self.state(id).ok()? == LocalModelState::Ready)
            .then(|| (self.target(model), model.context_tokens))
    }

    /// Starts (or resumes) the download on its own thread; `report` runs on progress (at most
    /// every 750 ms) and once at the end. Starting a running or finished download does nothing.
    pub fn start(
        self: &Arc<Self>,
        id: &str,
        report: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<(), StoreError> {
        let model = self.model(id)?.clone();
        if self.state(id)? == LocalModelState::Ready {
            return Ok(());
        }
        let progress = {
            let mut running = self.running.lock().unwrap_or_else(|e| e.into_inner());
            if running.contains_key(id) {
                return Ok(());
            }
            let progress = Progress::new();
            running.insert(
                id.to_owned(),
                Running {
                    progress: progress.clone(),
                },
            );
            progress
        };
        self.failed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id);
        let store = self.clone();
        std::thread::spawn(move || {
            let last = Mutex::new(Instant::now());
            let throttled = {
                let report = report.clone();
                move || {
                    let mut last = last.lock().unwrap_or_else(|e| e.into_inner());
                    if last.elapsed() >= REPORT_EVERY {
                        *last = Instant::now();
                        report();
                    }
                }
            };
            let result = download::fetch(&download::Request {
                url: &model.url,
                part: &store.part(&model),
                target: &store.target(&model),
                bytes: model.bytes,
                sha256: &model.sha256,
                done: &progress.done,
                cancel: &progress.cancel,
                progress: &throttled,
                allow_loopback_http: store.allow_loopback_http,
            });
            match result {
                Ok(()) => {
                    tracing::info!(target: "ondevice", model = %model.id, "model downloaded and verified")
                }
                Err(DownloadError::Cancelled) => {}
                Err(error) => {
                    tracing::warn!(target: "ondevice", model = %model.id, code = error.code(), "model download failed");
                    store
                        .failed
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .insert(model.id.clone(), error.code());
                }
            }
            store
                .running
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&model.id);
            report();
        });
        Ok(())
    }

    /// Stops a running download and removes the model and any partial file.
    pub fn delete(&self, id: &str) -> Result<(), StoreError> {
        let model = self.model(id)?;
        if let Some(running) = self
            .running
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(id)
        {
            running.progress.cancel.store(true, Ordering::Relaxed);
        }
        self.failed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id);
        // The download thread may still hold the part for a moment; a later delete or the next
        // download cleans up whatever it leaves.
        let _ = std::fs::remove_file(self.target(model));
        let _ = std::fs::remove_file(self.part(model));
        Ok(())
    }
}

/// HTTP providers plus the on-device model.
pub struct RoutingLlmClient {
    http: HttpLlmClient,
    store: Arc<ModelStore>,
    engine: Option<Arc<dyn LocalEngine>>,
}

impl RoutingLlmClient {
    pub fn new(
        http: HttpLlmClient,
        store: Arc<ModelStore>,
        engine: Option<Arc<dyn LocalEngine>>,
    ) -> Self {
        Self {
            http,
            store,
            engine,
        }
    }

    /// The on-device provider is offered only with an engine and a catalog model.
    pub fn offers_on_device(&self) -> bool {
        self.engine.is_some() && !self.store.catalog().is_empty()
    }
}

impl LlmClient for RoutingLlmClient {
    fn generate(
        &self,
        request: &LlmRequest,
        key: Option<&ApiKey>,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<LlmResponse, LlmFailure> {
        if request.protocol != LlmProtocol::OnDevice {
            return self.http.generate(request, key, cancelled);
        }
        // A missing engine or file is permanent until the learner acts: fail at once.
        let engine = self
            .engine
            .as_ref()
            .ok_or(LlmFailure::Rejected { status: 0 })?;
        let (path, context) = self
            .store
            .ready_path(&request.model)
            .ok_or(LlmFailure::Rejected { status: 0 })?;
        let generated = engine.generate(
            &path,
            context,
            &chatml_prompt(&request.system, &request.user),
            request.max_tokens,
            request.temperature.unwrap_or(0.7),
            cancelled,
        )?;
        Ok(LlmResponse {
            text: answer_text(&generated).to_owned(),
            model: request.model.clone(),
        })
    }

    fn local_model_ready(&self, model: &str) -> bool {
        self.engine.is_some() && self.store.ready_path(model).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prompt_is_chatml_and_the_answer_ends_at_the_turn() {
        let prompt = chatml_prompt("rules", "data");
        assert!(prompt.starts_with("<|im_start|>system\nrules<|im_end|>\n"));
        assert!(prompt.ends_with("<|im_start|>assistant\n"));
        assert_eq!(
            answer_text(" {\"association\": \"x\"}<|im_end|>\nmore"),
            "{\"association\": \"x\"}"
        );
    }
}
