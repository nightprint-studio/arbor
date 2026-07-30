<script lang="ts">
  /**
   * "There is nothing here" — the quiet line a list shows instead of rows.
   *
   * `description` is optional and second on purpose. A list that is empty for a
   * reason the user can act on ("no project matches this filter") needs the
   * reason said, and a list that is simply empty should not grow a second line of
   * text to say so twice.
   */
  interface Props {
    message: string;
    /** Why it is empty, when that is worth a sentence. */
    description?: string;
    compact?: boolean;
  }

  let { message, description, compact = false }: Props = $props();
</script>

<div class="empty" class:compact>
  <span class="empty-message">{message}</span>
  {#if description}
    <span class="empty-description">{description}</span>
  {/if}
</div>

<style>
  .empty {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 6px 20px;
    font-size: var(--font-size-xs);
    color: var(--text-disabled);
    font-style: italic;
  }
  .empty.compact { padding: 4px 16px; }

  /* The headline carries the weight; the reason stays subordinate to it. */
  .empty-description {
    font-style: normal;
    line-height: 1.45;
    max-width: 46ch;
  }
</style>
