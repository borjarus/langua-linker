import { screen, waitFor } from '@testing-library/svelte'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { probeSecretStore, type SecretStoreProbe } from '../src/api/nativeDiagnostics'
import DiagnosticsScreen from '../src/features/diagnostics/DiagnosticsScreen.svelte'
import { createTestServices, renderWith } from './helpers/services'

const native = vi.hoisted(() => ({ available: true, probe: vi.fn() }))

vi.mock('../src/api/nativeDiagnostics', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/api/nativeDiagnostics')>()),
  nativeDiagnosticsAvailable: () => native.available,
  probeSecretStore: native.probe,
}))

const report = (ok: boolean, persisted = false): SecretStoreProbe => ({
  backend: 'Android Keystore',
  persistedFromPreviousRun: persisted,
  steps: [
    { name: 'write', ok: true, error: null },
    { name: 'readBack', ok, error: ok ? null : 'secret store failure: unexpected value' },
  ],
})

describe('native diagnostics screen', () => {
  beforeEach(() => {
    document.body.innerHTML = ''
    native.available = true
    native.probe.mockReset()
  })

  it('runs the secret store probe and lists every step', async () => {
    native.probe.mockResolvedValue(report(true, true))
    renderWith(DiagnosticsScreen, await createTestServices({ language: 'en' }))
    await userEvent.click(screen.getByRole('button', { name: 'Run test' }))
    expect(await screen.findByText('All steps passed')).toBeInTheDocument()
    expect(screen.getByText('Android Keystore')).toBeInTheDocument()
    expect(screen.getByText('Yes')).toBeInTheDocument()
    expect(screen.getByText('readBack')).toBeInTheDocument()
  })

  it('reports failing steps with their error', async () => {
    native.probe.mockResolvedValue(report(false))
    renderWith(DiagnosticsScreen, await createTestServices({ language: 'en' }))
    await userEvent.click(screen.getByRole('button', { name: 'Run test' }))
    expect(await screen.findByText('Some steps failed')).toBeInTheDocument()
    expect(screen.getByText('secret store failure: unexpected value')).toBeInTheDocument()
    expect(screen.getByText('No (expected on the first run)')).toBeInTheDocument()
  })

  it('shows an error when the command is rejected and keeps the probe available', async () => {
    native.probe.mockRejectedValue('Command probe_secret_store not allowed by ACL')
    renderWith(DiagnosticsScreen, await createTestServices({ language: 'en' }))
    await userEvent.click(screen.getByRole('button', { name: 'Run test' }))
    expect(await screen.findByText('An unexpected error occurred.')).toBeInTheDocument()
    await waitFor(() => expect(screen.getByRole('button', { name: 'Run test' })).toBeEnabled())
  })

  it('does not offer the probe outside the native app', async () => {
    native.available = false
    renderWith(DiagnosticsScreen, await createTestServices({ language: 'en' }))
    expect(screen.getByText(/Available only in the native app/)).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Run test' })).toBeNull()
  })
})

describe('probeSecretStore', () => {
  it('invokes the dev-only command without arguments', async () => {
    const { probeSecretStore: real } = await vi.importActual<{ probeSecretStore: typeof probeSecretStore }>('../src/api/nativeDiagnostics')
    const invoke = vi.fn().mockResolvedValue(report(true))
    await expect(real(invoke)).resolves.toEqual(report(true))
    expect(invoke).toHaveBeenCalledWith('probe_secret_store')
  })
})
