<!--
  StudioTreePane — format-agnostic tree controller for the studio modal.

  Owns the whole tree surface:

    · Viewport scroll container + virtualised `<Tree>` instance + parse-
      error placeholder (Phase 2B-2.f.1 footprint).
    · Tree data model — `roots`, `byPid` index, `expanded` Set,
      `selectedNode`, `valueText`/`valueLoading`. All mutations go
      through the panel; parent reads via `bind:` and observes via the
      `onSelectionChange` callback.
    · Tree operations — `reloadTree`, `selectNode`, `loadChildren`,
      `reloadChildrenInPlace`, `dropIndex`, `refreshAfterMutation`,
      `expandAll`/`collapseAll`/`expandSubtree`/`collapseSubtree`,
      `jumpToPath`, `getNode`, `getChildKeysForPath`,
      `ensureChildrenLoadedForPath`, `scrollPidIntoView`,
      `forceRootsRefresh`, `resetState`.
    · Context menu — instance + position state + open helper. Items are
      computed by the format wrapper via `getContextMenuItems(node)` and
      menu-select events are forwarded via `onContextMenuSelect(id,
      node)`. Drag/drop, when added, will live here too.

  What this component STILL does NOT own:
    · Inline edit state (`editingPid`, `editLocation`, `editBuf`,
      `editError`). The Inspector also drives the same edit pipeline
      (f.2 boundary), so the trigger + commit/cancel functions live in
      the wrapper. The tree-row edit input refs are passed in as
      `$bindable` so the wrapper's focus dance (location='tree') keeps
      working.
    · Format-specific mutations (`insertField`, `removeAt`,
      `pickVariant`, etc.). They live in the wrapper because they hit
      the format-specific store (`ronStudioStore` today). After the
      mutation the wrapper calls back into the panel via
      `refreshAfterMutation(node, structural, removed?)` so the live
      tree mirrors the store.
    · Row content. Heavily format-specific (RON: schema decoration,
      cross-refs, named-type chip, inline edit input, …). The wrapper
      supplies it via the `rowContent` snippet, which renders in the
      wrapper's scope and reads its helpers directly.
-->
<script lang="ts" module>
  import type { Snippet } from 'svelte';
  import type {
    StudioBackend, StudioFormat, StudioNodeView,
  } from '$lib/ipc/studio-format';
  import type { MenuItem } from '../ContextMenu.svelte';
  import type { RowSnippetCtx } from '../ui/Tree.svelte';
  export type { RowSnippetCtx };

  /** Minimal shape every studio tree node must satisfy. Format-specific
   *  views (RON's `TNode`, JSON's equivalent) extend this with their own
   *  fields — the panel only reaches for the fields below. */
  export interface StudioTreeNodeBase<TKind extends string = string> {
    pid:         string;
    path:        string[];
    kind:        TKind;
    key:         string;
    preview:     string;
    variant_tag: string | null;
    child_count: number;
    children:    StudioTreeNodeBase<TKind>[] | null;
    loading?:    boolean;
  }

  /** Imperative surface exposed via `bind:this`. Wraps the tree's
   *  whole state-mutating + navigation API so the format wrapper can
   *  drive it without owning the underlying state. */
  export interface StudioTreePaneController<
    TKind extends string = string,
    TNode extends StudioTreeNodeBase<TKind> = StudioTreeNodeBase<TKind>,
  > {
    /** Centre the row at `pid` in the viewport. Retries across a few
     *  RAFs because Tree.svelte's virtualiser needs the spacer height
     *  + ResizeObserver to settle before `clientHeight` is coherent. */
    scrollPidIntoView(pid: string): void;

    // ── Lifecycle ────────────────────────────────────────────────────
    /** Full reload from the backend. Drops `byPid`, refetches the root
     *  + first level, restores selection if the pid still exists. */
    reloadTree(): Promise<void>;
    /** Drop everything (used when the doc closes / docId goes null). */
    resetState(): void;

    // ── Selection ────────────────────────────────────────────────────
    /** Update selection + load the primitive value for the right-side
     *  detail pane. Commits any pending inline edit first when the
     *  wrapper supplies `commitPendingEdit`. */
    selectNode(node: TNode): Promise<void>;

    // ── Node lookup ──────────────────────────────────────────────────
    /** Live `byPid` index lookup. Returns `null` when the pid hasn't
     *  been materialised yet (ancestor not expanded). */
    getNode(pid: string): TNode | null;
    /** Children keys at a path (for query bar autocomplete). `null`
     *  when the subtree isn't materialised — the caller usually pairs
     *  this with `ensureChildrenLoadedForPath`. */
    getChildKeysForPath(path: string[]): string[] | null;
    ensureChildrenLoadedForPath(path: string[]): void;

    // ── Expansion ────────────────────────────────────────────────────
    expandAll(): Promise<void>;
    collapseAll(): void;
    expandSubtree(node: TNode): Promise<void>;
    collapseSubtree(node: TNode): void;
    /** Walk root → target, lazy-loading every container along the
     *  way, then expand the chain, select the target, and centre the
     *  row in the viewport. Used by `jumpToQueryHit` + cross-ref
     *  navigation. */
    jumpToPath(path: string[]): Promise<void>;

    /** Lazy-load a node's children (no-op when already materialised).
     *  Format wrappers call this from inside mutation pipelines that
     *  need to look up the just-materialised inner via `getNode`. */
    loadChildren(node: TNode): Promise<void>;

    // ── Mutations refresh ────────────────────────────────────────────
    /** Refresh just the affected subtree after a wrapper-side
     *  mutation. Preserves the user's expansion state. */
    refreshAfterMutation(
      node:       TNode,
      structural: boolean,
      removed?:   boolean,
    ): Promise<void>;
    /** Lazy-load + reindex a single node's children. */
    reloadChildrenInPlace(node: TNode): Promise<void>;
    /** Force a `roots = [...roots]` reassignment so Svelte's $state
     *  proxy re-fires reactivity. Used by wrapper-side ops that mutate
     *  a node in place. */
    forceRootsRefresh(): void;

  }

  export interface StudioTreePaneProps<
    TKind extends string = string,
    TNode extends StudioTreeNodeBase<TKind> = StudioTreeNodeBase<TKind>,
  > {
    formatId: StudioFormat;
    /** Pre-bound backend. The panel calls `backend.getRoot` /
     *  `backend.getChildren` / `backend.getValue` directly. */
    backend: StudioBackend<TKind>;
    /** Active document id. The panel reloads whenever this changes;
     *  pass `null` to clear. */
    docId: string | null;
    /** When set, the placeholder takes over and the tree is hidden. */
    parseError: string | null;

    // ── Bindable state (parent + panel share) ────────────────────────
    /** Top-level tree nodes. Single root in practice for RON/JSON;
     *  array shape leaves room for multi-doc views later. */
    roots: TNode[];
    /** Expansion set. Mutated by the panel on chevron toggle; the
     *  wrapper may also mutate it (e.g. cross-ref jump) — bindable
     *  both ways. */
    expanded: Set<string>;
    /** Current selection. The panel writes here on row click +
     *  imperative `selectNode`. */
    selectedNode: TNode | null;
    /** Pretty-printed primitive value for the right-side detail pane.
     *  `null` for containers + while loading. */
    valueText: string | null;
    /** True while `selectNode` is fetching the primitive value. */
    valueLoading: boolean;
    /** True while `expandAll` is loading subtrees. Bindable so the
     *  wrapper's toolbar spinner reads it reactively. */
    expandAllBusy: boolean;

    // ── Format-specific helpers (callback props) ─────────────────────
    /** Wrap a raw `StudioNodeView<TKind>` from the backend into the
     *  wrapper's narrower `TNode` (adds `pid`, initialises `children
     *  = null`). Same factory every load path uses. */
    toTree: (view: StudioNodeView<TKind>) => TNode;
    /** Reorder children for display under the given parent. RON
     *  groups objects/arrays/options/rest under named-key parents;
     *  most formats can return the input list as-is. */
    sortChildren: (parentKind: TKind, kids: TNode[]) => TNode[];
    /** True when a kind is a container (struct/map/list/tuple/…) —
     *  the panel skips primitive-value loading for containers in
     *  `selectNode`. */
    isContainerKind: (kind: TKind) => boolean;
    /** Format-specific context-menu items for the right-clicked node.
     *  Recomputed each open. */
    getContextMenuItems: (node: TNode) => MenuItem[];

    // ── Callbacks ────────────────────────────────────────────────────
    /** Menu-item activation. The panel closes the menu before
     *  dispatching, so the wrapper only handles the action. */
    onContextMenuSelect: (id: string, node: TNode) => void | Promise<void>;
    /** Notification only — fires whenever the selected node changes.
     *  The wrapper uses this to react (e.g. clear stale errors). */
    onSelectionChange?: (node: TNode | null) => void;
    /** Hook the wrapper calls before the panel changes selection.
     *  Resolves once any pending inline edit is committed or
     *  cancelled. Optional — formats without inline edit can omit. */
    commitPendingEdit?: () => Promise<void>;
    /** Fires after a successful `selectNode` so the wrapper can stash
     *  the value text in places that don't observe the bindable
     *  (e.g. on-disk cache). */
    onAfterSelect?: (node: TNode, valueText: string | null) => void;

    // ── Tree visual config ───────────────────────────────────────────
    rowHeight?:   number;
    cellGap?:     number;
    indentSize?:  number;
    basePadding?: number;
    /** Default `true`. Disabled by the wrapper when the inspector
     *  pane is collapsed so the tree reads as one edge-to-edge surface. */
    showRightBorder?: boolean;
    ariaLabel?:   string;
    /** Copy shown next to the alert icon when `parseError` is set.
     *  Default: a generic format-agnostic line. */
    errorMessage?: string;

    // ── Snippets ─────────────────────────────────────────────────────
    /** Format-specific row content. Forwarded directly to the
     *  underlying `<Tree>`'s `row` snippet. Named `rowContent` (not
     *  `row`) to avoid shadowing inside the panel's own
     *  `{#snippet row(...)}` block. */
    rowContent: Snippet<[RowSnippetCtx<TNode>]>;
  }
</script>

<script
  lang="ts"
  generics="TKind extends string, TNode extends StudioTreeNodeBase<TKind>"
>
  import { tick, untrack } from 'svelte';
  import { AlertCircle } from 'lucide-svelte';
  import Tree from '../ui/Tree.svelte';
  import ContextMenu from '../ContextMenu.svelte';

  let {
    formatId: _formatId,
    backend,
    docId,
    parseError,
    roots = $bindable(),
    expanded = $bindable(),
    selectedNode = $bindable(),
    valueText = $bindable(),
    valueLoading = $bindable(),
    expandAllBusy = $bindable(),
    toTree,
    sortChildren,
    isContainerKind,
    getContextMenuItems,
    onContextMenuSelect,
    onSelectionChange,
    commitPendingEdit,
    onAfterSelect,
    rowHeight = 22,
    cellGap = 6,
    indentSize = 14,
    basePadding = 8,
    showRightBorder = true,
    ariaLabel = 'Studio document tree',
    errorMessage = "Document doesn't parse — switch to Errors or fix the text.",
    rowContent,
  }: StudioTreePaneProps<TKind, TNode> = $props();

  void untrack(() => _formatId);

  /** Scroll container. Bound directly to the `<div>` below. */
  let paneEl: HTMLDivElement | undefined = $state();

  /** Selected pid is derived from `selectedNode` so the wrapper doesn't
   *  need to set both. Tree.svelte uses this for the row's `selected`
   *  decoration. */
  const selectedPid = $derived(selectedNode?.pid ?? null);

  /** Live pid → node index. Rebuilt by `reloadTree`, patched by every
   *  state-mutating method below. Not reactive — Map mutations don't
   *  trigger Svelte reactivity, which is what we want here (the public
   *  reactive surface is `roots` + `expanded` + `selectedNode`). */
  const byPid = new Map<string, TNode>();
  function indexNode(n: TNode): void {
    byPid.set(n.pid, n);
    if (n.children) for (const c of n.children as TNode[]) indexNode(c);
  }
  function dropIndex(n: TNode): void {
    byPid.delete(n.pid);
    if (n.children) for (const c of n.children as TNode[]) dropIndex(c);
  }

  // ── Context menu ────────────────────────────────────────────────────
  /** Open context-menu state. Position + the node it targets. The
   *  panel renders the actual `<ContextMenu>` instance below. */
  let ctxMenu = $state<{ x: number; y: number; node: TNode } | null>(null);
  function openContextMenu(node: TNode, e: MouseEvent): void {
    ctxMenu = { x: e.clientX, y: e.clientY, node };
  }
  async function onMenuSelect(id: string): Promise<void> {
    const node = ctxMenu?.node;
    ctxMenu = null;
    if (!node) return;
    // Selection-before-action: matches IntelliJ behaviour. The
    // selected node is what most actions operate on, and the wrapper's
    // mutation handlers expect a consistent selectedNode.
    await selectNode(node);
    await onContextMenuSelect(id, node);
  }

  // ── Tree lifecycle ──────────────────────────────────────────────────
  export function resetState(): void {
    roots = [];
    expanded = new Set();
    selectedNode = null;
    valueText = null;
    byPid.clear();
  }

  export async function reloadTree(): Promise<void> {
    if (!docId || parseError) {
      resetState();
      return;
    }
    try {
      const r = await backend.getRoot(docId);
      if (!r) { roots = []; return; }
      const node = toTree(r);
      if (r.child_count > 0) {
        const kids = await backend.getChildren(docId, r.path);
        node.children = sortChildren(node.kind, kids.map(toTree) as TNode[]) as TNode[];
        const next = new Set(expanded);
        next.add(node.pid);
        expanded = next;
      } else {
        node.children = [] as TNode[];
      }
      byPid.clear();
      indexNode(node);
      roots = [node];
      if (selectedPid) {
        const n = byPid.get(selectedPid) ?? node;
        await selectNode(n);
      } else {
        await selectNode(node);
      }
    } catch (e) {
      console.warn('studio-tree: reloadTree failed', e);
    }
  }

  export async function loadChildren(node: TNode): Promise<void> {
    if (!docId || node.children !== null) return;
    node.loading = true;
    try {
      const kids = await backend.getChildren(docId, node.path);
      node.children = sortChildren(node.kind, kids.map(toTree) as TNode[]) as TNode[];
      for (const c of node.children as TNode[]) indexNode(c);
    } catch (e) {
      console.warn('studio-tree: loadChildren failed', e);
      node.children = [] as TNode[];
    } finally {
      node.loading = false;
      roots = [...roots];
    }
  }

  function onExpandToggle(id: string, next: boolean, node: TNode): void {
    const n = new Set(expanded);
    if (next) n.add(id); else n.delete(id);
    expanded = n;
    if (next && node.children === null && node.child_count > 0) {
      void loadChildren(node);
    }
  }

  // ── Selection ───────────────────────────────────────────────────────
  export async function selectNode(node: TNode): Promise<void> {
    // Commit any pending inline edit when moving to a different node —
    // matches IntelliJ behaviour where clicking elsewhere implicitly
    // confirms the value. The wrapper owns the edit state machine; we
    // just give it a chance to flush before changing selection.
    if (commitPendingEdit) {
      try { await commitPendingEdit(); } catch { /* wrapper handles fallback */ }
    }
    selectedNode = node;
    onSelectionChange?.(node);
    if (!docId) return;
    // Containers: skip serialising — preview is enough. We do lazy-
    // load the immediate children though, so the Inspector can render
    // a first-level content preview without forcing the user to
    // expand the row in the tree first. `loadChildren` is a no-op
    // when children are already materialised.
    if (isContainerKind(node.kind)) {
      valueText = null;
      if (node.children === null && node.child_count > 0) {
        void loadChildren(node);
      }
      onAfterSelect?.(node, null);
      return;
    }
    valueLoading = true;
    try {
      valueText = await backend.getValue(docId, node.path);
    } catch (e) {
      valueText = `(error loading value: ${e})`;
    } finally {
      valueLoading = false;
    }
    onAfterSelect?.(node, valueText);
  }

  function onRowClick(node: TNode): void {
    void selectNode(node);
  }
  function onRowContextMenu(node: TNode, e: MouseEvent): void {
    openContextMenu(node, e);
  }

  // ── Query bar support ───────────────────────────────────────────────
  export function getNode(pid: string): TNode | null {
    return byPid.get(pid) ?? null;
  }
  function pathToPid(path: string[]): string { return path.join('\x00'); }
  export function getChildKeysForPath(path: string[]): string[] | null {
    if (roots.length === 0) return null;
    const node = byPid.get(pathToPid(path));
    if (!node) return null;
    if (node.children === null) return null;
    return (node.children as TNode[]).map(c => c.key);
  }
  export function ensureChildrenLoadedForPath(path: string[]): void {
    if (roots.length === 0) return;
    const node = byPid.get(pathToPid(path));
    if (node && node.children === null && node.child_count > 0) {
      void loadChildren(node);
    }
  }

  // ── Expansion ───────────────────────────────────────────────────────
  /** Walk a subtree, lazily loading children and adding every
   *  container pid to the `expanded` set. Bounded by the schema-resolved
   *  child_count, so cycles in the data don't matter — every container
   *  is finite. */
  export async function expandSubtree(node: TNode): Promise<void> {
    if (!docId || node.child_count === 0) return;
    const next = new Set(expanded);
    async function walk(n: TNode): Promise<void> {
      if (n.child_count === 0) return;
      if (n.children === null) await loadChildren(n);
      next.add(n.pid);
      if (n.children) for (const c of n.children as TNode[]) await walk(c);
    }
    await walk(node);
    expanded = next;
    roots = [...roots];
  }

  export function collapseSubtree(node: TNode): void {
    const next = new Set(expanded);
    function walk(n: TNode): void {
      next.delete(n.pid);
      if (n.children) for (const c of n.children as TNode[]) walk(c);
    }
    walk(node);
    expanded = next;
  }

  /** Recursively expand AND lazily fetch every container so the user
   *  sees the whole structure unfolded. Safety cap MAX_NODES aborts
   *  the walk on huge documents — the user gets the upper-N-thousand
   *  expanded and can drill in manually. Same-level children are
   *  loaded in parallel for a wide-but-shallow fan-out. */
  export async function expandAll(): Promise<void> {
    if (!docId || roots.length === 0) return;
    const MAX_NODES = 5000;
    const nextExp = new Set(expanded);
    let count = 0;
    expandAllBusy = true;
    try {
      async function walk(nodesAtLevel: TNode[]): Promise<void> {
        if (count >= MAX_NODES) return;
        const unloaded = nodesAtLevel.filter(n => n.child_count > 0 && n.children === null);
        if (unloaded.length > 0) {
          await Promise.all(unloaded.map(n => loadChildren(n)));
        }
        const nextLevel: TNode[] = [];
        for (const n of nodesAtLevel) {
          if (count >= MAX_NODES) break;
          if (n.child_count > 0) {
            nextExp.add(n.pid);
            count++;
            if (n.children) nextLevel.push(...(n.children as TNode[]));
          }
        }
        if (nextLevel.length > 0 && count < MAX_NODES) {
          await walk(nextLevel);
        }
      }
      await walk(roots);
      expanded = nextExp;
    } finally {
      expandAllBusy = false;
    }
  }

  /** Collapse the entire tree except the root row — instant, no IPC. */
  export function collapseAll(): void {
    if (roots.length === 0) { expanded = new Set(); return; }
    expanded = new Set([roots[0].pid]);
  }

  // ── Navigation ──────────────────────────────────────────────────────
  /** Walks `roots[0]` to lazy-load every container along the way,
   *  expands the chain, selects the target, and centres the row.
   *
   *  Walking via `roots[0]` instead of `byPid` ancestors keeps every
   *  reference inside the proxy graph, matching what Tree.svelte
   *  iterates, so mutations light up reactivity reliably. (Bypassing
   *  the proxy via `byPid` silently no-ops in cold-start cases.) */
  export async function jumpToPath(path: string[]): Promise<void> {
    if (!docId || roots.length === 0) return;
    const nextExp = new Set(expanded);
    let cur: TNode = roots[0];
    nextExp.add(cur.pid);
    for (let depth = 0; depth < path.length; depth++) {
      if (cur.child_count > 0 && cur.children === null) {
        await loadChildren(cur);
      }
      if (!cur.children) break;
      const seg = path[depth];
      const child = (cur.children as TNode[]).find(c =>
        c.path[c.path.length - 1] === seg
      );
      if (!child) break;
      nextExp.add(child.pid);
      cur = child;
    }
    if (cur.child_count > 0 && cur.children === null) {
      await loadChildren(cur);
    }
    expanded = nextExp;
    roots = [...roots];
    await tick();
    await selectNode(cur);
    await tick();
    scrollPidIntoView(cur.pid);
  }

  // ── Mutations refresh ───────────────────────────────────────────────
  /** Reload a single node's children — drops stale indexed
   *  descendants from `byPid` first. Keeps `node.children = null`
   *  (lazy) when the user never expanded it. */
  export async function reloadChildrenInPlace(node: TNode): Promise<void> {
    if (!docId) return;
    if (node.children !== null) {
      for (const c of node.children as TNode[]) dropIndex(c);
      if (node.child_count > 0) {
        const kids = await backend.getChildren(docId, node.path);
        node.children = sortChildren(node.kind, kids.map(toTree) as TNode[]) as TNode[];
        for (const c of node.children as TNode[]) indexNode(c);
      } else {
        node.children = [] as TNode[];
        if (expanded.has(node.pid)) {
          const next = new Set(expanded);
          next.delete(node.pid);
          expanded = next;
        }
      }
    }
  }

  /** Refresh just the affected subtree after a mutation. Preserves
   *  the user's expansion state (which `reloadTree` would blow away).
   *  When the structure changed (option toggle, variant pick, remove
   *  …) the visible children are reloaded too, so the rendered slice
   *  stays consistent.
   *
   *  For `removed` nodes the parent's children list is reloaded and
   *  the selection falls back to the parent. */
  export async function refreshAfterMutation(
    node:       TNode,
    structural: boolean,
    removed:    boolean = false,
  ): Promise<void> {
    if (!docId) return;
    try {
      if (removed) {
        const parentPath = node.path.slice(0, -1);
        const parent = byPid.get(pathToPid(parentPath));
        if (parent) {
          await reloadChildrenInPlace(parent);
          await selectNode(parent);
        }
        return;
      }
      // Re-fetch this node's NodeView (kind/preview/variant_tag/child_count).
      let fresh: StudioNodeView<TKind> | null = null;
      if (node.path.length === 0) {
        fresh = await backend.getRoot(docId);
      } else {
        const parentPath = node.path.slice(0, -1);
        const kids = await backend.getChildren(docId, parentPath);
        fresh = (kids.find(k => k.key === node.key) ?? null) as StudioNodeView<TKind> | null;
      }
      if (fresh) {
        node.kind        = fresh.kind;
        node.preview     = fresh.preview;
        node.variant_tag = fresh.variant_tag;
        node.child_count = fresh.child_count;
      }
      if (structural) {
        await reloadChildrenInPlace(node);
      }
      // Refresh detail value pane for primitives.
      if (!isContainerKind(node.kind)) {
        try { valueText = await backend.getValue(docId, node.path); } catch { /* keep stale */ }
      } else {
        valueText = null;
      }
      roots = [...roots];
    } catch (e) {
      console.warn('studio-tree: refreshAfterMutation failed', e);
    }
  }

  export function forceRootsRefresh(): void {
    roots = [...roots];
  }

  // ── Scroll-into-view ────────────────────────────────────────────────
  /** DFS over the SAME flattened structure Tree.svelte renders —
   *  `roots` plus the `expanded` set. Returns the 0-based row index
   *  of `targetPid` or -1 when invisible. */
  function flatRowIndex(targetPid: string, currentExpanded: Set<string>): number {
    let result = -1;
    let counter = 0;
    function walk(node: TNode): boolean {
      if (node.pid === targetPid) { result = counter; return true; }
      counter++;
      if (currentExpanded.has(node.pid) && node.children) {
        for (const c of node.children as TNode[]) if (walk(c)) return true;
      }
      return false;
    }
    for (const r of roots) if (walk(r)) break;
    return result;
  }

  /** Centre the row at `pid` in the tree pane. Retries across a few
   *  animation frames because Tree.svelte's virtual scroller needs
   *  the ResizeObserver + spacer-height update to settle before
   *  `clientHeight` and the inner content height are coherent — too
   *  early and `scrollTop` gets clamped against a stale `scrollHeight`
   *  (the new rows haven't been measured yet) and the row ends up at
   *  the top edge instead of centred, or worse off-screen.
   *
   *  Uses `behavior: 'auto'` (instant). Smooth-scroll animations
   *  through a virtualised list are unreliable here: Tree.svelte's
   *  viewport listener recomputes on each scroll event, but a smooth
   *  scroll over a tall spacer fires many small events and the row
   *  layout flickers as the visible slice shifts every frame. Instant
   *  scroll gives Tree exactly one scroll event → one slice update →
   *  one paint. */
  export function scrollPidIntoView(pid: string): void {
    let attempts = 0;
    const tryScroll = () => {
      const pane = paneEl;
      if (!pane) return;
      const idx = flatRowIndex(pid, expanded);
      if (idx < 0) {
        if (attempts++ < 5) requestAnimationFrame(tryScroll);
        return;
      }
      const rowY      = idx * rowHeight;
      const viewportH = pane.clientHeight;
      const target    = Math.max(0, rowY - viewportH / 2 + rowHeight / 2);
      const band = viewportH / 3;
      const visibleTop    = pane.scrollTop + band;
      const visibleBottom = pane.scrollTop + viewportH - band;
      if (rowY < visibleTop || rowY + rowHeight > visibleBottom) {
        pane.scrollTo({ top: target, behavior: 'auto' });
      }
    };
    requestAnimationFrame(() => requestAnimationFrame(tryScroll));
  }

  // ── docId lifecycle ─────────────────────────────────────────────────
  // When the wrapper switches doc (or closes it), wire the panel
  // accordingly. Wrapped in `untrack` so the body only fires on docId
  // change, not on every selection / expansion write.
  $effect(() => {
    const id = docId;
    untrack(() => {
      if (!id) {
        resetState();
        return;
      }
      void reloadTree();
    });
  });
</script>

<div class="str-pane" class:str-pane-noborder={!showRightBorder} bind:this={paneEl}>
  {#if parseError}
    <div class="str-empty">
      <AlertCircle size={16} />
      <span>{errorMessage}</span>
    </div>
  {:else}
    <Tree
      nodes={roots as any[]}
      getId={(n: any) => (n as TNode).pid}
      getChildren={(n: any) => (n as TNode).children ?? undefined}
      hasChildren={(n: any) => (n as TNode).child_count > 0}
      expandedIds={expanded}
      onExpandToggle={onExpandToggle as any}
      selectedId={selectedPid}
      onSelect={(n: any) => onRowClick(n as TNode)}
      onContextMenu={(n: any, e: MouseEvent) => onRowContextMenu(n as TNode, e)}
      {rowHeight}
      {cellGap}
      {indentSize}
      {basePadding}
      showChevron={true}
      guides={true}
      {ariaLabel}
    >
      {#snippet row(ctx: RowSnippetCtx<any>)}
        {@render rowContent(ctx as RowSnippetCtx<TNode>)}
      {/snippet}
    </Tree>
  {/if}
</div>

{#if ctxMenu}
  <ContextMenu
    items={getContextMenuItems(ctxMenu.node)}
    x={ctxMenu.x}
    y={ctxMenu.y}
    onSelect={onMenuSelect}
    onClose={() => ctxMenu = null}
  />
{/if}

<style>
  /* Viewport scroll container — flex-fills the parent split row.
     `min-width: 0` is critical here: without it, a long row that
     overflows the panel makes the flex item compute its own width
     from the content and pushes the inspector off the modal. */
  .str-pane {
    flex: 1; min-width: 0; overflow-y: auto;
    border-right: 1px solid var(--border-subtle);
    padding: 4px 0;
  }
  /* Parent toggles this when the inspector pane is hidden so the tree
     reads as a single edge-to-edge surface. */
  .str-pane-noborder { border-right: none; }

  .str-empty {
    display: flex; align-items: center; gap: 8px;
    padding: 20px;
    color: var(--text-muted);
    font-size: 12px;
    justify-content: center;
  }
</style>
