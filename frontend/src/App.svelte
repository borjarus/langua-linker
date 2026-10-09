<script lang="ts">
  import { invoke, isTauri } from '@tauri-apps/api/core'
  import { listen } from '@tauri-apps/api/event'

  type AppInfo = {
    apiVersion: string
    appName: string
    mode: string
  }

  let appInfo = $state<AppInfo | null>(null)
  let eventStatus = $state('waiting')
  let error = $state<string | null>(null)

  $effect(() => {
    if (!isTauri()) {
      eventStatus = 'browser demo'
      return
    }

    let disposed = false
    let unlisten: (() => void) | undefined

    void listen<string>('app-ready', (event) => {
      if (!disposed) {
        eventStatus = event.payload
      }
    }).then((cleanup) => {
      if (disposed) {
        cleanup()
      } else {
        unlisten = cleanup
      }
    })

    return () => {
      disposed = true
      unlisten?.()
    }
  })

  async function loadAppInfo() {
    error = null

    if (!isTauri()) {
      appInfo = {
        apiVersion: '0.1',
        appName: 'LanguaLinker',
        mode: 'browser demo',
      }
      return
    }

    try {
      appInfo = await invoke<AppInfo>('get_app_info')
    } catch (caught) {
      error = caught instanceof Error ? caught.message : 'Unable to load application information.'
    }
  }
</script>

<svelte:head>
  <title>LanguaLinker</title>
</svelte:head>

<main>
  <p class="eyebrow">Phase 1 shell</p>
  <h1>LanguaLinker</h1>
  <p class="summary">Svelte 5 frontend and Tauri IPC smoke surface.</p>

  <section aria-labelledby="runtime-heading">
    <h2 id="runtime-heading">Runtime</h2>
    <dl>
      <div><dt>Event</dt><dd>{eventStatus}</dd></div>
      <div><dt>Adapter</dt><dd>{isTauri() ? 'Tauri native' : 'Browser demo'}</dd></div>
    </dl>
    <button onclick={loadAppInfo}>Load app info</button>
    {#if appInfo}
      <pre>{JSON.stringify(appInfo, null, 2)}</pre>
    {/if}
    {#if error}
      <p class="error" role="alert">{error}</p>
    {/if}
  </section>
</main>