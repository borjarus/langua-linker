# ADR-18: AI associations — provider, prompts, outbox, cache and moderation

- Status: Proposed (2026-11-10); implemented in Phase 13. Acceptance after the Phase 13 device run (Android: real key, airplane mode → queued → reconnect → attached; latency on a mobile network).
- Plan references: Phase 13 steps 1–18 and DoD; D-02 (BYOK/proxy), D-03 (no payments, no unsecured free endpoint); ADR-07 (secret store); ADR-09 (SQLite persistence); ADR-16 (learning flow); ADR-17 (settings, consent)

## Context

Phase 5 built the AI states (pending, offline queued, error, blocked), the AI badge and the AI settings screen on the demo adapter; Phase 12 stored consent and provider settings. Phase 13 makes `generateAssociation` and `setAiApiKey` real without changing the screens; contract v1 (1.2) stays frozen. Studying must never wait for, or depend on, the provider.

## Decision

### Provider (D-02)

1. **BYOK only in v1 beta**, as the plan's default for D-02: the learner's own key for Anthropic or for an OpenAI-compatible HTTPS endpoint. No proxy is built; a proxy (per-install token, quota, rate limit, shared cache privacy, kill switch, cost monitoring) is a separate component and needs its own decision (D-02/D-03).
2. **The key** goes from the settings form straight to the platform secret store (`ai.apiKey`, ADR-07). No command returns it; `setAiApiKey` answers with `AiSettingsDto` (`hasApiKey` only) and bumps the settings version. The key never enters a log, backup (ADR-17) or `Debug` output (`ApiKey` is redacted).
3. **Anthropic**: Messages API, `anthropic-version: 2026-10-21`, `output_config.format` with a JSON schema and `effort: "low"`; no `temperature` (that API version rejects it). `stop_reason: "refusal"` counts as a moderation block, `max_tokens` as invalid output. **OpenAI-compatible**: `/chat/completions` at the configured endpoint, bearer token, `temperature` 0.7; `finish_reason: "content_filter"` or a refusal counts as a block.
4. **Transport policy** (Rust only, never the WebView): HTTPS only, no redirects, no system proxy, connect timeout 10 s, call timeout 45 s, answers above 64 KiB refused, per-call Tokio runtime, cancellable while waiting. TLS is rustls with the `ring` provider and the Mozilla root store (`webpki-roots`): nothing has to be initialised over JNI on Android and no CMake/NASM is needed; the cost is that user-installed CAs are not trusted.

### Prompts (step 1–3)

5. Three versioned methods chosen from the note type: keyword substitution for words (`word-substitution-v1`), a story chain for sentences up to 12 words (`sentence-story-chain-v1`), a key-term chain for longer texts (`text-key-term-chain-v1`). L1 (Polish) and L2 (deck target language) are parameters.
6. The rules live only in the system prompt. Card text goes into the user message as a JSON object labelled as data; the system prompt says instructions inside it are ignored. Only the note's front and back and the variant number are sent — no deck name, tags, history or profile.
7. The answer must be exactly `{"association": "..."}` (a Markdown fence is tolerated), at most 400 characters, without URLs or markup. An empty association is a decline.

### Outbox (steps 11–15)

8. One `llm_requests` row per requested variant (1–3), written before the job starts, idempotent by `(operationId, variant)`. Rows are the outbox: they survive restarts, show on the card as placeholders (`AssociationDto.id` = `llm-<row id>`, so they never collide with stored ids) and can be dismissed with the existing delete button (`updateNoteContent` / `deleteAssociation`).
9. **Attempts**: a row's counter goes up *before* each provider call, so a kill while sending counts (the provider may have charged). `MAX_ATTEMPTS` = 3 provider calls in total per row, shared by timeouts, 429/5xx/529, invalid output, moderation and refusals. Timeouts and overloads wait 1.5 s × attempt; parse and moderation retries are immediate. Rejected keys (401/403), provider quota (402, `insufficient_quota`) and other 4xx fail at once. There is no exactly-once promise.
10. **Offline**: a call whose connection never opened is not an attempt. The row returns to `queued` with a retry time of 30 s doubling to 15 min (giving up after 200 tries) and shows as offline queued. The outbox resumes on start, on return to the foreground (at most every 15 s) and from a 60 s check while the app runs. Nothing runs after the app was killed: there is no native background work, and the UI text says "when you are online and the app is open".
11. **Stale answers**: each row stores a hash of the note text it was built from. A queued row whose note changed is dismissed without a call; an answer that arrives after the note changed (or the row was dismissed or blocked) is cached but never attached.
12. **Consent**: withdrawing AI consent blocks queued and sending rows in the same transaction (ADR-17); the runner re-checks consent before every call.
13. **Cache** (`association_cache`): key = SHA-256 over cache version, prompt version, provider, model, pinned parameters, L1, L2, normalized front (NFC, collapsed whitespace, case kept), normalized meaning and a variant slot. A request takes the first slot the note has not used (its associations, deleted ones too, and its requests), so "generate" on the same note always asks for a new association, while the same text and meaning in another note — or after the note was re-created — is served from the cache with zero network calls. Homographs differ by meaning; a text, prompt or model change gives a new key. Only moderated answers are cached; entries do not expire.

### Moderation (step 8)

14. A short folded term list (PL/EN/DE/ES slurs, sexual and abusive words; everyday words such as "pedał" are excluded) checks the card text before sending — a hit is blocked without any call — and every answer. Blocked answers are retried within the attempt limit, never stored as visible associations and never cached. Provider refusals count the same way. This is a safety net, not a classifier; provider-side moderation depends on the provider and is not claimed as compliance.

### Cost and measurement (steps 3, 18)

15. A local limit of 100 provider calls per UTC day (`AiSettingsDto.quota`; `QUOTA_EXCEEDED { resetsAt }` in epoch ms when a request would exceed it). BYOK providers report no quota of their own.
16. `llm_calls` logs every call with start time, duration, provider, model and outcome class — never content. `llm::latency_p90` computes the p90 of successful calls for the 4 s target; the app logs each call's outcome and duration.

### What the screens get

17. No screen changed. Placeholders map to the existing statuses: `pending` (queued or sending, including a retry wait), `offlineQueued`, `error` (with the error DTO: timeout, provider error, invalid output, quota, `PERMISSION_DENIED { reason: aiConsent | apiKey | apiKeyRejected | aiProvider | aiModel | aiEndpoint }`) and `blocked`. Generated associations carry `provenance: generated` (the AI badge) and the model. The job's status is informative only; the card reads the truth through the note queries after `collection_changed (notes)`.

## Consequences

- Rating, undo and the queue never touch the outbox; a pending or failed generation leaves studying unchanged.
- A provider without idempotency keys can charge for a timed-out call; the attempt limit and the daily limit bound that.
- Reporting an association with a preview of the data that was sent (step 9) needs a new screen action and is not in contract 1.2; it is carried over (no automatic telemetry log of prompts or outputs exists that it could replace).
- The variant picker (three variants at once) exists in the backend (1–3) but the screens still ask for one.

## Alternatives considered

- **Platform verifier / native TLS**: trusts user CAs, but needs JNI initialisation on Android (rustls-platform-verifier) or OpenSSL; rejected for v1.
- **Blocking reqwest client**: simpler, but cannot be cancelled while waiting.
- **Cache key without a slot**: every "generate" on the same note would return the cached answer again.
- **Proxy in v1**: excluded by D-02/D-03 until budget and abuse protection are approved.
