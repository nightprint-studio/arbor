<script lang="ts">
  /**
   * One cell of a result grid: the value, whether it has been edited, and the
   * editor when it is open.
   *
   * ## The empty string and NULL are both reachable
   *
   * They are different values and a grid that could only produce one of them would
   * be unusable for text columns, so:
   *
   *  * **Enter** writes what is in the box — including nothing, which is the empty
   *    string;
   *  * **Ctrl+Enter** writes `NULL`, whatever is in the box;
   *  * **Esc** leaves the cell alone.
   *
   * The distinction is spelled out in the input's own tooltip rather than left to
   * the documentation: it is needed at the moment of typing, and that is the only
   * moment it can be explained.
   *
   * ## An edited cell is marked, not replaced
   *
   * The new value is shown, with a left edge in the accent colour and the original
   * on the tooltip. Somebody who has changed nine cells needs to see which nine
   * without pressing anything, and needs to be able to check what each one *was*
   * before pressing Store.
   */
  import { Paperclip } from 'lucide-svelte';
  import DataCellValue from '$lib/components/shared/ui/DataCellValue.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import type { CellValue } from '$lib/types/picus';

  interface Props {
    value: CellValue;
    /** The pending value, when this cell has one. `undefined` means untouched. */
    edited?: string | null | undefined;
    /** True while this is the cell with the editor open. */
    editing?: boolean;
    /**
     * This column's value was not fetched — `value` is its size in bytes.
     *
     * A relation tab replaces its large objects with their length rather than
     * pulling every byte of every row across the connection to draw a grid that
     * cannot show any of them. The cell then says what is there and how much of it,
     * and reads the real value only when asked.
     */
    masked?: boolean;
    /** The user asked to see a masked value. */
    onReveal?: () => void;
    /** Enter, or Ctrl+Enter for `null`. */
    onCommit?: (next: string | null) => void;
    onCancel?: () => void;
  }

  let {
    value,
    edited,
    editing = false,
    masked = false,
    onReveal,
    onCommit,
    onCancel,
  }: Props = $props();

  /** `1.2 MB`, `840 bytes` — the size in the unit a person reads. */
  function size(n: number): string {
    if (n < 1024) return `${n} byte${n === 1 ? '' : 's'}`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} kB`;
    return `${(n / (1024 * 1024)).toFixed(1)} MB`;
  }

  const touched = $derived(edited !== undefined);
  /** What the cell shows: the pending value where there is one. */
  const shown = $derived<CellValue>(touched ? (edited ?? null) : value);

  /** Seeded from the value as it stands, so a small correction is a small edit. */
  let draft = $state('');
  let input = $state<HTMLInputElement | null>(null);

  $effect(() => {
    if (!editing) return;
    draft = shown === null || shown === undefined ? '' : String(shown);
    // Focused in an effect rather than with `autofocus`, which the a11y rules
    // rightly refuse: the element only exists once editing starts.
    input?.focus();
    input?.select();
  });

  function keydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      onCancel?.();
      return;
    }
    if (event.key !== 'Enter') return;
    event.preventDefault();
    event.stopPropagation();
    onCommit?.(event.ctrlKey || event.metaKey ? null : draft);
  }
</script>

{#if masked}
  <!-- A button, not a styled span: it is the affordance for reading the value, and
       an affordance you can only reach with a mouse is a hole in a keyboard-first
       window. Tab reaches it, Enter opens it. -->
  {#if value === null || value === undefined}
    <DataCellValue value={null} />
  {:else}
    <button
      type="button"
      class="rc-lob"
      onclick={() => onReveal?.()}
      use:tooltip={{ content: 'Not fetched with the row — open it to read the value, or to save it to a file' }}
    >
      <Paperclip size={10} />
      <span>{size(Number(value) || 0)}</span>
    </button>
  {/if}
{:else if editing}
  <input
    bind:this={input}
    class="rc-input"
    value={draft}
    oninput={(e) => (draft = e.currentTarget.value)}
    onkeydown={keydown}
    onblur={() => onCancel?.()}
    aria-label="Edit this value"
    use:tooltip={{
      content: 'Enter writes this value — empty means the empty string. Ctrl+Enter writes NULL. Esc leaves it alone.',
    }}
  />
{:else if touched}
  <span
    class="rc-edited"
    use:tooltip={{
      content: `Not saved yet. Was ${value === null || value === undefined ? 'NULL' : `“${value}”`}.`,
    }}
  >
    <DataCellValue value={shown} />
  </span>
{:else}
  <DataCellValue {value} />
{/if}

<style>
  /* Fills the cell rather than sitting inside it: an input that shrank the text
     would make every edit shift the column's contents sideways. */
  .rc-input {
    width: 100%;
    min-width: 0;
    padding: 0 2px;
    border: 1px solid var(--accent);
    border-radius: 2px;
    background: var(--bg-base);
    color: var(--text-primary);
    font-family: var(--font-code);
    font-size: inherit;
    line-height: 1.4;
    outline: none;
  }

  /* Reads as a chip rather than as data: the number in it is a size, and a cell
     that looked like a value would have the user believe the column holds one. */
  .rc-lob {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 0 5px;
    border: 1px dashed var(--border);
    border-radius: var(--radius-sm);
    background: none;
    color: var(--text-muted);
    font-family: inherit;
    font-size: var(--font-size-2xs);
    line-height: 1.5;
    cursor: pointer;
  }
  .rc-lob:hover,
  .rc-lob:focus-visible {
    border-color: var(--accent);
    border-style: solid;
    color: var(--accent);
    outline: none;
  }

  .rc-edited {
    display: inline-flex;
    align-items: center;
    max-width: 100%;
    padding-left: 4px;
    border-left: 2px solid var(--accent);
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
