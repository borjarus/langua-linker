<script lang="ts" module>
  export type ScheduleAction = { type: 'suspend' | 'unsuspend' | 'delete' } | { type: 'flag'; flag: CardFlag }
</script>

<script lang="ts">
  import { CARD_FLAGS, type CardDetailsDto, type CardFlag, type TargetLanguage } from '../../api/types'
  import { useServices } from '../../app/context'
  import Chip from '../../components/Chip.svelte'
  import Disclosure from '../../components/Disclosure.svelte'
  import Icon from '../../components/Icon.svelte'

  interface Props {
    card: CardDetailsDto
    /** The card the screen was opened for. */
    opened: boolean
    question: string
    targetLanguage: TargetLanguage
    busy: boolean
    onaction: (card: CardDetailsDto, action: ScheduleAction) => void
  }

  let { card, opened, question, targetLanguage, busy, onaction }: Props = $props()
  const { i18n } = useServices()
  const ids = $props.id()

  const suspended = $derived(card.queue === 'Suspended')
  const buried = $derived(card.queue === 'UserBuried' || card.queue === 'SchedBuried')
  const language = $derived(i18n.t(`deck.targetLanguages.${targetLanguage}`))
  const days = (value: number) => i18n.t('card.days', { count: Math.round(value * 10) / 10 })
</script>

<section class="surface stack schedule" class:opened aria-labelledby="{ids}-title">
  <div class="row heading">
    <h3 id="{ids}-title">
      {i18n.t(`card.direction.${card.direction}`, { language })}
    </h3>
    {#if opened}<Chip label={i18n.t('card.opened')} tone="primary" />{/if}
  </div>
  <p class="question"><span class="muted">{i18n.t('card.asks')}</span> <span dir="auto">{question}</span></p>
  <div class="row">
    <Chip label={i18n.t(`study.states.${card.state}`)} />
    {#if suspended}<Chip label={i18n.t('card.suspended')} tone="warning" icon="close" />{/if}
    {#if buried}<Chip label={i18n.t('card.buried')} tone="warning" icon="clock" />{/if}
    {#if card.flag !== 'none'}<Chip label={i18n.t('study.flagged', { flag: i18n.t(`study.flags.${card.flag}`) })} icon="flag" tone="warning" />{/if}
    <Chip label={i18n.t('card.inDeck', { name: card.deckName })} icon="folder" />
  </div>

  <dl class="stats">
    <div>
      <dt>{i18n.t('card.due')}</dt>
      <dd>{card.dueAt ? i18n.formatDate(card.dueAt, { dateStyle: 'medium', timeStyle: 'short' }) : i18n.t('card.notScheduled')}</dd>
    </div>
    <div>
      <dt><abbr title={i18n.t('card.stabilityHint')}>{i18n.t('card.stability')}</abbr></dt>
      <dd>{card.stability === null ? i18n.t('card.none') : days(card.stability)}</dd>
    </div>
    <div>
      <dt><abbr title={i18n.t('card.difficultyHint')}>{i18n.t('card.difficulty')}</abbr></dt>
      <dd>{card.difficulty === null ? i18n.t('card.none') : i18n.t('card.difficultyValue', { value: i18n.formatNumber(card.difficulty, { maximumFractionDigits: 1 }) })}</dd>
    </div>
    <div>
      <dt><abbr title={i18n.t('card.retrievabilityHint')}>{i18n.t('card.retrievability')}</abbr></dt>
      <dd>
        {#if card.retrievability}
          {i18n.formatPercent(card.retrievability.value)}
          <span class="muted computed">{i18n.t('card.computedAt', { time: i18n.formatDate(card.retrievability.computedAt, { dateStyle: 'medium', timeStyle: 'short' }) })}</span>
        {:else}
          {i18n.t('card.none')}
        {/if}
      </dd>
    </div>
    <div>
      <dt>{i18n.t('card.reps')}</dt>
      <dd>{i18n.formatNumber(card.reps)}</dd>
    </div>
    <div>
      <dt>{i18n.t('card.lapses')}</dt>
      <dd>{i18n.formatNumber(card.lapses)}</dd>
    </div>
  </dl>

  <Disclosure title={i18n.t('card.history')} count={card.history.length}>
    {#if card.history.length === 0}
      <p class="muted">{i18n.t('card.noHistory')}</p>
    {:else}
      <div class="table-scroll">
        <table>
          <caption class="visually-hidden">{i18n.t('card.history')}</caption>
          <thead>
            <tr>
              <th scope="col">{i18n.t('card.reviewedAt')}</th>
              <th scope="col">{i18n.t('card.rating')}</th>
              <th scope="col">{i18n.t('card.stateBefore')}</th>
              <th scope="col">{i18n.t('card.interval')}</th>
            </tr>
          </thead>
          <tbody>
            {#each card.history as log (log.id)}
              <tr>
                <td>{i18n.formatDate(log.reviewedAt, { dateStyle: 'short', timeStyle: 'short' })}</td>
                <td>{i18n.t(`study.ratings.${log.rating}`)}</td>
                <td>{i18n.t(`study.states.${log.stateBefore}`)}</td>
                <td>{i18n.formatInterval(log.intervalSeconds)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  </Disclosure>

  <div class="row actions">
    <button type="button" class="btn" disabled={busy} onclick={() => onaction(card, { type: suspended ? 'unsuspend' : 'suspend' })}>
      <Icon name={suspended ? 'refresh' : 'close'} />{i18n.t(suspended ? 'card.unsuspend' : 'card.suspend')}
    </button>
    <label class="flag">
      <span class="visually-hidden">{i18n.t('study.actions.flag')}</span>
      <select class="input" value={card.flag} disabled={busy} onchange={(event) => onaction(card, { type: 'flag', flag: event.currentTarget.value as CardFlag })}>
        {#each CARD_FLAGS as option (option)}<option value={option}>{i18n.t(`study.flags.${option}`)}</option>{/each}
      </select>
    </label>
    <button type="button" class="btn btn-ghost danger" disabled={busy} onclick={() => onaction(card, { type: 'delete' })}>
      <Icon name="trash" />{i18n.t('card.deleteCard')}
    </button>
  </div>
</section>

<style>
  .schedule.opened {
    border-color: var(--color-primary);
  }

  .heading {
    justify-content: space-between;
  }

  h3 {
    font-size: var(--text-lg);
    min-width: 0;
  }

  .question {
    overflow-wrap: anywhere;
  }

  .stats {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 9rem), 1fr));
    gap: var(--space-3);
    margin: 0;
  }

  .stats div {
    display: grid;
    gap: var(--space-1);
  }

  dt {
    color: var(--color-text-muted);
    font-size: var(--text-sm);
    /* "Przywoływalność" alone is wider than a phone column at 200% text. */
    hyphens: auto;
    overflow-wrap: anywhere;
  }

  dd {
    margin: 0;
    font-weight: var(--weight-medium);
  }

  abbr {
    text-decoration: underline dotted;
  }

  .computed {
    display: block;
    font-size: var(--text-sm);
    font-weight: normal;
  }

  .table-scroll {
    max-width: 100%;
    overflow-x: auto;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--text-sm);
  }

  th,
  td {
    padding: var(--space-1) var(--space-2);
    border-bottom: 1px solid var(--color-border);
    text-align: start;
  }

  /* A select is as wide as its longest option unless its width is relative. */
  .flag {
    flex: 1 1 9rem;
    min-width: 0;
  }

  .flag select {
    width: 100%;
    min-height: var(--touch-target);
  }

  .danger {
    color: var(--color-error);
  }
</style>
