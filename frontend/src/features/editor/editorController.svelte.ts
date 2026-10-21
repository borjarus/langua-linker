import type { AppApi } from '../../api/appApi'
import { toErrorDto } from '../../api/errors'
import type {
  CardDirection,
  DeckCapabilitiesDto,
  DuplicateCheckDto,
  EditorDataDto,
  EditorDeckOptionDto,
  ErrorDto,
  Id,
  NoteDetailsDto,
  NoteTypeKind,
  WordLinkDraftDto,
  WordMatchDto,
} from '../../api/types'
import { newOperationId } from '../../services/operationId'

export type FieldProblem = 'required' | 'tooLong'
/** What saving does to a card template of an existing note. */
export type TemplateStatus = 'new' | 'kept' | 'restored' | 'archived'

export interface TemplatePreview {
  ordinal: number
  direction: CardDirection
  question: string
  answer: string
  status: TemplateStatus
}

export interface EditorLink extends WordLinkDraftDto {
  /** Stable identity in the form while ranges move with edits. */
  key: number
  lemma: string | null
  translation: string | null
}

/** `moved`: the words moved after an edit and the link follows them; `lost`: the words are gone. */
export interface LinkState {
  link: EditorLink
  status: 'ok' | 'moved' | 'lost'
}

export interface EditorControllerDeps {
  api: AppApi
  operationId?: () => string
}

const NO_CAPABILITIES: DeckCapabilitiesDto = { associations: false, grammarTips: false, wordLinks: false, tts: false }

/**
 * Note editor state. Validity and dirtiness are derived; nothing is saved from an effect —
 * `save()` runs only from the submit handler, with one operationId per save intent.
 */
export class EditorController {
  data = $state<EditorDataDto | null>(null)
  /** The stored note when editing; `null` for a new note. */
  details = $state<NoteDetailsDto | null>(null)
  deckId = $state('')
  noteTypeKind = $state<NoteTypeKind>('Basic')
  front = $state('')
  back = $state('')
  tags = $state<string[]>([])
  manualAssociation = $state('')
  newTips = $state<string[]>([])
  links = $state<EditorLink[]>([])
  generateAfterSave = $state(false)
  /** Set by the first save attempt; field errors show from then on. */
  touched = $state(false)
  saving = $state(false)
  error = $state<ErrorDto | null>(null)
  /** The note changed elsewhere since it was opened. */
  conflict = $state<{ currentVersion: number | null } | null>(null)
  duplicates = $state<DuplicateCheckDto['duplicates']>([])

  readonly #api: AppApi
  readonly #operationId: () => string
  #pendingOperationId: string
  #expectedVersion = $state<number | null>(null)
  #snapshot = $state('')
  #duplicateRun = 0
  #linkKey = 1

  constructor({ api, operationId = newOperationId }: EditorControllerDeps) {
    this.#api = api
    this.#operationId = operationId
    this.#pendingOperationId = operationId()
  }

  readonly isNew = $derived(this.details === null)
  readonly deck = $derived<EditorDeckOptionDto | null>(this.data?.decks.find((deck) => deck.id === this.deckId) ?? null)
  readonly capabilities = $derived(this.deck?.capabilities ?? NO_CAPABILITIES)
  readonly maxLength = $derived(this.data?.maxFieldLength ?? 2000)
  /** Linking words applies to sentences in decks that show word links. */
  readonly linksEnabled = $derived(this.noteTypeKind === 'Sentence' && this.capabilities.wordLinks)

  readonly problems = $derived.by(() => {
    const check = (value: string): FieldProblem | null => (value.trim() === '' ? 'required' : value.trim().length > this.maxLength ? 'tooLong' : null)
    return { front: check(this.front), back: check(this.back), deck: this.deck ? null : ('required' as FieldProblem | null) }
  })
  readonly valid = $derived(this.problems.front === null && this.problems.back === null && this.problems.deck === null)
  readonly dirty = $derived(
    this.#serialize() !== this.#snapshot || this.manualAssociation.trim() !== '' || this.newTips.some((tip) => tip.trim() !== ''),
  )

  readonly linkStates = $derived.by((): LinkState[] => {
    const taken: [number, number][] = []
    const free = (start: number, end: number) => taken.every(([s, e]) => end <= s || start >= e)
    return [...this.links]
      .sort((a, b) => a.startIndex - b.startIndex)
      .map((link) => {
        if (this.front.slice(link.startIndex, link.endIndex) === link.surface && free(link.startIndex, link.endIndex)) {
          taken.push([link.startIndex, link.endIndex])
          return { link, status: 'ok' as const }
        }
        let from = this.front.indexOf(link.surface)
        while (from !== -1 && !free(from, from + link.surface.length)) from = this.front.indexOf(link.surface, from + 1)
        if (from === -1) return { link, status: 'lost' as const }
        taken.push([from, from + link.surface.length])
        return { link: { ...link, startIndex: from, endIndex: from + link.surface.length }, status: 'moved' as const }
      })
  })

  readonly templates = $derived.by((): TemplatePreview[] => {
    const type = this.data?.noteTypes.find((candidate) => candidate.kind === this.noteTypeKind)
    if (!type) return []
    const preview = (ordinal: number, direction: CardDirection, status: TemplateStatus): TemplatePreview => ({
      ordinal,
      direction,
      question: direction === 'forward' ? this.front : this.back,
      answer: direction === 'forward' ? this.back : this.front,
      status,
    })
    const live = new Set(this.details?.cards.map((card) => card.ordinal) ?? [])
    const archived = new Set(this.details?.archivedOrdinals ?? [])
    const wanted = type.templates.map((template) => {
      const status: TemplateStatus = this.isNew ? 'new' : live.has(template.ordinal) ? 'kept' : archived.has(template.ordinal) ? 'restored' : 'new'
      return preview(template.ordinal, template.direction, status)
    })
    const removed = (this.details?.cards ?? [])
      .filter((card) => !type.templates.some((template) => template.ordinal === card.ordinal))
      .map((card) => preview(card.ordinal, card.direction, 'archived'))
    return [...wanted, ...removed]
  })

  /** Loads the form; `details` is the stored note when editing. */
  init(data: EditorDataDto, details: NoteDetailsDto | null, defaultDeckId: Id | null): void {
    this.data = data
    this.details = details
    this.error = null
    this.conflict = null
    this.touched = false
    this.manualAssociation = ''
    this.newTips = []
    this.generateAfterSave = false
    if (details) {
      this.deckId = details.cards[0]?.deckId ?? details.deckId
      this.noteTypeKind = details.note.noteTypeKind
      this.front = details.note.front
      this.back = details.note.back
      this.tags = [...details.note.tags]
      this.links = details.wordLinks.map((link) => ({
        key: this.#linkKey++,
        startIndex: link.startIndex,
        endIndex: link.endIndex,
        surface: link.surface,
        wordNoteId: link.wordNoteId,
        lemma: link.lemma,
        translation: link.translation,
      }))
      this.#expectedVersion = details.note.version
    } else {
      this.deckId = data.decks.some((deck) => deck.id === defaultDeckId) ? (defaultDeckId as Id) : (data.decks[0]?.id ?? '')
      this.noteTypeKind = 'Basic'
      this.front = ''
      this.back = ''
      this.tags = []
      this.links = []
      this.#expectedVersion = null
    }
    this.#snapshot = this.#serialize()
    this.#pendingOperationId = this.#operationId()
  }

  /** After "save and add next": keeps deck, type and tags, clears the content. */
  resetForNext(): void {
    this.details = null
    this.front = ''
    this.back = ''
    this.manualAssociation = ''
    this.newTips = []
    this.links = []
    this.generateAfterSave = false
    this.touched = false
    this.error = null
    this.duplicates = []
    this.#expectedVersion = null
    this.#snapshot = this.#serialize()
  }

  addTag(raw: string): boolean {
    const tag = raw.normalize('NFC').trim().replace(/\s+/gu, '-')
    if (!tag || this.tags.includes(tag)) return false
    this.tags = [...this.tags, tag]
    return true
  }

  removeTag(tag: string): void {
    this.tags = this.tags.filter((candidate) => candidate !== tag)
  }

  /** Marks the selected range of the front; returns `false` for an empty or overlapping selection. */
  addLink(start: number, end: number, match: WordMatchDto | null): boolean {
    let from = Math.max(0, Math.min(start, end))
    let to = Math.min(this.front.length, Math.max(start, end))
    while (from < to && /\s/u.test(this.front[from] ?? '')) from += 1
    while (to > from && /\s/u.test(this.front[to - 1] ?? '')) to -= 1
    if (from === to) return false
    const placed = this.linkStates.filter((state) => state.status !== 'lost').map((state) => state.link)
    if (placed.some((link) => from < link.endIndex && to > link.startIndex)) return false
    const link: EditorLink = {
      key: this.#linkKey++,
      startIndex: from,
      endIndex: to,
      surface: this.front.slice(from, to),
      wordNoteId: match?.noteId ?? null,
      lemma: match?.lemma ?? null,
      translation: match?.translation ?? null,
    }
    // Moved links adopt their new ranges; lost ones stay listed so the warning remains visible.
    this.links = [...this.linkStates.map((state) => state.link), link].sort((a, b) => a.startIndex - b.startIndex)
    return true
  }

  removeLink(key: number): void {
    this.links = this.links.filter((link) => link.key !== key)
  }

  /** A read: warns about notes with the same front, never blocks a valid homograph. */
  async checkDuplicates(): Promise<void> {
    const run = ++this.#duplicateRun
    if (!this.front.trim() || !this.deck) {
      this.duplicates = []
      return
    }
    try {
      const result = await this.#api.checkDuplicate({ deckId: this.deckId, front: this.front, excludeNoteId: this.details?.note.id ?? null })
      if (run === this.#duplicateRun) this.duplicates = result.duplicates
    } catch {
      // The warning is advisory; a failed check must not block editing.
      if (run === this.#duplicateRun) this.duplicates = []
    }
  }

  async save(): Promise<NoteDetailsDto | null> {
    this.touched = true
    if (!this.valid || this.saving) return null
    this.saving = true
    this.error = null
    const caps = this.capabilities
    // The backend stores trimmed NFC text; link ranges shift with the trimmed leading whitespace.
    const front = this.front.normalize('NFC')
    const lead = front.length - front.trimStart().length
    try {
      const saved = await this.#api.saveNote({
        operationId: this.#pendingOperationId,
        noteId: this.details?.note.id ?? null,
        expectedVersion: this.#expectedVersion,
        deckId: this.deckId,
        noteTypeKind: this.noteTypeKind,
        front: front.trim(),
        back: this.back.trim(),
        tags: this.tags,
        manualAssociation: caps.associations && this.manualAssociation.trim() ? this.manualAssociation.trim() : null,
        grammarTips: caps.grammarTips ? this.newTips.map((tip) => tip.trim()).filter(Boolean) : [],
        wordLinks: this.linksEnabled
          ? this.linkStates
              .filter((state) => state.status !== 'lost')
              .map(({ link }) => ({ startIndex: link.startIndex - lead, endIndex: link.endIndex - lead, surface: link.surface, wordNoteId: link.wordNoteId }))
          : this.isNew ? [] : null,
      })
      this.#pendingOperationId = this.#operationId()
      this.details = saved
      this.#expectedVersion = saved.note.version
      this.manualAssociation = ''
      this.newTips = []
      this.#snapshot = this.#serialize()
      return saved
    } catch (caught) {
      const error = toErrorDto(caught)
      if (error.code === 'VERSION_CONFLICT') {
        const current = error.details?.currentVersion
        this.conflict = { currentVersion: typeof current === 'number' ? current : null }
      } else {
        this.error = error
      }
      // A retryable failure resends the same intent; anything else needs a corrected request.
      if (!error.retryable) this.#pendingOperationId = this.#operationId()
      return null
    } finally {
      this.saving = false
    }
  }

  /**
   * Resolves a version conflict with the stored note: `keep` keeps the draft and saves over the
   * latest version on the next save; otherwise the draft is replaced by the latest version.
   */
  async resolveConflict(keep: boolean): Promise<ErrorDto | null> {
    const cardId = this.details?.cards[0]?.id
    if (!cardId || !this.data) return null
    try {
      const latest = await this.#api.getNoteDetails(cardId)
      if (keep) {
        this.details = latest
        this.#expectedVersion = latest.note.version
      } else {
        this.init(this.data, latest, null)
      }
      this.conflict = null
      return null
    } catch (caught) {
      return toErrorDto(caught)
    }
  }

  #serialize(): string {
    const links = this.links.map((link) => [link.startIndex, link.endIndex, link.wordNoteId])
    return JSON.stringify([this.deckId, this.noteTypeKind, this.front, this.back, this.tags, links])
  }
}
