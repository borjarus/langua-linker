-- LanguaLinker schema v8: decks outside study sessions (2026-11-19).
--
-- A deck with `in_study = 0` stays in the collection for reference: its words remain link
-- targets of sentences and can be browsed, but no session (of the deck, a parent or all decks)
-- takes its cards and it adds nothing to the due counts. Chosen-card sessions are not affected.

ALTER TABLE decks ADD COLUMN in_study INTEGER NOT NULL DEFAULT 1 CHECK (in_study IN (0, 1));
