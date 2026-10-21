<script lang="ts">
  import { toErrorDto } from '../../api/errors'
  import type { ConsentState, ErrorDto } from '../../api/types'
  import { useServices } from '../../app/context'
  import ErrorState from '../../components/ErrorState.svelte'
  import Icon from '../../components/Icon.svelte'
  import Skeleton from '../../components/Skeleton.svelte'
  import { newOperationId } from '../../services/operationId'
  import Screen from '../shared/Screen.svelte'
  import SaveStatus from './SaveStatus.svelte'
  import { SettingsController } from './settingsController.svelte'

  const { api, i18n, toasts } = useServices()
  const ids = $props.id()
  const controller = new SettingsController(api)
  void controller.load()

  let model = $state('')
  let endpoint = $state('')
  let apiKey = $state('')
  let keySaving = $state(false)
  let keyError = $state<ErrorDto | null>(null)

  const ai = $derived(controller.settings?.ai ?? null)
  const provider = $derived(ai?.providers.find((item) => item.id === ai.provider) ?? null)
  const endpointValid = $derived(endpoint.trim() === '' || /^https:\/\/[^\s/]+/iu.test(endpoint.trim()))

  // The free-text fields start from the stored values whenever the provider changes.
  let shownProvider: string | null = null
  $effect.pre(() => {
    if (!ai || ai.provider === shownProvider) return
    shownProvider = ai.provider
    model = ai.model ?? ''
    endpoint = ai.endpoint ?? ''
  })

  async function setConsent(consent: ConsentState) {
    await controller.update('consent', { aiConsent: consent })
  }

  /** The key goes to the SecretStore and is never shown again; the field is cleared either way. */
  async function saveKey(value: string | null) {
    if (keySaving) return
    keySaving = true
    keyError = null
    try {
      await api.setAiApiKey({ operationId: newOperationId(), apiKey: value })
      // The key changed the settings version too; reload so the next change is not a conflict.
      await controller.load()
      apiKey = ''
      toasts.show(i18n.t(value === null ? 'ai.key.removed' : 'ai.key.saved'), 'success')
    } catch (caught) {
      keyError = toErrorDto(caught)
    } finally {
      keySaving = false
    }
  }
</script>

<Screen title={i18n.t('routes.settingsAi')} back notice={i18n.t('ai.prototype')}>
  {#if controller.loadError}
    <ErrorState error={controller.loadError} onretry={() => void controller.load()} />
  {:else if !ai}
    <Skeleton lines={6} />
  {:else}
    <section class="surface stack" aria-labelledby="{ids}-consent">
      <h2 id="{ids}-consent">{i18n.t('ai.consent.title')}</h2>
      <p>{i18n.t('ai.consent.what')}</p>
      <p class="muted hint">{i18n.t('ai.consent.never')}</p>
      <fieldset class="field">
        <legend class="visually-hidden">{i18n.t('ai.consent.title')}</legend>
        <label class="choice">
          <input type="radio" name="{ids}-consent" checked={ai.consent === 'granted'} onchange={() => setConsent('granted')} />
          {i18n.t('ai.consent.granted')}
        </label>
        <label class="choice">
          <input type="radio" name="{ids}-consent" checked={ai.consent === 'declined'} onchange={() => setConsent('declined')} />
          {i18n.t('ai.consent.declined')}
        </label>
      </fieldset>
      {#if ai.consent !== 'granted'}
        <p class="note" role="status"><Icon name="info" />{i18n.t(ai.consent === 'declined' ? 'ai.consent.declinedNote' : 'ai.consent.unsetNote')}</p>
      {/if}
      <SaveStatus {controller} field="consent" />
    </section>

    {#if ai.consent === 'granted'}
      <section class="surface stack" aria-labelledby="{ids}-provider">
        <h2 id="{ids}-provider">{i18n.t('ai.provider.title')}</h2>
        <div class="field">
          <label for="{ids}-provider-select">{i18n.t('ai.provider.label')}</label>
          <select id="{ids}-provider-select" class="input" value={ai.provider ?? ''} onchange={(event) => controller.update('provider', { aiProvider: event.currentTarget.value })}>
            {#each ai.providers as option (option.id)}
              <option value={option.id}>{option.name}</option>
            {/each}
          </select>
          <p class="muted hint">{i18n.t('ai.provider.changeHint')}</p>
          <SaveStatus {controller} field="provider" />
        </div>

        {#if provider}
          <div class="field">
            <label for="{ids}-model">{i18n.t('ai.model.label')}</label>
            {#if provider.models.length > 0}
              <select id="{ids}-model" class="input" value={ai.model ?? ''} onchange={(event) => controller.update('model', { aiModel: event.currentTarget.value })}>
                {#each provider.models as option (option)}
                  <option value={option}>{option}</option>
                {/each}
              </select>
            {:else}
              <div class="row">
                <input id="{ids}-model" class="input grow" autocomplete="off" spellcheck="false" maxlength="100" bind:value={model} />
                <button type="button" class="btn" disabled={model.trim() === (ai.model ?? '')} onclick={() => controller.update('model', { aiModel: model.trim() || null })}>{i18n.t('common.save')}</button>
              </div>
            {/if}
            <SaveStatus {controller} field="model" />
          </div>

          {#if provider.customEndpoint}
            <div class="field">
              <label for="{ids}-endpoint">{i18n.t('ai.endpoint.label')}</label>
              <div class="row">
                <input
                  id="{ids}-endpoint"
                  class="input grow"
                  type="url"
                  inputmode="url"
                  autocomplete="off"
                  spellcheck="false"
                  placeholder={i18n.t('ai.endpoint.placeholder')}
                  bind:value={endpoint}
                  aria-invalid={!endpointValid}
                  aria-describedby="{ids}-endpoint-hint"
                />
                <button type="button" class="btn" disabled={!endpointValid || endpoint.trim() === (ai.endpoint ?? '')} onclick={() => controller.update('endpoint', { aiEndpoint: endpoint.trim() || null })}>{i18n.t('common.save')}</button>
              </div>
              <p id="{ids}-endpoint-hint" class="muted hint" class:invalid={!endpointValid}>{i18n.t(endpointValid ? 'ai.endpoint.hint' : 'ai.endpoint.httpsOnly')}</p>
              <SaveStatus {controller} field="endpoint" />
            </div>
          {/if}

          {#if provider.requiresKey}
            <div class="field">
              <span class="label" id="{ids}-key-label">{i18n.t('ai.key.label')}</span>
              <p class="key-state">
                <Icon name={ai.hasApiKey ? 'check' : 'info'} />
                {i18n.t(ai.hasApiKey ? 'ai.key.stored' : 'ai.key.none')}
              </p>
              <form
                class="row"
                aria-labelledby="{ids}-key-label"
                onsubmit={(event) => {
                  event.preventDefault()
                  if (apiKey.trim()) void saveKey(apiKey)
                }}
              >
                <label for="{ids}-key" class="visually-hidden">{i18n.t(ai.hasApiKey ? 'ai.key.replace' : 'ai.key.enter')}</label>
                <input
                  id="{ids}-key"
                  class="input grow"
                  type="password"
                  autocomplete="off"
                  spellcheck="false"
                  placeholder={i18n.t(ai.hasApiKey ? 'ai.key.replace' : 'ai.key.enter')}
                  bind:value={apiKey}
                />
                <button type="submit" class="btn" disabled={keySaving || !apiKey.trim()} aria-busy={keySaving}>{i18n.t('ai.key.save')}</button>
              </form>
              {#if ai.hasApiKey}
                <button type="button" class="btn btn-ghost danger" disabled={keySaving} onclick={() => saveKey(null)}><Icon name="trash" />{i18n.t('ai.key.remove')}</button>
              {/if}
              <p class="muted hint">{i18n.t('ai.key.hint')}</p>
              {#if keyError}<ErrorState error={keyError} compact />{/if}
            </div>
          {/if}
        {/if}
      </section>

      <section class="surface stack" aria-labelledby="{ids}-quota">
        <h2 id="{ids}-quota">{i18n.t('ai.quota.title')}</h2>
        {#if ai.quota}
          <p>{i18n.t('ai.quota.used', { used: ai.quota.used, limit: ai.quota.limit })}</p>
          <meter min="0" max={ai.quota.limit} value={ai.quota.used} low={ai.quota.limit * 0.8} high={ai.quota.limit * 0.95} optimum="0" aria-label={i18n.t('ai.quota.title')}></meter>
          <p class="muted hint">{i18n.t('ai.quota.resets', { time: i18n.formatDate(ai.quota.resetsAt, { dateStyle: 'medium', timeStyle: 'short' }) })}</p>
          {#if ai.quota.used >= ai.quota.limit}<p class="note"><Icon name="info" />{i18n.t('ai.quota.reached')}</p>{/if}
        {:else}
          <p class="muted">{i18n.t('ai.quota.none')}</p>
        {/if}
      </section>
    {/if}
  {/if}
</Screen>

<style>
  h2 {
    font-size: var(--text-lg);
  }

  fieldset {
    display: grid;
    margin: 0;
    padding: 0;
    border: 0;
  }

  .label {
    font-weight: var(--weight-medium);
  }

  .choice {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    min-height: var(--touch-target);
  }

  .choice input {
    flex: none;
    width: 1.25rem;
    height: 1.25rem;
    accent-color: var(--color-primary);
  }

  .grow {
    flex: 1 1 12rem;
    min-width: 0;
  }

  .hint {
    font-size: var(--text-sm);
  }

  .invalid,
  .danger {
    color: var(--color-error);
  }

  .note,
  .key-state {
    display: flex;
    align-items: start;
    gap: var(--space-2);
  }

  .note {
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-sm);
    background: var(--color-info-soft);
  }

  .danger {
    justify-self: start;
  }

  meter {
    width: 100%;
    height: 0.75rem;
  }
</style>
