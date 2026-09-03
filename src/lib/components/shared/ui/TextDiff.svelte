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
   * Both are the same windowed list; only the pairing differs. A split row carries a left and
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

  /**
   * The width both columns are given, in characters of the longest line either side holds.
   *
   * A per-row `1fr 1fr` would let every row size its own columns, and the divider would
   * zig-zag down the panel. One width for the whole diff keeps the two texts on rails: the
   * columns still share any spare room equally (`flex: 1 1 0` below), so a short file fills the
   * panel and a long-lined one scrolls sideways as a unit — the same escape hatch the unified
   * mode already uses.
   */
  const sideChars = $derived.by(() => {
    if (mode !== 'split') return 0;
    let max = 0;
    for (const h of hunks) {
      for (const l of h.lines) {
        // A tab advances to the next stop, so it is worth up to `tab-size` columns rather than
        // one — and `tab-size` is pinned to 4 in the CSS precisely so this can be counted. An
        // upper bound (a tab mid-column advances less), which is the right side to err on: too
        // wide is a little empty room, too narrow is one column's text under the other's.
        let width = l.text.length;
        for (let i = 0; i < l.text.length; i++) if (l.text.charCodeAt(i) === 9) width += 3;
        if (width > max) max = width;
      }
    }
    return max;
  });

  function sign(kind: DiffLineKind): string {
    return kind === 'add' ? '+' : kind === 'del' ? '−' : ' ';
  }
</script>

{#if identical}
  <div class="td-empty"><EmptyState message={identicalMessage} /></div>
{:else if rows.length === 0}
  <div class="td-empty"><EmptyState message={emptyMessage} /></div>
{:else}
  <VirtualList
    items={rows}
    {rowHeight}
    class="td"
    role="list"
    {ariaLabel}
    getKey={(_, i) => i}
    scrollX
  >
    {#snippet row({ item })}
      {#if item.row === 'gap'}
        <div class="td-gap" style="height: {rowHeight}px">{item.label}</div>
      {:else if item.row === 'pair'}
        <div
          class="td-pair"
          style="height: {rowHeight}px; --td-side: calc({sideChars}ch + 64px)"
        >
          <div class="td-side {item.left ? `td-${item.left.kind}` : 'td-filler'}">
            <span class="td-no">{item.left?.old ?? ''}</span>
            <span class="td-text">{item.left ? item.left.text || ' ' : ''}</span>
          </div>
          <div class="td-side {item.right ? `td-${item.right.kind}` : 'td-filler'}">
            <span class="td-no">{item.right?.new ?? ''}</span>
            <span class="td-text">{item.right ? item.right.text || ' ' : ''}</span>
          </div>
        </div>
      {:else}
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

  .td-line, .td-gap, .td-pair {
    display: flex;
    align-items: center;
    white-space: pre;
    line-height: 1;
    min-width: max-content;
  }

  /* Side-by-side. Each column is at least as wide as the diff's longest line (`--td-side`,
     set per row from one figure for the whole diff) and shares any spare width equally, so
     the divider is a straight line whether the panel scrolls sideways or not. */
  .td-side {
    display: flex;
    align-items: center;
    flex: 1 1 0;
    min-width: var(--td-side, 320px);
    height: 100%;
    /* Backstop: the width above is an upper bound, so this should never fire — but one line
       reaching under the other column would be worse than one line cut short. */
    overflow: hidden;
  }
  .td-side + .td-side { border-left: 1px solid var(--border); }
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
