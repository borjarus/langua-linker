import type { PlatformBridge } from '../api/platform'
import type { Unsubscribe } from '../api/types'

/** `unsupported`: no speech engine at all; `noVoice`: an engine without a voice for the language. */
export type SpeechAvailability = 'available' | 'noVoice' | 'unsupported'

/**
 * Text-to-speech port. Not part of `AppApi`: audio is a platform adapter (plan §3, Phase 15), and
 * the mock never simulates it. Android uses the native engine (`NativePlugin`, ADR-20) because its
 * WebView has no Web Speech API; iOS and desktop use the WebView engine (ADR-12).
 */
export interface SpeechService {
  availability(lang: string): SpeechAvailability
  /** `rate`: 0.5–1.5, from the settings. */
  speak(text: string, lang: string, rate?: number): void
  stop(): void
  /** Voices load asynchronously; availability may change after this fires. */
  onChange(listener: () => void): Unsubscribe
}

type SynthesisLike = Pick<SpeechSynthesis, 'getVoices' | 'speak' | 'cancel' | 'addEventListener' | 'removeEventListener'>

export function createWebSpeechService(
  synthesis: SynthesisLike | undefined = globalThis.speechSynthesis,
  Utterance: typeof SpeechSynthesisUtterance | undefined = globalThis.SpeechSynthesisUtterance,
): SpeechService {
  const voiceFor = (lang: string) => {
    const prefix = lang.toLowerCase()
    return synthesis?.getVoices().find((voice) => voice.lang.toLowerCase().replace('_', '-').startsWith(prefix))
  }
  return {
    availability(lang) {
      if (!synthesis || !Utterance) return 'unsupported'
      return voiceFor(lang) ? 'available' : 'noVoice'
    },
    speak(text, lang, rate = 1) {
      if (!synthesis || !Utterance) return
      const voice = voiceFor(lang)
      if (!voice) return
      synthesis.cancel()
      const utterance = new Utterance(text)
      utterance.lang = voice.lang
      utterance.voice = voice
      utterance.rate = rate
      synthesis.speak(utterance)
    },
    stop() {
      synthesis?.cancel()
    },
    onChange(listener) {
      if (!synthesis) return () => {}
      synthesis.addEventListener('voiceschanged', listener)
      return () => synthesis.removeEventListener('voiceschanged', listener)
    },
  }
}

/** Languages the app teaches; their native status is asked for once at start. */
export const SPEECH_LANGUAGES = ['de', 'en', 'es'] as const

/**
 * Native engine first, WebView engine as the proven fallback (plan Phase 15 step 10). The native
 * status arrives asynchronously: until it does, the fallback answers, and `onChange` fires when
 * the native answer changes the picture.
 */
export function createNativeSpeechService(bridge: PlatformBridge, fallback: SpeechService): SpeechService {
  let engine: boolean | null = null
  const voices = new Map<string, boolean>()
  const asked = new Set<string>()
  const listeners = new Set<() => void>()
  const notify = () => listeners.forEach((listener) => listener())

  const ask = (lang: string) => {
    if (asked.has(lang) || engine === false) return
    asked.add(lang)
    bridge
      .speechStatus(lang)
      .then((status) => {
        engine = status.engine
        voices.set(lang, status.voice)
        notify()
      })
      .catch(() => {
        engine = false
        notify()
      })
  }
  SPEECH_LANGUAGES.forEach(ask)

  const native = (lang: string) => engine === true && voices.has(lang)
  return {
    availability(lang) {
      ask(lang)
      if (native(lang)) return voices.get(lang) ? 'available' : 'noVoice'
      return fallback.availability(lang)
    },
    speak(text, lang, rate = 1) {
      if (native(lang)) {
        void bridge.speak(text, lang, Math.min(1.5, Math.max(0.5, rate))).catch(() => notify())
      } else {
        fallback.speak(text, lang, rate)
      }
    },
    stop() {
      if (engine === true) void bridge.stopSpeech().catch(() => {})
      fallback.stop()
    },
    onChange(listener) {
      listeners.add(listener)
      const unsubscribe = fallback.onChange(listener)
      return () => {
        listeners.delete(listener)
        unsubscribe()
      }
    },
  }
}
