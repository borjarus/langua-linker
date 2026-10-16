<script lang="ts">
  import type { Id } from '../../api/types'
  import { useServices } from '../../app/context'
  import AssociationPanel from '../../components/AssociationPanel.svelte'
  import GrammarTipList from '../../components/GrammarTipList.svelte'
  import { useQuery } from '../../services/query.svelte'
  import QueryView from '../shared/QueryView.svelte'

  interface Props {
    noteId: Id
    lemma: string | null
  }

  let { noteId, lemma }: Props = $props()
  const { api, i18n, queries } = useServices()
  const content = useQuery(queries, () => ({ key: ['noteContent', noteId], fetch: () => api.getNoteContent(noteId) }))
</script>

<QueryView query={content} skeletonLines={3}>
  {#snippet ready(note)}
    <div class="stack">
      <p class="translation"><strong lang={note.targetLanguage}>{note.front}</strong> — <span lang="pl">{note.back}</span></p>
      {#if lemma && lemma !== note.front}<p class="muted">{i18n.t('wordSheet.baseForm', { lemma })}</p>{/if}
      <!-- In the sheet the user asked for the details, so sections start expanded. -->
      {#if note.capabilities.grammarTips}<GrammarTipList tips={note.grammarTips} expanded />{/if}
      {#if note.capabilities.associations}<AssociationPanel associations={note.associations} expanded />{/if}
    </div>
  {/snippet}
</QueryView>

<style>
  .translation {
    font-size: var(--text-lg);
  }
</style>
