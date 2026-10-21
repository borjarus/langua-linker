<script lang="ts">
  import type { TargetLanguage } from '../../api/types'
  import { useServices } from '../../app/context'
  import Icon from '../../components/Icon.svelte'
  import { useQuery } from '../../services/query.svelte'

  interface Props {
    text: string
    lang: TargetLanguage
  }

  let { text, lang }: Props = $props()
  const { api, i18n, queries, speech } = useServices()
  // Speech can be turned off in the settings; until they load, the button behaves as enabled.
  const settings = useQuery(queries, () => ({ key: ['settings'], fetch: () => api.getSettings() }))
  const enabled = $derived(settings.state.status !== 'ready' || settings.state.data.ttsEnabled)
  const rate = $derived(settings.state.status === 'ready' ? settings.state.data.ttsRate : 1)

  // Voices arrive asynchronously; bumping the revision re-reads availability.
  let revision = $state(0)
  $effect(() => speech.onChange(() => (revision += 1)))
  $effect(() => () => speech.stop())

  const availability = $derived.by(() => {
    void revision
    return speech.availability(lang)
  })
  const language = $derived(i18n.t(`deck.targetLanguages.${lang}`))
  const label = $derived(
    availability === 'available'
      ? i18n.t('tts.listen', { language })
      : availability === 'noVoice'
        ? i18n.t('tts.noVoice', { language })
        : i18n.t('tts.unsupported'),
  )
</script>

<!-- Unavailable speech stays visible (aria-disabled, still focusable) so the reason can be read. -->
{#if enabled}<button
  type="button"
  class="btn btn-ghost speak"
  aria-disabled={availability !== 'available'}
  data-availability={availability}
  onclick={() => {
    if (availability === 'available') speech.speak(text, lang, rate)
  }}
>
  <Icon name="volume" />
  <span>{label}</span>
</button>{/if}

<style>
  .speak {
    justify-self: center;
    font-size: var(--text-sm);
  }

  .speak[aria-disabled='true'] {
    opacity: 1;
    color: var(--color-text-muted);
    cursor: default;
  }
</style>
