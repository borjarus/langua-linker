-- LanguaLinker schema v9: grammar in the app language (2026-11-20).
--
-- Bundled grammar tips and chapter titles are written in the learner's native language (Polish).
-- The content now also ships translations of them, keyed by UI language (`{"en": "…"}`); the
-- Grammar screen and the study card show the one of the app language and fall back to the text.
-- A tip the learner edits loses its translations (they no longer match the text).

ALTER TABLE grammar_tips ADD COLUMN translations TEXT NOT NULL DEFAULT '{}';
ALTER TABLE deck_sections ADD COLUMN title_translations TEXT NOT NULL DEFAULT '{}';
