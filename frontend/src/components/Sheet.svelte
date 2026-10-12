<script lang="ts">
  import type { Snippet } from 'svelte'
  import Dialog from './Dialog.svelte'

  interface Props {
    open: boolean
    title: string
    onclose: () => void
    children: Snippet
    actions?: Snippet
  }

  let { open, title, onclose, children, actions }: Props = $props()
</script>

<Dialog {open} {title} {onclose} {children} {actions} variant="sheet" />
