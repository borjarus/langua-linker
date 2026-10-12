<script lang="ts">
  import BrowserScreen from '../features/browser/BrowserScreen.svelte'
  import CardScreen from '../features/card/CardScreen.svelte'
  import DeckScreen from '../features/deck/DeckScreen.svelte'
  import EditorScreen from '../features/editor/EditorScreen.svelte'
  import HomeScreen from '../features/home/HomeScreen.svelte'
  import NotFoundScreen from '../features/NotFoundScreen.svelte'
  import OnboardingScreen from '../features/onboarding/OnboardingScreen.svelte'
  import AttributionsScreen from '../features/settings/AttributionsScreen.svelte'
  import BackupScreen from '../features/settings/BackupScreen.svelte'
  import SettingsScreen from '../features/settings/SettingsScreen.svelte'
  import TransferScreen from '../features/settings/TransferScreen.svelte'
  import Skeleton from '../components/Skeleton.svelte'
  import StatsScreen from '../features/stats/StatsScreen.svelte'
  import StudyScreen from '../features/study/StudyScreen.svelte'
  import SummaryScreen from '../features/summary/SummaryScreen.svelte'
  import { useServices } from './context'

  const { router } = useServices()
  /** Statically false in beta/release builds, so the gallery chunk is not emitted. */
  const GALLERY_ENABLED = import.meta.env.VITE_APP_PROFILE !== 'release' && import.meta.env.VITE_APP_PROFILE !== 'beta'
  const route = $derived(router.current)
  // Each navigation remounts the screen, so per-screen state and subscriptions start clean.
  const routeKey = $derived(`${route.name}:${JSON.stringify(route.params)}:${JSON.stringify(route.query)}`)
</script>

{#key routeKey}
  {#if route.name === 'home'}
    <HomeScreen />
  {:else if route.name === 'deck'}
    <DeckScreen id={route.params.id} />
  {:else if route.name === 'study'}
    <StudyScreen deckId={route.params.deckId} sessionId={route.query.session ?? null} />
  {:else if route.name === 'summary'}
    <SummaryScreen sessionId={route.params.sessionId} />
  {:else if route.name === 'card'}
    <CardScreen id={route.params.id} />
  {:else if route.name === 'editor'}
    <EditorScreen deckId={route.query.deckId ?? null} noteId={route.query.noteId ?? null} />
  {:else if route.name === 'browser'}
    <BrowserScreen query={route.query.q ?? ''} deckId={route.query.deck ?? null} />
  {:else if route.name === 'stats'}
    <StatsScreen />
  {:else if route.name === 'settings'}
    <SettingsScreen />
  {:else if route.name === 'settingsTransfer'}
    <TransferScreen />
  {:else if route.name === 'settingsBackup'}
    <BackupScreen />
  {:else if route.name === 'settingsAttributions'}
    <AttributionsScreen />
  {:else if route.name === 'onboarding'}
    <OnboardingScreen />
  {:else if route.name === 'gallery' && GALLERY_ENABLED}
    {#await import('../features/gallery/GalleryScreen.svelte')}
      <Skeleton lines={8} />
    {:then gallery}
      <gallery.default />
    {/await}
  {:else}
    <NotFoundScreen path={route.path} />
  {/if}
{/key}
