<script lang="ts">
  import type { GrammarTipDto, Id, NoteContentChange } from '../../api/types'
  import { useServices } from '../../app/context'
  import Icon from '../../components/Icon.svelte'

  interface Props {
    tips: GrammarTipDto[]
    busy: boolean
    /** Resolves to `true` when the change was saved. */
    onchange: (change: NoteContentChange) => Promise<boolean>
    ondelete: (tip: GrammarTipDto) => void
  }

  let { tips, busy, onchange, ondelete }: Props = $props()
  const { i18n } = useServices()
  const ids = $props.id()

  const ordered = $derived([...tips].sort((a, b) => a.order - b.order))
  let draft = $state('')
  let editing = $state<{ id: Id; text: string } | null>(null)

  async function add(event: SubmitEvent) {
    event.preventDefault()
    if (!draft.trim()) return
    if (await onchange({ type: 'addTip', text: draft })) draft = ''
  }

  async function saveEdit() {
    if (!editing || !editing.text.trim()) return
    if (await onchange({ type: 'editTip', tipId: editing.id, text: editing.text })) editing = null
  }

  function move(index: number, offset: -1 | 1) {
    const order = ordered.map((tip) => tip.id)
    const target = index + offset
    const moved = order[index]
    const other = order[target]
    if (moved === undefined || other === undefined) return
    order[index] = other
    order[target] = moved
    void onchange({ type: 'reorderTips', tipIds: order })
  }
</script>

<section class="stack" aria-labelledby="{ids}-title">
  <h2 id="{ids}-title">{i18n.t('grammar.title')}</h2>
  {#if ordered.length === 0}
    <p class="muted">{i18n.t('grammar.empty')}</p>
  {:else}
    <ol class="tips">
      {#each ordered as tip, index (tip.id)}
        <li class="tip">
          {#if editing?.id === tip.id}
            <div class="field">
              <label for="{ids}-edit">{i18n.t('card.tips.editLabel')}</label>
              <textarea id="{ids}-edit" class="input" rows="2" maxlength="500" bind:value={editing.text}></textarea>
            </div>
            <div class="row">
              <button type="button" class="btn btn-primary" disabled={busy || !editing.text.trim()} onclick={saveEdit}>{i18n.t('common.save')}</button>
              <button type="button" class="btn" onclick={() => (editing = null)}>{i18n.t('common.cancel')}</button>
            </div>
          {:else}
            <p class="text">{tip.text}</p>
            <div class="row tools">
              <button type="button" class="btn btn-ghost btn-icon" disabled={busy || index === 0} aria-label={i18n.t('card.tips.moveUp', { position: index + 1 })} onclick={() => move(index, -1)}>
                <Icon name="chevronUp" />
              </button>
              <button type="button" class="btn btn-ghost btn-icon" disabled={busy || index === ordered.length - 1} aria-label={i18n.t('card.tips.moveDown', { position: index + 1 })} onclick={() => move(index, 1)}>
                <Icon name="chevronDown" />
              </button>
              <button type="button" class="btn btn-ghost" disabled={busy} onclick={() => (editing = { id: tip.id, text: tip.text })}>
                <Icon name="edit" />{i18n.t('card.edit')}
              </button>
              <button type="button" class="btn btn-ghost danger" disabled={busy} onclick={() => ondelete(tip)}>
                <Icon name="trash" />{i18n.t('card.delete')}
              </button>
            </div>
          {/if}
        </li>
      {/each}
    </ol>
  {/if}
  <form class="stack add" onsubmit={add}>
    <div class="field">
      <label for="{ids}-new">{i18n.t('card.tips.newLabel')}</label>
      <textarea id="{ids}-new" class="input" rows="2" maxlength="500" bind:value={draft}></textarea>
    </div>
    <button type="submit" class="btn" disabled={busy || !draft.trim()}><Icon name="plus" />{i18n.t('card.tips.add')}</button>
  </form>
</section>

<style>
  h2 {
    font-size: var(--text-lg);
  }

  .tips {
    display: grid;
    gap: var(--space-2);
    margin: 0;
    padding-inline-start: var(--space-5);
  }

  .tip {
    display: grid;
    gap: var(--space-2);
    padding: var(--space-2) 0;
    border-bottom: 1px solid var(--color-border);
  }

  .text {
    overflow-wrap: anywhere;
  }

  .tools {
    gap: var(--space-1);
  }

  .add {
    justify-items: start;
  }

  .add .field {
    width: 100%;
  }

  .danger {
    color: var(--color-error);
  }
</style>
