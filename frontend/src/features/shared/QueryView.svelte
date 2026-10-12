<script lang="ts" generics="T">
  import type { Snippet } from 'svelte'
  import { useServices } from '../../app/context'
  import EmptyState from '../../components/EmptyState.svelte'
  import ErrorState from '../../components/ErrorState.svelte'
  import Skeleton from '../../components/Skeleton.svelte'
  import type { QueryHandle } from '../../services/query.svelte'

  interface Props {
    query: QueryHandle<T>
    ready: Snippet<[T]>
    /** Defaults to a generic empty state; screens should provide a specific CTA. */
    empty?: Snippet<[T]>
    skeletonLines?: number
  }

  let { query, ready, empty, skeletonLines = 4 }: Props = $props()
  const { i18n } = useServices()
</script>

{#if query.state.status === 'loading'}
  <Skeleton lines={skeletonLines} />
{:else if query.state.status === 'error'}
  <ErrorState error={query.state.error} onretry={() => void query.refresh()} />
{:else}
  {#if query.refreshError}
    <ErrorState error={query.refreshError} compact onretry={() => void query.refresh()} />
  {:else if query.stale && query.fetching}
    <p class="visually-hidden" role="status">{i18n.t('state.stale')}</p>
  {/if}
  {#if query.state.status === 'empty'}
    {#if empty}{@render empty(query.state.data)}{:else}<EmptyState title={i18n.t('state.emptyTitle')} />{/if}
  {:else}
    {@render ready(query.state.data)}
  {/if}
{/if}
