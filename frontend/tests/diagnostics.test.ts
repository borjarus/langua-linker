import { screen, waitFor } from '@testing-library/svelte'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type * as NativeDiagnostics from '../src/api/nativeDiagnostics'
import type { FileProbe, SecretStoreProbe } from '../src/api/nativeDiagnostics'
import DiagnosticsScreen from '../src/features/diagnostics/DiagnosticsScreen.svelte'
import { createTestServices, renderWith } from './helpers/services'

const native = vi.hoisted(() => ({
  available: true,
  probe: vi.fn(),
  fileOpen: vi.fn(),
  fileSave: vi.fn(),
  listen: vi.fn(),
  send: vi.fn(),
}))

vi.mock('../src/api/nativeDiagnostics', async (importOriginal) => ({
  ...(await importOriginal<typeof NativeDiagnostics>()),
  nativeDiagnosticsAvailable: () => native.available,
  probeSecretStore: native.probe,
  probeFileOpen: native.fileOpen,
  probeFileSave: native.fileSave,
  listenToNotifications: native.listen,
  sendProbeNotification: native.send,
}))

const actual = () => vi.importActual<typeof NativeDiagnostics>('../src/api/nativeDiagnostics')

const report = (ok: boolean, persisted = false): SecretStoreProbe => ({
  backend: 'Android Keystore',
  persistedFromPreviousRun: persisted,
  steps: [
    { name: 'write', ok: true, error: null },
    { name: 'readBack', ok, error: ok ? null : 'secret store failure: unexpected value' },
  ],
})

const fileReport: FileProbe = {
  cancelled: false,
  location: 'content',
  bytes: 2048,
  kind: 'zip',
  steps: [
    { name: 'pick', ok: true, error: null },
    { name: 'openAndRead', ok: true, error: null },
  ],
}

async function renderScreen() {
  renderWith(DiagnosticsScreen, await createTestServices({ language: 'en' }))
}

describe('native diagnostics screen', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
    native.available = true
    for (const mock of [native.probe, native.fileOpen, native.fileSave, native.listen, native.send]) mock.mockReset()
    native.listen.mockResolvedValue(() => undefined)
  })

  it('runs the secret store probe and lists every step', async () => {
    native.probe.mockResolvedValue(report(true, true))
    await renderScreen()
    await userEvent.click(screen.getByRole('button', { name: 'Run test' }))
    expect(await screen.findByText('All steps passed')).toBeInTheDocument()
    expect(screen.getByText('Android Keystore')).toBeInTheDocument()
    expect(screen.getByText('Yes')).toBeInTheDocument()
    expect(screen.getByText('readBack')).toBeInTheDocument()
  })

  it('reports failing steps with their error', async () => {
    native.probe.mockResolvedValue(report(false))
    await renderScreen()
    await userEvent.click(screen.getByRole('button', { name: 'Run test' }))
    expect(await screen.findByText('Some steps failed')).toBeInTheDocument()
    expect(screen.getByText('secret store failure: unexpected value')).toBeInTheDocument()
    expect(screen.getByText('No (expected on the first run)')).toBeInTheDocument()
  })

  it('shows an error when the command is rejected and keeps the probe available', async () => {
    native.probe.mockRejectedValue('Command probe_secret_store not allowed by ACL')
    await renderScreen()
    await userEvent.click(screen.getByRole('button', { name: 'Run test' }))
    expect(await screen.findByText('An unexpected error occurred.')).toBeInTheDocument()
    await waitFor(() => expect(screen.getByRole('button', { name: 'Run test' })).toBeEnabled())
  })

  it('does not offer native probes outside the native app', async () => {
    native.available = false
    await renderScreen()
    expect(screen.getAllByText(/Available only in the native app/)).toHaveLength(3)
    expect(screen.queryByRole('button', { name: 'Run test' })).toBeNull()
    expect(screen.queryByRole('button', { name: 'Open CSV/APKG' })).toBeNull()
    expect(native.listen).not.toHaveBeenCalled()
  })

  it('opens a filtered file and shows what Rust read', async () => {
    native.fileOpen.mockResolvedValue(fileReport)
    await renderScreen()
    await userEvent.click(screen.getByRole('button', { name: 'Open CSV/APKG' }))
    expect(await screen.findByText('2,048 B')).toBeInTheDocument()
    expect(native.fileOpen).toHaveBeenCalledWith(true)
    expect(screen.getByText('content')).toBeInTheDocument()
    expect(screen.getByText('zip')).toBeInTheDocument()
    expect(screen.getByText('openAndRead')).toBeInTheDocument()
  })

  it('reports a cancelled save picker and a rejected command', async () => {
    native.fileSave.mockResolvedValue({ cancelled: true, location: null, bytes: null, kind: null, steps: [] })
    await renderScreen()
    await userEvent.click(screen.getByRole('button', { name: 'Save test backup' }))
    expect(await screen.findByText('The picker was closed without a file.')).toBeInTheDocument()
    native.fileOpen.mockRejectedValue('failed to open file: permission denied')
    await userEvent.click(screen.getByRole('button', { name: 'Open any file' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('permission denied')
    expect(native.fileOpen).toHaveBeenCalledWith(false)
  })

  it('logs notification taps and failed sends', async () => {
    let emit: (event: NativeDiagnostics.NotificationEvent) => void = () => undefined
    native.listen.mockImplementation(async (onEvent: typeof emit) => {
      emit = onEvent
      return () => undefined
    })
    native.send.mockRejectedValue('Command plugin:notification|notify not allowed by ACL')
    await renderScreen()
    await waitFor(() => expect(native.listen).toHaveBeenCalled())
    emit({ kind: 'tapped', id: 7002, extra: '{"probe":"scheduled"}' })
    expect(await screen.findByText('Tapped: id 7002, extra {"probe":"scheduled"}')).toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: 'Send now' }))
    expect(await screen.findByText(/notify failed: .*not allowed/)).toBeInTheDocument()
  })

  it('says when the WebView has no speech synthesis or Web Share API', async () => {
    await renderScreen()
    expect(await screen.findByText(/no speech synthesis/)).toBeInTheDocument()
    expect(screen.getByText(/no Web Share API/)).toBeInTheDocument()
  })
})

describe('native diagnostics API', () => {
  it('invokes the dev-only commands with their arguments', async () => {
    const api = await actual()
    const invoke = vi.fn().mockResolvedValue(report(true))
    await expect(api.probeSecretStore(invoke)).resolves.toEqual(report(true))
    expect(invoke).toHaveBeenCalledWith('probe_secret_store')
    invoke.mockResolvedValue(fileReport)
    await api.probeFileOpen(false, invoke)
    expect(invoke).toHaveBeenLastCalledWith('probe_file_open', { filtered: false })
    await api.probeFileSave(invoke)
    expect(invoke).toHaveBeenLastCalledWith('probe_file_save')
  })

  it('schedules the probe notification and checks the pending list', async () => {
    const api = await actual()
    const id = api.PROBE_NOTIFICATION_IDS.scheduled
    const invoke = vi.fn(async (command: string) => (command === 'plugin:notification|get_pending' ? [{ id }] : null))
    const pending = await api.scheduleProbeNotification({ title: 't', body: 'b' }, 10, invoke, () => Date.UTC(2026, 9, 14, 10))
    expect(pending).toBe(true)
    expect(invoke.mock.calls.map(([command]) => command)).toEqual([
      'plugin:notification|cancel',
      'plugin:notification|notify',
      'plugin:notification|get_pending',
    ])
    const options = (invoke.mock.calls[1] as unknown as [string, { options: { schedule: { at: { date: Date } } } }])[1].options
    expect(options.schedule.at.date.toISOString()).toBe('2026-10-14T10:00:10.000Z')
  })

  it('reads ids and extras from tap and delivery payloads', async () => {
    const { notificationEvent } = await actual()
    expect(notificationEvent('tapped', { actionId: 'tap', notification: { id: 7002, extra: { probe: 'scheduled' } } })).toEqual({
      kind: 'tapped',
      id: 7002,
      extra: '{"probe":"scheduled"}',
    })
    expect(notificationEvent('received', { id: 7001 })).toEqual({ kind: 'received', id: 7001, extra: 'null' })
    expect(notificationEvent('tapped', null)).toEqual({ kind: 'tapped', id: null, extra: 'null' })
  })

  it('lists voices per language, waiting for them to load', async () => {
    const { speechSupport, voicesFor } = await actual()
    const voices = [
      { lang: 'de_DE', name: 'Anna', localService: true },
      { lang: 'en-US', name: 'Samantha', localService: false },
    ]
    let loaded = false
    const listeners: Array<() => void> = []
    const synth = {
      getVoices: () => (loaded ? voices : []),
      addEventListener: (_: string, listener: () => void) => listeners.push(listener),
      removeEventListener: vi.fn(),
    } as unknown as SpeechSynthesis
    const support = speechSupport({ speechSynthesis: synth }, 5000)
    loaded = true
    for (const listener of listeners) listener()
    const result = await support
    expect(result.available).toBe(true)
    expect(voicesFor(result.voices, 'de-DE')).toEqual([{ lang: 'de_DE', name: 'Anna', local: true }])
    expect(voicesFor(result.voices, 'en-us')).toHaveLength(1)
    await expect(speechSupport({})).resolves.toEqual({ available: false, voices: [] })
  })

  it('detects Web Share support and reports share outcomes', async () => {
    const { shareProbe, shareSupport } = await actual()
    expect(shareSupport({})).toEqual({ text: false, files: false })
    const share = vi.fn().mockResolvedValue(undefined)
    expect(shareSupport({ share, canShare: () => true })).toEqual({ text: true, files: true })
    expect(shareSupport({ share })).toEqual({ text: true, files: false })
    await expect(shareProbe({ text: 'x' }, { share })).resolves.toBe('shared')
    share.mockRejectedValue(new DOMException('closed', 'AbortError'))
    await expect(shareProbe({ text: 'x' }, { share })).resolves.toBe('AbortError')
    await expect(shareProbe({ text: 'x' }, {})).resolves.toBe('unavailable')
  })
})
