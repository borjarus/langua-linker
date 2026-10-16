<script lang="ts">
  import { RATINGS, type Rating, type RatingPreviewDto } from '../api/types'
  import { useServices } from '../app/context'

  interface Props {
    /** Interval previews from the backend; without them buttons show only the rating. */
    preview: RatingPreviewDto | null
    /** True while a rating is being saved; blocks a second rating of the same card. */
    disabled?: boolean
    selected?: Rating | null
    onrate: (rating: Rating) => void
  }

  let { preview, disabled = false, selected = null, onrate }: Props = $props()
  const { i18n } = useServices()
  const groupId = $props.id()
</script>

<div class="ratings" role="group" aria-labelledby={groupId}>
  <span id={groupId} class="visually-hidden">{i18n.t('study.rateGroup')}</span>
  {#each RATINGS as rating, index (rating)}
    {@const name = i18n.t(`study.ratings.${rating}`)}
    {@const interval = preview ? i18n.formatInterval(preview.intervals[rating].intervalSeconds) : null}
    <button
      type="button"
      class="btn rating rating-{rating.toLowerCase()}"
      class:selected={selected === rating}
      {disabled}
      aria-keyshortcuts={String(index + 1)}
      aria-label={interval ? i18n.t('study.ratingWithInterval', { rating: name, interval }) : name}
      onclick={() => {
        if (!disabled) onrate(rating)
      }}
    >
      <span class="name">{name}</span>
      {#if interval}<span class="interval" aria-hidden="true">{interval}</span>{/if}
    </button>
  {/each}
</div>

<style>
  /* Wrapping flex, not a rem/em grid: Android WebView text zoom enlarges the drawn text but not
     CSS lengths (1em stays 16 px), so fixed column minimums kept three narrow columns at 200% and
     broke intervals mid-word ("minute/s"). The longest word (min-content) is measured from the
     real text, so buttons move to the next line instead of splitting words. */
  .ratings {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .rating {
    flex: 1 1 6.5rem;
    /* Overrides `.btn`'s fixed touch minimum; a word plus padding is always wider than 48 px. */
    min-width: min-content;
    flex-direction: column;
    gap: 0;
    border-top-width: 4px;
    overflow-wrap: normal;
  }

  .interval {
    color: var(--color-text-muted);
    font-size: var(--text-xs);
    font-weight: var(--weight-regular);
  }

  .rating-again {
    border-top-color: var(--color-rating-again);
  }

  .rating-hard {
    border-top-color: var(--color-rating-hard);
  }

  .rating-good {
    border-top-color: var(--color-rating-good);
  }

  .rating-easy {
    border-top-color: var(--color-rating-easy);
  }

  .selected {
    background: var(--color-primary-soft);
  }
</style>
