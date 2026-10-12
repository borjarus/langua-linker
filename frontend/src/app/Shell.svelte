<script lang="ts">
  import { tick } from 'svelte'
  import Icon, { type IconName } from '../components/Icon.svelte'
  import Toast from '../components/Toast.svelte'
  import type { TranslationKey } from '../locales/types'
  import { useServices } from './context'
  import DemoBanner from './DemoBanner.svelte'
  import RouteOutlet from './RouteOutlet.svelte'
  import type { RouteName } from './routes'

  const { i18n, router } = useServices()

  const NAV: { route: RouteName; icon: IconName; label: TranslationKey; matches: RouteName[] }[] = [
    { route: 'home', icon: 'home', label: 'nav.home', matches: ['home', 'deck', 'study', 'summary'] },
    { route: 'browser', icon: 'search', label: 'nav.browser', matches: ['browser', 'card'] },
    { route: 'editor', icon: 'plus', label: 'nav.add', matches: ['editor'] },
    { route: 'stats', icon: 'chart', label: 'nav.stats', matches: ['stats'] },
    { route: 'settings', icon: 'settings', label: 'nav.settings', matches: ['settings', 'settingsTransfer', 'settingsBackup', 'settingsAttributions', 'gallery'] },
  ]
  const activeName = $derived(router.current.name)

  // Focus and scroll are restored after each navigation so keyboard and screen-reader users land
  // on the new screen's heading instead of the previous position.
  $effect(() => {
    void router.current
    const kind = router.kind
    void tick().then(() => {
      router.restoreScroll()
      if (kind === 'initial') return
      document.querySelector<HTMLElement>('[data-screen-heading]')?.focus({ preventScroll: true })
    })
  })
</script>

<a class="skip" href="#main" onclick={(event) => {
  event.preventDefault()
  document.getElementById('main')?.focus()
}}>{i18n.t('app.skipToContent')}</a>

<DemoBanner />

<div class="layout">
  <nav class="nav" aria-label={i18n.t('nav.primary')}>
    <ul>
      {#each NAV as item (item.route)}
        {@const current = item.matches.includes(activeName as RouteName)}
        <li>
          <a href={`#/${item.route}`} class="nav-link" aria-current={current ? 'page' : undefined}>
            <Icon name={item.icon} size={24} />
            <span>{i18n.t(item.label)}</span>
          </a>
        </li>
      {/each}
    </ul>
  </nav>

  <main id="main" tabindex="-1">
    <RouteOutlet />
  </main>
</div>

<Toast />

<style>
  .skip {
    position: absolute;
    inset-inline-start: var(--space-2);
    top: -10rem;
    z-index: 100;
    padding: var(--space-2) var(--space-4);
    border-radius: var(--radius-sm);
    background: var(--color-primary);
    color: var(--color-on-primary);
  }

  .skip:focus {
    top: var(--space-2);
  }

  main {
    width: min(100%, var(--content-width));
    margin-inline: auto;
    padding: var(--space-4);
    padding-bottom: calc(var(--bottom-nav-height) + var(--space-6) + env(safe-area-inset-bottom));
    padding-inline: max(var(--space-4), env(safe-area-inset-left)) max(var(--space-4), env(safe-area-inset-right));
  }

  .nav {
    position: fixed;
    inset-inline: 0;
    bottom: 0;
    z-index: 40;
    border-top: 1px solid var(--color-border);
    background: var(--color-surface);
    padding-bottom: env(safe-area-inset-bottom);
  }

  .nav ul {
    display: grid;
    grid-template-columns: repeat(5, 1fr);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .nav-link {
    display: grid;
    justify-items: center;
    gap: 0.125rem;
    min-height: var(--bottom-nav-height);
    padding: var(--space-2) var(--space-1);
    color: var(--color-text-muted);
    font-size: var(--text-xs);
    text-align: center;
    text-decoration: none;
  }

  .nav-link[aria-current='page'] {
    color: var(--color-primary);
    font-weight: var(--weight-bold);
  }

  /* Sidebar on tablets and the development desktop window. */
  @media (min-width: 52rem) {
    .layout {
      display: grid;
      grid-template-columns: var(--sidebar-width) minmax(0, 1fr);
      min-height: 100dvh;
    }

    .nav {
      position: sticky;
      top: 0;
      height: 100dvh;
      border-top: 0;
      border-inline-end: 1px solid var(--color-border);
      padding: var(--space-4) var(--space-2);
    }

    .nav ul {
      grid-template-columns: 1fr;
      gap: var(--space-1);
    }

    .nav-link {
      grid-template-columns: auto 1fr;
      justify-items: start;
      align-items: center;
      gap: var(--space-3);
      min-height: var(--touch-target);
      padding: var(--space-2) var(--space-3);
      border-radius: var(--radius-md);
      font-size: var(--text-md);
    }

    .nav-link[aria-current='page'] {
      background: var(--color-primary-soft);
    }

    main {
      padding-bottom: var(--space-6);
    }
  }
</style>
