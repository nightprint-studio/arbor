<script lang="ts" module>
  /**
   * How the rows are ordered. Each is a question someone actually arrives with.
   *
   * In the module script because the parent both *applies* the sort (the keys are facts about the
   * graph, which it owns) and imports the type — an instance-script `export type` is not part of a
   * component's importable surface.
   */
  export type GraphSort = 'name' | 'impact' | 'layer' | 'external';

  const SORT_LABELS: Record<GraphSort, string> = {
    name: 'Name',
    impact: 'Most rebuilt on',
    layer: 'Layer',
    external: 'Most third-party',
  };
</script>

<script lang="ts">
  /**
   * The index into the picture: every module as a row, filtered and sorted.
   *
   * It is not a lesser view of the drawing — it answers what a drawing cannot. *Find the crate called
   * something-like-parser* in a picture of forty boxes is a scan; here it is three keystrokes. *Which
   * crate has the most riding on it* is invisible in a layout and is one sort here. And it is the
   * **keyboard surface**: arrows walk it, Enter opens the manifest, so the whole window is usable
   * without touching the graph at all.
   *
   * Owns its own search box and sort, because both are questions about this list. The parent owns the
   * selection — the drawing and the detail panel move with it — so this reports and does not decide.
   */
  import { ArrowDownAZ, Circle } from 'lucide-svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import type { GraphNode } from '$lib/ipc/bennu/deps';

  let {
    /** Already filtered and sorted by the parent, which owns the graph. */
    rows,
    selected = null,
    query = $bindable(''),
    sort = $bindable<GraphSort>('impact'),
    /** `crates` / `modules`, in the ecosystem's own vocabulary. */
    words,
    onPick,
    onOpen,
  }: {
    rows: { index: number; node: GraphNode }[];
    selected?: number | null;
    query?: string;
    sort?: GraphSort;
    words: string;
    onPick: (index: number) => void;
    onOpen: (index: number) => void;
  } = $props();

  let listEl = $state<HTMLDivElement | null>(null);

  const sortItems = $derived<DropdownItem[]>(
    (Object.keys(SORT_LABELS) as GraphSort[]).map((id) => ({
      kind: 'item' as const,
      id,
      label: SORT_LABELS[id],
      active: id === sort,
      onclick: () => (sort = id),
    })),
  );

  /**
   * Arrows walk the rows, Enter opens the manifest.
   *
   * On the container rather than on each row: the rows are buttons, and thirty keydown handlers that
   * all mean "move the selection" is thirty chances for one of them to differ. With nothing selected,
   * either arrow starts at the top — a first press that does nothing is a key that looks broken.
   */
  function onKey(e: KeyboardEvent) {
    if (!rows.length) return;
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp' && e.key !== 'Enter') return;
    if (e.key === 'Enter') {
      if (selected === null) return;
      e.preventDefault();
      onOpen(selected);
      return;
    }
    e.preventDefault();
    const at = rows.findIndex((r) => r.index === selected);
    const next = at < 0
      ? 0
      : Math.max(0, Math.min(rows.length - 1, at + (e.key === 'ArrowDown' ? 1 : -1)));
    onPick(rows[next].index);
    listEl
      ?.querySelector<HTMLElement>(`[data-row="${rows[next].index}"]`)
      ?.scrollIntoView({ block: 'nearest' });
  }

  /** The kind's colour class — the same names the drawing's kind bar uses, so one vocabulary. */
  function kindClass(kind: string): string {
    return `mg-dot k-${(kind || 'unknown').replace('+', '-')}`;
  }
</script>

<div class="mg-search">
  <SearchBar
    bind:query
    showRegex={false}
    showCounter={false}
    autofocus
    placeholder={`Filter ${words}…`}
  />
</div>

<div class="mg-sort">
  <Dropdown items={sortItems} position="fixed" direction="down">
    {#snippet trigger()}
      <span class="mg-sort-trigger" use:tooltip={`Sort — ${SORT_LABELS[sort]}`}>
        <ArrowDownAZ size={11} />
        {SORT_LABELS[sort]}
      </span>
    {/snippet}
  </Dropdown>
  <span class="mg-n">{rows.length}</span>
</div>

<div
  class="mg-list"
  bind:this={listEl}
  role="listbox"
  tabindex="0"
  aria-label={`The project's ${words}`}
  onkeydown={onKey}
>
  {#each rows as r (r.index)}
    <button
      class="mg-row"
      class:sel={r.index === selected}
      class:ring={r.node.in_cycle}
      type="button"
      role="option"
      aria-selected={r.index === selected}
      data-row={r.index}
      use:tooltip={r.node.id}
      onclick={() => onPick(r.index)}
      ondblclick={() => onOpen(r.index)}
    >
      <Circle size={8} class={kindClass(r.node.kind)} />
      <span class="mg-row-name">{r.node.name || r.node.id}</span>
      <!-- The two numbers that make a row worth reading: who needs it, and what changing it costs.
           Everything else is in the detail panel below. -->
      <span class="mg-row-num" use:tooltip={`${r.node.dependents} direct dependents`}>
        {r.node.dependents}
      </span>
      <span
        class="mg-row-num mg-row-impact"
        use:tooltip={`${r.node.impact} ${words} rebuild when this changes`}
      >
        {r.node.impact}
      </span>
    </button>
  {/each}
  {#if !rows.length}
    <p class="mg-empty">No {words} match.</p>
  {/if}
</div>

<style>
  .mg-search { padding: 6px; }
  .mg-sort { display: flex; align-items: center; gap: 6px; padding: 0 8px 4px 8px; }
  .mg-sort-trigger {
    display: inline-flex; align-items: center; gap: 4px;
    color: var(--text-muted); cursor: pointer; font-size: var(--font-size-2xs);
  }
  .mg-sort-trigger:hover { color: var(--text-primary); }
  .mg-n {
    margin-left: auto;
    font-family: var(--font-code); font-size: var(--font-size-3xs); color: var(--text-disabled);
  }

  .mg-list {
    flex: 1; min-height: 0; overflow: auto;
    border-top: 1px solid var(--border-subtle);
    padding: 3px 0;
  }
  .mg-list:focus-visible { outline: 1px solid var(--accent); outline-offset: -1px; }

  .mg-row {
    display: flex; align-items: center; gap: 6px; width: 100%;
    padding: 2px 8px; min-height: 22px;
    background: none; border: none; cursor: pointer; text-align: left;
  }
  .mg-row:hover { background: var(--bg-hover); }
  .mg-row.sel { background: color-mix(in srgb, var(--accent) 16%, transparent); }
  .mg-row-name {
    flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-size: var(--font-size-xs); color: var(--text-primary);
  }
  .mg-row.ring .mg-row-name { color: var(--error); }
  .mg-row-num {
    flex-shrink: 0; min-width: 18px; text-align: right;
    font-family: var(--font-code); font-size: var(--font-size-3xs); color: var(--text-disabled);
  }
  .mg-row-impact { color: var(--text-muted); }
  .mg-empty {
    margin: 8px; color: var(--text-disabled); font-size: var(--font-size-2xs); font-style: italic;
  }

  /* The kind dot, coloured exactly like the drawing's kind bar. `:global` because the class lands on
     a component's own element — the one case the widget rules allow it. */
  .mg-row :global(.mg-dot) { flex-shrink: 0; }
  .mg-row :global(.k-lib), .mg-row :global(.k-jar) { color: var(--info); }
  .mg-row :global(.k-bin), .mg-row :global(.k-war), .mg-row :global(.k-ear) { color: var(--success); }
  .mg-row :global(.k-lib-bin) { color: var(--warning); }
  .mg-row :global(.k-proc-macro) { color: var(--accent); }
  .mg-row :global(.k-pom), .mg-row :global(.k-unknown) { color: var(--border-strong); }
</style>
