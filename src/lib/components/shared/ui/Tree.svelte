<script lang="ts" module>
  /**
   * Tree — generic virtualised tree view.
   *
   * The widget owns wrapper, indentation, expansion state, filtering,
   * selection visuals, hover, focus management and context-menu wiring.
   * Each row's *content* (icon, label, badges, actions) is supplied by
   * the consumer via the `row` snippet — keeping the visuals fully owned
   * upstream while letting us deduplicate the recursion + a11y plumbing.
   *
   * Design notes:
   *   - Node shape is generic. Callers supply `getChildren` (default
   *     `n.children`) and `getId` (default `n.id`) so any pre-existing
   *     hierarchical type can be used without remapping.
   *   - Expansion can be uncontrolled (default) — toggled on row click
   *     for nodes with children — or controlled by the parent via
   *     `expandedIds` + `onExpandToggle`. Controlled mode is what the
   *     stage area and file panel use, since they persist expansion in
   *     their own stores keyed by full paths.
   *   - Filter auto-expands subtrees containing matches without polluting
   *     the user-driven expand state — keyboard-search UX stays intact.
   *   - Virtualisation: the tree is flattened DFS into a row list each
   *     time `nodes`, expansion state or the filter changes. Only rows
   *     intersecting the nearest scroll ancestor's viewport (plus a
   *     small overscan) are mounted in the DOM, while a sized spacer
   *     reserves the full list height so the host's scrollbar stays
   *     accurate. Tree does NOT own its scroll container — it climbs to
   *     the first ancestor with `overflow-y: auto/scroll/overlay` and
   *     piggy-backs on its scroll events. This keeps every existing
   *     consumer (`FileTreePanel`'s `.tree-body`, `StageArea`'s
   *     `.file-list`, `PluginTreeSidebar`'s PanelShell body, the modal
   *     in `DependencyTreeModal`) working without CSS surgery, and lets
   *     a JSON-explorer-style host with 100k+ nodes stay responsive.
   *     The snippet API is unchanged: consumers still receive the same
   *     `RowSnippetCtx`, and `:global(.tree .tree-row…)` overrides keep
   *     applying because the row class names are preserved.
   *
   * Sibling shared widgets: `Tabs.svelte` (horizontal selectables) and
   * `Dropdown.svelte` (popovers). All three follow the same pattern of
   * "shell owns mechanics, snippet owns content".
   */
  import type { Snippet } from 'svelte';

  // Intentionally non-structural: the widget accepts ANY hierarchical
  // node type (StageTreeNode, FileTreeNode, JSON viewer nodes, …) and
  // relies on `getId` / `getChildren` accessor props for the shape.
  // Using a marker-only object type avoids TS "weak type" complaints
  // and lets the generic `T` be inferred from consumer-supplied props.
  // eslint-disable-next-line @typescript-eslint/no-empty-object-type
  export type TreeNodeBase = object;

  export type RowSnippetCtx<T> = {
    node: T;
    depth: number;
    expanded: boolean;
    selected: boolean;
    hasChildren: boolean;
    /** Toggle this node's expansion. Useful when the row has its own
     *  "expand" button separate from selection. */
    toggle: () => void;
  };
</script>

<script lang="ts" generics="T extends TreeNodeBase">
  import { ChevronRight, ChevronDown } from 'lucide-svelte';
  import { onMount, tick } from 'svelte';
  import { tooltip } from '$lib/actions/tooltip';

  interface Props {
    // ── Data ────────────────────────────────────────────────────────────
    nodes: T[];
    /** Children accessor. Default: `n.children`. */
    getChildren?: (node: T) => T[] | undefined | null;
    /** Id accessor. Default: `n.id`. */
    getId?:       (node: T) => string;
    /** Override the default "has-children" check. Useful for **lazy**
     *  hosts where `getChildren` returns `null` until the children are
     *  fetched on expansion — without this override, the row would render
     *  as a leaf (no chevron, no click-to-expand). When provided, the row
     *  shows the chevron + accepts toggles even if `getChildren` is
     *  currently empty/null; `onExpandToggle` is then expected to load
     *  the children and update the tree. */
    hasChildren?: (node: T) => boolean;

    // ── Selection ──────────────────────────────────────────────────────
    selectedId?:  string | null;
    /** Override the default selectability check (default: any leaf or
     *  any node with `selectable !== false`). */
    selectable?:  (node: T) => boolean;

    // ── Expansion (uncontrolled) ───────────────────────────────────────
    /** Ids that should start expanded. */
    defaultExpanded?: Iterable<string>;
    /** Per-node initial expansion fallback (called once per node). */
    initialExpanded?: (node: T) => boolean;

    // ── Expansion (controlled) ─────────────────────────────────────────
    /** When provided, the parent owns expansion state. The widget calls
     *  `onExpandToggle` instead of mutating its own internal map. */
    expandedIds?:    Set<string>;
    onExpandToggle?: (id: string, next: boolean, node: T) => void;

    // ── Filtering ──────────────────────────────────────────────────────
    filter?:         string;
    /** Custom match callback. Default: case-insensitive substring search
     *  on the row's `String(node.label ?? node.name ?? id)`. */
    match?:          (node: T, q: string) => boolean;
    /** When true (default), ancestor folders of matching nodes are
     *  force-expanded. */
    expandOnFilter?: boolean;

    // ── Visual config ──────────────────────────────────────────────────
    /** Pixels per depth level. Default: 14. */
    indentSize?:  number;
    /** Pixels reserved at depth 0. Default: 6. */
    basePadding?: number;
    /** Row height in px. Default: 22. */
    rowHeight?:   number;
    /** Horizontal gap between in-row cells (chevron, icon, label, badges).
     *  Default: 6. */
    cellGap?:     number;
    /** Render vertical indent guide lines. Default: false. */
    guides?:      boolean;
    /** Show the disclosure chevron on the left of each row. Default:
     *  true. Set false when the row snippet handles its own. */
    showChevron?: boolean;
    /** Off-screen rows kept mounted on each side of the viewport.
     *  Default: 8. */
    overscan?:    number;

    // ── Behavior ───────────────────────────────────────────────────────
    /** Toggle expansion when the row is clicked (and the node has
     *  children). Default: true. */
    toggleOnClick?: boolean;

    // ── Events ─────────────────────────────────────────────────────────
    onSelect?:      (node: T, e: MouseEvent | KeyboardEvent) => void;
    onActivate?:    (node: T) => void;
    onContextMenu?: (node: T, e: MouseEvent) => void;

    // ── Per-row hooks ──────────────────────────────────────────────────
    /** Extra class on the row wrapper, computed per node. */
    rowClass?:    (ctx: RowSnippetCtx<T>) => string | undefined;
    /** Native title (tooltip) for the row. */
    rowTitle?:    (node: T) => string | undefined;

    // ── Drag-and-drop (opt-in, Phase 6.2) ─────────────────────────────
    /** Returns true when the row is an HTML5 drag source. Default: false. */
    draggable?:    (node: T) => boolean;
    /** Returns true when the row accepts drops. Default: false. */
    dropTarget?:   (node: T) => boolean;
    /** Fired when a draggable row is dropped on a drop-target row. The Tree
     *  refuses self-drops (source.id === target.id) before firing. */
    onDropOnNode?: (source: T, target: T) => void;

    // ── Snippets ───────────────────────────────────────────────────────
    row:        Snippet<[RowSnippetCtx<T>]>;
    emptyState?: Snippet;

    // ── A11y / styling ─────────────────────────────────────────────────
    ariaLabel?: string;
    class?:     string;
  }

  let {
    nodes,
    getChildren     = (n) => (n as any).children,
    getId           = (n) => (n as any).id,
    hasChildren,
    selectedId      = null,
    selectable,
    defaultExpanded,
    initialExpanded,
    expandedIds     = undefined,
    onExpandToggle,
    filter          = '',
    match,
    expandOnFilter  = true,
    indentSize      = 14,
    basePadding     = 6,
    rowHeight       = 22,
    cellGap         = 6,
    guides          = false,
    showChevron     = true,
    overscan        = 8,
    toggleOnClick   = true,
    onSelect,
    onActivate,
    onContextMenu,
    rowClass,
    rowTitle,
    draggable,
    dropTarget,
    onDropOnNode,
    row,
    emptyState,
    ariaLabel,
    class: rootClass = '',
  }: Props = $props();

  // ── Expansion state (uncontrolled fallback) ─────────────────────────
  // We keep a per-id override map. On first reference the widget seeds
  // from `defaultExpanded` / `initialExpanded(node)`; once the user
  // toggles, the override sticks until the node id disappears.
  const seeded = new Set<string>();
  let expandOverride = $state<Record<string, boolean>>({});

  // svelte-ignore state_referenced_locally
  if (defaultExpanded) {
    // svelte-ignore state_referenced_locally
    for (const id of defaultExpanded) {
      // svelte-ignore state_referenced_locally
      expandOverride[id] = true;
      seeded.add(id);
    }
  }

  function isControlled(): boolean { return expandedIds !== undefined; }

  function isExpanded(node: T): boolean {
    const id = getId(node);
    if (isControlled()) return expandedIds!.has(id);
    if (id in expandOverride) return expandOverride[id];
    if (!seeded.has(id) && initialExpanded) {
      seeded.add(id);
      const v = !!initialExpanded(node);
      // Defer the persisted seed write so we don't mutate reactive state
      // from inside the flat-list $derived computation.
      if (v) queueMicrotask(() => { expandOverride = { ...expandOverride, [id]: true }; });
      return v;
    }
    return false;
  }

  function toggle(node: T): void {
    const id   = getId(node);
    const next = !isExpanded(node);
    if (isControlled()) {
      onExpandToggle?.(id, next, node);
      return;
    }
    expandOverride = { ...expandOverride, [id]: next };
    onExpandToggle?.(id, next, node);
  }

  // ── Filter ──────────────────────────────────────────────────────────
  const normalizedFilter = $derived(filter.trim().toLowerCase());

  function defaultMatch(node: T, q: string): boolean {
    const labelish = String((node as any).label ?? (node as any).name ?? getId(node));
    return labelish.toLowerCase().includes(q);
  }

  function matchFn(node: T, q: string): boolean {
    if (!q) return true;
    const m = match ?? defaultMatch;
    if (m(node, q)) return true;
    const kids = getChildren(node);
    if (!kids) return false;
    for (const k of kids) if (matchFn(k, q)) return true;
    return false;
  }

  function effectiveExpanded(node: T): boolean {
    if (expandOnFilter && normalizedFilter && (getChildren(node)?.length ?? 0) > 0) return true;
    return isExpanded(node);
  }

  function defaultSelectable(node: T): boolean {
    return (node as any).selectable !== false;
  }

  // ── Flattened row list (DFS over expanded subtrees, filter-aware) ───
  type FlatRow = {
    node:        T;
    depth:       number;
    id:          string;
    hasChildren: boolean;
    expanded:    boolean;
  };

  const flat = $derived.by<FlatRow[]>(() => {
    // Re-read reactive deps so $derived tracks them — `expandedIds` and
    // `expandOverride` aren't directly used inside the recursion (it
    // calls helpers), so reference them explicitly.
    void expandedIds; void expandOverride; void normalizedFilter;
    const out: FlatRow[] = [];
    const q = normalizedFilter;
    const walk = (node: T, depth: number) => {
      if (!matchFn(node, q)) return;
      const kids   = getChildren(node) ?? [];
      // Lazy hosts (JSON Studio, …) override `hasChildren` so the chevron
      // appears + clicks route to onExpandToggle even when `kids` is still
      // empty (children fetched on demand). Default falls back to the
      // kids-length check so non-lazy callers behave as before.
      const hasKids   = hasChildren ? hasChildren(node) : kids.length > 0;
      const expanded  = hasKids && effectiveExpanded(node);
      out.push({ node, depth, id: getId(node), hasChildren: hasKids, expanded });
      if (expanded) for (const k of kids) walk(k, depth + 1);
    };
    if (nodes) for (const n of nodes) walk(n, 0);
    return out;
  });

  // ── Viewport tracking via the nearest scroll ancestor ───────────────
  // We don't own a scroll container ourselves — Tree's host already
  // provides one (e.g. `.tree-body` in FileTreePanel, `.file-list` in
  // StageArea, the PanelShell `.ps-body.scrollable`, the modal card
  // body in DependencyTreeModal). We climb the DOM, latch onto the
  // first scrollable ancestor, and compute which slice of the flat list
  // intersects its visible rect.
  let treeEl:        HTMLDivElement | undefined = $state();
  let scrollParent:  Element | null = null;
  let viewportTop = $state(0);   // top of visible window in Tree-local coords
  let viewportH   = $state(0);   // height of visible window (clipped to Tree)
  let scrollRaf   = 0;

  function findScrollParent(el: Element): Element {
    let p: Element | null = el.parentElement;
    while (p) {
      const s = getComputedStyle(p);
      if (/(auto|scroll|overlay)/.test(s.overflowY) || /(auto|scroll|overlay)/.test(s.overflow)) {
        return p;
      }
      p = p.parentElement;
    }
    return document.scrollingElement || document.documentElement;
  }

  function recomputeViewport() {
    if (!treeEl) return;
    const treeRect = treeEl.getBoundingClientRect();
    const parentIsRoot =
      !scrollParent || scrollParent === document.scrollingElement || scrollParent === document.documentElement;
    const parentRect = parentIsRoot
      ? { top: 0, height: window.innerHeight }
      : (scrollParent as Element).getBoundingClientRect();

    // Tree's top relative to the parent viewport. Negative once the
    // user has scrolled past the top of the Tree.
    const treeTopInParent = treeRect.top - parentRect.top;

    const top    = Math.max(0, -treeTopInParent);
    const bottom = Math.min(treeRect.height, parentRect.height - treeTopInParent);
    viewportTop = top;
    viewportH   = Math.max(0, bottom - top);
  }

  function scheduleRecompute() {
    if (scrollRaf) return;
    scrollRaf = requestAnimationFrame(() => {
      scrollRaf = 0;
      recomputeViewport();
    });
  }

  onMount(() => {
    if (!treeEl) return;
    scrollParent = findScrollParent(treeEl);

    const scrollTarget: EventTarget =
      scrollParent === document.scrollingElement || scrollParent === document.documentElement
        ? window
        : scrollParent;
    scrollTarget.addEventListener('scroll', scheduleRecompute, { passive: true });

    // Re-measure on layout shifts of the Tree itself or the scroll
    // ancestor (host panel resize, window resize, sibling collapse, …).
    const ro = new ResizeObserver(scheduleRecompute);
    ro.observe(treeEl);
    if (scrollParent && scrollParent !== document.scrollingElement && scrollParent !== document.documentElement) {
      ro.observe(scrollParent);
    }
    window.addEventListener('resize', scheduleRecompute, { passive: true });

    // First measurement after layout settles.
    tick().then(recomputeViewport);

    return () => {
      scrollTarget.removeEventListener('scroll', scheduleRecompute);
      window.removeEventListener('resize', scheduleRecompute);
      ro.disconnect();
      if (scrollRaf) cancelAnimationFrame(scrollRaf);
    };
  });

  // Recompute when the flat list grows/shrinks: the spacer height
  // changes, so the visible slice may shift.
  $effect(() => {
    void flat.length;
    scheduleRecompute();
  });

  const totalH   = $derived(flat.length * rowHeight);
  const startIdx = $derived(Math.max(0, Math.floor(viewportTop / rowHeight) - overscan));
  const endIdx   = $derived(
    Math.min(flat.length, Math.ceil((viewportTop + Math.max(viewportH, rowHeight)) / rowHeight) + overscan),
  );
  const visible  = $derived(flat.slice(startIdx, endIdx));

  // ── Click handlers ──────────────────────────────────────────────────
  function handleClick(node: T, hasKids: boolean, e: MouseEvent) {
    e.stopPropagation();
    if (toggleOnClick && hasKids) toggle(node);
    if ((selectable ?? defaultSelectable)(node)) onSelect?.(node, e);
  }
  function handleDblClick(node: T, e: MouseEvent) {
    e.stopPropagation();
    if ((selectable ?? defaultSelectable)(node) || (node as any).default_action) onActivate?.(node);
  }
  function handleContextMenu(node: T, e: MouseEvent) {
    if (!onContextMenu) return;
    e.preventDefault();
    e.stopPropagation();
    onContextMenu(node, e);
  }
  // ── Drag-and-drop (Phase 6.2) ───────────────────────────────────────
  // Single-instance dnd: we cache the source node by id (DataTransfer is
  // string-only, so passing the node itself across rows requires this
  // sidecar). Cleared on `dragend` whether the drop succeeded or not.
  // `dropHoverId` drives the row-level `.tree-row-drop-hover` class for
  // visual feedback while the cursor sits over a valid target.
  let dragSourceId: string | null = $state(null);
  let dropHoverId:  string | null = $state(null);
  let dragSourceNode: T | null = null;

  function handleDragStart(node: T, e: DragEvent) {
    if (!draggable || !draggable(node)) return;
    const id = getId(node);
    dragSourceId   = id;
    dragSourceNode = node;
    try {
      e.dataTransfer?.setData('text/plain', id);
      if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move';
    } catch { /* IE-style errors; nothing to do */ }
  }
  function handleDragEnd() {
    dragSourceId = null;
    dropHoverId  = null;
    dragSourceNode = null;
  }
  function canDropOn(target: T): boolean {
    if (!dropTarget || !dropTarget(target)) return false;
    if (!dragSourceId) return false;
    if (getId(target) === dragSourceId) return false;
    return true;
  }
  function handleDragOver(node: T, e: DragEvent) {
    if (!canDropOn(node)) return;
    e.preventDefault();        // accept the drop (without this, drop won't fire)
    if (e.dataTransfer) e.dataTransfer.dropEffect = 'move';
    const id = getId(node);
    if (dropHoverId !== id) dropHoverId = id;
  }
  function handleDragLeave(node: T) {
    if (dropHoverId === getId(node)) dropHoverId = null;
  }
  function handleDrop(node: T, e: DragEvent) {
    if (!canDropOn(node)) return;
    e.preventDefault();
    e.stopPropagation();
    const src = dragSourceNode;
    dropHoverId  = null;
    dragSourceId = null;
    dragSourceNode = null;
    if (src) onDropOnNode?.(src, node);
  }

  function handleKeydown(node: T, hasKids: boolean, e: KeyboardEvent) {
    // Minimal keyboard model — Enter / Space activate, Right opens,
    // Left closes. Up/Down list-nav is left to the browser's default
    // tab order so we don't fight focus traps in modal hosts.
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      if (hasKids) toggle(node);
      if ((selectable ?? defaultSelectable)(node)) onSelect?.(node, e);
    } else if (e.key === 'ArrowRight' && hasKids && !isExpanded(node)) {
      e.preventDefault();
      toggle(node);
    } else if (e.key === 'ArrowLeft' && hasKids && isExpanded(node)) {
      e.preventDefault();
      toggle(node);
    }
  }
</script>

<div
  class="tree {rootClass}"
  role="tree"
  aria-label={ariaLabel}
  bind:this={treeEl}
  style:--tree-row-h="{rowHeight}px"
>
  {#if !nodes || nodes.length === 0 || flat.length === 0}
    {#if emptyState}{@render emptyState()}{:else}
      <div class="tree-empty">No items.</div>
    {/if}
  {:else}
    <div
      class="tree-scroller"
      style="position: relative; width: 100%; height: {totalH}px;"
    >
      {#each visible as r, vi (r.id)}
        {@const isSelected = !!selectedId && r.id === selectedId}
        {@const ctx        = { node: r.node, depth: r.depth, expanded: r.expanded, selected: isSelected, hasChildren: r.hasChildren, toggle: () => toggle(r.node) }}
        {@const extraClass = rowClass?.(ctx) ?? ''}
        {@const isDraggable = !!draggable?.(r.node)}
        {@const isDropHover = dropHoverId === r.id && !!dropTarget?.(r.node) && dropHoverId !== dragSourceId}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          class="tree-row {extraClass}"
          class:tree-row-selected={isSelected}
          class:tree-row-expandable={r.hasChildren}
          class:tree-row-leaf={!r.hasChildren}
          class:tree-row-drop-hover={isDropHover}
          style="position: absolute; left: 0; right: 0; top: {(startIdx + vi) * rowHeight}px; height: {rowHeight}px; min-height: {rowHeight}px; padding-left: {basePadding + r.depth * indentSize}px; display: flex; align-items: center; gap: {cellGap}px; box-sizing: border-box;"
          role="treeitem"
          aria-expanded={r.hasChildren ? r.expanded : undefined}
          aria-selected={isSelected}
          tabindex={(selectable ?? defaultSelectable)(r.node) || r.hasChildren ? 0 : -1}
          draggable={isDraggable}
          use:tooltip={rowTitle?.(r.node) ?? ''}
          onclick={(e) => handleClick(r.node, r.hasChildren, e)}
          ondblclick={(e) => handleDblClick(r.node, e)}
          oncontextmenu={(e) => handleContextMenu(r.node, e)}
          onkeydown={(e) => handleKeydown(r.node, r.hasChildren, e)}
          ondragstart={(e) => handleDragStart(r.node, e)}
          ondragend={handleDragEnd}
          ondragover={(e) => handleDragOver(r.node, e)}
          ondragleave={() => handleDragLeave(r.node)}
          ondrop={(e) => handleDrop(r.node, e)}
        >
          {#if guides && r.depth > 0}
            <span class="tree-guides" aria-hidden="true">
              {#each Array(r.depth) as _, gi}
                <span
                  class="tree-guide"
                  style:left="{basePadding + gi * indentSize + 11}px"
                ></span>
              {/each}
            </span>
          {/if}

          {#if showChevron}
            <span class="tree-caret" class:tree-caret-empty={!r.hasChildren}>
              {#if r.hasChildren}
                {#if r.expanded}
                  <ChevronDown size={11} />
                {:else}
                  <ChevronRight size={11} />
                {/if}
              {/if}
            </span>
          {/if}

          {@render row(ctx)}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .tree {
    display: flex;
    flex-direction: column;
    user-select: none;
    font-family: var(--font-ui-sans);
  }

  .tree-empty {
    padding: 18px 12px;
    text-align: center;
    font-size: 11px;
    color: var(--text-muted);
    font-style: italic;
  }

  /* Virtual scroller: holds the full list height so the host's
     scrollbar stays accurate; rows are absolutely positioned inside. */
  :global(.tree .tree-scroller) {
    position: relative;
    width: 100%;
  }

  /* Rows are styled :global so consumer-side snippets that introduce
     extra classes (.tree-row.is-folder, etc.) match without restating
     the base rules — same approach as Tabs.svelte's :global() shell. */
  :global(.tree .tree-row) {
    position: absolute;
    left: 0;
    right: 0;
    display: flex;
    align-items: center;
    gap: 5px;
    height: var(--tree-row-h, 22px);
    min-height: var(--tree-row-h, 22px);
    padding-right: 6px;
    color: var(--text-primary);
    font-size: 12px;
    cursor: default;
    background: transparent;
    border: none;
    /* No background/colour transitions — virtualised rows are recycled
       on scroll and a transition would flash on every reuse. */
    text-align: left;
    box-sizing: border-box;
  }
  :global(.tree .tree-row:focus-visible) {
    outline: 1px solid var(--border-focus, var(--accent));
    outline-offset: -1px;
  }
  :global(.tree .tree-row:hover) {
    background: var(--bg-hover);
  }
  :global(.tree .tree-row-expandable) { cursor: pointer; }
  :global(.tree .tree-row-selected) {
    background: var(--accent-subtle);
    color: var(--text-primary);
  }
  /* Drop-target hover (Phase 6.2). Inset outline so it sits inside the row
     rect — virtualised rows recycle on scroll, an outline outside the box
     would clip against the next row. */
  :global(.tree .tree-row-drop-hover) {
    background: color-mix(in srgb, var(--accent) 16%, transparent);
    outline: 1px dashed var(--accent);
    outline-offset: -2px;
  }

  /* Disclosure caret. Always reserves the same width whether or not
     the node has children, so labels line up across siblings. */
  :global(.tree .tree-caret) {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 12px;
    height: 12px;
    flex-shrink: 0;
    color: var(--text-muted);
  }

  /* Indent guides — opt-in via the `guides` prop. With a flat windowed
     list we can no longer rely on nested DOM containers to draw guides,
     so each row paints its own vertical 1px lines at every ancestor
     depth. The guide layer is pointer-transparent so chevrons / icons
     keep their hit areas. */
  :global(.tree .tree-guides) {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }
  :global(.tree .tree-guide) {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 1px;
    background: var(--border-subtle);
  }

  /* ── Row content conventions ────────────────────────────────────────────
     These classes are NOT required, but every consumer that puts an icon /
     label / badge / actions inside the `row` snippet ends up needing the
     same styling. Surfacing them here as :global rules lets every Tree
     consumer (PluginTreeSidebar, FileTreePanel, StageArea, …) drop into a
     consistent look without restating the rules each time. */

  :global(.tree .tree-icon) {
    display: inline-flex;
    align-items: center;
    flex-shrink: 0;
    color: var(--text-secondary);
  }
  :global(.tree .tree-row-selected .tree-icon) { color: var(--accent); }

  :global(.tree .tree-label) {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  :global(.tree .tree-badge) {
    flex-shrink: 0;
    font-family: var(--font-code);
    font-size: 10px;
    line-height: 1;
    padding: 2px 6px;
    border-radius: 999px;
    white-space: nowrap;
    border: 1px solid transparent;
  }
  :global(.tree .tree-badge-muted)   { color: var(--text-muted);     background: var(--bg-overlay); }
  :global(.tree .tree-badge-info)    { color: var(--text-secondary); background: var(--bg-elevated); }
  :global(.tree .tree-badge-accent)  { color: var(--accent);         background: var(--accent-subtle);
                                       border-color: color-mix(in srgb, var(--accent) 30%, transparent); }
  :global(.tree .tree-badge-success) { color: var(--success);
                                       background: color-mix(in srgb, var(--success) 14%, transparent);
                                       border-color: color-mix(in srgb, var(--success) 30%, transparent); }
  :global(.tree .tree-badge-warning) { color: var(--warning);
                                       background: color-mix(in srgb, var(--warning) 14%, transparent);
                                       border-color: color-mix(in srgb, var(--warning) 30%, transparent); }
  :global(.tree .tree-badge-error)   { color: var(--error);
                                       background: color-mix(in srgb, var(--error) 14%, transparent);
                                       border-color: color-mix(in srgb, var(--error) 30%, transparent); }

  /* Always-on decorations (status icons, dates, branches). */
  :global(.tree .tree-decorator) {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    flex-shrink: 0;
    color: var(--text-muted);
  }

  /* Hover-reveal action zone. Mirrors `.card-item-actions` in
     PluginSidebarPanel for visual continuity. */
  :global(.tree .tree-actions) {
    display: inline-flex;
    align-items: center;
    gap: 1px;
    flex-shrink: 0;
    opacity: 0;
    transform: translateX(3px);
    transition: opacity var(--transition-fast), transform var(--transition-fast);
  }
  :global(.tree .tree-row:hover .tree-actions),
  :global(.tree .tree-row-selected .tree-actions) {
    opacity: 1;
    transform: none;
  }

  /* Compact action button sized to fit the 22px row. Children of
     `.tree-actions` rendered via the snippet apply this class. */
  :global(.tree .tree-row-action) {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    padding: 0;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  :global(.tree .tree-row-action:hover) {
    background: var(--bg-base);
    color: var(--text-primary);
  }
  :global(.tree .tree-row-action.accent:hover) { color: var(--accent); }
  :global(.tree .tree-row-action.danger:hover) { color: var(--error); }
  :global(.tree .tree-row-action:disabled)     { opacity: 0.35; cursor: not-allowed; }

  /* IntelliJ-style group header rows — muted, smaller, no selection
     highlight. Apply via `rowClass={(ctx) => ctx.node.kind === 'section' ? 'tree-row-section' : ''}`. */
  :global(.tree .tree-row.tree-row-section) {
    color: var(--text-secondary);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.02em;
  }
  :global(.tree .tree-row.tree-row-section:hover) { background: var(--bg-overlay); }
  :global(.tree .tree-row.tree-row-section.tree-row-selected) { background: transparent; }
</style>
