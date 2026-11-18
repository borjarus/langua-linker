-- LanguaLinker schema v5: extended associations (2026-11-18).
--
-- A bundled association may carry a long Markdown version (sound split, story, chain, example
-- sentences, grammar, culture, related words) shown collapsed under the short text. NULL when
-- there is none; learners never edit it, the importer keeps it in step with the bundled content.

ALTER TABLE associations ADD COLUMN extended TEXT;
