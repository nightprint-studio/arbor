<script lang="ts">
  /**
   * "Insert table" — the grid you drag a size out of.
   *
   * The shape every office suite and Obsidian settled on, and for a good reason: the question is
   * "how big", the answer is two numbers, and pointing at a cell answers both at once. A dialog
   * with two number fields asks the same question in four gestures.
   *
   * The grid grows as you reach its edge — start at 6×6, and hovering the last row or column adds
   * another — so a 3×2 costs no scrolling and a 9×7 is still reachable without a second control
   * saying "more". The keyboard walks it with the arrows and commits with Enter, because a
   * picker that only answers to a pointer is a feature half the people here cannot use.
   */
  import Dropdown from '$lib/components/shared/ui/Dropdown.svelte';

  let {
    /** Rows × columns, both counting the header row out: `onPick(3, 4)` = a header plus 3 rows. */
    onPick,
  }: { onPick: (rows: number, cols: number) => void } = $props();

  /**
   * A fixed 8×8, not a grid that grows.
   *
   * Growing is prettier and it is what Obsidian does, but the popover is positioned once when it
   * opens: a grid that gets wider as you reach the last column pushes its own right edge past the
   * screen, which is exactly what happened. Eight by eight covers every table anyone inserts by
   * hand, and it is the same size every time you open it — which is worth more than the last two
   * columns, because a control you can aim at without reading is faster than a clever one.
   */
  const SIZE = 8;
  /** Cell + gap, in px. Kept here because the popover's width is derived from it: the grid has to
   *  fit the panel exactly or the panel is positioned around a size it does not have. */
  const CELL = 20;
  const GAP = 3;
  const PAD = 9;
  const WIDTH = SIZE * CELL + (SIZE - 1) * GAP + PAD * 2;

  let rows = $state(0);
  let cols = $state(0);
  const gridRows = SIZE;
  const gridCols = SIZE;

  const label = $derived(rows && cols ? `${cols} × ${rows}` : 'Pick a size');

  function commit(close: () => void) {
    if (!rows || !cols) return;
    onPick(rows, cols);
    rows = 0;
    cols = 0;
    close();
  }

  function onKey(e: KeyboardEvent, close: () => void) {
    const step = (dr: number, dc: number) => {
      e.preventDefault();
      rows = Math.min(SIZE, Math.max(1, (rows || 1) + dr));
      cols = Math.min(SIZE, Math.max(1, (cols || 1) + dc));
    };
    if (e.key === 'ArrowDown')  step(1, 0);
    if (e.key === 'ArrowUp')    step(-1, 0);
    if (e.key === 'ArrowRight') step(0, 1);
    if (e.key === 'ArrowLeft')  step(0, -1);
    if (e.key === 'Enter') { e.preventDefault(); commit(close); }
  }
</script>

<!-- An explicit width, in px: `position="fixed"` measures the panel to place it, and a panel
     whose width it had to guess ended up hanging off the right of the window. -->
<Dropdown position="fixed" direction="down" width="{WIDTH}px">
  {#snippet trigger({ open, toggle })}
    <button
      class="ti-trigger"
      class:active={open}
      type="button"
      onclick={toggle}
      aria-haspopup="dialog"
      aria-expanded={open}
      title="Insert a table"
    >⊞</button>
  {/snippet}

  {#snippet children({ close }: { close: () => void })}
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div
      class="ti"
      role="dialog"
      aria-label="Insert table"
      tabindex="-1"
      onkeydown={(e) => onKey(e, close)}
      onmouseleave={() => { rows = 0; cols = 0; }}
    >
      <div class="ti-grid" style="--ti-cols: {gridCols}">
        {#each Array(gridRows) as _, r}
          {#each Array(gridCols) as _, c}
            <button
              type="button"
              class="ti-cell"
              class:on={r < rows && c < cols}
              aria-label={`${c + 1} by ${r + 1}`}
              onmouseenter={() => { rows = r + 1; cols = c + 1; }}
              onfocus={() => { rows = r + 1; cols = c + 1; }}
              onclick={() => commit(close)}
            ></button>
          {/each}
        {/each}
      </div>
      <p class="ti-label">{label}</p>
    </div>
  {/snippet}
</Dropdown>

<style>
  .ti-trigger {
    display: inline-flex; align-items: center; justify-content: center;
    width: 26px; height: 26px;
    border: none; background: transparent; border-radius: var(--radius-sm);
    color: var(--text-secondary); font-size: 15px; line-height: 1; cursor: pointer;
  }
  .ti-trigger:hover, .ti-trigger.active { color: var(--text-primary); background: var(--bg-hover); }

  .ti { padding: 9px; }
  .ti-grid {
    display: grid;
    grid-template-columns: repeat(var(--ti-cols), 20px);
    gap: 3px;
  }
  .ti-cell {
    width: 20px; height: 20px; padding: 0;
    border: 1px solid var(--border-subtle); border-radius: 2px;
    background: var(--bg-base); cursor: pointer;
  }
  /* Lit up to the pointer: the selection IS the preview, so it needs to read at a glance from
     the far corner of the grid. */
  .ti-cell.on { background: var(--accent); border-color: var(--accent); }
  .ti-label {
    margin: 8px 2px 0; text-align: center;
    font-size: var(--font-size-2xs); color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }
</style>
