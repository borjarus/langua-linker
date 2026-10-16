<script lang="ts">
  import type { TargetLanguage } from '../../api/types'
  import { useServices } from '../../app/context'
  import Icon from '../../components/Icon.svelte'

  interface Props {
    text: string
    lang: TargetLanguage
  }

  let { text, lang }: Props = $props()
  const { i18n, speech } = useServices()

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
<button
  type="button"
  class="btn btn-ghost speak"
  aria-disabled={availability !== 'available'}
  data-availability={availability}
  onclick={() => {
    if (availability === 'available') speech.speak(text, lang)
  }}
>
  <Icon name="volume" />
  <span>{label}</span>
</button>

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
