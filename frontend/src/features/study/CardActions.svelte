<script lang="ts" module>
  export type HidingAction = 'bury' | 'suspend' | 'delete'
</script>

<script lang="ts">
  import { CARD_FLAGS, type CardFlag, type StudyCardDto } from '../../api/types'
  import { useServices } from '../../app/context'
  import Dialog from '../../components/Dialog.svelte'
  import Icon from '../../components/Icon.svelte'

  interface Props {
    card: StudyCardDto
    disabled: boolean
    onhide: (action: HidingAction) => void
    onflag: (flag: CardFlag) => void
    onedit: () => void
  }

  let { card, disabled, onhide, onflag, onedit }: Props = $props()
  const { i18n } = useServices()
  const flagGroup = $props.id()

  let open = $state<'menu' | 'flag' | 'delete' | null>(null)
  // Follows the card until the user picks another flag in the dialog.
  let flag = $derived<CardFlag>(card.flag)

  function run(action: HidingAction) {
    open = null
    onhide(action)
  }
</script>

<button type="button" class="btn" {disabled} aria-haspopup="dialog" onclick={() => (open = 'menu')}>
  <Icon name="settings" />
  {i18n.t('study.moreActions')}
</button>

<Dialog open={open === 'menu'} title={i18n.t('study.moreActions')} onclose={() => (open = null)} variant="sheet">
  <ul class="menu">
    <li>
      <button type="button" class="btn btn-ghost item" onclick={() => { open = null; onedit() }}>
        <Icon name="cards" />{i18n.t('study.actions.edit')}
      </button>
    </li>
    <li>
      <button type="button" class="btn btn-ghost item" onclick={() => (open = 'flag')}>
        <Icon name="flag" />{i18n.t('study.actions.flag')}
      </button>
    </li>
    <li>
      <button type="button" class="btn btn-ghost item" onclick={() => run('bury')}>
        <Icon name="clock" />{i18n.t('study.actions.bury')}
      </button>
    </li>
    <li>
      <button type="button" class="btn btn-ghost item" onclick={() => run('suspend')}>
        <Icon name="close" />{i18n.t('study.actions.suspend')}
      </button>
    </li>
    <li>
      <button type="button" class="btn btn-ghost item danger" onclick={() => (open = 'delete')}>
        <Icon name="alert" />{i18n.t('study.actions.delete')}
      </button>
    </li>
  </ul>
</Dialog>

<Dialog open={open === 'flag'} title={i18n.t('study.flagTitle')} onclose={() => (open = null)}>
  <fieldset class="flags">
    <legend class="visually-hidden">{i18n.t('study.flagTitle')}</legend>
    {#each CARD_FLAGS as option (option)}
      <label class="flag-option">
        <input type="radio" name={flagGroup} value={option} bind:group={flag} />
        <span class="swatch flag-{option}" aria-hidden="true"></span>
        <span>{i18n.t(`study.flags.${option}`)}</span>
      </label>
    {/each}
  </fieldset>
  {#snippet actions()}
    <button type="button" class="btn" onclick={() => (open = null)}>{i18n.t('common.cancel')}</button>
    <button type="button" class="btn btn-primary" onclick={() => { open = null; onflag(flag) }}>{i18n.t('study.flagApply')}</button>
  {/snippet}
</Dialog>

<!-- Destructive: a stray backdrop tap must not decide it. -->
<Dialog open={open === 'delete'} title={i18n.t('study.deleteTitle')} onclose={() => (open = null)} dismissible={false}>
  <p>{i18n.t('study.deleteBody')}</p>
  {#snippet actions()}
    <button type="button" class="btn" onclick={() => (open = null)}>{i18n.t('common.cancel')}</button>
    <button type="button" class="btn btn-danger" onclick={() => run('delete')}>{i18n.t('study.deleteConfirm')}</button>
  {/snippet}
</Dialog>

<style>
  .menu {
    display: grid;
    gap: var(--space-1);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .item {
    justify-content: flex-start;
    width: 100%;
    text-align: start;
  }

  .danger {
    color: var(--color-error);
  }

  .flags {
    display: grid;
    gap: var(--space-1);
    margin: 0;
    padding: 0;
    border: 0;
  }

  .flag-option {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-height: var(--touch-target);
    cursor: pointer;
  }

  input {
    width: 1.25rem;
    height: 1.25rem;
    flex: none;
  }

  .swatch {
    width: 1rem;
    height: 1rem;
    flex: none;
    border: 1px solid var(--color-border-strong);
    border-radius: 50%;
  }

  .flag-red { background: var(--color-flag-red); }
  .flag-orange { background: var(--color-flag-orange); }
  .flag-green { background: var(--color-flag-green); }
  .flag-blue { background: var(--color-flag-blue); }
  .flag-pink { background: var(--color-flag-pink); }
  .flag-turquoise { background: var(--color-flag-turquoise); }
  .flag-purple { background: var(--color-flag-purple); }
</style>
