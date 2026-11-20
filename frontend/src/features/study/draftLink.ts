import type { Id, TargetLanguage, WordLinkDto } from '../../api/types'
import type { WordDraft } from '../../components/LinkedText.svelte'

const DRAFT_PREFIX = 'draft:'

/**
 * A word of a sentence without a stored link, shown in the word sheet like an unresolved link.
 * Its id is never sent to the backend: linking it calls `addWordLink` with the range.
 */
export function draftLink(sentenceNoteId: Id, word: WordDraft, targetLanguage: TargetLanguage): WordLinkDto {
  return {
    id: `${DRAFT_PREFIX}${word.startIndex}-${word.endIndex}`,
    sentenceNoteId,
    surface: word.surface,
    startIndex: word.startIndex,
    endIndex: word.endIndex,
    // The base form pre-fills a new word; German nouns keep their capital letter.
    lemma: targetLanguage === 'de' ? word.surface : word.surface.toLocaleLowerCase(targetLanguage),
    resolution: 'unresolved',
    wordNoteId: null,
    translation: null,
    candidates: [],
  }
}

export function isDraftLink(link: WordLinkDto): boolean {
  return link.id.startsWith(DRAFT_PREFIX)
}
