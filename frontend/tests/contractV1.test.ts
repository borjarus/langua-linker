import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import type { InvokeFn, ListenFn } from '../src/api/tauriAppApi'
import { createTauriAppApi } from '../src/api/tauriAppApi'
import type { CollectionChangedEventDto, JobEventDto } from '../src/api/types'
import { MockAppApi } from '../src/demo/mockAppApi'
import { catalogue, recordContractSamples, replaySamples, settleJob, shapeProblems, uncovered, type ContractSamples } from './contract/samples'
import { assertTransportRules } from './contract/transport'

// Contract v1 (plan Phase 6, steps 4–5). `UPDATE_CONTRACT=1 npx vitest run tests/contractV1.test.ts`
// rewrites the frozen files — only together with an API_VERSION bump (docs/contracts/README.md).

// Vitest runs from `frontend/`; jsdom gives `import.meta.url` no file scheme.
const DIR = resolve(process.cwd(), '../crates/contracts/v1')
const UPDATE = process.env.UPDATE_CONTRACT === '1'
const BUMP = 'The v1 contract changed: bump API_VERSION by the versioning rules and regenerate with UPDATE_CONTRACT=1.'

function frozen<T>(name: string, current: T): T {
  const file = resolve(DIR, name)
  if (UPDATE || !existsSync(file)) {
    mkdirSync(DIR, { recursive: true })
    writeFileSync(file, `${JSON.stringify(current, null, 2)}\n`)
  }
  return JSON.parse(readFileSync(file, 'utf8')) as T
}

const recorded = recordContractSamples(new MockAppApi())

describe('frozen contract v1', () => {
  it('keeps the operation catalogue', () => {
    const current = catalogue()
    expect(current, BUMP).toEqual(frozen('catalogue.json', current))
  })

  it('has a sample for every callable operation, each following the transport rules', async () => {
    const samples = frozen('samples.json', await recorded)
    expect(uncovered(samples)).toEqual([])
    for (const sample of samples.calls) {
      assertTransportRules(sample.args, `${sample.operation}.args`)
      assertTransportRules(sample.output ?? sample.error, sample.operation)
    }
    assertTransportRules(samples.events)
  })

  it('records the same calls and payload shapes as the frozen samples', async () => {
    const current = await recorded
    const samples = frozen('samples.json', current)
    expect(current.apiVersion, BUMP).toBe(samples.apiVersion)
    expect(current.calls.map((call) => call.operation), BUMP).toEqual(samples.calls.map((call) => call.operation))
    const problems = current.calls.flatMap((call, index) => {
      const sample = samples.calls[index]!
      return [
        ...shapeProblems(call.args, sample.args, `#${index} ${call.operation}.args`),
        ...shapeProblems(call.output ?? call.error, sample.output ?? sample.error, `#${index} ${call.operation}`),
      ]
    })
    problems.push(...shapeProblems(current.events, samples.events, 'events'))
    expect(problems, BUMP).toEqual([])
  })
})

/** A stand-in for the Rust commands: answers each call with the next frozen sample, in order. */
function sampleBackend(samples: ContractSamples) {
  let next = 0
  const mismatches: string[] = []
  const invoke: InvokeFn = async (command, args) => {
    const sample = samples.calls[next++]
    if (!sample) throw new Error(`unexpected command ${command}`)
    const expected = sample.args.length === 0 ? undefined : { input: sample.args[0] }
    if (command !== sample.command) mismatches.push(`#${next - 1}: ${command}, expected ${sample.command}`)
    else if (JSON.stringify(args) !== JSON.stringify(expected)) mismatches.push(`#${next - 1} ${command}: input differs`)
    if (sample.error) throw sample.error
    return sample.output
  }
  return { invoke, mismatches }
}

describe('adapter conformance (both adapters, same samples)', () => {
  const samples = (): ContractSamples => JSON.parse(readFileSync(resolve(DIR, 'samples.json'), 'utf8')) as ContractSamples

  it('MockAppApi', async () => {
    const mock = new MockAppApi()
    expect(await replaySamples(mock, samples(), (jobId) => settleJob(mock, jobId))).toEqual([])
  })

  it('TauriAppApi over the frozen command payloads', async () => {
    const frozenSamples = samples()
    const backend = sampleBackend(frozenSamples)
    const api = createTauriAppApi(backend.invoke, async () => () => {})
    expect(await replaySamples(api, frozenSamples)).toEqual([])
    expect(backend.mismatches).toEqual([])
  })

  it('TauriAppApi delivers the frozen events unchanged', async () => {
    const { events } = samples()
    const listen: ListenFn = async (event, handler) => {
      queueMicrotask(() => handler({ payload: events[event as keyof typeof events] as never }))
      return () => {}
    }
    const api = createTauriAppApi(async () => null, listen)
    const received: { job?: JobEventDto; collection?: CollectionChangedEventDto } = {}
    const stops = [api.onJobEvent((event) => (received.job = event)), api.onCollectionChanged((event) => (received.collection = event))]
    await new Promise((resolve) => setTimeout(resolve, 0))
    stops.forEach((stop) => stop())
    expect(received).toEqual({ job: events.job_progress, collection: events.collection_changed })
  })
})
