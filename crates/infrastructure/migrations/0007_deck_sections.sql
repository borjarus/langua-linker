-- LanguaLinker schema v7: deck level and chapters (2026-11-19).
--
-- A deck installed from content keeps the CEFR level of its manifest (`A1`, or a range such as
-- `A1-A2`; NULL for decks made by the learner) and its chapters in reading order. A note belongs
-- to a chapter through a note tag equal to `tag`; the Grammar screen groups grammar tips by
-- chapter. The importer replaces both on every install or update of the deck.

ALTER TABLE decks ADD COLUMN level TEXT;

CREATE TABLE deck_sections (
    deck_id     INTEGER NOT NULL REFERENCES decks (id) ON DELETE CASCADE,
    tag         TEXT    NOT NULL,
    title       TEXT    NOT NULL,
    sort_order  INTEGER NOT NULL,
    PRIMARY KEY (deck_id, tag)
);
CREATE INDEX idx_deck_sections_order ON deck_sections (deck_id, sort_order);
