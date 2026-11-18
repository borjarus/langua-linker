-- LanguaLinker schema v6: FSRS optimizer runs (Phase 18, ADR-22).
--
-- One row per finished optimizer run of a preset: the weights before and the proposal, the held-out
-- metrics and workload preview (`report`, JSON), and when it was applied or reverted. Review logs are
-- never changed by the optimizer; applying writes only the preset's weights, so a revert restores
-- `weights_before` exactly.

CREATE TABLE optimizer_runs (
    id                INTEGER PRIMARY KEY,
    preset_id         INTEGER NOT NULL REFERENCES deck_config (id) ON DELETE CASCADE,
    status            INTEGER NOT NULL CHECK (status BETWEEN 0 AND 2),
    optimizer_version TEXT    NOT NULL,
    fsrs_version      TEXT    NOT NULL,
    dataset_hash      TEXT    NOT NULL,
    weights_before    TEXT    NOT NULL CHECK (json_valid(weights_before)),
    weights_proposed  TEXT    NOT NULL CHECK (json_valid(weights_proposed)),
    report            TEXT    NOT NULL CHECK (json_valid(report)),
    created_at        INTEGER NOT NULL,
    applied_at        INTEGER,
    reverted_at       INTEGER
);
CREATE INDEX idx_optimizer_runs_preset ON optimizer_runs (preset_id, created_at);
