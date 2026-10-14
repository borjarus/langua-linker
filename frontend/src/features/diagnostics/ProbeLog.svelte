<script lang="ts" module>
  export interface LogEntry {
    id: number
    time: string
    text: string
    failed?: boolean
  }

  let nextId = 0

  /** Appends a timestamped entry; the newest is listed first. */
  export function logEntry(text: string, failed = false): LogEntry {
    nextId += 1
    return { id: nextId, time: new Date().toLocaleTimeString(), text, failed }
  }
</script>

<script lang="ts">
  import { useServices } from '../../app/context'

  let { entries }: { entries: LogEntry[] } = $props()
  const { i18n } = useServices()
</script>

<div class="stack">
  <h3>{i18n.t('diagnostics.events')}</h3>
  {#if entries.length === 0}
    <p class="muted">{i18n.t('diagnostics.noEvents')}</p>
  {:else}
    <ol class="log" role="log">
      {#each entries as entry (entry.id)}
        <li data-failed={entry.failed ?? false}><time class="muted">{entry.time}</time> <span>{entry.text}</span></li>
      {/each}
    </ol>
  {/if}
</div>

<style>
  .log {
    display: grid;
    gap: var(--space-1);
    margin: 0;
    padding: 0;
    list-style: none;
    overflow-wrap: anywhere;
  }

  .log li[data-failed='true'] {
    color: var(--color-error);
  }
</style>
