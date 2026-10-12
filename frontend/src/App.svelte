<script lang="ts">
  import { provideServices, type AppServices } from './app/context'
  import Shell from './app/Shell.svelte'

  interface Props {
    services: AppServices
    /** Starts router/theme/event listeners; returns their cleanup. */
    start: () => () => void
  }

  let { services, start }: Props = $props()
  // The root owns one isolated service graph; screens read it from context only.
  // svelte-ignore state_referenced_locally
  provideServices(services)

  $effect(() => start())
</script>

<Shell />
