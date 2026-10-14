import { invoke, isTauri } from '@tauri-apps/api/core'
import type { InvokeFn } from './tauriAppApi'

/**
 * Phase 1 native spikes (ADR-07, ADR-12). Not part of `AppApi`: the commands exist only for the
 * dev variant's diagnostics screen and are denied by capabilities in beta/release.
 * DTOs mirror `crates/contracts`.
 */
export interface ProbeStep {
  name: string
  ok: boolean
  error: string | null
}

export interface SecretStoreProbe {
  backend: string
  persistedFromPreviousRun: boolean
  steps: ProbeStep[]
}

/** True inside the Tauri WebView; the browser demo has no native backend to probe. */
export const nativeDiagnosticsAvailable = (): boolean => isTauri()

export async function probeSecretStore(invokeFn: InvokeFn = invoke): Promise<SecretStoreProbe> {
  return (await invokeFn('probe_secret_store')) as SecretStoreProbe
}

/** Mirrors `FileProbeDto`: the location kind, never the file name or contents. */
export interface FileProbe {
  cancelled: boolean
  location: string | null
  bytes: number | null
  kind: string | null
  steps: ProbeStep[]
}

/** Picks a CSV/APKG (or, unfiltered, any) file and streams it through Rust. */
export async function probeFileOpen(filtered: boolean, invokeFn: InvokeFn = invoke): Promise<FileProbe> {
  return (await invokeFn('probe_file_open', { filtered })) as FileProbe
}

/** Saves a small backup-like file where the user chooses and reads it back in Rust. */
export async function probeFileSave(invokeFn: InvokeFn = invoke): Promise<FileProbe> {
  return (await invokeFn('probe_file_save')) as FileProbe
}

/**
 * Notifications: the official plugin's commands are invoked directly instead of through
 * `sendNotification`, which drops errors, so a rejected `notify` shows up in the report.
 */
export const PROBE_NOTIFICATION_IDS = { now: 7001, scheduled: 7002 } as const

export interface NotificationContent {
  title: string
  body: string
}

export async function notificationPermissionGranted(invokeFn: InvokeFn = invoke): Promise<boolean | null> {
  return (await invokeFn('plugin:notification|is_permission_granted')) as boolean | null
}

export async function requestNotificationPermission(invokeFn: InvokeFn = invoke): Promise<string> {
  return (await invokeFn('plugin:notification|request_permission')) as string
}

export async function sendProbeNotification(content: NotificationContent, invokeFn: InvokeFn = invoke): Promise<void> {
  await invokeFn('plugin:notification|notify', {
    options: { id: PROBE_NOTIFICATION_IDS.now, ...content, extra: { probe: 'now' } },
  })
}

/** Schedules the probe notification and returns whether the platform lists it as pending. */
export async function scheduleProbeNotification(
  content: NotificationContent,
  delaySeconds: number,
  invokeFn: InvokeFn = invoke,
  now: () => number = Date.now,
): Promise<boolean> {
  const id = PROBE_NOTIFICATION_IDS.scheduled
  await invokeFn('plugin:notification|cancel', { notifications: [id] })
  await invokeFn('plugin:notification|notify', {
    options: {
      id,
      ...content,
      extra: { probe: 'scheduled' },
      schedule: { at: { date: new Date(now() + delaySeconds * 1000), repeating: false, allowWhileIdle: true } },
    },
  })
  const pending = (await invokeFn('plugin:notification|get_pending')) as Array<{ id: number }>
  return pending.some((notification) => notification.id === id)
}

export interface NotificationEvent {
  kind: 'received' | 'tapped'
  id: number | null
  extra: string
}

/** Listens for delivered (foreground) and tapped notifications; resolves to an unsubscribe. */
export async function listenToNotifications(onEvent: (event: NotificationEvent) => void): Promise<() => void> {
  const { onAction, onNotificationReceived } = await import('@tauri-apps/plugin-notification')
  const forward = (kind: NotificationEvent['kind']) => (payload: unknown) => onEvent(notificationEvent(kind, payload))
  const listeners = await Promise.all([onNotificationReceived(forward('received')), onAction(forward('tapped'))])
  return () => {
    for (const listener of listeners) void listener.unregister()
  }
}

const asRecord = (value: unknown): Record<string, unknown> =>
  typeof value === 'object' && value !== null ? (value as Record<string, unknown>) : {}

/** A tap carries `{ actionId, notification }`; a delivery carries the notification itself. */
export function notificationEvent(kind: NotificationEvent['kind'], payload: unknown): NotificationEvent {
  const outer = asRecord(payload)
  const notification = 'notification' in outer ? asRecord(outer.notification) : outer
  return {
    kind,
    id: typeof notification.id === 'number' ? notification.id : null,
    extra: JSON.stringify(notification.extra ?? null),
  }
}

/**
 * TTS: the WebView's Web Speech API, tried first per ADR-12. A native plugin is needed only where
 * it is missing or lacks offline de-DE/en-US voices.
 */
export interface SpeechVoice {
  lang: string
  name: string
  local: boolean
}

export interface SpeechSupport {
  available: boolean
  voices: SpeechVoice[]
}

export interface SpeechHost {
  speechSynthesis?: SpeechSynthesis
}

const sameLang = (lang: string, wanted: string) => lang.replace('_', '-').toLowerCase() === wanted.toLowerCase()

/** Voices load asynchronously on most engines; waits for `voiceschanged` up to `timeoutMs`. */
export async function speechSupport(host: SpeechHost = window, timeoutMs = 1500): Promise<SpeechSupport> {
  const synth = host.speechSynthesis
  if (!synth) return { available: false, voices: [] }
  let voices = synth.getVoices()
  if (voices.length === 0) {
    voices = await new Promise<SpeechSynthesisVoice[]>((resolve) => {
      const done = () => {
        synth.removeEventListener('voiceschanged', done)
        clearTimeout(timer)
        resolve(synth.getVoices())
      }
      const timer = setTimeout(done, timeoutMs)
      synth.addEventListener('voiceschanged', done)
    })
  }
  return { available: true, voices: voices.map((voice) => ({ lang: voice.lang, name: voice.name, local: voice.localService })) }
}

/** Voices for a BCP-47 tag; engines report either `de-DE` or `de_DE`. */
export function voicesFor(voices: SpeechVoice[], lang: string): SpeechVoice[] {
  return voices.filter((voice) => sameLang(voice.lang, lang))
}

export interface SpeakRequest {
  text: string
  lang: string
  rate: number
}

/** Speaks and reports lifecycle events: `start`, `end`, `error:<code>`. */
export function speak(request: SpeakRequest, onEvent: (event: string) => void, host: SpeechHost = window): void {
  const synth = host.speechSynthesis
  if (!synth) {
    onEvent('error:unavailable')
    return
  }
  const utterance = new SpeechSynthesisUtterance(request.text)
  utterance.lang = request.lang
  utterance.rate = request.rate
  const voice = synth.getVoices().find((candidate) => sameLang(candidate.lang, request.lang))
  if (voice) utterance.voice = voice
  utterance.onstart = () => onEvent('start')
  utterance.onend = () => onEvent('end')
  utterance.onerror = (event) => onEvent(`error:${event.error}`)
  synth.cancel()
  synth.speak(utterance)
}

export function stopSpeaking(host: SpeechHost = window): void {
  host.speechSynthesis?.cancel()
}

/** Sharing: the Web Share API first; a native share-sheet plugin only where it is missing. */
export interface ShareSupport {
  text: boolean
  files: boolean
}

export type ShareHost = Partial<Pick<Navigator, 'share' | 'canShare'>>

export function probeShareFile(): File {
  return new File(['front,back\nHaus,house\n'], 'langualinker-probe.csv', { type: 'text/csv' })
}

export function shareSupport(host: ShareHost = navigator): ShareSupport {
  const text = typeof host.share === 'function'
  let files = false
  try {
    files = text && typeof host.canShare === 'function' && host.canShare({ files: [probeShareFile()] })
  } catch {
    files = false
  }
  return { text, files }
}

/** Resolves to `shared` or the DOM error name (`AbortError` = the user closed the sheet). */
export async function shareProbe(data: ShareData, host: ShareHost = navigator): Promise<string> {
  if (typeof host.share !== 'function') return 'unavailable'
  try {
    await host.share(data)
    return 'shared'
  } catch (caught) {
    // DOMException is not an `Error` subclass in every engine.
    const name = asRecord(caught).name
    return typeof name === 'string' ? name : String(caught)
  }
}
