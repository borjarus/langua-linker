<script lang="ts">
  import { untrack } from 'svelte'
  import { toErrorDto } from '../../api/errors'
  import { FSRS_WEIGHT_COUNT, type ErrorDto, type PresetConfigDto, type PresetDto } from '../../api/types'
  import { useServices } from '../../app/context'
  import Dialog from '../../components/Dialog.svelte'
  import Disclosure from '../../components/Disclosure.svelte'
  import ErrorState from '../../components/ErrorState.svelte'
  import { newOperationId } from '../../services/operationId'

  interface Props {
    preset: PresetDto
    deckName: string
    onclose: () => void
    /** `asNew`: saved as a copy that the caller assigns to the deck. */
    onsaved: (preset: PresetDto, asNew: boolean) => void
  }

  let { preset, deckName, onclose, onsaved }: Props = $props()
  const { api, i18n } = useServices()
  const ids = $props.id()

  // The form starts from the preset as opened; a background refresh does not overwrite it.
  const initial = untrack(() => preset)
  let name = $state(initial.name)
  let newPerDay = $state(initial.config.newPerDay)
  let reviewsPerDay = $state(initial.config.reviewsPerDay)
  let retentionPercent = $state(Math.round(initial.config.desiredRetention * 100))
  let maxIntervalDays = $state(initial.config.maxIntervalDays)
  let learningSteps = $state(initial.config.learningStepsMinutes.join(' '))
  let relearningSteps = $state(initial.config.relearningStepsMinutes.join(' '))
  let weights = $state(initial.config.weights.join(', '))
  let touched = $state(false)
  let saving = $state(false)
  let error = $state<ErrorDto | null>(null)
  let operationId = newOperationId()

  const numbers = (text: string): number[] => text.split(/[\s,;]+/u).filter(Boolean).map(Number)
  const steps = (text: string) => numbers(text)

  const problems = $derived.by(() => {
    const out: Partial<Record<'name' | 'newPerDay' | 'reviewsPerDay' | 'retention' | 'maxInterval' | 'learning' | 'relearning' | 'weights', string>> = {}
    const intIn = (value: number, min: number, max: number) => Number.isInteger(value) && value >= min && value <= max
    if (!name.trim()) out.name = i18n.t('deck.preset.errors.name')
    if (!intIn(newPerDay, 0, 9999)) out.newPerDay = i18n.t('deck.preset.errors.range', { min: 0, max: 9999 })
    if (!intIn(reviewsPerDay, 0, 99_999)) out.reviewsPerDay = i18n.t('deck.preset.errors.range', { min: 0, max: 99_999 })
    if (!intIn(retentionPercent, 70, 99)) out.retention = i18n.t('deck.preset.errors.range', { min: 70, max: 99 })
    if (!intIn(maxIntervalDays, 1, 36_500)) out.maxInterval = i18n.t('deck.preset.errors.range', { min: 1, max: 36_500 })
    const validSteps = (list: number[]) => list.length <= 10 && list.every((step) => Number.isFinite(step) && step > 0 && step <= 43_200)
    if (!validSteps(steps(learningSteps))) out.learning = i18n.t('deck.preset.errors.steps')
    if (!validSteps(steps(relearningSteps))) out.relearning = i18n.t('deck.preset.errors.steps')
    const parsed = numbers(weights)
    if (parsed.length !== FSRS_WEIGHT_COUNT || parsed.some((weight) => !Number.isFinite(weight) || weight < 0 || weight > 100)) {
      out.weights = i18n.t('deck.preset.errors.weights', { number: FSRS_WEIGHT_COUNT })
    }
    return out
  })
  const valid = $derived(Object.keys(problems).length === 0)

  function config(): PresetConfigDto {
    return {
      newPerDay,
      reviewsPerDay,
      desiredRetention: retentionPercent / 100,
      maxIntervalDays,
      learningStepsMinutes: steps(learningSteps),
      relearningStepsMinutes: steps(relearningSteps),
      weights: numbers(weights),
    }
  }

  async function save(asNew: boolean) {
    touched = true
    if (!valid || saving) return
    saving = true
    error = null
    try {
      const saved = await api.savePreset({
        operationId,
        presetId: asNew ? null : initial.id,
        expectedVersion: asNew ? null : initial.version,
        name: asNew && name.trim() === initial.name ? i18n.t('deck.preset.copyName', { name: initial.name }) : name.trim(),
        config: config(),
      })
      operationId = newOperationId()
      onsaved(saved, asNew)
    } catch (caught) {
      error = toErrorDto(caught)
      if (!error.retryable) operationId = newOperationId()
    } finally {
      saving = false
    }
  }

  const show = (key: keyof typeof problems) => (touched ? (problems[key] ?? null) : null)
</script>

{#snippet number(key: 'newPerDay' | 'reviewsPerDay' | 'retention' | 'maxInterval', label: string, min: number, max: number, value: number, set: (next: number) => void)}
  <div class="field">
    <label for="{ids}-{key}">{label}</label>
    <input
      id="{ids}-{key}"
      class="input"
      type="number"
      inputmode="numeric"
      {min}
      {max}
      step="1"
      {value}
      oninput={(event) => set(event.currentTarget.valueAsNumber)}
      aria-invalid={show(key) !== null}
      aria-describedby={show(key) ? `${ids}-${key}-error` : undefined}
    />
    {#if show(key)}<p id="{ids}-{key}-error" class="field-error">{show(key)}</p>{/if}
  </div>
{/snippet}

<Dialog open title={i18n.t('deck.preset.editTitle', { name: initial.name })} onclose={() => !saving && onclose()}>
  <form id="{ids}-form" class="stack" novalidate onsubmit={(event) => { event.preventDefault(); void save(false) }}>
    {#if initial.dependentDecks > 1}
      <p class="warning" role="status">{i18n.t('deck.preset.sharedWarning', { count: initial.dependentDecks })}</p>
    {/if}
    <div class="field">
      <label for="{ids}-name">{i18n.t('deck.preset.name')}</label>
      <input id="{ids}-name" class="input" maxlength="80" bind:value={name} aria-invalid={show('name') !== null} />
      {#if show('name')}<p class="field-error">{show('name')}</p>{/if}
    </div>
    <div class="grid">
      {@render number('newPerDay', i18n.t('deck.preset.newPerDay'), 0, 9999, newPerDay, (next) => (newPerDay = next))}
      {@render number('reviewsPerDay', i18n.t('deck.preset.reviewsPerDay'), 0, 99_999, reviewsPerDay, (next) => (reviewsPerDay = next))}
      {@render number('retention', i18n.t('deck.preset.retention'), 70, 99, retentionPercent, (next) => (retentionPercent = next))}
      {@render number('maxInterval', i18n.t('deck.preset.maxInterval'), 1, 36_500, maxIntervalDays, (next) => (maxIntervalDays = next))}
    </div>
    <p class="muted hint">{i18n.t('deck.preset.retentionHint')}</p>
    <div class="field">
      <label for="{ids}-learning">{i18n.t('deck.preset.learningSteps')}</label>
      <input id="{ids}-learning" class="input" inputmode="decimal" bind:value={learningSteps} aria-invalid={show('learning') !== null} aria-describedby="{ids}-steps-hint" />
      {#if show('learning')}<p class="field-error">{show('learning')}</p>{/if}
    </div>
    <div class="field">
      <label for="{ids}-relearning">{i18n.t('deck.preset.relearningSteps')}</label>
      <input id="{ids}-relearning" class="input" inputmode="decimal" bind:value={relearningSteps} aria-invalid={show('relearning') !== null} aria-describedby="{ids}-steps-hint" />
      {#if show('relearning')}<p class="field-error">{show('relearning')}</p>{/if}
    </div>
    <p id="{ids}-steps-hint" class="muted hint">{i18n.t('deck.preset.stepsHint')}</p>
    <Disclosure title={i18n.t('deck.preset.advanced')}>
      <div class="field">
        <label for="{ids}-weights">{i18n.t('deck.preset.weights', { number: FSRS_WEIGHT_COUNT })}</label>
        <textarea id="{ids}-weights" class="input mono" rows="4" bind:value={weights} aria-invalid={show('weights') !== null}></textarea>
        {#if show('weights')}<p class="field-error">{show('weights')}</p>{/if}
        <p class="muted hint">{i18n.t('deck.preset.weightsHint')}</p>
      </div>
    </Disclosure>
    {#if touched && !valid}<p class="field-error" role="alert">{i18n.t('editor.errors.summary')}</p>{/if}
    {#if error}<ErrorState {error} compact />{/if}
  </form>
  {#snippet actions()}
    <button type="button" class="btn" disabled={saving} onclick={onclose}>{i18n.t('common.cancel')}</button>
    <button type="button" class="btn" disabled={saving} onclick={() => save(true)}>{i18n.t('deck.preset.saveAsNew', { name: deckName })}</button>
    <button type="submit" form="{ids}-form" class="btn btn-primary" disabled={saving} aria-busy={saving}>{i18n.t('deck.preset.save')}</button>
  {/snippet}
</Dialog>

<style>
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 10rem), 1fr));
    gap: var(--space-3);
  }

  .hint {
    font-size: var(--text-sm);
  }

  .mono {
    font-family: var(--font-mono, monospace);
  }

  .warning {
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--color-warning);
    border-radius: var(--radius-sm);
    background: var(--color-warning-soft);
  }

  .field-error {
    color: var(--color-error);
    font-size: var(--text-sm);
  }
</style>
