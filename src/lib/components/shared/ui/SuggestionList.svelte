<script lang="ts">
  /**
   * SuggestionList — the small list that opens under or over a text field while you type, offering
   * completions: `/skill` names in the assistant's composer, and the same shape anywhere a field wants
   * to suggest.
   *
   * The field keeps the keyboard focus the whole time, so this list never takes it: the owner moves
   * `active` with the arrow keys and picks with Enter or Tab, and a mouse press on a row picks it
   * without stealing the focus from the field (`mousedown` is cancelled). The list only draws; where it
   * sits is the owner's (wrap it in a positioned element).
   */
  export interface Suggestion {
    id: string;
    label: string;
    /** A line of explanation under the label. */
    description?: string;
    /** Small text on the right. */
    meta?: string;
  }

  let {
    items,
    active = 0,
    id,
    ariaLabel,
    onpick,
  }: {
    items: readonly Suggestion[];
    /** Index of the highlighted row. */
    active?: number;
    /** DOM id of the list, for the field's `aria-controls`. Rows are `<id>-<n>`. */
    id: string;
    ariaLabel: string;
    onpick: (item: Suggestion) => void;
  } = $props();

  let list = $state<HTMLElement | null>(null);

  // The highlighted row stays in view as the arrow keys walk past the visible part.
  $effect(() => {
    void active;
    list?.querySelector<HTMLElement>('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest' });
  });
</script>

<div class="sl" role="listbox" {id} aria-label={ariaLabel} bind:this={list}>
  {#each items as item, i (item.id)}
    <div
      class="row"
      class:active={i === active}
      role="option"
      id="{id}-{i}"
      tabindex="-1"
      aria-selected={i === active}
      onmousedown={(e) => { e.preventDefault(); onpick(item); }}
    >
      <span class="label">{item.label}</span>
      {#if item.meta}<span class="meta">{item.meta}</span>{/if}
      {#if item.description}<span class="desc">{item.description}</span>{/if}
    </div>
  {/each}
</div>

<style>
  .sl {
    display: flex; flex-direction: column;
    max-height: 220px; overflow-y: auto;
    padding: 4px;
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-md);
  }
  .row {
    display: grid; grid-template-columns: minmax(0, 1fr) auto; column-gap: 8px;
    padding: 5px 8px;
    border-radius: var(--radius-sm);
    cursor: pointer;
  }
  .row:hover, .row.active { background: var(--bg-hover); }
  .row.active { box-shadow: inset 2px 0 0 var(--accent); }
  .label { font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .meta { font-size: var(--font-size-2xs); color: var(--text-muted); }
  .desc { grid-column: 1 / -1; font-size: var(--font-size-2xs); color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
