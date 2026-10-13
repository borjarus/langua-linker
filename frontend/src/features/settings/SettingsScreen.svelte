<script lang="ts">
  import type { ThemePreference, UiLanguage } from '../../api/types'
  import { savePreference } from '../../app/bootstrap'
  import { useServices } from '../../app/context'
  import Icon from '../../components/Icon.svelte'
  import { UI_LANGUAGES } from '../../services/i18n.svelte'
  import type { TextScale } from '../../services/theme.svelte'
  import { useQuery } from '../../services/query.svelte'
  import Screen from '../shared/Screen.svelte'

  const { api, i18n, theme, profile, queries } = useServices()
  const fieldId = $props.id()
  const info = useQuery(queries, () => ({ key: ['appInfo'], fetch: () => api.getAppInfo() }))
  const THEMES: ThemePreference[] = ['system', 'light', 'dark']
  const SCALES: TextScale[] = ['normal', 'large', 'huge']

  async function changeLanguage(language: UiLanguage) {
    await i18n.setLanguage(language)
    savePreference(profile, 'language', language)
  }

  function changeTheme(preference: ThemePreference) {
    theme.setPreference(preference)
    savePreference(profile, 'theme', preference)
  }

  function changeScale(scale: TextScale) {
    theme.setTextScale(scale)
    savePreference(profile, 'textScale', scale)
  }
</script>

<Screen title={i18n.t('routes.settings')} phase={5}>
  <section class="surface stack">
    <div class="field">
      <label for="{fieldId}-language">{i18n.t('settings.uiLanguage')}</label>
      <select id="{fieldId}-language" class="input" value={i18n.language} onchange={(event) => changeLanguage(event.currentTarget.value as UiLanguage)}>
        {#each UI_LANGUAGES as language (language)}
          <option value={language} lang={language}>{i18n.t(`language.${language}`)}</option>
        {/each}
      </select>
    </div>
    <fieldset class="field">
      <legend>{i18n.t('settings.theme')}</legend>
      <div class="row">
        {#each THEMES as option (option)}
          <label class="choice">
            <input type="radio" name="{fieldId}-theme" value={option} checked={theme.preference === option} onchange={() => changeTheme(option)} />
            {i18n.t(`settings.themes.${option}`)}
          </label>
        {/each}
      </div>
    </fieldset>
    <fieldset class="field">
      <legend>{i18n.t('settings.textScale')}</legend>
      <div class="row">
        {#each SCALES as option (option)}
          <label class="choice">
            <input type="radio" name="{fieldId}-scale" value={option} checked={theme.textScale === option} onchange={() => changeScale(option)} />
            {i18n.t(`settings.textScales.${option}`)}
          </label>
        {/each}
      </div>
    </fieldset>
  </section>

  <nav aria-label={i18n.t('settings.sections')}>
    <ul class="links">
      <li><a class="btn" href="#/settings/transfer">{i18n.t('routes.settingsTransfer')}<Icon name="back" /></a></li>
      <li><a class="btn" href="#/settings/backup">{i18n.t('routes.settingsBackup')}<Icon name="back" /></a></li>
      <li><a class="btn" href="#/settings/attributions">{i18n.t('routes.settingsAttributions')}<Icon name="back" /></a></li>
      <li><a class="btn" href="#/onboarding">{i18n.t('routes.onboarding')}<Icon name="back" /></a></li>
      {#if profile !== 'release'}
        <li><a class="btn" href="#/gallery">{i18n.t('routes.gallery')}<Icon name="back" /></a></li>
        <li><a class="btn" href="#/diagnostics">{i18n.t('routes.diagnostics')}<Icon name="back" /></a></li>
      {/if}
    </ul>
  </nav>

  <section class="surface">
    <h2>{i18n.t('settings.about')}</h2>
    {#if info.state.status === 'ready'}
      <p class="muted">{info.state.data.appName} {info.state.data.appVersion} · {i18n.t('settings.apiVersion', { version: info.state.data.apiVersion })} · {info.state.data.mode}</p>
    {/if}
  </section>
</Screen>

<style>
  fieldset {
    margin: 0;
    padding: 0;
    border: 0;
  }

  legend {
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
    width: 1.25rem;
    height: 1.25rem;
    accent-color: var(--color-primary);
  }

  .links {
    display: grid;
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .links .btn {
    justify-content: space-between;
    width: 100%;
  }

  .links .btn :global(.icon) {
    transform: rotate(180deg);
  }
</style>
