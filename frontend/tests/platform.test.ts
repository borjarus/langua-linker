import { describe, expect, it, vi } from 'vitest'
import { createPlatformBridge, type PlatformBridge } from '../src/api/platform'
import { syncUiLanguage } from '../src/app/bootstrap'
import { openLaunchLink, watchLaunchLinks } from '../src/app/launchLink'
import { createNativeSpeechService, type SpeechService } from '../src/services/speech'

const flush = () => new Promise((resolve) => setTimeout(resolve, 0))

function fallback(): SpeechService & { spoken: string[] } {
  const spoken: string[] = []
  return {
    spoken,
    availability: () => 'unsupported',
    speak: (text) => spoken.push(text),
    stop: vi.fn(),
    onChange: () => () => {},
  }
}

function bridge(overrides: Partial<PlatformBridge> = {}): PlatformBridge {
  return {
    speechStatus: vi.fn(async (lang: string) => ({ engine: true, voice: lang !== 'es', offline: lang === 'de' })),
    speak: vi.fn(async () => {}),
    stopSpeech: vi.fn(async () => {}),
    takeLaunchUrl: vi.fn(async () => null),
    ...overrides,
  }
}

describe('platform bridge', () => {
  it('sends the commands of src-tauri/src/native.rs', async () => {
    const invoke = vi.fn(async (command: string) => (command === 'speech_status' ? { engine: false, voice: false, offline: false } : null))
    const platform = createPlatformBridge(invoke)
    await platform.speechStatus('de')
    await platform.speak('Hallo', 'de', 1.2)
    await platform.stopSpeech()
    expect(await platform.takeLaunchUrl()).toBeNull()
    expect(invoke.mock.calls).toEqual([
      ['speech_status', { input: { lang: 'de' } }],
      ['speak', { input: { text: 'Hallo', lang: 'de', rate: 1.2 } }],
      ['stop_speech'],
      ['take_launch_url'],
    ])
  })
})

describe('native speech service', () => {
  it('uses the native engine once it reports a voice, and says when a language has none', async () => {
    const native = bridge()
    const web = fallback()
    const speech = createNativeSpeechService(native, web)
    const changed = vi.fn()
    speech.onChange(changed)
    // Before the native answer arrives the fallback answers.
    expect(speech.availability('de')).toBe('unsupported')
    await flush()
    expect(changed).toHaveBeenCalled()
    expect(speech.availability('de')).toBe('available')
    expect(speech.availability('es')).toBe('noVoice')
    speech.speak('Hallo', 'de', 3)
    expect(native.speak).toHaveBeenCalledWith('Hallo', 'de', 1.5)
    expect(web.spoken).toEqual([])
    speech.stop()
    expect(native.stopSpeech).toHaveBeenCalled()
    expect(web.stop).toHaveBeenCalled()
  })

  it('falls back to the WebView engine where there is no native one (iOS, desktop)', async () => {
    const native = bridge({ speechStatus: vi.fn(async () => ({ engine: false, voice: false, offline: false })) })
    const web = fallback()
    const speech = createNativeSpeechService(native, web)
    await flush()
    expect(speech.availability('de')).toBe('unsupported')
    speech.speak('Hallo', 'de')
    expect(web.spoken).toEqual(['Hallo'])
    expect(native.speak).not.toHaveBeenCalled()
  })

  it('treats a failing bridge as no native engine', async () => {
    const web = fallback()
    const speech = createNativeSpeechService(bridge({ speechStatus: vi.fn(async () => Promise.reject(new Error('x'))) }), web)
    await flush()
    speech.speak('Hi', 'en')
    expect(web.spoken).toEqual(['Hi'])
  })
})

describe('launch links', () => {
  const router = () => ({ navigate: vi.fn(async () => true) })

  it('opens a validated reminder link', async () => {
    const nav = router()
    expect(await openLaunchLink({ takeLaunchUrl: async () => 'langualinker://app/home' }, nav)).toBe(true)
    expect(nav.navigate).toHaveBeenCalledWith('/home')
  })

  it.each([
    'https://example.com/home',
    'langualinker://evil/home',
    'langualinker://app/settings/ai',
    'langualinker://app/diagnostics',
    'javascript:alert(1)',
    'not a url',
  ])('ignores %s', async (url) => {
    const nav = router()
    expect(await openLaunchLink({ takeLaunchUrl: async () => url }, nav)).toBe(false)
    expect(nav.navigate).not.toHaveBeenCalled()
  })

  it('ignores a failing bridge and checks again when the app becomes visible', async () => {
    const nav = router()
    expect(await openLaunchLink({ takeLaunchUrl: async () => Promise.reject(new Error('x')) }, nav)).toBe(false)
    const take = vi.fn(async () => null as string | null)
    const stop = watchLaunchLinks({ takeLaunchUrl: take }, nav)
    await flush()
    expect(take).toHaveBeenCalledTimes(1)
    take.mockResolvedValueOnce('langualinker://app/stats')
    document.dispatchEvent(new Event('visibilitychange'))
    await flush()
    expect(nav.navigate).toHaveBeenCalledWith('/stats')
    stop()
  })
})

describe('UI language sync', () => {
  const services = (stored: 'pl' | 'en', shown: 'pl' | 'en') => {
    const updateSettings = vi.fn(async () => ({}))
    return {
      updateSettings,
      value: {
        api: { getSettings: async () => ({ uiLanguage: stored, version: 4 }), updateSettings },
        i18n: { language: shown },
      } as unknown as Parameters<typeof syncUiLanguage>[0],
    }
  }

  it('stores the shown language so native text (reminders) matches the UI', async () => {
    const { value, updateSettings } = services('pl', 'en')
    expect(await syncUiLanguage(value)).toBe(true)
    expect(updateSettings).toHaveBeenCalledWith(expect.objectContaining({ expectedVersion: 4, patch: { uiLanguage: 'en' } }))
  })

  it('does nothing when they already agree', async () => {
    const { value, updateSettings } = services('en', 'en')
    expect(await syncUiLanguage(value)).toBe(false)
    expect(updateSettings).not.toHaveBeenCalled()
  })
})
