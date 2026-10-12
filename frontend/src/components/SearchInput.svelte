<script lang="ts">
  import { useServices } from '../app/context'
  import Icon from './Icon.svelte'

  interface Props {
    value: string
    label: string
    placeholder?: string
    /** Debounced callback; the parent owns the query. */
    onsearch: (value: string) => void
    debounceMs?: number
    /** Parser error message from the backend, linked via `aria-describedby`. */
    error?: string | null
  }

  let { value, label, placeholder, onsearch, debounceMs = 250, error = null }: Props = $props()
  const { i18n } = useServices()
  const inputId = $props.id()
  // Adopts external value changes (e.g. a chip that rewrites the query) without echoing them back.
  let draft = $derived(value)
  let timer: ReturnType<typeof setTimeout> | undefined

  $effect(() => () => clearTimeout(timer))

  function schedule(next: string, immediate = false) {
    draft = next
    clearTimeout(timer)
    if (immediate || debounceMs === 0) onsearch(next)
    else timer = setTimeout(() => onsearch(next), debounceMs)
  }
</script>

<div class="search">
  <label for={inputId}>{label}</label>
  <div class="control" class:invalid={!!error}>
    <Icon name="search" />
    <input
      id={inputId}
      type="search"
      class="input"
      value={draft}
      {placeholder}
      autocomplete="off"
      spellcheck="false"
      aria-invalid={error ? 'true' : undefined}
      aria-describedby={error ? `${inputId}-error` : undefined}
      oninput={(event) => schedule(event.currentTarget.value)}
      onkeydown={(event) => {
        if (event.key === 'Enter') schedule(draft, true)
      }}
    />
    {#if draft}
      <button type="button" class="btn btn-ghost btn-icon" aria-label={i18n.t('search.clear')} onclick={() => schedule('', true)}>
        <Icon name="close" />
      </button>
    {/if}
  </div>
  {#if error}<p id="{inputId}-error" class="error">{error}</p>{/if}
</div>

<style>
  .search {
    display: grid;
    gap: var(--space-1);
  }

  label {
    font-weight: var(--weight-medium);
  }

  .control {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding-inline-start: var(--space-3);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-md);
    background: var(--color-surface);
  }

  .control:focus-within {
    box-shadow: var(--focus-ring);
  }

  .control.invalid {
    border-color: var(--color-error);
  }

  .input {
    flex: 1;
    min-width: 0;
    border: 0;
    background: transparent;
    outline: none;
  }

  .error {
    color: var(--color-error);
    font-size: var(--text-sm);
  }
</style>
