-- LanguaLinker schema v2: provenance of bundled notes (plan Phase 8 steps 9–11, ADR-14).
-- A note installed from bundled content gets one row here. The importer uses it to tell bundled
-- notes from notes the user added to a bundled deck, to find notes a newer content version no
-- longer ships, and to restore exactly the cards it archived itself. Scheduling columns of
-- `cards` are never written by the importer.

CREATE TABLE bundled_notes (
    note_id              INTEGER PRIMARY KEY REFERENCES notes (id) ON DELETE CASCADE,
    deck_guid            TEXT    NOT NULL,
    -- Last content version that shipped the note.
    content_version      TEXT    NOT NULL,
    -- Version that no longer ships it; NULL while it is shipped.
    removed_in_version   TEXT,
    -- 1 when the importer archived the cards because the note was removed without progress.
    archived_by_import   INTEGER NOT NULL DEFAULT 0 CHECK (archived_by_import IN (0, 1)),
    CHECK (archived_by_import = 0 OR removed_in_version IS NOT NULL)
) WITHOUT ROWID;
CREATE INDEX idx_bundled_notes_deck ON bundled_notes (deck_guid);
