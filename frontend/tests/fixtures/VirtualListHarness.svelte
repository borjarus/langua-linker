<script lang="ts">
  import VirtualList from '../../src/components/VirtualList.svelte'

  interface Props {
    count: number
    onrange?: (start: number, end: number) => void
  }

  let { count, onrange }: Props = $props()
</script>

<VirtualList itemCount={count} label="rows" {onrange}>
  {#snippet row(index)}
    <span>row {index}</span>
  {/snippet}
</VirtualList>
