import type { Unsubscribe } from '../api/types'

/** `unsupported`: no speech engine at all; `noVoice`: an engine without a voice for the language. */
export type SpeechAvailability = 'available' | 'noVoice' | 'unsupported'

/**
 * Text-to-speech port. Not part of `AppApi`: audio is a platform adapter (plan §3, Phase 15), and
 * the mock never simulates it. Until the native plugin exists, the WebView engine is used where
 * present (ADR-12: iOS WKWebView yes, Android WebView no).
 */
export interface SpeechService {
  availability(lang: string): SpeechAvailability
  speak(text: string, lang: string): void
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
    speak(text, lang) {
      if (!synthesis || !Utterance) return
      const voice = voiceFor(lang)
      if (!voice) return
      synthesis.cancel()
      const utterance = new Utterance(text)
      utterance.lang = voice.lang
      utterance.voice = voice
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
