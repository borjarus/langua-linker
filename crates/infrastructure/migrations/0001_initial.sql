-- LanguaLinker schema v1 (plan Phase 7 steps 6–18; ADR-08: fresh database, no KMP import).
-- Conventions: ids are INTEGER (i64, strings over IPC); timestamps are UTC epoch milliseconds;
-- enums are stored as explicit integer codes defined in crates/domain (never enum order);
-- user-owned rows carry guid/created_at/updated_at/deleted_at/usn and, when edited through a
-- form, version. A soft-deleted row keeps its tombstone for future synchronization.
-- Applied once inside a transaction by crates/infrastructure/src/sqlite/migrations.rs; its
-- SHA-256 is recorded and checked on every open, so this file must never change after release.

-- Collection-wide facts: study-day boundary and the original Anki creation time of an import.
CREATE TABLE collection (
    id                   INTEGER PRIMARY KEY CHECK (id = 1),
    created_at           INTEGER NOT NULL,
    time_zone            TEXT    NOT NULL DEFAULT 'UTC',
    day_cutoff_minutes   INTEGER NOT NULL DEFAULT 240 CHECK (day_cutoff_minutes BETWEEN 0 AND 1439),
    anki_crt             INTEGER,
    usn                  INTEGER NOT NULL DEFAULT 0
);

-- Shared presets. One FK source: decks.config_id (plan step 8).
CREATE TABLE deck_config (
    id                       INTEGER PRIMARY KEY,
    guid                     TEXT    NOT NULL UNIQUE,
    name                     TEXT    NOT NULL,
    built_in                 INTEGER NOT NULL DEFAULT 0 CHECK (built_in IN (0, 1)),
    new_per_day              INTEGER NOT NULL CHECK (new_per_day BETWEEN 0 AND 9999),
    reviews_per_day          INTEGER NOT NULL CHECK (reviews_per_day BETWEEN 0 AND 99999),
    desired_retention        REAL    NOT NULL CHECK (desired_retention BETWEEN 0.70 AND 0.99),
    maximum_interval_days    INTEGER NOT NULL CHECK (maximum_interval_days BETWEEN 1 AND 36500),
    learning_steps_minutes   TEXT    NOT NULL CHECK (json_valid(learning_steps_minutes)),
    relearning_steps_minutes TEXT    NOT NULL CHECK (json_valid(relearning_steps_minutes)),
    fsrs_weights             TEXT    NOT NULL CHECK (json_valid(fsrs_weights) AND json_array_length(fsrs_weights) = 21),
    fuzz                     INTEGER NOT NULL DEFAULT 1 CHECK (fuzz IN (0, 1)),
    new_card_order           INTEGER NOT NULL DEFAULT 1,
    bury_siblings            INTEGER NOT NULL DEFAULT 1 CHECK (bury_siblings IN (0, 1)),
    leech_threshold          INTEGER NOT NULL DEFAULT 8 CHECK (leech_threshold BETWEEN 1 AND 99),
    leech_action             INTEGER NOT NULL DEFAULT 1,
    created_at               INTEGER NOT NULL,
    updated_at               INTEGER NOT NULL,
    deleted_at               INTEGER,
    usn                      INTEGER NOT NULL DEFAULT 0,
    version                  INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE decks (
    id                INTEGER PRIMARY KEY,
    guid              TEXT    NOT NULL UNIQUE,
    name              TEXT    NOT NULL,
    parent_id         INTEGER REFERENCES decks (id),
    description       TEXT    NOT NULL DEFAULT '',
    native_language   INTEGER NOT NULL,
    target_language   INTEGER NOT NULL,
    config_id         INTEGER NOT NULL REFERENCES deck_config (id),
    content_version   TEXT,
    bundled           INTEGER NOT NULL DEFAULT 0 CHECK (bundled IN (0, 1)),
    created_at        INTEGER NOT NULL,
    updated_at        INTEGER NOT NULL,
    deleted_at        INTEGER,
    usn               INTEGER NOT NULL DEFAULT 0,
    version           INTEGER NOT NULL DEFAULT 1,
    CHECK (parent_id IS NULL OR parent_id <> id)
);
CREATE INDEX idx_decks_parent ON decks (parent_id);

-- Per-deck presentation settings and sync fields; deliberately no second config_id (step 8).
CREATE TABLE deck_settings (
    deck_id      INTEGER PRIMARY KEY REFERENCES decks (id) ON DELETE CASCADE,
    deck_type    INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL,
    usn          INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE note_types (
    id           INTEGER PRIMARY KEY,
    guid         TEXT    NOT NULL UNIQUE,
    kind         INTEGER NOT NULL,
    name         TEXT    NOT NULL,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL,
    deleted_at   INTEGER,
    usn          INTEGER NOT NULL DEFAULT 0,
    version      INTEGER NOT NULL DEFAULT 1
);
CREATE UNIQUE INDEX idx_note_types_builtin_kind ON note_types (kind) WHERE deleted_at IS NULL;

CREATE TABLE card_templates (
    id             INTEGER PRIMARY KEY,
    guid           TEXT    NOT NULL UNIQUE,
    note_type_id   INTEGER NOT NULL REFERENCES note_types (id),
    ordinal        INTEGER NOT NULL CHECK (ordinal >= 0),
    direction      INTEGER NOT NULL,
    name           TEXT    NOT NULL,
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL,
    usn            INTEGER NOT NULL DEFAULT 0,
    version        INTEGER NOT NULL DEFAULT 1,
    UNIQUE (note_type_id, ordinal)
);

CREATE TABLE notes (
    id             INTEGER PRIMARY KEY,
    guid           TEXT    NOT NULL UNIQUE,
    note_type_id   INTEGER NOT NULL REFERENCES note_types (id),
    fields         TEXT    NOT NULL CHECK (json_valid(fields)),
    sort_field     TEXT    NOT NULL,
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL,
    deleted_at     INTEGER,
    usn            INTEGER NOT NULL DEFAULT 0,
    version        INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX idx_notes_sort_field ON notes (sort_field) WHERE deleted_at IS NULL;

CREATE TABLE tags (
    id           INTEGER PRIMARY KEY,
    name         TEXT    NOT NULL UNIQUE COLLATE NOCASE,
    created_at   INTEGER NOT NULL,
    usn          INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE note_tags (
    note_id    INTEGER NOT NULL REFERENCES notes (id) ON DELETE CASCADE,
    tag_id     INTEGER NOT NULL REFERENCES tags (id) ON DELETE CASCADE,
    position   INTEGER NOT NULL,
    PRIMARY KEY (note_id, tag_id)
) WITHOUT ROWID;
CREATE INDEX idx_note_tags_tag ON note_tags (tag_id);

-- The scheduling deck belongs to the card (step 10). New cards have due_at NULL and use
-- position; scheduled cards have due_at in UTC ms (step 16). R and average interval are
-- computed, never stored.
CREATE TABLE cards (
    id                 INTEGER PRIMARY KEY,
    guid               TEXT    NOT NULL UNIQUE,
    note_id            INTEGER NOT NULL REFERENCES notes (id) ON DELETE CASCADE,
    deck_id            INTEGER NOT NULL REFERENCES decks (id),
    template_ordinal   INTEGER NOT NULL CHECK (template_ordinal >= 0),
    position           INTEGER NOT NULL DEFAULT 0,
    flags              INTEGER NOT NULL DEFAULT 0 CHECK (flags BETWEEN 0 AND 7),
    state              INTEGER NOT NULL DEFAULT 0,
    queue              INTEGER NOT NULL DEFAULT 0,
    due_at             INTEGER,
    stability          REAL,
    difficulty         REAL CHECK (difficulty IS NULL OR difficulty BETWEEN 1 AND 10),
    legacy_ease        INTEGER,
    last_review_at     INTEGER,
    scheduled_days     INTEGER NOT NULL DEFAULT 0,
    elapsed_days       INTEGER NOT NULL DEFAULT 0,
    reps               INTEGER NOT NULL DEFAULT 0 CHECK (reps >= 0),
    lapses             INTEGER NOT NULL DEFAULT 0 CHECK (lapses >= 0),
    learning_step      INTEGER,
    buried_until       INTEGER,
    archived_at        INTEGER,
    created_at         INTEGER NOT NULL,
    updated_at         INTEGER NOT NULL,
    deleted_at         INTEGER,
    usn                INTEGER NOT NULL DEFAULT 0,
    version            INTEGER NOT NULL DEFAULT 1,
    CHECK ((state = 0) = (due_at IS NULL)),
    CHECK ((stability IS NULL) = (difficulty IS NULL))
);
-- One live card per template of a note; an archived card is restored, not duplicated (step 11).
CREATE UNIQUE INDEX idx_cards_note_template ON cards (note_id, template_ordinal) WHERE deleted_at IS NULL;
CREATE INDEX idx_cards_note ON cards (note_id);
CREATE INDEX idx_cards_deck_queue_due ON cards (deck_id, queue, due_at)
    WHERE deleted_at IS NULL AND archived_at IS NULL;
CREATE INDEX idx_cards_deck_queue_position ON cards (deck_id, queue, position)
    WHERE deleted_at IS NULL AND archived_at IS NULL;

CREATE TABLE study_sessions (
    id              INTEGER PRIMARY KEY,
    deck_id         INTEGER REFERENCES decks (id),
    status          TEXT    NOT NULL CHECK (status IN ('active', 'paused', 'finished')),
    config_id       INTEGER REFERENCES deck_config (id),
    config_version  INTEGER,
    cursor          INTEGER NOT NULL DEFAULT 0,
    current_card_id INTEGER REFERENCES cards (id),
    started_at      INTEGER NOT NULL,
    paused_at       INTEGER,
    finished_at     INTEGER,
    active_ms       INTEGER NOT NULL DEFAULT 0,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL
);
-- At most one unfinished session (contract: startStudy closes the previous one).
CREATE UNIQUE INDEX idx_study_sessions_open ON study_sessions (status <> 'finished') WHERE status <> 'finished';

CREATE TABLE session_items (
    session_id   INTEGER NOT NULL REFERENCES study_sessions (id) ON DELETE CASCADE,
    position     INTEGER NOT NULL,
    card_id      INTEGER NOT NULL REFERENCES cards (id) ON DELETE CASCADE,
    status       TEXT    NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'answered', 'skipped')),
    PRIMARY KEY (session_id, position)
) WITHOUT ROWID;

-- Full before/after card snapshots so undo restores due/reps/lapses/queue/siblings (step 14).
CREATE TABLE review_logs (
    id               INTEGER PRIMARY KEY,
    card_id          INTEGER NOT NULL REFERENCES cards (id) ON DELETE CASCADE,
    session_id       INTEGER REFERENCES study_sessions (id) ON DELETE SET NULL,
    operation_id     TEXT    NOT NULL UNIQUE,
    rating           INTEGER NOT NULL CHECK (rating BETWEEN 1 AND 4),
    kind             INTEGER NOT NULL,
    reviewed_at      INTEGER NOT NULL,
    duration_ms      INTEGER NOT NULL CHECK (duration_ms >= 0),
    card_before      TEXT    NOT NULL CHECK (json_valid(card_before)),
    card_after       TEXT    NOT NULL CHECK (json_valid(card_after)),
    fsrs_version     TEXT    NOT NULL,
    parameters_hash  TEXT    NOT NULL,
    invalidated_at   INTEGER,
    usn              INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_review_logs_card_time ON review_logs (card_id, reviewed_at);
CREATE INDEX idx_review_logs_time ON review_logs (reviewed_at);

CREATE TABLE review_undo_entries (
    id              INTEGER PRIMARY KEY,
    session_id      INTEGER NOT NULL REFERENCES study_sessions (id) ON DELETE CASCADE,
    review_log_id   INTEGER NOT NULL REFERENCES review_logs (id) ON DELETE CASCADE,
    sibling_changes TEXT    NOT NULL DEFAULT '[]' CHECK (json_valid(sibling_changes)),
    created_at      INTEGER NOT NULL,
    consumed_at     INTEGER
);

-- Associations and grammar tips belong to the note (step 12).
CREATE TABLE associations (
    id               INTEGER PRIMARY KEY,
    guid             TEXT    NOT NULL UNIQUE,
    note_id          INTEGER NOT NULL REFERENCES notes (id) ON DELETE CASCADE,
    text             TEXT    NOT NULL,
    provenance       INTEGER NOT NULL,
    native_language  TEXT    NOT NULL,
    target_language  TEXT    NOT NULL,
    prompt_version   TEXT,
    model            TEXT,
    moderation       INTEGER NOT NULL DEFAULT 0,
    cache_key        TEXT,
    sort_order       INTEGER NOT NULL DEFAULT 0,
    is_favorite      INTEGER NOT NULL DEFAULT 0 CHECK (is_favorite IN (0, 1)),
    created_at       INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL,
    deleted_at       INTEGER,
    usn              INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_associations_note ON associations (note_id, sort_order);
CREATE UNIQUE INDEX idx_associations_favorite ON associations (note_id)
    WHERE is_favorite = 1 AND deleted_at IS NULL;

CREATE TABLE grammar_tips (
    id           INTEGER PRIMARY KEY,
    guid         TEXT    NOT NULL UNIQUE,
    note_id      INTEGER NOT NULL REFERENCES notes (id) ON DELETE CASCADE,
    text         TEXT    NOT NULL,
    provenance   INTEGER NOT NULL,
    sort_order   INTEGER NOT NULL DEFAULT 0,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL,
    deleted_at   INTEGER,
    usn          INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_grammar_tips_note ON grammar_tips (note_id, sort_order);

-- Ranges are UTF-16 code units over the NFC front, half-open. Deleting the word leaves the link
-- unresolved (step 13); deleting the sentence removes its links.
CREATE TABLE sentence_word_links (
    id                      INTEGER PRIMARY KEY,
    sentence_note_id        INTEGER NOT NULL REFERENCES notes (id) ON DELETE CASCADE,
    word_note_id            INTEGER REFERENCES notes (id) ON DELETE SET NULL,
    preferred_word_card_id  INTEGER REFERENCES cards (id) ON DELETE SET NULL,
    surface_form            TEXT    NOT NULL,
    lemma                   TEXT,
    start_index             INTEGER NOT NULL CHECK (start_index >= 0),
    end_index               INTEGER NOT NULL,
    resolution              INTEGER NOT NULL,
    created_at              INTEGER NOT NULL,
    updated_at              INTEGER NOT NULL,
    CHECK (end_index > start_index)
);
CREATE INDEX idx_word_links_sentence ON sentence_word_links (sentence_note_id, start_index);
CREATE INDEX idx_word_links_word ON sentence_word_links (word_note_id);

CREATE TABLE settings (
    key          TEXT    PRIMARY KEY,
    value        TEXT    NOT NULL CHECK (json_valid(value)),
    updated_at   INTEGER NOT NULL,
    version      INTEGER NOT NULL DEFAULT 1
) WITHOUT ROWID;

-- Durable job status (step 30): get_job_status reads this, events are only signals.
CREATE TABLE jobs (
    id              INTEGER PRIMARY KEY,
    kind            TEXT    NOT NULL,
    status          TEXT    NOT NULL CHECK (status IN ('queued', 'running', 'succeeded', 'failed', 'cancelled')),
    sequence        INTEGER NOT NULL DEFAULT 0,
    progress_done   INTEGER,
    progress_total  INTEGER,
    operation_id    TEXT    UNIQUE,
    started_at      INTEGER NOT NULL,
    finished_at     INTEGER,
    error_code      TEXT,
    error_details   TEXT    CHECK (error_details IS NULL OR json_valid(error_details)),
    result          TEXT    CHECK (result IS NULL OR json_valid(result))
);
CREATE INDEX idx_jobs_unfinished ON jobs (status) WHERE status IN ('queued', 'running');

-- Idempotency: the first result of every mutation, keyed by its operationId (contract rule 4).
CREATE TABLE operations (
    operation_id  TEXT    PRIMARY KEY,
    operation     TEXT    NOT NULL,
    result        TEXT    NOT NULL,
    created_at    INTEGER NOT NULL
) WITHOUT ROWID;

-- LLM outbox (Phase 13): requests survive restarts and offline periods.
CREATE TABLE llm_requests (
    id               INTEGER PRIMARY KEY,
    note_id          INTEGER NOT NULL REFERENCES notes (id) ON DELETE CASCADE,
    job_id           INTEGER REFERENCES jobs (id) ON DELETE SET NULL,
    operation_id     TEXT    NOT NULL UNIQUE,
    status           TEXT    NOT NULL CHECK (status IN ('queued', 'sending', 'done', 'failed', 'blocked')),
    attempts         INTEGER NOT NULL DEFAULT 0,
    next_attempt_at  INTEGER,
    prompt_version   TEXT    NOT NULL,
    model            TEXT    NOT NULL,
    cache_key        TEXT    NOT NULL,
    error_code       TEXT,
    created_at       INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL
);
CREATE INDEX idx_llm_requests_pending ON llm_requests (status, next_attempt_at);

CREATE TABLE association_cache (
    cache_key       TEXT    PRIMARY KEY,
    text            TEXT    NOT NULL,
    model           TEXT    NOT NULL,
    prompt_version  TEXT    NOT NULL,
    moderation      INTEGER NOT NULL,
    created_at      INTEGER NOT NULL,
    expires_at      INTEGER
) WITHOUT ROWID;

CREATE TABLE media (
    id           INTEGER PRIMARY KEY,
    guid         TEXT    NOT NULL UNIQUE,
    file_name    TEXT    NOT NULL UNIQUE,
    mime_type    TEXT    NOT NULL,
    sha256       TEXT    NOT NULL,
    size_bytes   INTEGER NOT NULL CHECK (size_bytes >= 0),
    created_at   INTEGER NOT NULL,
    deleted_at   INTEGER
);

CREATE TABLE import_batches (
    id            INTEGER PRIMARY KEY,
    job_id        INTEGER REFERENCES jobs (id) ON DELETE SET NULL,
    source_kind   TEXT    NOT NULL CHECK (source_kind IN ('bundled', 'csv', 'apkg')),
    source_name   TEXT    NOT NULL,
    counts        TEXT    NOT NULL DEFAULT '{}' CHECK (json_valid(counts)),
    started_at    INTEGER NOT NULL,
    finished_at   INTEGER
);

-- Identity of imported Anki entities, so a re-import updates instead of duplicating.
CREATE TABLE anki_origin (
    entity_kind       TEXT    NOT NULL CHECK (entity_kind IN ('deck', 'note', 'card', 'notetype')),
    entity_id         INTEGER NOT NULL,
    anki_id           INTEGER NOT NULL,
    anki_guid         TEXT,
    import_batch_id   INTEGER REFERENCES import_batches (id) ON DELETE SET NULL,
    PRIMARY KEY (entity_kind, entity_id)
) WITHOUT ROWID;

-- User edits to bundled content, kept across content updates (Phase 8).
CREATE TABLE content_overrides (
    note_id                    INTEGER NOT NULL REFERENCES notes (id) ON DELETE CASCADE,
    field                      TEXT    NOT NULL,
    original_content_version   TEXT    NOT NULL,
    updated_at                 INTEGER NOT NULL,
    PRIMARY KEY (note_id, field)
) WITHOUT ROWID;

-- Full-text search over note fields (Phase 11 builds the query language on top).
CREATE VIRTUAL TABLE notes_fts USING fts5 (front, back, tokenize = 'unicode61 remove_diacritics 2');
CREATE TRIGGER notes_fts_insert AFTER INSERT ON notes BEGIN
    INSERT INTO notes_fts (rowid, front, back)
    VALUES (new.id, json_extract(new.fields, '$.front'), json_extract(new.fields, '$.back'));
END;
CREATE TRIGGER notes_fts_update AFTER UPDATE OF fields ON notes BEGIN
    DELETE FROM notes_fts WHERE rowid = old.id;
    INSERT INTO notes_fts (rowid, front, back)
    VALUES (new.id, json_extract(new.fields, '$.front'), json_extract(new.fields, '$.back'));
END;
CREATE TRIGGER notes_fts_delete AFTER DELETE ON notes BEGIN
    DELETE FROM notes_fts WHERE rowid = old.id;
END;

-- Built-in note types and templates; codes from crates/domain/src/note.rs.
INSERT INTO note_types (id, guid, kind, name, created_at, updated_at) VALUES
    (1, 'notetype-basic', 1, 'Basic', 0, 0),
    (2, 'notetype-basic-reversed', 2, 'Basic (and reversed card)', 0, 0),
    (3, 'notetype-sentence', 3, 'Sentence', 0, 0),
    (4, 'notetype-sentence-reversed', 4, 'Sentence (and reversed card)', 0, 0);
INSERT INTO card_templates (guid, note_type_id, ordinal, direction, name, created_at, updated_at) VALUES
    ('template-basic-0', 1, 0, 0, 'Card 1', 0, 0),
    ('template-basic-reversed-0', 2, 0, 0, 'Card 1', 0, 0),
    ('template-basic-reversed-1', 2, 1, 1, 'Card 2', 0, 0),
    ('template-sentence-0', 3, 0, 0, 'Card 1', 0, 0),
    ('template-sentence-reversed-0', 4, 0, 0, 'Card 1', 0, 0),
    ('template-sentence-reversed-1', 4, 1, 1, 'Card 2', 0, 0);

-- Built-in default preset; defaults from crates/domain/src/deck.rs (DeckConfigOptions).
INSERT INTO deck_config (
    id, guid, name, built_in, new_per_day, reviews_per_day, desired_retention,
    maximum_interval_days, learning_steps_minutes, relearning_steps_minutes, fsrs_weights,
    created_at, updated_at
) VALUES (
    1, 'preset-default', 'Default', 1, 20, 200, 0.9, 36500, '[1,10]', '[10]',
    '[0.2120,1.2931,2.3065,8.2956,6.4133,0.8334,3.0194,0.0010,1.8722,0.1666,0.7960,1.4835,0.0614,0.2629,1.6483,0.6014,1.8729,0.5425,0.0912,0.0658,0.1542]',
    0, 0
);
