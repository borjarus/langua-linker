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
  .ratings {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 6.5rem), 1fr));
    gap: var(--space-2);
  }

  .rating {
    flex-direction: column;
    gap: 0;
    border-top-width: 4px;
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
