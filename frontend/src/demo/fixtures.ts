import type {
  CardFlag,
  CardState,
  DeckCapabilitiesDto,
  DeckType,
  NoteTypeKind,
  QueueState,
  TargetLanguage,
  WordLinkResolution,
} from '../api/types'
import { DAY_MS, MINUTE_MS, createRandom, pick } from './random'

/**
 * Demonstration fixtures. Content intentionally covers Polish diacritics, German compounds,
 * multi-word units, emoji (ZWJ sequences) and RTL text (plan Phase 2, step 18).
 */

export const DEMO_EPOCH_MS = Date.UTC(2026, 2, 2, 8, 0, 0)

export function capabilitiesFor(deckType: DeckType): DeckCapabilitiesDto {
  switch (deckType) {
    case 'Simple':
      return { associations: false, grammarTips: false, wordLinks: false, tts: true }
    case 'TextWithAssociations':
      return { associations: true, grammarTips: false, wordLinks: false, tts: true }
    case 'Linguistic':
      return { associations: true, grammarTips: true, wordLinks: true, tts: true }
  }
}

export interface DeckRecord {
  id: string
  guid: string
  name: string
  parentId: string | null
  targetLanguage: TargetLanguage
  deckType: DeckType
  description: string
  bundled: boolean
  presetId: string
  version: number
}

export interface NoteRecord {
  id: string
  guid: string
  deckId: string
  noteTypeKind: NoteTypeKind
  front: string
  back: string
  tags: string[]
  version: number
  createdAtMs: number
  updatedAtMs: number
}

export interface CardRecord {
  id: string
  noteId: string
  deckId: string
  ordinal: number
  state: CardState
  queue: QueueState
  flag: CardFlag
  dueAtMs: number | null
  intervalSeconds: number
  stability: number | null
  difficulty: number | null
  reps: number
  lapses: number
  version: number
  deleted: boolean
  /** Removed by a note-type change, not by the user: switching back restores it with its history. */
  archived: boolean
  /** Order in the new queue; `null` once the card has been studied. */
  position: number | null
}

export interface PresetRecord {
  id: string
  name: string
  version: number
  builtIn: boolean
  newPerDay: number
  reviewsPerDay: number
  desiredRetention: number
  maxIntervalDays: number
  learningStepsMinutes: number[]
  relearningStepsMinutes: number[]
  weights: number[]
}

/** FSRS-6 default weights w0–w20 (fixture copy; the pinned reference arrives in Phase 9). */
export const DEFAULT_WEIGHTS = [
  0.212, 1.2931, 2.3065, 8.2956, 6.4133, 0.8334, 3.0194, 0.001, 1.8722, 0.1666, 0.796, 1.4835, 0.0614, 0.2629, 1.6483, 0.6014,
  1.8729, 0.5425, 0.0912, 0.0658, 0.1542,
]

export function createPresets(): PresetRecord[] {
  const base = { version: 1, builtIn: true, reviewsPerDay: 200, desiredRetention: 0.9, maxIntervalDays: 36_500, learningStepsMinutes: [1, 10], relearningStepsMinutes: [10], weights: [...DEFAULT_WEIGHTS] }
  return [
    { ...base, id: 'preset-default', name: 'Domyślny (20 nowych/dzień)', newPerDay: 20 },
    { ...base, id: 'preset-light', name: 'Lekki (10 nowych/dzień)', newPerDay: 10, reviewsPerDay: 100 },
  ]
}

export interface AssociationRecord {
  id: string
  noteId: string
  text: string
  provenance: 'bundled' | 'generated' | 'manual'
  favorite: boolean
  status: 'ready' | 'pending' | 'offlineQueued' | 'error' | 'blocked'
  /** Why a generated association failed: provider error, unusable output or moderation. */
  errorCode?: 'PROVIDER_ERROR' | 'INVALID_PROVIDER_OUTPUT' | 'MODERATION_BLOCKED'
}

export interface TipRecord {
  id: string
  noteId: string
  text: string
  order: number
}

export interface WordLinkRecord {
  id: string
  sentenceNoteId: string
  surface: string
  startIndex: number
  endIndex: number
  lemma: string | null
  resolution: WordLinkResolution
  wordNoteId: string | null
  candidateNoteIds: string[]
}

export interface ReviewLogRecord {
  id: string
  cardId: string
  reviewedAtMs: number
  rating: 'Again' | 'Hard' | 'Good' | 'Easy'
  stateBefore: CardState
  intervalSeconds: number
}

export interface Collection {
  decks: DeckRecord[]
  notes: NoteRecord[]
  cards: CardRecord[]
  associations: AssociationRecord[]
  tips: TipRecord[]
  wordLinks: WordLinkRecord[]
  logs: ReviewLogRecord[]
}

interface NoteSeed {
  front: string
  back: string
  tags?: string[]
  association?: string
  tips?: string[]
  reversed?: boolean
}

const DE_WORDS: NoteSeed[] = [
  { front: 'der Kühlschrank', back: 'lodówka', tags: ['dom'], association: '„Kul się, szrank!” — chłód z lodówki każe się skulić ❄️', tips: ['Rodzaj męski: der Kühlschrank, die Kühlschränke.', 'Złożenie: kühl (chłodny) + der Schrank (szafa).'] },
  { front: 'das Frühstück', back: 'śniadanie', tags: ['jedzenie'], association: 'Früh (wcześnie) + Stück (kawałek) — wczesny kawałek dnia przy stole 🥐', tips: ['Rodzaj nijaki: das Frühstück.'] },
  { front: 'heute Abend', back: 'dziś wieczorem', tags: ['czas'], association: 'Heute = „hojnie” dziś, Abend = „abendowo” wieczorem 🌙', tips: ['Wyrażenie wielowyrazowe — nie rozdzielaj w nauce.'] },
  { front: 'der Tee', back: 'herbata', tags: ['jedzenie'], association: 'Tee brzmi jak „T” — kształt filiżanki herbaty ☕' },
  { front: 'die Bank (Sitz)', back: 'ławka', tags: ['miasto', 'homograf'], association: 'Bank jak bankiet na ławce w parku 🪑', tips: ['Liczba mnoga: die Bänke.'] },
  { front: 'die Bank (Geld)', back: 'bank', tags: ['miasto', 'homograf'], association: 'Bank to bank — pieniądze 🏦', tips: ['Liczba mnoga: die Banken.'] },
  { front: 'trinken', back: 'pić', tags: ['czasowniki'], association: 'Trinken — „drink” w kinie 🥤', tips: ['Odmiana nieregularna: trank, getrunken.'] },
  { front: 'gemütlich', back: 'przytulny, wygodny', tags: ['przymiotniki'], association: 'Gemütlich — „gem” jak klejnot domowej przytulności 🛋️' },
  { front: 'Entschuldigung', back: 'przepraszam', tags: ['zwroty'], association: 'Ent-schuld — odejmuję winę (Schuld) 🙏' },
  { front: 'die Brücke', back: 'most', tags: ['miasto'], association: 'Brücke — „bruk” prowadzi na most 🌉', tips: ['Rodzaj żeński: die Brücke.'] },
]

const DE_COMPOUNDS: NoteSeed[] = [
  { front: 'die Straßenbahnhaltestelle', back: 'przystanek tramwajowy', tags: ['miasto', 'złożenia'], association: 'Straße + Bahn + Halte + Stelle: ulica, tor, stój, miejsce 🚋', tips: ['Rodzaj określa ostatni człon: die Stelle.'] },
  { front: 'das Donaudampfschifffahrtsgesellschaftskapitänspatent', back: 'patent kapitana towarzystwa żeglugi parowej na Dunaju', tags: ['złożenia'], association: 'Najdłuższy test zawijania tekstu w interfejsie 📏', tips: ['Fixture: bardzo długie słowo bez spacji.'] },
  { front: 'der Handschuh', back: 'rękawiczka', tags: ['ubrania', 'złożenia'], association: 'Hand + Schuh — but na dłoń 🧤' },
]

const DE_SENTENCES: NoteSeed[] = [
  { front: 'Ich trinke heute Abend Tee.', back: 'Dziś wieczorem piję herbatę.', tags: ['zdania'], association: 'Wieczorny rytuał z herbatą ☕🌙' },
  { front: 'Die Bank ist geschlossen.', back: 'Bank (ławka?) jest zamknięty.', tags: ['zdania', 'homograf'] },
  { front: 'Entschuldigung, wo ist die Straßenbahnhaltestelle?', back: 'Przepraszam, gdzie jest przystanek tramwajowy?', tags: ['zdania'] },
  { front: 'Wir frühstücken gemütlich zusammen.', back: 'Jemy razem spokojne śniadanie.', tags: ['zdania'] },
]

const EN_WORDS: NoteSeed[] = [
  { front: 'the weather', back: 'pogoda', tags: ['nature'], association: 'Weather — „wet her”: deszcz zmoczył ją znowu 🌧️' },
  { front: 'to borrow', back: 'pożyczać (od kogoś)', tags: ['verbs'], association: 'Borrow — „bo row”: pożyczam łódkę, bo wiosłuję 🚣' },
  { front: 'developer 👩‍💻', back: 'programistka / programista', tags: ['work', 'emoji'], association: 'Emoji ZWJ: 👩‍💻 — test zakresów UTF-16' },
  { front: 'hello (Hebrew: שלום)', back: 'cześć (hebr. szalom)', tags: ['rtl'], association: 'Fixture RTL: שלום czytamy od prawej do lewej ↩️' },
  { front: 'zażółć gęślą jaźń', back: 'pangram (fixture diakrytyków)', tags: ['fixture'], association: 'Wszystkie polskie znaki diakrytyczne w jednym zdaniu ✍️' },
]

const EN_BASICS: NoteSeed[] = [
  { front: 'thank you', back: 'dziękuję', reversed: true },
  { front: 'good morning', back: 'dzień dobry (rano)', reversed: true },
  { front: 'see you later', back: 'do zobaczenia', reversed: true },
  { front: 'excuse me', back: 'przepraszam (zwracając uwagę)', reversed: true },
]

const DECK_SEEDS: { record: Omit<DeckRecord, 'version'>; notes: NoteSeed[]; noteType: NoteTypeKind }[] = [
  {
    record: { id: '1', guid: 'deck-de-words-a1', name: 'Niemiecki — słówka A1', parentId: null, targetLanguage: 'de', deckType: 'Linguistic', description: 'Podstawowe słownictwo z mnemotechnikami i wskazówkami gramatycznymi.', bundled: true, presetId: 'preset-default' },
    notes: DE_WORDS,
    noteType: 'Basic',
  },
  {
    record: { id: '5', guid: 'deck-de-compounds', name: 'Złożenia (Komposita)', parentId: '1', targetLanguage: 'de', deckType: 'Linguistic', description: 'Rzeczowniki złożone — rodzaj określa ostatni człon.', bundled: true, presetId: 'preset-default' },
    notes: DE_COMPOUNDS,
    noteType: 'Basic',
  },
  {
    record: { id: '2', guid: 'deck-de-sentences-a1', name: 'Niemiecki — zdania A1', parentId: null, targetLanguage: 'de', deckType: 'Linguistic', description: 'Zdania z linkami do słówek.', bundled: true, presetId: 'preset-default' },
    notes: DE_SENTENCES,
    noteType: 'Sentence',
  },
  {
    record: { id: '3', guid: 'deck-en-words-a1', name: 'English — words A1', parentId: null, targetLanguage: 'en', deckType: 'TextWithAssociations', description: 'Słówka angielskie z mnemotechnikami.', bundled: true, presetId: 'preset-default' },
    notes: EN_WORDS,
    noteType: 'Basic',
  },
  {
    record: { id: '4', guid: 'deck-en-basics', name: 'English — zwroty (własna)', parentId: null, targetLanguage: 'en', deckType: 'Simple', description: 'Własna talia: dwukierunkowe karty.', bundled: false, presetId: 'preset-light' },
    notes: EN_BASICS,
    noteType: 'BasicAndReversed',
  },
]

/** Word-link targets: sentence front → [surface, lemma, word-note front or null, ambiguous candidates]. */
const SENTENCE_LINKS: Record<string, [string, string | null, string | null, string[]][]> = {
  'Ich trinke heute Abend Tee.': [
    ['trinke', 'trinken', 'trinken', []],
    ['heute Abend', 'heute Abend', 'heute Abend', []],
    ['Tee', 'Tee', 'der Tee', []],
  ],
  'Die Bank ist geschlossen.': [
    ['Bank', 'Bank', null, ['die Bank (Sitz)', 'die Bank (Geld)']],
    ['geschlossen', 'schließen', null, []],
  ],
  'Entschuldigung, wo ist die Straßenbahnhaltestelle?': [
    ['Entschuldigung', 'Entschuldigung', 'Entschuldigung', []],
    ['Straßenbahnhaltestelle', 'Straßenbahnhaltestelle', 'die Straßenbahnhaltestelle', []],
  ],
  'Wir frühstücken gemütlich zusammen.': [
    ['frühstücken', 'frühstücken', null, ['das Frühstück']],
    ['gemütlich', 'gemütlich', 'gemütlich', []],
  ],
}

const STATES: CardState[] = ['New', 'New', 'Review', 'Review', 'Review', 'Learning', 'Relearning']

export interface CollectionOptions {
  seed: number
  nowMs: number
  ambiguousLinks: boolean
}

export function createCollection({ seed, nowMs, ambiguousLinks }: CollectionOptions): Collection {
  const random = createRandom(seed)
  const collection: Collection = { decks: [], notes: [], cards: [], associations: [], tips: [], wordLinks: [], logs: [] }
  let noteSeq = 100
  let cardSeq = 1000
  let extraSeq = 5000
  let positionSeq = 1

  for (const { record, notes, noteType } of DECK_SEEDS) {
    collection.decks.push({ ...record, version: 1 })
    for (const seedNote of notes) {
      const noteId = String(noteSeq++)
      const createdAtMs = nowMs - Math.floor(random() * 60) * DAY_MS
      const kind: NoteTypeKind = seedNote.reversed ? 'BasicAndReversed' : noteType
      collection.notes.push({
        id: noteId,
        guid: `note-${record.guid}-${noteId}`,
        deckId: record.id,
        noteTypeKind: kind,
        front: seedNote.front.normalize('NFC'),
        back: seedNote.back.normalize('NFC'),
        tags: seedNote.tags ?? [],
        version: 1,
        createdAtMs,
        updatedAtMs: createdAtMs,
      })
      const ordinals = kind === 'BasicAndReversed' ? [0, 1] : [0]
      for (const ordinal of ordinals) {
        const state = pick(random, STATES)
        const intervalDays = state === 'Review' ? 1 + Math.floor(random() * 40) : 0
        const dueOffset = state === 'New' ? null : Math.floor((random() - 0.45) * 6 * DAY_MS)
        const cardId = String(cardSeq++)
        const reps = state === 'New' ? 0 : 1 + Math.floor(random() * 12)
        collection.cards.push({
          id: cardId,
          noteId,
          deckId: record.id,
          ordinal,
          state,
          queue: state === 'New' ? 'New' : state === 'Review' ? 'Review' : 'Learning',
          flag: random() < 0.1 ? 'red' : 'none',
          dueAtMs: dueOffset === null ? null : nowMs + dueOffset,
          intervalSeconds: intervalDays * 86_400,
          stability: state === 'New' ? null : Math.round((1 + random() * 40) * 100) / 100,
          difficulty: state === 'New' ? null : Math.round((1 + random() * 9) * 100) / 100,
          reps,
          lapses: state === 'Relearning' ? 1 + Math.floor(random() * 3) : 0,
          version: 1,
          deleted: false,
          archived: false,
          position: state === 'New' ? positionSeq++ : null,
        })
        for (let index = 0; index < Math.min(reps, 5); index += 1) {
          collection.logs.push({
            id: String(extraSeq++),
            cardId,
            reviewedAtMs: nowMs - (index + 1) * 3 * DAY_MS - Math.floor(random() * 8) * 60 * MINUTE_MS,
            rating: pick(random, ['Again', 'Hard', 'Good', 'Good', 'Easy'] as const),
            stateBefore: index === reps - 1 ? 'New' : 'Review',
            intervalSeconds: Math.max(600, intervalDays * 86_400 - index * 86_400),
          })
        }
      }
      if (seedNote.association) {
        collection.associations.push({ id: String(extraSeq++), noteId, text: seedNote.association, provenance: 'bundled', favorite: true, status: 'ready' })
      }
      ;(seedNote.tips ?? []).forEach((text, order) => {
        collection.tips.push({ id: String(extraSeq++), noteId, text, order })
      })
    }
  }

  const noteByFront = new Map(collection.notes.map((note) => [note.front, note]))
  for (const [sentence, links] of Object.entries(SENTENCE_LINKS)) {
    const note = noteByFront.get(sentence)
    if (!note) continue
    for (const [surface, lemma, target, candidates] of links) {
      const startIndex = note.front.indexOf(surface)
      const wordNote = target ? noteByFront.get(target) : undefined
      const candidateIds = candidates.flatMap((front) => {
        const candidate = noteByFront.get(front)
        return candidate ? [candidate.id] : []
      })
      const forceAmbiguous = ambiguousLinks && wordNote !== undefined
      collection.wordLinks.push({
        id: String(extraSeq++),
        sentenceNoteId: note.id,
        surface,
        startIndex,
        endIndex: startIndex + surface.length,
        lemma,
        resolution: forceAmbiguous || candidateIds.length > 1 ? 'ambiguous' : wordNote ? 'resolved' : 'unresolved',
        wordNoteId: forceAmbiguous ? null : (wordNote?.id ?? null),
        candidateNoteIds: forceAmbiguous && wordNote ? [wordNote.id, ...candidateIds] : candidateIds,
      })
    }
  }

  return collection
}

export const BUNDLED_DECKS = DECK_SEEDS.filter(({ record }) => record.bundled && record.parentId === null).map(
  ({ record, notes }) => ({
    guid: record.guid,
    name: record.name,
    targetLanguage: record.targetLanguage,
    deckType: record.deckType,
    noteCount: notes.length,
    cardCount: notes.length,
    level: 'A1',
  }),
)

const LARGE_WORDS = ['Haus', 'Baum', 'Wasser', 'Stadt', 'Freund', 'Arbeit', 'Zeit', 'Schule', 'Zug', 'Buch']
const LARGE_PL = ['dom', 'drzewo', 'woda', 'miasto', 'przyjaciel', 'praca', 'czas', 'szkoła', 'pociąg', 'książka']

export const LARGE_COLLECTION_SIZE = 50_000

/** Synthetic row for the large-collection scenario; generated on demand, never materialized wholesale. */
export function largeRow(index: number): { front: string; back: string; state: CardState; dueOffsetDays: number } {
  const word = LARGE_WORDS[index % LARGE_WORDS.length] ?? 'Wort'
  const pl = LARGE_PL[index % LARGE_PL.length] ?? 'słowo'
  const states: CardState[] = ['New', 'Learning', 'Review', 'Relearning']
  return {
    front: `${word} #${index + 1}`,
    back: `${pl} #${index + 1}`,
    state: states[index % states.length] ?? 'New',
    dueOffsetDays: (index * 7) % 90,
  }
}
