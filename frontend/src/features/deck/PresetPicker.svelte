<script lang="ts">
  import { untrack } from 'svelte'
  import type { ErrorDto, Id, PresetDto } from '../../api/types'
  import { useServices } from '../../app/context'
  import Chip from '../../components/Chip.svelte'
  import Dialog from '../../components/Dialog.svelte'
  import ErrorState from '../../components/ErrorState.svelte'
  import Icon from '../../components/Icon.svelte'
  import { useQuery } from '../../services/query.svelte'
  import QueryView from '../shared/QueryView.svelte'

  interface Props {
    currentId: Id
    onclose: () => void
    onpick: (preset: PresetDto) => Promise<ErrorDto | null>
    onedit: (preset: PresetDto) => void
  }

  let { currentId, onclose, onpick, onedit }: Props = $props()
  const { api, i18n, queries } = useServices()
  const presets = useQuery(queries, () => ({ key: ['presets'], fetch: () => api.listPresets() }))
  const group = $props.id()

  let choice = $state<Id>(untrack(() => currentId))
  let saving = $state(false)
  let error = $state<ErrorDto | null>(null)

  async function apply(list: PresetDto[]) {
    const preset = list.find((item) => item.id === choice)
    if (!preset || saving) return
    if (preset.id === currentId) {
      onclose()
      return
    }
    saving = true
    error = await onpick(preset)
    saving = false
  }
</script>

<Dialog open title={i18n.t('deck.preset.pickTitle')} onclose={() => !saving && onclose()}>
  <QueryView query={presets}>
    {#snippet ready(list)}
      <fieldset class="presets">
        <legend class="visually-hidden">{i18n.t('deck.preset.pickTitle')}</legend>
        {#each list as preset (preset.id)}
          <div class="preset" class:checked={choice === preset.id}>
            <label class="choice">
              <input type="radio" name={group} value={preset.id} bind:group={choice} />
              <span class="text">
                <strong>{preset.name}</strong>
                <span class="muted">
                  {i18n.t('deck.preset.summary', { newPerDay: preset.config.newPerDay, reviewsPerDay: preset.config.reviewsPerDay, retention: i18n.formatPercent(preset.config.desiredRetention) })}
                </span>
                <span class="row">
                  <Chip label={i18n.t('deck.preset.usedBy', { count: preset.dependentDecks })} />
                  {#if preset.builtIn}<Chip label={i18n.t('deck.preset.builtIn')} tone="info" />{/if}
                  {#if preset.id === currentId}<Chip label={i18n.t('deck.preset.current')} tone="primary" />{/if}
                </span>
              </span>
            </label>
            <button type="button" class="btn btn-ghost" onclick={() => onedit(preset)}>
              <Icon name="edit" />{i18n.t('deck.preset.editNamed', { name: preset.name })}
            </button>
          </div>
        {/each}
      </fieldset>
      {#if error}<ErrorState {error} compact />{/if}
      <div class="row footer">
        <button type="button" class="btn" disabled={saving} onclick={onclose}>{i18n.t('common.cancel')}</button>
        <button type="button" class="btn btn-primary" disabled={saving} aria-busy={saving} onclick={() => apply(list)}>{i18n.t('deck.preset.use')}</button>
      </div>
    {/snippet}
  </QueryView>
</Dialog>

<style>
  .presets {
    display: grid;
    gap: var(--space-2);
    margin: 0 0 var(--space-4);
    padding: 0;
    border: 0;
  }

  .preset {
    display: grid;
    gap: var(--space-1);
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-sm);
  }

  .preset.checked {
    border-color: var(--color-primary);
    background: var(--color-primary-soft);
  }

  .choice {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    min-height: var(--touch-target);
    cursor: pointer;
  }

  .choice input {
    width: 1.25rem;
    height: 1.25rem;
    margin-top: 0.15rem;
    flex: none;
  }

  .text {
    display: grid;
    gap: var(--space-1);
    min-width: 0;
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }

  .footer {
    justify-content: flex-end;
  }
</style>
