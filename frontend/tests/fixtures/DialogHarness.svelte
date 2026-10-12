<script lang="ts">
  import Dialog from '../../src/components/Dialog.svelte'

  // Test harness: literal strings here are test data, not UI (tests/ is outside the i18n lint).
  let open = $state(false)
</script>

<button type="button" onclick={() => (open = true)}>open</button>
<Dialog {open} title="Example dialog" onclose={() => (open = false)}>
  <p>Body</p>
  {#snippet actions()}
    <button type="button">first</button>
    <button type="button">second</button>
  {/snippet}
</Dialog>
