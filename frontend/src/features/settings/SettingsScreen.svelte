<script lang="ts">
  import { toErrorDto } from '../../api/errors'
  import type { ErrorDto, SettingsDto, TargetLanguage, ThemePreference, UiLanguage } from '../../api/types'
  import { savePreference } from '../../app/bootstrap'
  import { useServices } from '../../app/context'
  import { href, type RouteTarget } from '../../app/routes'
  import ErrorState from '../../components/ErrorState.svelte'
  import Icon, { type IconName } from '../../components/Icon.svelte'
  import Skeleton from '../../components/Skeleton.svelte'
  import type { TranslationKey } from '../../locales/types'
  import { UI_LANGUAGES } from '../../services/i18n.svelte'
  import { newOperationId } from '../../services/operationId'
  import { useQuery } from '../../services/query.svelte'
  import type { TextScale } from '../../services/theme.svelte'
  import Screen from '../shared/Screen.svelte'
  import SaveStatus from './SaveStatus.svelte'
  import { SettingsController } from './settingsController.svelte'

  const { api, i18n, theme, profile, queries, speech } = useServices()
  const ids = $props.id()
  const controller = new SettingsController(api)
  const presets = useQuery(queries, () => ({ key: ['presets'], fetch: () => api.listPresets() }))
  const THEMES: ThemePreference[] = ['system', 'light', 'dark']
  const SCALES: TextScale[] = ['normal', 'large', 'huge']
  const TARGETS: TargetLanguage[] = ['de', 'en']
  const HOURS = Array.from({ length: 24 }, (_, hour) => hour)
  const FALLBACK_ZONES = ['Europe/Warsaw', 'Europe/Berlin', 'Europe/London', 'Europe/Dublin', 'America/New_York', 'UTC']

  let goal = $state(20)
  let requesting = $state(false)
  let permissionError = $state<ErrorDto | null>(null)
  let voices = $state(0)

  void controller.load().then(() => {
    if (controller.settings) goal = controller.settings.dailyGoal
  })
  // Voice lists load asynchronously; re-read availability when they change.
  $effect(() => speech.onChange(() => (voices += 1)))

  const zones = $derived.by(() => {
    const supported = typeof Intl.supportedValuesOf === 'function' ? Intl.supportedValuesOf('timeZone') : FALLBACK_ZONES
    const current = controller.settings?.timeZone
    return current && !supported.includes(current) ? [current, ...supported] : supported
  })
  const goalValid = $derived(Number.isInteger(goal) && goal >= 1 && goal <= 500)
  const hourLabel = (hour: number) => `${String(hour).padStart(2, '0')}:00`
  const availability = (lang: string) => {
    void voices
    return speech.availability(lang)
  }

  async function changeLanguage(language: UiLanguage) {
    await i18n.setLanguage(language)
    savePreference(profile, 'language', language)
    await controller.update('uiLanguage', { uiLanguage: language })
  }

  async function changeTheme(preference: ThemePreference) {
    theme.setPreference(preference)
    savePreference(profile, 'theme', preference)
    await controller.update('theme', { theme: preference })
  }

  function changeScale(scale: TextScale) {
    theme.setTextScale(scale)
    savePreference(profile, 'textScale', scale)
  }

  async function saveGoal(settings: SettingsDto) {
    if (!goalValid || goal === settings.dailyGoal) return
    await controller.update('dailyGoal', { dailyGoal: goal })
  }

  async function allowNotifications() {
    if (requesting) return
    requesting = true
    permissionError = null
    try {
      controller.replace(await api.requestNotificationPermission({ operationId: newOperationId() }))
    } catch (caught) {
      permissionError = toErrorDto(caught)
    } finally {
      requesting = false
    }
  }

  const LINKS: { target: RouteTarget; label: TranslationKey; icon: IconName; devOnly?: boolean }[] = [
    { target: { name: 'settingsAi' }, label: 'routes.settingsAi', icon: 'sparkles' },
    { target: { name: 'settingsPrivacy' }, label: 'routes.settingsPrivacy', icon: 'info' },
    { target: { name: 'settingsTransfer' }, label: 'routes.settingsTransfer', icon: 'copy' },
    { target: { name: 'settingsBackup' }, label: 'routes.settingsBackup', icon: 'folder' },
    { target: { name: 'settingsAbout' }, label: 'routes.settingsAbout', icon: 'help' },
    { target: { name: 'settingsAttributions' }, label: 'routes.settingsAttributions', icon: 'link' },
    { target: { name: 'onboarding', query: { from: 'settings' } }, label: 'settings.onboardingAgain', icon: 'refresh' },
    { target: { name: 'gallery' }, label: 'routes.gallery', icon: 'cards', devOnly: true },
    { target: { name: 'diagnostics' }, label: 'routes.diagnostics', icon: 'settings', devOnly: true },
  ]
</script>

<Screen title={i18n.t('routes.settings')} notice={i18n.t('settings.prototype')}>
  {#if controller.loadError}
    <ErrorState error={controller.loadError} onretry={() => void controller.load()} />
  {:else if !controller.settings}
    <Skeleton lines={8} />
  {:else}
    {@const settings = controller.settings}
    <section class="surface stack" aria-labelledby="{ids}-languages">
      <h2 id="{ids}-languages">{i18n.t('settings.languages')}</h2>
      <div class="field">
        <label for="{ids}-language">{i18n.t('settings.uiLanguage')}</label>
        <select id="{ids}-language" class="input" value={i18n.language} onchange={(event) => changeLanguage(event.currentTarget.value as UiLanguage)}>
          {#each UI_LANGUAGES as language (language)}
            <option value={language} lang={language}>{i18n.t(`language.${language}`)}</option>
          {/each}
        </select>
        <SaveStatus {controller} field="uiLanguage" />
      </div>
      <div class="field">
        <span class="label">{i18n.t('settings.nativeLanguage')}</span>
        <p>{i18n.t(`settings.nativeLanguages.${settings.nativeLanguage}`)}</p>
        <p class="muted hint">{i18n.t('settings.nativeLanguageHint')}</p>
      </div>
      <fieldset class="field">
        <legend>{i18n.t('settings.targetLanguage')}</legend>
        <div class="row">
          {#each TARGETS as language (language)}
            <label class="choice">
              <input type="radio" name="{ids}-target" checked={settings.defaultTargetLanguage === language} onchange={() => controller.update('defaultTargetLanguage', { defaultTargetLanguage: language })} />
              {i18n.t(`deck.targetLanguages.${language}`)}
            </label>
          {/each}
        </div>
        <p class="muted hint">{i18n.t('settings.targetLanguageHint')}</p>
        <SaveStatus {controller} field="defaultTargetLanguage" />
      </fieldset>
    </section>

    <section class="surface stack" aria-labelledby="{ids}-appearance">
      <h2 id="{ids}-appearance">{i18n.t('settings.appearance')}</h2>
      <fieldset class="field">
        <legend>{i18n.t('settings.theme')}</legend>
        <div class="row">
          {#each THEMES as option (option)}
            <label class="choice">
              <input type="radio" name="{ids}-theme" value={option} checked={theme.preference === option} onchange={() => changeTheme(option)} />
              {i18n.t(`settings.themes.${option}`)}
            </label>
          {/each}
        </div>
        <SaveStatus {controller} field="theme" />
      </fieldset>
      <fieldset class="field">
        <legend>{i18n.t('settings.textScale')}</legend>
        <div class="row">
          {#each SCALES as option (option)}
            <label class="choice">
              <input type="radio" name="{ids}-scale" value={option} checked={theme.textScale === option} onchange={() => changeScale(option)} />
              {i18n.t(`settings.textScales.${option}`)}
            </label>
          {/each}
        </div>
        <p class="muted hint">{i18n.t('settings.textScaleHint')}</p>
      </fieldset>
    </section>

    <section class="surface stack" aria-labelledby="{ids}-study">
      <h2 id="{ids}-study">{i18n.t('settings.study')}</h2>
      <div class="field">
        <label for="{ids}-goal">{i18n.t('settings.dailyGoal')}</label>
        <input
          id="{ids}-goal"
          class="input narrow"
          type="number"
          inputmode="numeric"
          min="1"
          max="500"
          step="1"
          value={goal}
          oninput={(event) => (goal = event.currentTarget.valueAsNumber)}
          onchange={() => saveGoal(settings)}
          aria-invalid={!goalValid}
          aria-describedby="{ids}-goal-hint"
        />
        <p id="{ids}-goal-hint" class="muted hint" class:invalid={!goalValid}>{i18n.t(goalValid ? 'settings.dailyGoalHint' : 'settings.dailyGoalError')}</p>
        <SaveStatus {controller} field="dailyGoal" />
      </div>
      <div class="field">
        <label for="{ids}-zone">{i18n.t('settings.timeZone')}</label>
        <select id="{ids}-zone" class="input" value={settings.timeZone} onchange={(event) => controller.update('timeZone', { timeZone: event.currentTarget.value })}>
          {#each zones as zone (zone)}
            <option value={zone}>{zone.replaceAll('_', ' ')}</option>
          {/each}
        </select>
        <SaveStatus {controller} field="timeZone" />
      </div>
      <div class="field">
        <label for="{ids}-cutoff">{i18n.t('settings.dayCutoff')}</label>
        <select id="{ids}-cutoff" class="input narrow" value={settings.dayCutoffHour} onchange={(event) => controller.update('dayCutoffHour', { dayCutoffHour: Number(event.currentTarget.value) })} aria-describedby="{ids}-cutoff-hint">
          {#each HOURS as hour (hour)}
            <option value={hour}>{hourLabel(hour)}</option>
          {/each}
        </select>
        <p id="{ids}-cutoff-hint" class="muted hint">{i18n.t('settings.dayCutoffHint', { time: hourLabel(settings.dayCutoffHour) })}</p>
        <SaveStatus {controller} field="dayCutoffHour" />
      </div>
      <div class="field">
        <label for="{ids}-preset">{i18n.t('settings.defaultPreset')}</label>
        <select
          id="{ids}-preset"
          class="input"
          value={settings.defaultPresetId}
          disabled={presets.state.status !== 'ready'}
          onchange={(event) => controller.update('defaultPresetId', { defaultPresetId: event.currentTarget.value })}
          aria-describedby="{ids}-preset-hint"
        >
          {#if presets.state.status === 'ready'}
            {#each presets.state.data as preset (preset.id)}
              <option value={preset.id}>{preset.name}</option>
            {/each}
          {/if}
        </select>
        <p id="{ids}-preset-hint" class="muted hint">{i18n.t('settings.defaultPresetHint')}</p>
        {#if presets.state.status === 'error'}<ErrorState error={presets.state.error} compact onretry={() => void presets.refresh()} />{/if}
        <SaveStatus {controller} field="defaultPresetId" />
      </div>
    </section>

    <section class="surface stack" aria-labelledby="{ids}-reminders">
      <h2 id="{ids}-reminders">{i18n.t('settings.reminders')}</h2>
      <p class="permission">
        <Icon name={settings.notifications === 'granted' ? 'check' : 'info'} />
        {i18n.t(`settings.notifications.${settings.notifications}`)}
      </p>
      {#if settings.notifications === 'prompt'}
        <button type="button" class="btn" disabled={requesting} aria-busy={requesting} onclick={allowNotifications}>{i18n.t('settings.notifications.allow')}</button>
      {/if}
      {#if permissionError}<ErrorState error={permissionError} compact />{/if}
      <div class="field">
        <label for="{ids}-reminder">{i18n.t('settings.reminderTime')}</label>
        <div class="row">
          <input
            id="{ids}-reminder"
            class="input"
            type="time"
            value={settings.reminderTime ?? ''}
            onchange={(event) => controller.update('reminderTime', { reminderTime: event.currentTarget.value || null })}
            aria-describedby="{ids}-reminder-hint"
          />
          {#if settings.reminderTime}
            <button type="button" class="btn btn-ghost" onclick={() => controller.update('reminderTime', { reminderTime: null })}>{i18n.t('settings.reminderOff')}</button>
          {/if}
        </div>
        <p id="{ids}-reminder-hint" class="muted hint">{i18n.t(settings.notifications === 'granted' ? 'settings.reminderHint' : 'settings.reminderHintNoPermission')}</p>
        <SaveStatus {controller} field="reminderTime" />
      </div>
    </section>

    <section class="surface stack" aria-labelledby="{ids}-speech">
      <h2 id="{ids}-speech">{i18n.t('settings.speech')}</h2>
      <label class="choice">
        <input type="checkbox" checked={settings.ttsEnabled} onchange={(event) => controller.update('ttsEnabled', { ttsEnabled: event.currentTarget.checked })} />
        {i18n.t('settings.ttsEnabled')}
      </label>
      <SaveStatus {controller} field="ttsEnabled" />
      <div class="field">
        <label for="{ids}-rate">{i18n.t('settings.ttsRate', { rate: i18n.formatNumber(settings.ttsRate, { minimumFractionDigits: 1 }) })}</label>
        <input
          id="{ids}-rate"
          type="range"
          min="0.5"
          max="1.5"
          step="0.1"
          value={settings.ttsRate}
          disabled={!settings.ttsEnabled}
          onchange={(event) => controller.update('ttsRate', { ttsRate: Number(event.currentTarget.value) })}
        />
        <SaveStatus {controller} field="ttsRate" />
      </div>
      <ul class="voices">
        {#each TARGETS as language (language)}
          <li>
            <span>{i18n.t(`deck.targetLanguages.${language}`)}: {i18n.t(`settings.voice.${availability(language)}`)}</span>
            {#if settings.ttsEnabled && availability(language) === 'available'}
              <button type="button" class="btn btn-ghost" onclick={() => speech.speak(i18n.t(`settings.voice.samples.${language}`), language, settings.ttsRate)}>
                <Icon name="volume" />{i18n.t('settings.voice.test')}
              </button>
            {/if}
          </li>
        {/each}
      </ul>
    </section>
  {/if}

  <nav aria-label={i18n.t('settings.sections')}>
    <ul class="links">
      {#each LINKS.filter((link) => !link.devOnly || profile !== 'release') as link (link.label)}
        <li><a class="btn" href={href(link.target)}><Icon name={link.icon} /><span>{i18n.t(link.label)}</span><Icon name="chevronRight" /></a></li>
      {/each}
    </ul>
  </nav>
</Screen>

<style>
  h2 {
    font-size: var(--text-lg);
  }

  fieldset {
    margin: 0;
    padding: 0;
    border: 0;
  }

  legend,
  .label {
    margin-bottom: var(--space-1);
    font-weight: var(--weight-medium);
  }

  .choice {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    min-height: var(--touch-target);
    padding-inline-end: var(--space-3);
  }

  .choice input {
    flex: none;
    width: 1.25rem;
    height: 1.25rem;
    accent-color: var(--color-primary);
  }

  .field > .narrow {
    width: min(100%, 10rem);
  }

  input[type='range'] {
    width: 100%;
    min-height: var(--touch-target);
    accent-color: var(--color-primary);
  }

  .hint {
    font-size: var(--text-sm);
  }

  .invalid {
    color: var(--color-error);
  }

  .permission {
    display: flex;
    align-items: start;
    gap: var(--space-2);
  }

  .voices {
    display: grid;
    gap: var(--space-1);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .voices li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
  }

  .links {
    display: grid;
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .links .btn {
    justify-content: start;
    width: 100%;
    text-align: start;
  }

  .links .btn span {
    flex: 1;
  }
</style>
