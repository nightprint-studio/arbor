<script lang="ts" module>
  /**
   * A read-only unified diff of two texts.
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

  /** A flattened row: either a line, or the gap between two hunks. */
  type Row =
    | { row: 'line'; line: DiffLineModel }
    | { row: 'gap'; label: string };
</script>

<script lang="ts">
  import VirtualList from './VirtualList.svelte';
  import EmptyState from './EmptyState.svelte';

  let {
    hunks,
    identical = false,
    emptyMessage = 'Nothing to compare.',
    identicalMessage = 'The two versions are identical.',
    rowHeight = 19,
    ariaLabel = 'Diff',
  }: {
    hunks: DiffHunkModel[];
    /** The two sides are the same. Said out loud, because an empty panel reads as a
     *  failure to load rather than as an answer. */
    identical?: boolean;
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
      for (const line of h.lines) out.push({ row: 'line', line });
    }
    return out;
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
  >
    {#snippet row({ item })}
      {#if item.row === 'gap'}
        <div class="td-gap" style="height: {rowHeight}px">{item.label}</div>
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
    /* The long-line escape hatch. A wrapped diff line would break the fixed row height
       the window arithmetic depends on, so the panel scrolls sideways instead. */
    overflow-x: auto;
    font-family: var(--font-code);
    font-size: var(--font-size-xs);
  }

  .td-line, .td-gap {
    display: flex;
    align-items: center;
    white-space: pre;
    line-height: 1;
    min-width: max-content;
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
