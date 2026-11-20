//! llama.cpp engine of the on-device model (ADR-23, step 4), through `llama-cpp-2`.
//!
//! - One backend per process; the model stays loaded between generations (loading a 1 GB file
//!   takes seconds, a generation should not), and is replaced when another file is asked for.
//! - A fresh context per generation: no state leaks from one card into the next.
//! - CPU only, at most four threads (the big cores of a phone); the model file is memory-mapped.
//! - llama.cpp's own logging is switched off: it would print paths and timings, and nothing here
//!   may log prompts or answers.

use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use langualinker_application::ports::LlmFailure;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::sampling::LlamaSampler;

use super::LocalEngine;

const MAX_THREADS: usize = 4;
const TOP_P: f32 = 0.9;

fn backend() -> Option<&'static LlamaBackend> {
    static BACKEND: OnceLock<Option<LlamaBackend>> = OnceLock::new();
    BACKEND
        .get_or_init(|| {
            llama_cpp_2::send_logs_to_tracing(
                llama_cpp_2::LogOptions::default().with_logs_enabled(false),
            );
            LlamaBackend::init().ok()
        })
        .as_ref()
}

fn threads() -> i32 {
    std::thread::available_parallelism()
        .map_or(2, |count| count.get().min(MAX_THREADS))
        .try_into()
        .unwrap_or(2)
}

/// The llama.cpp engine; one generation at a time.
#[derive(Default)]
pub struct LlamaEngine {
    loaded: Mutex<Option<(PathBuf, LlamaModel)>>,
}

impl LlamaEngine {
    pub fn new() -> Self {
        Self::default()
    }
}

impl LocalEngine for LlamaEngine {
    fn generate(
        &self,
        model_path: &Path,
        context_tokens: u32,
        prompt: &str,
        max_tokens: u32,
        temperature: f32,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<String, LlmFailure> {
        // A broken backend or file does not get better by retrying.
        let backend = backend().ok_or(LlmFailure::Rejected { status: 0 })?;
        let mut loaded = self.loaded.lock().unwrap_or_else(|e| e.into_inner());
        if loaded.as_ref().is_none_or(|(path, _)| path != model_path) {
            *loaded = None;
            let model =
                LlamaModel::load_from_file(backend, model_path, &LlamaModelParams::default())
                    .map_err(|_| LlmFailure::Rejected { status: 0 })?;
            *loaded = Some((model_path.to_path_buf(), model));
        }
        let Some((_, model)) = loaded.as_ref() else {
            return Err(LlmFailure::Rejected { status: 0 });
        };
        if cancelled() {
            return Err(LlmFailure::Cancelled);
        }

        let vocab = model.vocab();
        // `parse_special`: the ChatML markers are single special tokens, not text.
        let tokens = vocab.tokenize(prompt.as_bytes(), true, true);
        let context = context_tokens.max(256) as usize;
        if tokens.is_empty() || tokens.len() + max_tokens as usize > context {
            return Err(LlmFailure::Malformed);
        }
        let params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(context_tokens.max(256)))
            .with_n_batch(context_tokens.max(256))
            .with_n_threads(threads())
            .with_n_threads_batch(threads());
        let mut ctx = model
            .new_context(backend, params)
            .map_err(|_| LlmFailure::Unavailable { status: 0 })?;

        let mut batch = LlamaBatch::new(context, 1);
        let last = tokens.len() - 1;
        for (position, token) in tokens.iter().enumerate() {
            batch
                .add(*token, position as i32, &[0], position == last)
                .map_err(|_| LlmFailure::Malformed)?;
        }
        ctx.decode(&mut batch)
            .map_err(|_| LlmFailure::Unavailable { status: 0 })?;

        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(7, |elapsed| elapsed.subsec_nanos());
        let mut sampler = LlamaSampler::chain_simple([
            LlamaSampler::top_p(TOP_P, 1),
            LlamaSampler::temp(temperature),
            LlamaSampler::dist(seed),
        ]);
        let mut position = tokens.len() as i32;
        let mut answer = Vec::new();
        for _ in 0..max_tokens {
            if cancelled() {
                return Err(LlmFailure::Cancelled);
            }
            let token = sampler.sample(&ctx, batch.n_tokens() - 1);
            sampler.accept(token);
            if vocab.is_eog(token) {
                return Ok(String::from_utf8_lossy(&answer).into_owned());
            }
            answer.extend(vocab.token_to_piece(token, false, None));
            batch.clear();
            batch
                .add(token, position, &[0], true)
                .map_err(|_| LlmFailure::Malformed)?;
            position += 1;
            ctx.decode(&mut batch)
                .map_err(|_| LlmFailure::Unavailable { status: 0 })?;
        }
        // Cut off at `max_tokens`: the JSON is unlikely to be complete, the parser decides.
        Ok(String::from_utf8_lossy(&answer).into_owned())
    }
}
