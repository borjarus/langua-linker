<script lang="ts">
  import type { MarkdownBlock } from '../services/markdownBlocks'
  import RichText from './RichText.svelte'
  import MarkdownBlocks from './MarkdownBlocks.svelte'

  /** Parsed block Markdown, rendered as text nodes (never HTML). Headings start below the card's h3. */
  let { blocks }: { blocks: MarkdownBlock[] } = $props()
</script>

{#each blocks as block, index (index)}
  {#if block.kind === 'heading'}
    {#if block.level <= 2}
      <h4 class="md-heading"><RichText text={block.text} /></h4>
    {:else}
      <h5 class="md-heading md-minor"><RichText text={block.text} /></h5>
    {/if}
  {:else if block.kind === 'paragraph'}
    <p class="md-paragraph">
      {#each block.lines as line, lineIndex (lineIndex)}{#if lineIndex > 0}<br />{/if}<RichText text={line} />{/each}
    </p>
  {:else if block.kind === 'list'}
    <ul class="md-list">
      {#each block.items as item, itemIndex (itemIndex)}
        <li><RichText text={item} /></li>
      {/each}
    </ul>
  {:else if block.kind === 'quote'}
    <blockquote class="md-quote"><MarkdownBlocks blocks={block.blocks} /></blockquote>
  {:else}
    <hr class="md-rule" />
  {/if}
{/each}

<style>
  .md-heading {
    margin: var(--space-3) 0 var(--space-2);
    font-size: 1rem;
    line-height: 1.3;
  }

  .md-minor {
    font-size: 0.95rem;
  }

  .md-paragraph {
    margin: 0 0 var(--space-2);
    line-height: 1.5;
  }

  .md-list {
    margin: 0 0 var(--space-2);
    padding-inline-start: 1.25rem;
    line-height: 1.5;
  }

  .md-quote {
    margin: 0 0 var(--space-2);
    padding: var(--space-2) var(--space-3);
    border-inline-start: 3px solid var(--color-primary);
    border-radius: var(--radius-sm);
    background: var(--color-surface-sunken);
  }

  .md-quote :global(.md-paragraph:last-child) {
    margin-bottom: 0;
  }

  .md-rule {
    margin: var(--space-3) 0;
    border: 0;
    border-top: 1px solid var(--color-border);
  }
</style>
