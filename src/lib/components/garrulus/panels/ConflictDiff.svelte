<script lang="ts">
  /**
   * Two versions of one note, side by side, read-only.
   *
   * Garrulus resolves a conflict **per note**, not per line: the answer is "keep
   * mine", "take theirs", or "let me edit the note first". So this view exists to
   * be *read* — there is nothing to tick, and every control Corvus's
   * `ConflictDiffColumns` carries (per-line checkboxes, take-ours/take-theirs per
   * block, a master checkbox per column) would be a control with nothing behind
   * it. What is shared with Corvus is the part worth sharing: the same
   * `computeDiff` LCS and the same `buildDisplayItems` walker, so line numbers,
   * block boundaries and oversized-context clipping behave identically in both
   * products and are fixed in one place.
   *
   * Long lines **wrap** rather than scroll horizontally. These are notes: a
   * paragraph is one line, and asking the reader to scroll sideways through prose
   * to compare two versions of it is not a comparison.
   */
  import { ChevronDown } from 'lucide-svelte';
  import { highlight } from '$lib/utils/diff-formatter';
  import { buildDisplayItems } from '$lib/utils/conflict/conflict-display';
  import type { Region } from '$lib/utils/conflict/region-types';

  interface Props {
    /** Vault-relative path — the highlighter's only hint about the language. */
    path: string;
    /** The aligned region stream. Computed by the caller, which also counts the
     *  unmergeable blocks for its heading: running the same LCS twice over the
     *  same two texts would be the only cost of computing it here instead. */
    regions: Region[];
    /** Column headings — who wrote each side, in the user's vocabulary. */
    localLabel: string;
    remoteLabel: string;
  }

  let { path, regions, localLabel, remoteLabel }: Props = $props();

  /** Context blocks the reader asked to see in full, by the walker's own key. */
  let expanded = $state<string[]>([]);
  const expandedKeys = $derived(new Set(expanded));

  // The selection maps are empty on purpose: nothing here is selectable, and the
  // walker falls back to its defaults for fields this view never reads.
  const items = $derived(
    buildDisplayItems({
      regions,
      oursSelected: {},
      theirsSelected: {},
      fileKey: path,
      fullFile: false,
      expandedKeys,
    }),
  );
</script>

<div class="cd">
  <div class="cd-heads">
    <div class="cd-head cd-head-mine">{localLabel}</div>
    <div class="cd-head cd-head-theirs">{remoteLabel}</div>
  </div>

  {#each items as item, i (i)}
    {#if item.kind === 'context'}
      {#each item.lines as line, n (n)}
        <div class="cd-row cd-row-context">
          <div class="cd-cell">
            <span class="cd-num">{item.oursStart + n}</span>
            <code class="cd-code">{@html highlight(line, path)}</code>
          </div>
          <div class="cd-cell">
            <span class="cd-num">{item.theirsStart + n}</span>
            <code class="cd-code">{@html highlight(line, path)}</code>
          </div>
        </div>
      {/each}
    {:else if item.kind === 'collapsed'}
      <button
        type="button"
        class="cd-collapsed"
        onclick={() => (expanded = [...expanded, item.contextKey])}
      >
        <ChevronDown size={11} />
        <span>{item.hiddenLines} identical lines hidden — show them</span>
      </button>
    {:else}
      <div class="cd-row">
        <div class="cd-side cd-side-mine">
          {#if item.oursLines.length === 0}
            <div class="cd-absent">nothing here</div>
          {:else}
            {#each item.oursLines as line, n (n)}
              <div class="cd-cell">
                <span class="cd-num">{item.oursStart + n}</span>
                <code class="cd-code">{@html highlight(line, path)}</code>
              </div>
            {/each}
          {/if}
        </div>
        <div class="cd-side cd-side-theirs">
          {#if item.theirsLines.length === 0}
            <div class="cd-absent">nothing here</div>
          {:else}
            {#each item.theirsLines as line, n (n)}
              <div class="cd-cell">
                <span class="cd-num">{item.theirsStart + n}</span>
                <code class="cd-code">{@html highlight(line, path)}</code>
              </div>
            {/each}
          {/if}
        </div>
      </div>
    {/if}
  {/each}
</div>

<style>
  /* One grid for the whole view, every row subgrids from it: the two columns
     cannot drift apart, however the rows inside them are built. */
  .cd {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    align-content: start;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    overflow: hidden;
    background: var(--bg-base);
  }

  /* Not sticky: the card is the scrolling unit's child, not the scroller, and a
     `position: sticky` header inside this `overflow: hidden` box would stick to
     nothing. The headings ride up with their own card, which is what a reader
     comparing two short notes actually wants. */
  .cd-heads {
    grid-column: 1 / -1;
    display: grid;
    grid-template-columns: subgrid;
  }
  .cd-head {
    padding: 5px 10px;
    background: var(--bg-elevated);
    border-bottom: 1px solid var(--border-subtle);
    font-size: var(--font-size-xs);
    font-family: var(--font-ui-sans);
    color: var(--text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  /* Mine green, theirs blue — the pairing Corvus already uses for a merge
     (`ConflictDiffColumns`, theme "merge"). The other machine's version is not
     an error, so it does not get the error colour. */
  .cd-head-mine   { border-top: 2px solid color-mix(in srgb, var(--success) 55%, transparent); }
  .cd-head-theirs {
    border-top: 2px solid color-mix(in srgb, var(--accent) 55%, transparent);
    border-left: 1px solid var(--border-subtle);
  }

  .cd-row {
    grid-column: 1 / -1;
    display: grid;
    grid-template-columns: subgrid;
  }
  .cd-row-context > .cd-cell:last-child { border-left: 1px solid var(--border-subtle); }

  .cd-side { display: flex; flex-direction: column; min-width: 0; }
  .cd-side-mine   { background: color-mix(in srgb, var(--success) 6%, transparent); }
  .cd-side-theirs {
    background: color-mix(in srgb, var(--accent) 6%, transparent);
    border-left: 1px solid var(--border-subtle);
  }

  .cd-cell {
    display: flex;
    align-items: baseline;
    gap: 0;
    min-width: 0;
    padding: 1px 0;
  }

  .cd-num {
    flex-shrink: 0;
    min-width: 34px;
    padding: 0 8px;
    text-align: right;
    font-family: var(--font-code);
    font-size: var(--font-size-xs);
    line-height: 1.6;
    color: var(--text-disabled);
    user-select: none;
  }

  /* Wrapping, not scrolling — see the header comment. */
  .cd-code {
    flex: 1;
    min-width: 0;
    padding-right: 8px;
    font-family: var(--font-code);
    font-size: var(--font-size-sm);
    line-height: 1.6;
    color: var(--text-primary);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .cd-absent {
    padding: 4px 10px 4px 42px;
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-xs);
    font-style: italic;
    color: var(--text-disabled);
  }

  .cd-collapsed {
    grid-column: 1 / -1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 4px 10px;
    background: var(--bg-elevated);
    border: none;
    border-top: 1px dashed var(--border-subtle);
    border-bottom: 1px dashed var(--border-subtle);
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-2xs);
    cursor: pointer;
  }
  .cd-collapsed:hover { background: var(--bg-hover); color: var(--text-primary); }
</style>
