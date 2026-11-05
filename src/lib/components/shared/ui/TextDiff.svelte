<script lang="ts" module>
  /**
   * A read-only diff of two texts — unified, or side by side.
   *
   * ## What it is not
   *
   * Not Corvus's `DiffViewer`. That one is a **git** surface: it stages lines, builds
   * patches, pins encodings, and takes git's own `DiffFile` shape. Everything it does
   * beyond drawing lines is about a working tree, and none of it means anything to a
   * viewer comparing two revisions of a file that may not even be tracked.
   *
   * This is the drawing half on its own, over a neutral line model — so anything with
   * two texts and a diff of them can render it: local history today, and, the day
   * somebody untangles the staging chrome from the rendering, Corvus's own hunks.
   *
   * ## Windowed
   *
   * The first revision of a long file against the current one is one enormous hunk, and
   * a diff panel that renders four thousand rows to show you six is a panel that stalls
   * on open. Rows are flattened (hunk separators included) and handed to
   * {@link VirtualList}, which is why every row has to be exactly `rowHeight` tall.
   *
   * ## Two modes, one row model
   *
   * `unified` is the patch: one column, `+` and `−` in the margin. `split` is the two texts
   * beside each other, which is how you read a *rewrite* — a line whose old and new form sit
   * ten rows apart in a unified diff is one glance in a split one.
   *
   * Both are windowed lists over the same rows; only the pairing differs, and `split` is TWO lists
   * (one per side) bound to one vertical scroll, each scrolling sideways by itself — so a long line
   * on one side widens that side only, instead of pushing the other column off the panel as one
   * shared width did. A split row carries a left and
   * a right line, either of which may be absent (a pure insertion has no left), and the pairing
   * walks each hunk taking a run of deletions against the run of additions beside it — index
   * for index, which is what puts a line above its own replacement.
   */

  /** What one line of the diff is. */
  export type DiffLineKind = 'context' | 'add' | 'del';

  /** One rendered line. Both numbers are 1-based; the one that does not apply is absent
   *  (an added line has no old number), which is exactly what the gutter draws. */
  export interface DiffLineModel {
    kind: DiffLineKind;
    old?: number;
    new?: number;
    text: string;
  }

  /** A run of changed lines with its surrounding context. */
  export interface DiffHunkModel {
    old_start: number;
    new_start: number;
    lines: DiffLineModel[];
  }

  /** How the two sides are laid out. */
  export type DiffLayout = 'unified' | 'split';

  /** A flattened row: a unified line, a side-by-side pair, or the gap between two hunks. */
  type Row =
    | { row: 'line'; line: DiffLineModel }
    | { row: 'pair'; left: DiffLineModel | null; right: DiffLineModel | null }
    | { row: 'gap'; label: string };

  /** One side-by-side row. Either side may be absent — that is the filler band. */
  export interface DiffPair {
    left: DiffLineModel | null;
    right: DiffLineModel | null;
  }

  /**
   * Pair one hunk's lines into left/right rows.
   *
   * Context lines pair with themselves. A **change block** — every consecutive line that is not
   * context — is split into its deletions and its additions and zipped by index, so the first
   * line removed sits opposite the first line added. The longer run's leftovers pair with
   * nothing, which is what draws the filler band on the shorter side.
   *
   * Exported because it is the only real logic here and it is pure: the rest of this component
   * is markup.
   */
  export function pairLines(lines: DiffLineModel[]): DiffPair[] {
    const out: DiffPair[] = [];
    let i = 0;
    while (i < lines.length) {
      const line = lines[i];
      if (line.kind === 'context') {
        out.push({ left: line, right: line });
        i++;
        continue;
      }
      const dels: DiffLineModel[] = [];
      const adds: DiffLineModel[] = [];
      while (i < lines.length && lines[i].kind !== 'context') {
        (lines[i].kind === 'del' ? dels : adds).push(lines[i]);
        i++;
      }
      for (let k = 0; k < Math.max(dels.length, adds.length); k++) {
        out.push({ left: dels[k] ?? null, right: adds[k] ?? null });
      }
    }
    return out;
  }
</script>

<script lang="ts">
  import VirtualList from './VirtualList.svelte';
  import EmptyState from './EmptyState.svelte';

  let {
    hunks,
    identical = false,
    mode = 'unified',
    emptyMessage = 'Nothing to compare.',
    identicalMessage = 'The two versions are identical.',
    rowHeight = 19,
    ariaLabel = 'Diff',
  }: {
    hunks: DiffHunkModel[];
    /** The two sides are the same. Said out loud, because an empty panel reads as a
     *  failure to load rather than as an answer. */
    identical?: boolean;
    /** `unified` (the patch) or `split` (the two texts beside each other). */
    mode?: DiffLayout;
    emptyMessage?: string;
    identicalMessage?: string;
    /** Row height in px. Must match the CSS — the window arithmetic depends on it. */
    rowHeight?: number;
    ariaLabel?: string;
  } = $props();

  const rows = $derived.by<Row[]>(() => {
    const out: Row[] = [];
    for (const h of hunks) {
      if (out.length) {
        // The gap says which line the next hunk resumes at. Without it, two hunks a
        // thousand lines apart read as adjacent, and the numbers in the gutter are the
        // only hint that they are not.
        out.push({ row: 'gap', label: `@@ ${h.old_start} → ${h.new_start} @@` });
      }
      if (mode === 'split') {
        for (const pair of pairLines(h.lines)) out.push({ row: 'pair', ...pair });
      } else {
        for (const line of h.lines) out.push({ row: 'line', line });
      }
    }
    return out;
  });

  /** Shared by the two halves of a split diff, so they scroll down together. */
  let sharedTop = $state(0);

  function sign(kind: DiffLineKind): string {
    return kind === 'add' ? '+' : kind === 'del' ? '−' : ' ';
  }
</script>

{#snippet half(item: Row, side: 'left' | 'right')}
  {#if item.row === 'gap'}
    <div class="td-gap" style="height: {rowHeight}px">{side === 'left' ? item.label : ''}</div>
  {:else if item.row === 'pair'}
    {@const line = side === 'left' ? item.left : item.right}
    <div class="td-half {line ? `td-${line.kind}` : 'td-filler'}" style="height: {rowHeight}px">
      <span class="td-no">{(side === 'left' ? line?.old : line?.new) ?? ''}</span>
      <span class="td-text">{line ? line.text || ' ' : ''}</span>
    </div>
  {/if}
{/snippet}

{#if identical}
  <div class="td-empty"><EmptyState message={identicalMessage} /></div>
{:else if rows.length === 0}
  <div class="td-empty"><EmptyState message={emptyMessage} /></div>
{:else}
  {#if mode === 'split'}
    <!-- Two lists, one scroll. Each half scrolls sideways on its own, so a long line on one side
         widens THAT side and never pushes the other column off the panel; vertically they are bound
         to the same offset and move as one. -->
    <div class="td-split">
      <VirtualList
        items={rows} {rowHeight} class="td td-half-list td-left" role="list" ariaLabel={`${ariaLabel}, before`}
        getKey={(_, i) => i} scrollX bind:scrollTop={sharedTop}
      >
        {#snippet row({ item })}{@render half(item, 'left')}{/snippet}
      </VirtualList>
      <VirtualList
        items={rows} {rowHeight} class="td td-half-list td-right" role="list" ariaLabel={`${ariaLabel}, after`}
        getKey={(_, i) => i} scrollX bind:scrollTop={sharedTop}
      >
        {#snippet row({ item })}{@render half(item, 'right')}{/snippet}
      </VirtualList>
    </div>
  {:else}
    <VirtualList items={rows} {rowHeight} class="td" role="list" {ariaLabel} getKey={(_, i) => i} scrollX>
      {#snippet row({ item })}
        {#if item.row === 'gap'}
          <div class="td-gap" style="height: {rowHeight}px">{item.label}</div>
        {:else if item.row === 'line'}
          <div class="td-line td-{item.line.kind}" style="height: {rowHeight}px">
            <span class="td-no">{item.line.old ?? ''}</span>
            <span class="td-no">{item.line.new ?? ''}</span>
            <span class="td-sign" aria-hidden="true">{sign(item.line.kind)}</span>
            <span class="td-text">{item.line.text || ' '}</span>
          </div>
        {/if}
      {/snippet}
    </VirtualList>
  {/if}
{/if}

<style>
  .td-empty { display: flex; align-items: center; justify-content: center; height: 100%; }

  :global(.td) {
    height: 100%;
    /* The long-line escape hatch is `scrollX` on the list itself (a `overflow-x` here loses to
       the widget's own scoped rule): a wrapped diff line would break the fixed row height the
       window arithmetic depends on, so the panel scrolls sideways instead of wrapping. */
    font-family: var(--font-code);
    font-size: var(--font-size-xs);
    /* Pinned rather than left to the browser's 8, so a tab is worth a known number of columns:
       the side-by-side column width is computed in characters and a tab has to be counted. */
    tab-size: 4;
  }

  .td-line, .td-gap {
    display: flex;
    align-items: center;
    white-space: pre;
    line-height: 1;
    min-width: max-content;
  }

  /* Side-by-side: two lists in a row, each half of the panel. The divider is the right list's own
     left border, so it is one straight line however far either side has scrolled. */
  .td-split { display: flex; height: 100%; min-height: 0; }
  .td-split > :global(.td) { flex: 1 1 0; min-width: 0; }
  .td-split > :global(.td.td-right) { border-left: 1px solid var(--border); }
  /* Both halves keep a horizontal scrollbar, even when one has nothing to scroll: a bar on one side
     only would make that viewport shorter, and the two could no longer be scrolled to the same
     bottom. The left half hides its vertical bar — the right one is the scrollbar for the pair. */
  .td-split :global(.vl.vl-x.td-half-list) { overflow-x: scroll; }
  .td-split :global(.td-left::-webkit-scrollbar:vertical) { width: 0; }

  .td-half {
    display: flex;
    align-items: center;
    white-space: pre;
    line-height: 1;
    min-width: max-content;
  }
  /* The half of a row that has no line — a hatch rather than a colour, so it reads as
     "nothing here" instead of as a third kind of change. */
  .td-filler {
    background: repeating-linear-gradient(
      -45deg,
      transparent 0 5px,
      color-mix(in srgb, var(--text-faint) 12%, transparent) 5px 10px
    );
  }

  .td-gap {
    padding-left: 8px;
    color: var(--text-faint);
    background: var(--bg-elevated);
    border-top: 1px solid var(--border-subtle);
    border-bottom: 1px solid var(--border-subtle);
  }

  .td-no {
    flex: none;
    width: 42px;
    padding-right: 8px;
    text-align: right;
    color: var(--text-faint);
    user-select: none;
  }
  .td-sign { flex: none; width: 14px; text-align: center; user-select: none; }
  .td-text { color: var(--text-secondary); padding-right: 12px; }

  /* Tinted backgrounds rather than coloured text: the line stays as readable as an
     unchanged one, and which side it belongs to is carried by the band it sits in. */
  .td-add { background: color-mix(in srgb, var(--success) 14%, transparent); }
  .td-del { background: color-mix(in srgb, var(--error) 14%, transparent); }
  .td-add .td-sign, .td-add .td-no { color: var(--success); }
  .td-del .td-sign, .td-del .td-no { color: var(--error); }
  .td-add .td-text, .td-del .td-text { color: var(--text-primary); }
</style>
