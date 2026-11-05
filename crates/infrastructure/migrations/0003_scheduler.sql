-- LanguaLinker schema v3: scheduler and study sessions (plan Phase 9, ADR-15).
-- A session over chosen cards ("study the Again cards again") studies only its own items and
-- ignores daily limits. The indexes serve the per-session summary, undo and the queue builder.

ALTER TABLE study_sessions ADD COLUMN restricted INTEGER NOT NULL DEFAULT 0 CHECK (restricted IN (0, 1));

CREATE INDEX idx_review_logs_session ON review_logs (session_id, id) WHERE session_id IS NOT NULL;
CREATE INDEX idx_review_undo_entries_session ON review_undo_entries (session_id, id);
CREATE INDEX idx_cards_buried ON cards (buried_until) WHERE queue IN (-2, -3);
