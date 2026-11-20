# ADR-23: On-device association model — Bielik 1.5B, LoRA, GGUF from GitHub Releases

- Status: Accepted (2026-11-20). Steps 1–5 complete. Verified on physical Samsung Galaxy A56 (`SM-A566B`, Android 17).
- References: ADR-18 (providers, prompts, outbox, cache, moderation), ADR-14 (encrypted bundled content), ADR-17 (settings, consent), D-02 (BYOK)

## Context

AI associations need a provider key today (ADR-18). An optional model that runs on the phone removes the key, the cost and the network round trip, and keeps card text on the device. It must stay optional: the APK must not grow, and nothing may be downloaded unless the learner picks it.

## Decision

### Model

1. **Base: `speakleash/Bielik-1.5B-v3.0-Instruct`** (Apache 2.0, 1.6 B parameters, ChatML). The associations are Polish sound-alikes and images; Bielik is trained mostly on Polish, which the multilingual small models handle worse. Its German/English/Spanish is weaker, but the model only has to *read* the L2 word and write Polish.
2. **Fine-tuning: LoRA** (rank 16, all linear layers, 2 epochs, loss on the answer only), merged into the base and converted with llama.cpp to **GGUF `Q4_K_M`** (about 1 GB). Notebook: `content/tools/finetune/bielik_associations.ipynb` (Colab, T4 GPU).
3. **Training data = the app's own frame.** `llm-dataset` (`crates/application/src/bin/llm-dataset.rs`, logic in `llm::dataset`) writes, for every stored association of the bundled decks, the exact system prompt and user message the app sends for that note (`prompt::system_prompt`, `prompt::user_prompt`, method chosen by `PromptKind::for_note`) with `{"association": "..."}` as the answer. The model therefore answers in the format `prompt::parse_answer` accepts, and its output goes through the same moderation and cache as any provider. Associations that are empty, longer than 400 characters or caught by the moderation list are left out. 5 % of the notes (by a hash of the GUID) are held out for evaluation. First export: 4 753 training and 271 evaluation examples (words: 1 056 / 67, sentences: 3 697 / 204).

### Distribution

4. **GitHub Releases** of this repository, tag `model-<name>-v<N>`, with the GGUF file and the notebook's `model-manifest.json`.
5. **Catalog compiled into the app**: `content/models/catalog.json` (`llm::local`). An entry is the manifest: `id`, `name`, `file`, `url`, `sha256`, `bytes`, `contextTokens`. Entries are validated when the app starts: HTTPS URL on `github.com`, `objects.githubusercontent.com` or `release-assets.githubusercontent.com`; a plain `.gguf` file name; a 64-digit hex digest. An invalid entry is dropped, so a bad edit cannot make the app download from elsewhere. An empty catalog hides the provider.
6. **Download only on request** (`downloadAiModel`), into `<data dir>/models/<file>.part`, resumed with `Range: bytes=N-` after an interruption. Redirects are followed only between the hosts above (at most 5). Connect timeout 15 s, read timeout 60 s, no overall limit. The file is renamed into place only after its size and SHA-256 match the catalog; a mismatch deletes it. A file in place is therefore a verified model. Progress reaches the screen as `collection_changed { settings }` at most every 750 ms. There is no new job kind, which would be a major contract change. `deleteAiModel` cancels the download and removes the files.
7. The settings screen shows the size, recommends Wi‑Fi and never starts a download by itself.

### In the app

8. **Provider `onDevice`** (`settings::AI_PROVIDERS`, `on_device: true`, no key, models from the catalog) and **protocol `LlmProtocol::OnDevice`**. It reuses the whole ADR-18 pipeline: outbox rows, attempt limit, moderation of input and answer, cache key (provider `onDevice`, model id, `Params` 256 tokens / temperature 0.7), daily limit, stale-note handling. A local call never counts as offline (`NotSent` does not happen).
9. **`RoutingLlmClient`** (`infrastructure::ondevice`) sends `OnDevice` requests to the engine and everything else to `HttpLlmClient`. The prompt is ChatML (`<|im_start|>system … <|im_start|>assistant\n`); the answer ends at `<|im_end|>`. A missing file or engine fails at once (`Rejected { status: 0 }`). `generateAssociation` answers `PERMISSION_DENIED { reason: "localModel" }` until the model is ready.
10. **Engine port `LocalEngine`**, implemented with llama.cpp (`ondevice::llama`, crate `llama-cpp-2` 0.1.161) behind the infrastructure feature `on-device`. The shell enables it **on Android only**; desktop and iOS stay without until they are measured, and there `default_engine()` is `None` and the provider is not listed. One backend per process, logging of llama.cpp off; the model stays loaded between generations and a fresh context is made per card; CPU only, at most four threads, memory-mapped file; the prompt is tokenized with special tokens parsed (ChatML markers), and generation stops at end-of-generation, after `max_tokens` or on cancellation (checked per token). A file that does not load fails for good (`Rejected`), a decode failure is retried (`Unavailable`), a prompt that leaves no room for the answer is `Malformed`.

### Contract 1.10 (minor)

11. `AiProviderOptionDto.onDevice`, `AiSettingsDto.localModels: LocalModelDto[]` (`id`, `name`, `bytes`, `state`: `notDownloaded` | `downloading` | `ready` | `failed`, `downloadedBytes`, `error`), mutations `downloadAiModel` and `deleteAiModel` (`AiModelInput { operationId, modelId }` → `AiSettingsDto`). Older payloads deserialize with `false` and `[]`.

### Publication — decided 2026-11-20: (a)

12. The repository is **public**, so a release asset is public too. A model fine-tuned on the bundled associations can reproduce many of them word for word. That partly undoes ADR-14, which encrypts the decks precisely to keep their text out of the public repository. The content rights are also still pending (D-04, `rightsConfirmed: false`). Options:
    - (a) Publish anyway: the associations are the project's own (generated for it), and the decks ship inside every APK anyway.
    - (b) Train on associations generated for words **outside** the bundled decks. The same exporter frame works with any NDJSON in format v2; the cost is a generation run with the API.
    - (c) Host the model somewhere the app can reach without a token but which is not listed publicly. This is weaker, and it is not GitHub Releases.

    **Decision (owner, 2026-11-20): (a)** — the model is published on GitHub Releases of this repository, trained on the bundled associations.

## Steps

1. ✅ Dataset exporter with tests (`llm::dataset`, `llm-dataset`).
2. ✅ Fine-tuning pipeline (`train_local.py`): LoRA on Metal/MPS (bfloat16), evaluation (JSON validity 100%, length 100%, no URLs 100%, names card word 72%), merge, GGUF `Q4_K_M`, llama.cpp smoke test, manifest, release command.
3. ✅ App: catalog, store with resumable verified download, routing client, provider, contract 1.10, settings UI, tests against a local HTTP server and a stand-in engine.
4. ✅ **Engine** (decision 10). Builds need CMake and libclang; Android builds also the NDK, pinned in CI to the NDK Tauri uses (`ANDROID_NDK`, `ANDROID_API_LEVEL=26`; `mobile.yml`, `release.yml`), with libc++ linked statically. Verified on Linux x86-64 and physical Android arm64-v8a.
5. ✅ **First release and physical device validation (Samsung Galaxy A56, Exynos 1580, Android 17):**
   - Release: `https://github.com/borjarus/langua-linker/releases/tag/model-bielik-associations-v1` (`bielik-1.5b-associations-v1.Q4_K_M.gguf`, 927.7 MB, SHA-256 `a74abb91fbf7ba16aa316a1daefa3236ae2c51086ebb524a5bbf6cddf65aa0e9`).
   - Catalog entry: `content/models/catalog.json`.
   - **Load time**: ~1.75 s into memory (mmap).
   - **Latency**: average ~21–23 s per word association (~1.8–2.0 tokens/s on 4 CPU threads with NEON/dotprod).
   - **Memory**: peak PSS ~1,246 MB, peak RSS ~1,322 MB, resting PSS ~1,180 MB (budget ~1.5 GB met with 250 MB headroom).
   - **Battery (20 generations)**: 3% battery drop, 155 mAh total (~7.75 mAh per generation), device temperature rose from 30.7°C to 34.1°C.
   - **Quality**: 20/20 valid JSON, concise Polish mnemonics, passed input/output moderation and SQLite storage. Status: **Accepted**.

## Consequences

- No change for learners until steps 4–5: the provider is hidden without an engine and a catalog entry.
- The settings screen, the outbox and the cache need no special cases beyond the provider flag and the readiness check.
- A model update is a new catalog entry (new id): the cache keys change with the model id, and the old file stays until the learner removes it.
- Answer quality of a 1.6 B model is below Claude or Gemini. The evaluation and a review of samples decide whether it ships at all; the API providers remain.

## Alternatives considered

- **Multilingual small models (Qwen2.5 1.5B, Gemma 3 1B)**: stronger in L2, weaker in Polish wordplay. Bielik was chosen by the owner (2026-11-20).
- **Model inside the APK**: +1 GB for every user, including those who never use AI. Rejected.
- **Hugging Face hosting**: the base model is gated there; GitHub Releases keeps distribution with the code.
- **MLC / ONNX Runtime / MediaPipe LLM**: GGUF + llama.cpp has the widest Android/iOS support and the conversion path from the notebook; the engine stays behind a port, so this can change.
