-- LanguaLinker schema v4: LLM outbox, call log and association cache (plan Phase 13, ADR-18).
--
-- One `llm_requests` row is one association variant the learner asked for. The row is the
-- outbox entry: it survives restarts, carries its attempts and next retry time, and shows on the
-- card as a placeholder (pending, offline queued, error, blocked) until it is done or dismissed.
-- Nothing wrote to the v1 table (it only had a consent-withdrawal UPDATE), so it is rebuilt
-- instead of altered: SQLite cannot change a CHECK constraint in place.

DROP INDEX idx_llm_requests_pending;
DROP TABLE llm_requests;

CREATE TABLE llm_requests (
    id               INTEGER PRIMARY KEY,
    note_id          INTEGER NOT NULL REFERENCES notes (id) ON DELETE CASCADE,
    job_id           INTEGER REFERENCES jobs (id) ON DELETE SET NULL,
    operation_id     TEXT    NOT NULL,
    variant          INTEGER NOT NULL CHECK (variant BETWEEN 0 AND 2),
    status           TEXT    NOT NULL CHECK (status IN ('queued', 'sending', 'done', 'failed', 'blocked', 'dismissed')),
    -- Provider calls started for this row (a kill while sending counts: the provider may charge).
    attempts         INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    -- Times the request could not leave the device; drives the offline back-off.
    offline_retries  INTEGER NOT NULL DEFAULT 0 CHECK (offline_retries >= 0),
    next_attempt_at  INTEGER,
    prompt_kind      TEXT    NOT NULL,
    prompt_version   TEXT    NOT NULL,
    provider         TEXT    NOT NULL,
    model            TEXT    NOT NULL,
    cache_key        TEXT    NOT NULL,
    -- SHA-256 of the note text the prompt was built from; a different note text is never attached.
    source_hash      TEXT    NOT NULL,
    association_id   INTEGER REFERENCES associations (id) ON DELETE SET NULL,
    error_code       TEXT,
    created_at       INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL,
    UNIQUE (operation_id, variant)
);
CREATE INDEX idx_llm_requests_pending ON llm_requests (status, next_attempt_at);
CREATE INDEX idx_llm_requests_note ON llm_requests (note_id, status);

-- Every provider call, without any content: time, duration and outcome code. Feeds the local
-- daily call limit and the latency measurement (plan Phase 13 step 18).
CREATE TABLE llm_calls (
    id           INTEGER PRIMARY KEY,
    request_id   INTEGER REFERENCES llm_requests (id) ON DELETE SET NULL,
    started_at   INTEGER NOT NULL,
    duration_ms  INTEGER NOT NULL CHECK (duration_ms >= 0),
    provider     TEXT    NOT NULL,
    model        TEXT    NOT NULL,
    outcome      TEXT    NOT NULL
);
CREATE INDEX idx_llm_calls_started ON llm_calls (started_at);

CREATE INDEX idx_associations_cache_key ON associations (note_id, cache_key) WHERE cache_key IS NOT NULL;
