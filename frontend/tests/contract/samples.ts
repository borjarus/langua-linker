import type { AppApi, AppApiOperation } from '../../src/api/appApi'
import { isErrorDto, toErrorDto } from '../../src/api/errors'
import { OPERATIONS, OPERATION_NAMES, type OperationSpec } from '../../src/api/operations'
import { API_VERSION, ERROR_CODES, type CollectionChangedEventDto, type ErrorDto, type JobDto, type JobEventDto } from '../../src/api/types'

// Contract v1 freeze (plan Phase 6, steps 4–5). The frozen files live with the Rust crate that
// becomes the DTO source in Phase 7: `crates/contracts/v1/catalogue.json` and `samples.json`.
// Both adapters replay the same samples; the Rust tests check the same files.

export type CallableOperation = {
  [K in AppApiOperation]: (typeof OPERATIONS)[K]['kind'] extends 'subscription' ? never : K
}[AppApiOperation]

/** One recorded call: `args` is `[]` or `[input]`; exactly one of `output` / `error` is set. */
export interface ContractSample {
  operation: CallableOperation
  command: string
  args: unknown[]
  output?: unknown
  error?: ErrorDto
}

export interface ContractSamples {
  apiVersion: string
  calls: ContractSample[]
  events: { job_progress: JobEventDto; collection_changed: CollectionChangedEventDto }
}

export interface CatalogueEntry {
  kind: OperationSpec['kind']
  command: string
  errors: readonly string[]
  emits: readonly string[]
  permission: OperationSpec['permission']
}

/**
 * What v1 freezes: names, kinds, commands, declared errors, events and permissions. Screens and
 * backend phases are planning data, not contract, so changing them needs no version bump.
 */
export function catalogue(): { apiVersion: string; errorCodes: readonly string[]; operations: Record<string, CatalogueEntry> } {
  const operations: Record<string, CatalogueEntry> = {}
  for (const name of OPERATION_NAMES) {
    const { kind, command, errors, emits, permission } = OPERATIONS[name]
    operations[name] = { kind, command, errors, emits, permission }
  }
  return { apiVersion: API_VERSION, errorCodes: ERROR_CODES, operations }
}

const TERMINAL: ReadonlySet<JobDto['status']> = new Set(['succeeded', 'failed', 'cancelled'])

/** Waits for a mock job to finish by polling `getJobStatus` on the unrecorded adapter. */
export async function settleJob(api: AppApi, jobId: string): Promise<void> {
  for (let attempt = 0; attempt < 500; attempt += 1) {
    if (TERMINAL.has((await api.getJobStatus(jobId)).status)) return
    await new Promise((resolve) => setTimeout(resolve, 0))
  }
  throw new Error(`job ${jobId} did not finish`)
}

/** Proxies an adapter and records every operation call with its input and result. */
function recorder(api: AppApi) {
  const calls: ContractSample[] = []
  const proxy = new Proxy(api, {
    get(target, property) {
      const value = Reflect.get(target, property, target) as unknown
      if (typeof value !== 'function' || typeof property !== 'string' || !(property in OPERATIONS)) return value
      const operation = property as AppApiOperation
      if (OPERATIONS[operation].kind === 'subscription') return (value as (...args: unknown[]) => unknown).bind(target)
      return async (...args: unknown[]) => {
        const sample: ContractSample = { operation: operation as CallableOperation, command: OPERATIONS[operation].command, args: clone(args) }
        calls.push(sample)
        try {
          const output = await (value as (...args: unknown[]) => Promise<unknown>).apply(target, args)
          sample.output = clone(output)
          return output
        } catch (caught) {
          sample.error = toErrorDto(caught)
          throw caught
        }
      }
    },
  })
  return { api: proxy as AppApi, calls }
}

const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T

function required<T>(value: T | null | undefined, what: string): T {
  if (value === null || value === undefined) throw new Error(`contract script: ${what} missing in the demo data`)
  return value
}

/**
 * Calls every operation once or more on the `happy` demo collection, in an order that keeps the
 * collection usable (destructive operations last), and records the transport payloads.
 */
export async function recordContractSamples(raw: AppApi): Promise<ContractSamples> {
  const { api, calls } = recorder(raw)
  const events: { job?: JobEventDto; collection?: CollectionChangedEventDto } = {}
  const stopJobs = raw.onJobEvent((event) => (events.job ??= clone(event)))
  const stopCollection = raw.onCollectionChanged((event) => (events.collection ??= clone(event)))
  let serial = 0
  const operationId = () => `contract-${++serial}`
  const finish = async (job: JobDto) => {
    await settleJob(raw, job.jobId)
    return api.getJobStatus(job.jobId)
  }

  // app
  await api.getAppInfo()
  await api.getAttributions()
  await api.getAboutInfo()
  await api.getDiagnosticsReport()

  // dashboard / decks / presets
  await api.getDashboard()
  await api.getDeck('1')
  await api.getDeck('999999').catch(() => null)
  const presets = await api.listPresets()
  const preset = required(presets[0], 'preset')
  await api.savePreset({ operationId: operationId(), presetId: null, expectedVersion: null, name: 'Kontrakt', config: preset.config })
  const created = await api.createDeck({ operationId: operationId(), name: 'Kontrakt', parentId: null, targetLanguage: 'de', deckType: 'Simple' })
  const renamed = await api.updateDeck({ operationId: operationId(), deckId: created.deck.id, expectedVersion: created.deck.version, patch: { name: 'Kontrakt v1' } })
  await api.deleteDeck({ operationId: operationId(), deckId: renamed.deck.id, expectedVersion: renamed.deck.version })

  // study
  const session = await api.startStudy({ operationId: operationId(), deckId: '1' })
  const card = required(session.current, 'due card')
  await api.resumeStudy(session.sessionId)
  const preview = await api.getRatingPreview({ sessionId: session.sessionId, cardId: card.cardId, cardVersion: card.version })
  await api.rateCard({ operationId: operationId(), sessionId: session.sessionId, cardId: card.cardId, cardVersion: card.version, previewToken: preview.previewToken, rating: 'Good', answerMs: 4200 })
  await api.undoReview({ operationId: operationId(), sessionId: session.sessionId })
  await api.updateStudySession({ operationId: operationId(), sessionId: session.sessionId, action: 'pause' })
  const resumed = await api.updateStudySession({ operationId: operationId(), sessionId: session.sessionId, action: 'resume' })
  const again = required(resumed.current, 'card after resume')
  const againPreview = await api.getRatingPreview({ sessionId: session.sessionId, cardId: again.cardId, cardVersion: again.version })
  await api.rateCard({ operationId: operationId(), sessionId: session.sessionId, cardId: again.cardId, cardVersion: again.version, previewToken: againPreview.previewToken, rating: 'Again', answerMs: 9100 })
  await api.getNoteContent(card.noteId)
  await api.updateStudySession({ operationId: operationId(), sessionId: session.sessionId, action: 'end' })
  await api.getSessionSummary(session.sessionId)

  // details / editor
  const details = await api.getNoteDetails(card.cardId)
  const detailCard = required(details.cards[0], 'card details')
  await api.applyCardAction({ operationId: operationId(), cardId: detailCard.id, expectedVersion: detailCard.version, action: 'flag', flag: 'red' })
  const association = details.associations.find((item) => !item.favorite && item.status === 'ready') ?? details.associations[0]
  if (association) await api.setFavoriteAssociation({ operationId: operationId(), noteId: details.note.id, associationId: association.id })
  await api.getEditorData()
  await api.checkDuplicate({ deckId: '1', front: details.note.front, excludeNoteId: null })
  const saved = await api.saveNote({
    operationId: operationId(),
    noteId: null,
    expectedVersion: null,
    deckId: '1',
    noteTypeKind: 'Basic',
    front: 'der Vertrag',
    back: 'umowa',
    tags: ['kontrakt'],
    manualAssociation: 'Wer trägt den Vertrag? — wór z umową',
    grammarTips: ['der Vertrag, die Verträge'],
    wordLinks: null,
  })
  await api.updateNoteContent({ operationId: operationId(), noteId: saved.note.id, change: { type: 'addTip', text: 'vertragen — znosić' } })
  await api.searchWords({ targetLanguage: 'de', text: 'Tee', limit: 5 })
  const generation = await api.generateAssociation({ operationId: operationId(), noteId: saved.note.id, variants: 1 })
  await finish(generation)
  await api.deleteNote({ operationId: operationId(), noteId: saved.note.id, expectedVersion: saved.note.version })

  // A sentence with a word link, found without recording the search.
  const sentences = await raw.searchCards({ query: '', deckId: null, tag: null, states: [], flags: [], sort: 'due', direction: 'asc', cursor: null, pageSize: 200 })
  let linked: { linkId: string; wordNoteId: string; cardId: string } | null = null
  for (const row of sentences.items) {
    const note = await raw.getNoteDetails(row.cardId)
    const link = note.wordLinks.find((item) => item.wordNoteId ?? item.candidates[0])
    if (link) {
      linked = { linkId: link.id, wordNoteId: required(link.wordNoteId ?? link.candidates[0]?.noteId, 'word note'), cardId: row.cardId }
      break
    }
  }
  const wordLink = required(linked, 'sentence with a word link')
  await api.getNoteDetails(wordLink.cardId)
  await api.resolveWordLink({ operationId: operationId(), linkId: wordLink.linkId, wordNoteId: wordLink.wordNoteId })

  // browser
  const page = await api.searchCards({ query: '', deckId: '1', tag: null, states: [], flags: [], sort: 'due', direction: 'asc', cursor: null, pageSize: 5 })
  const row = required(page.items[0], 'browser row')
  const bulk = { selection: { kind: 'ids' as const, cardIds: [row.cardId] }, action: { type: 'addTag' as const, tag: 'kontrakt' } }
  await api.previewBulkAction(bulk)
  await api.applyBulkAction({ operationId: operationId(), ...bulk })

  // statistics / settings / onboarding
  await api.getStats({ deckId: '1', range: '1m' })
  const settings = await api.getSettings()
  await api.updateSettings({ operationId: operationId(), expectedVersion: settings.version, patch: { dailyGoal: 25 } })
  await api.setAiApiKey({ operationId: operationId(), apiKey: 'contract-test-key' })
  await api.requestNotificationPermission({ operationId: operationId() })
  const onboarding = await api.getOnboardingState()
  await finish(await api.completeOnboarding({ operationId: operationId(), nativeLanguage: 'pl', targetLanguages: onboarding.targetLanguages, dailyGoal: 25, deckGuids: [] }))

  // transfer
  const file = required(await api.pickFile({ purpose: 'import' }), 'import file')
  const importInput = { fileToken: file.token, delimiter: null, hasHeader: true, mapping: null, duplicatePolicy: 'skip' as const, targetDeckId: '1' }
  const cancelled = await api.commitImport({ operationId: operationId(), previewToken: (await api.previewImport(importInput)).previewToken })
  await api.cancelJob(cancelled.jobId)
  await finish(cancelled)
  const imported = await api.commitImport({ operationId: operationId(), previewToken: (await api.previewImport(importInput)).previewToken })
  await finish(imported)
  const exported = await api.exportDeck({ operationId: operationId(), deckId: '1', format: 'csv', includeScheduling: true })
  await finish(exported)
  await api.shareFile({ operationId: operationId(), target: { kind: 'export', jobId: exported.jobId } })

  // backup / restore / reset
  const backups = await api.listBackups()
  await finish(await api.createBackup({ operationId: operationId() }))
  await api.shareFile({ operationId: operationId(), target: { kind: 'backup', backupId: required(backups[0], 'backup').id } })
  await api.validateRestore({ source: { kind: 'file', fileToken: 'demo-restore-corrupt' } }).catch(() => null)
  const restore = await api.validateRestore({ source: { kind: 'backup', backupId: required(backups[0], 'backup').id } })
  await finish(await api.restoreBackup({ operationId: operationId(), previewToken: restore.previewToken }))
  const reset = await api.previewReset({ scope: 'progress' })
  await finish(await api.resetCollection({ operationId: operationId(), previewToken: reset.previewToken, safetyBackupId: null, confirmationPhrase: reset.confirmationPhrase }))

  stopJobs()
  stopCollection()
  return {
    apiVersion: API_VERSION,
    calls,
    events: { job_progress: required(events.job, 'job event'), collection_changed: required(events.collection, 'collection event') },
  }
}

/**
 * Replays frozen samples against an adapter and returns every structural difference. The same
 * function runs against the mock and the Tauri adapter; `settle` lets the mock finish its jobs.
 */
export async function replaySamples(api: AppApi, samples: ContractSamples, settle?: (jobId: string) => Promise<void>): Promise<string[]> {
  const problems: string[] = []
  for (const [index, sample] of samples.calls.entries()) {
    const label = `#${index} ${sample.operation}`
    if (sample.operation === 'getJobStatus' && settle) await settle(sample.args[0] as string)
    const method = api[sample.operation] as (...args: unknown[]) => Promise<unknown>
    try {
      const output = await method.apply(api, clone(sample.args))
      if (sample.error) problems.push(`${label}: resolved, expected ${sample.error.code}`)
      else problems.push(...shapeProblems(clone(output), sample.output, label))
    } catch (caught) {
      const dto = isErrorDto((caught as { dto?: unknown }).dto) ? (caught as { dto: ErrorDto }).dto : toErrorDto(caught)
      if (!sample.error) problems.push(`${label}: rejected with ${dto.code}`)
      else if (dto.code !== sample.error.code || dto.messageKey !== sample.error.messageKey) problems.push(`${label}: ${dto.code}, expected ${sample.error.code}`)
    }
  }
  return problems
}

const kindOf = (value: unknown): string => (value === null ? 'null' : Array.isArray(value) ? 'array' : typeof value)

/**
 * Structural compatibility with a frozen payload: same key sets, same primitive kinds; `null` is
 * compatible with anything (nullable field), and each array element must match one frozen element
 * (so discriminated unions are compared variant by variant).
 */
export function shapeProblems(actual: unknown, frozen: unknown, path = '$'): string[] {
  const [actualKind, frozenKind] = [kindOf(actual), kindOf(frozen)]
  if (actualKind === 'null' || frozenKind === 'null') return []
  if (actualKind !== frozenKind) return [`${path}: ${actualKind}, frozen ${frozenKind}`]
  if (actualKind === 'array') {
    const [items, variants] = [actual as unknown[], frozen as unknown[]]
    if (variants.length === 0) return []
    return items.flatMap((item, index) => {
      const attempts = variants.map((variant) => shapeProblems(item, variant, `${path}[${index}]`))
      return attempts.some((attempt) => attempt.length === 0) ? [] : (attempts[0] ?? [])
    })
  }
  if (actualKind !== 'object') return []
  const [left, right] = [actual as Record<string, unknown>, frozen as Record<string, unknown>]
  const problems: string[] = []
  for (const key of Object.keys(right)) if (!(key in left)) problems.push(`${path}.${key}: missing`)
  for (const key of Object.keys(left)) {
    if (!(key in right)) problems.push(`${path}.${key}: not in the frozen contract`)
    else problems.push(...shapeProblems(left[key], right[key], `${path}.${key}`))
  }
  return problems
}

/** Operations without a sample; v1 requires one for every callable operation. */
export function uncovered(samples: ContractSamples): string[] {
  const covered = new Set(samples.calls.map((sample) => sample.operation))
  return OPERATION_NAMES.filter((name) => OPERATIONS[name].kind !== 'subscription' && !covered.has(name as CallableOperation))
}
