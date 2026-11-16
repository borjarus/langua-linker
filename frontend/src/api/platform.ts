import { invoke } from '@tauri-apps/api/core'
import type { InvokeFn } from './tauriAppApi'

/**
 * Phase 15 platform adapters outside `AppApi` (ADR-20): native speech and the deep link of a
 * tapped reminder. Commands are in `src-tauri/src/native.rs`; on Android they reach the Kotlin
 * `NativePlugin`, elsewhere they answer "no engine" / `null`.
 */
export interface NativeSpeechStatus {
  /** A native engine exists; `false` means the WebView engine (if any) is used instead. */
  engine: boolean
  voice: boolean
  /** A voice for the language is installed (works without a connection). */
  offline: boolean
}

export interface PlatformBridge {
  speechStatus(lang: string): Promise<NativeSpeechStatus>
  speak(text: string, lang: string, rate: number): Promise<void>
  stopSpeech(): Promise<void>
  /** The `langualinker://` URL the app was opened or resumed with, at most once. */
  takeLaunchUrl(): Promise<string | null>
}

export function createPlatformBridge(invokeFn: InvokeFn = invoke): PlatformBridge {
  return {
    speechStatus: async (lang) => (await invokeFn('speech_status', { input: { lang } })) as NativeSpeechStatus,
    speak: async (text, lang, rate) => {
      await invokeFn('speak', { input: { text, lang, rate } })
    },
    stopSpeech: async () => {
      await invokeFn('stop_speech')
    },
    takeLaunchUrl: async () => ((await invokeFn('take_launch_url')) as string | null) ?? null,
  }
}
