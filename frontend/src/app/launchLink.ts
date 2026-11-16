import type { PlatformBridge } from '../api/platform'
import { deepLinkToPath } from './routes'
import type { Router } from './router.svelte'

/**
 * Opens the screen a tapped reminder points to (plan Phase 15 steps 8 and 12). The URL comes
 * from the native side, is validated by the deep-link adapter (only `langualinker://app/<route>`
 * of an allowed route, parameters dropped) and anything else is ignored, so the app stays where
 * it is. A deck or card that no longer exists is handled by the target screen's not-found state
 * with its way back home.
 */
export async function openLaunchLink(bridge: Pick<PlatformBridge, 'takeLaunchUrl'>, router: Pick<Router, 'navigate'>): Promise<boolean> {
  let url: string | null
  try {
    url = await bridge.takeLaunchUrl()
  } catch {
    return false
  }
  const path = url ? deepLinkToPath(url) : null
  if (!path) return false
  return router.navigate(path)
}

/** Checks for a launch link now and whenever the app returns to the foreground. */
export function watchLaunchLinks(bridge: Pick<PlatformBridge, 'takeLaunchUrl'>, router: Pick<Router, 'navigate'>, doc: Document = document): () => void {
  void openLaunchLink(bridge, router)
  const onVisible = () => {
    if (doc.visibilityState === 'visible') void openLaunchLink(bridge, router)
  }
  doc.addEventListener('visibilitychange', onVisible)
  return () => doc.removeEventListener('visibilitychange', onVisible)
}
