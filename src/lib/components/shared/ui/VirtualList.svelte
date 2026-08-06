<script lang="ts" module>
  /**
   * A windowed list: renders only the rows the viewport can show.
   *
   * ## Why this exists beside `VirtualTextView`
   *
   * That one is about **text** — a log, a console transcript: rows are strings, it follows the
   * tail, its role is `log`. This one is about **items**: arbitrary values rendered by a snippet,
   * with a selected index the caller drives from the keyboard. Bending the text widget into
   * taking objects would have given one component two jobs and two sets of props that only make
   * sense half the time.
   *
   * ## Fixed row height, and why that is not a limitation here
   *
   * The window is computed from `scrollTop / rowHeight`, so every row must be exactly
   * `rowHeight` tall — measure it or set it in CSS, but do not let it vary. Lists that want
   * per-row heights need a different (and much more expensive) widget; a results list does not,
   * and paying for one would be paying for a case that never arrives.
   *
   * A **heterogeneous** list still works: a file header and a hit row are different markup, and
   * as long as both are laid out to the same height the window arithmetic does not care. That is
   * what lets a grouped result list virtualize at all.
   */
</script>

<script lang="ts" generics="T">
  import type { Snippet } from 'svelte';

  interface Props {
    /** The rows, already flattened — one entry per rendered line. */
    items: T[];
    /** Every row's height in px. Must match what `row` actually renders. */
    rowHeight: number;
    /** Rows kept beyond each edge so a fast scroll does not show blank bands. */
    overscan?: number;
    /** Stable identity per row, for `{#each}` reconciliation. */
    getKey?: (item: T, index: number) => string | number;
    /**
     * Index to keep visible — the caller's selection.
     *
     * Scrolled to only when it is off-screen, never re-centred: a list that jumped on every
     * arrow key would be unreadable, and the row you just moved onto is almost always already
     * in view.
     */
    scrollTo?: number | null;
    class?: string;
    role?: string;
    ariaLabel?: string;
    row: Snippet<[{ item: T; index: number }]>;
  }

  let {
    items,
    rowHeight,
    overscan = 6,
    getKey,
    scrollTo = null,
    class: klass = '',
    role = 'listbox',
    ariaLabel,
    row,
  }: Props = $props();

  let viewport = $state<HTMLDivElement | null>(null);
  let scrollTop = $state(0);
  let height = $state(0);

  const total = $derived(items.length);
  const first = $derived(Math.max(0, Math.floor(scrollTop / rowHeight) - overscan));
  const visible = $derived(
    Math.min(total - first, Math.ceil((height || rowHeight) / rowHeight) + overscan * 2),
  );
  /** The slice actually in the DOM, with its absolute starting index. */
  const window_ = $derived(items.slice(first, first + Math.max(visible, 0)));

  function onScroll() {
    scrollTop = viewport?.scrollTop ?? 0;
  }

  /**
   * Bring a row into view by **index**, not by element: the whole point of a windowed list is
   * that an off-screen row has no element to scroll to — which is exactly the case that needs
   * scrolling. So the position is computed instead.
   */
  export function scrollToIndex(index: number) {
    const el = viewport;
    if (!el || index < 0 || index >= total) return;
    const top = index * rowHeight;
    const bottom = top + rowHeight;
    if (top < el.scrollTop) el.scrollTop = top;
    else if (bottom > el.scrollTop + el.clientHeight) el.scrollTop = bottom - el.clientHeight;
  }

  $effect(() => {
    if (scrollTo === null || scrollTo === undefined) return;
    const index = scrollTo;
    // On a frame: a selection that moved because rows arrived needs the new height measured
    // first, and `clientHeight` in this tick is the previous layout's.
    const frame = requestAnimationFrame(() => scrollToIndex(index));
    return () => cancelAnimationFrame(frame);
  });
</script>

<div
  bind:this={viewport}
  bind:clientHeight={height}
  class="vl {klass}"
  {role}
  aria-label={ariaLabel}
  tabindex="-1"
  onscroll={onScroll}
>
  <!-- The spacer gives the scrollbar the full list's height; the window is positioned inside it,
       so scrolling is the browser's own and costs nothing per row. -->
  <div class="vl-spacer" style:height={`${total * rowHeight}px`}>
    <div class="vl-window" style:transform={`translateY(${first * rowHeight}px)`}>
      {#each window_ as item, i (getKey ? getKey(item, first + i) : first + i)}
        <div class="vl-row" style:height={`${rowHeight}px`}>
          {@render row({ item, index: first + i })}
        </div>
      {/each}
    </div>
  </div>
</div>

<style>
  .vl { overflow-y: auto; overflow-x: hidden; min-height: 0; outline: none; }
  .vl-spacer { position: relative; width: 100%; }
  .vl-window { position: absolute; top: 0; left: 0; right: 0; will-change: transform; }
  /* The row box owns the height so the window arithmetic and the DOM agree; what the consumer
     renders inside it is free to be anything that fits. */
  .vl-row { overflow: hidden; }
</style>
