<script lang="ts">
  import type { FilePurpose } from '../api/types'
  import type { DemoScenario } from '../demo/scenarios'
  import { useServices } from './context'

  // Type-only demo imports: this component ships in release but renders nothing without `demo`.
  const { demo, i18n, queries, router, toasts } = useServices()
  const selectId = $props.id()
  let scenario = $state<DemoScenario>(demo?.scenario ?? 'happy')
  // The mock cannot open a real picker: on file screens the banner chooses what the picker returns.
  const purpose = $derived<FilePurpose | null>(
    router.current.name === 'settingsTransfer' ? 'import' : router.current.name === 'settingsBackup' ? 'restore' : null,
  )
  let pickedVersion = $state(0)
  const picked = $derived.by(() => {
    void pickedVersion
    return purpose && demo ? (demo.pickedFile(purpose) ?? '') : ''
  })

  function changeFile(token: string) {
    if (!demo || !purpose) return
    demo.setPickedFile(purpose, token || null)
    pickedVersion += 1
  }

  // Lets a reviewer reach save-error and retry flows (plan Phase 6 step 3) without a test hook.
  function failNextSave() {
    demo?.failNextSave('DB_BUSY')
    toasts.show(i18n.t('demo.failNextSaveArmed'))
  }

  function changeScenario(next: DemoScenario) {
    if (!demo) return
    scenario = next
    demo.setScenario(next)
    queries.clear()
    void router.replace(next === 'newUser' ? { name: 'onboarding' } : { name: 'home' })
  }
</script>

{#if demo}
  <aside class="banner" aria-label={i18n.t('demo.scenario')}>
    <p>{i18n.t('demo.banner')}</p>
    <div class="row">
      <label for={selectId}>{i18n.t('demo.scenario')}</label>
      <select
        id={selectId}
        class="input"
        value={scenario}
        onchange={(event) => changeScenario(event.currentTarget.value as DemoScenario)}
      >
        {#each demo.scenarios as option (option)}
          <option value={option}>{i18n.t(`demo.scenarios.${option}`)}</option>
        {/each}
      </select>
    </div>
    <button type="button" class="btn" onclick={failNextSave}>{i18n.t('demo.failNextSave')}</button>
    {#if purpose && demo}
      <div class="row">
        <label for="{selectId}-file">{i18n.t('demo.file')}</label>
        <select id="{selectId}-file" class="input" value={picked} onchange={(event) => changeFile(event.currentTarget.value)}>
          {#each demo.demoFiles[purpose] as file (file.token)}
            <option value={file.token}>{file.name}</option>
          {/each}
          <option value="">{i18n.t('demo.fileCancel')}</option>
        </select>
      </div>
    {/if}
  </aside>
{/if}

<style>
  .banner {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2) var(--space-4);
    padding: var(--space-2) var(--space-4);
    padding-top: calc(var(--space-2) + env(safe-area-inset-top));
    background: var(--color-demo);
    color: var(--color-on-demo);
    font-size: var(--text-sm);
  }

  /* A flex item defaults to its min-content width, which is the select's longest option; at 200%
     text that is wider than a phone, so let the row shrink and the select cap at its width. */
  .row {
    min-width: 0;
  }

  label {
    font-weight: var(--weight-medium);
  }

  .input,
  .btn {
    min-height: 2.5rem;
    color: var(--color-text);
  }
</style>
