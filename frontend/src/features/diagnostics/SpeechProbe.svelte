<script lang="ts">
  import { speak, speechSupport, stopSpeaking, voicesFor, type SpeechSupport } from '../../api/nativeDiagnostics'
  import { useServices } from '../../app/context'
  import ProbeLog, { logEntry, type LogEntry } from './ProbeLog.svelte'

  /** Test sentences in the target languages — content, not UI text, so not translated. */
  const SAMPLES = [
    { lang: 'de-DE', text: 'Guten Morgen! Das ist ein Test der Sprachausgabe mit Umlauten: Grüße aus München.' },
    { lang: 'en-US', text: 'Good morning! This is a speech output test, long enough to press stop halfway through.' },
  ] as const

  const { i18n } = useServices()
  let support = $state<SpeechSupport | null>(null)
  let rate = $state(1)
  let entries = $state<LogEntry[]>([])

  const log = (text: string, failed = false) => {
    entries = [logEntry(text, failed), ...entries]
  }

  $effect(() => {
    void speechSupport().then((result) => (support = result))
    return () => stopSpeaking()
  })

  function say(lang: string, text: string) {
    log(i18n.t('diagnostics.speakRequested', { lang, rate }))
    speak({ lang, text, rate }, (event) => log(`${lang}: ${event}`, event.startsWith('error')))
  }

  function stop() {
    stopSpeaking()
    log(i18n.t('diagnostics.stopRequested'))
  }
</script>

{#if support === null}
  <p class="muted" role="status">{i18n.t('diagnostics.loadingVoices')}</p>
{:else if !support.available}
  <p role="status">{i18n.t('diagnostics.speechUnavailable')}</p>
{:else}
  <dl class="facts">
    <dt>{i18n.t('diagnostics.voicesTotal')}</dt>
    <dd>{support.voices.length}</dd>
    {#each SAMPLES as sample (sample.lang)}
      {@const voices = voicesFor(support.voices, sample.lang)}
      <dt><code>{sample.lang}</code></dt>
      <dd>
        {#if voices.length === 0}
          {i18n.t('diagnostics.noVoice')}
        {:else}
          {#each voices as voice, index (voice.name)}
            {index > 0 ? ', ' : ''}{voice.name} ({voice.local ? i18n.t('diagnostics.voiceOffline') : i18n.t('diagnostics.voiceNetwork')})
          {/each}
        {/if}
      </dd>
    {/each}
  </dl>
  <div class="field">
    <label for="speech-rate">{i18n.t('diagnostics.rate', { rate })}</label>
    <input id="speech-rate" class="input" type="range" min="0.5" max="1.5" step="0.25" bind:value={rate} />
  </div>
  <div class="row">
    {#each SAMPLES as sample (sample.lang)}
      <button type="button" class="btn btn-primary" onclick={() => say(sample.lang, sample.text)}>{i18n.t('diagnostics.speak', { lang: sample.lang })}</button>
    {/each}
    <button type="button" class="btn" onclick={stop}>{i18n.t('diagnostics.stop')}</button>
  </div>
  <ProbeLog {entries} />
{/if}
