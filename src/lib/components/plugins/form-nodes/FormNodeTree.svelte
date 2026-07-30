<!--
  FormNodeTree — the `tree` field node (single + multi select), now also the
  dynamic "data tree": lazy children, scoped selection/expand events, keyboard
  navigation and row virtualization for large trees.

  Value model (unchanged from the static tree): value-bearing. The selected
  node `value` (or `value[]` in `multi`) lives in `ctx.values[name]` and is
  submitted like any field. The legacy whole-form `change_action` still fires
  for single-select master/detail.

  Dynamic opt-ins (all additive — absent ⇒ today's static behaviour):
    · lazy + on_expand — expanding a node that advertises `has_children` but
      has no loaded `children` fires the scoped `on_expand` slot
      (`{ id, value, path }`) and shows a spinner row until the plugin patches
      the children in (merge `children` + clear `loading`, addressed by the
      row's stable `id`).
    · on_select — scoped selection event (ships the new value), preferred over
      `change_action` when both are set.
    · virtualize_threshold / row_height — window the flattened visible rows
      when they exceed the threshold (fixed row height, like VirtualHunk).
    · searchable + search_placeholder — inline filter input at the top of the
      tree; case-insensitive substring match on `label` + `description`;
      ancestors of matches auto-expand and matched substring is highlighted.
    · reorderable + on_reorder — HTML5 drag-drop reorder among rows. The
      cursor's vertical position over the target row picks a drop zone
      (`before` | `inside` | `after`); the scoped slot ships the source +
      target paths and the chosen zone so the plugin can mutate its model.
      Per-row overrides via `tnode.draggable` / `tnode.drop_target`.
    · menu_items + on_context_menu — per-row right-click menu. `menu_items`
      on the tree applies globally; per-row `tnode.menu_items` wins when set.
      Each item carries its own `action` / `dispatch`; the optional
      `on_context_menu` slot is the fallback handler for items without one.

  Rendering is driven by a flat list of the currently-visible rows so that
  virtualization and roving-focus keyboard nav share one model.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { untrack } from 'svelte';
  import { ChevronDown, Check, Loader2, Search, X, ArrowUp, ArrowDown } from 'lucide-svelte';
  import PluginIcon from '$lib/components/plugins/PluginIcon.svelte';
  import TypePill   from '$lib/components/shared/internal/TypePill.svelte';
  import ContextMenu, { type MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import { PLUGIN_ICONS } from '$lib/utils/plugin-icons';
  import { tooltip } from '$lib/actions/tooltip';
  import type { FormNode, FormTreeNode } from '$lib/types/plugin';
  import type { FormNodeCtx } from './ctx';

  interface Props {
    node: FormNode;
    ctx:  FormNodeCtx;
    /** Recursive node dispatcher — needed to render a leaf's inline `edit_node`.
     *  Absent when the tree is mounted without editable rows. */
    renderNode?: Snippet<[FormNode]>;
  }
  let { node, ctx, renderNode }: Props = $props();

  // ── Inline edit (leaf `edit_node`) ───────────────────────────────────────
  // One row editable at a time; entering edit on an editable leaf swaps its
  // value cell for the editor. The editor's own dispatch fires the mutation;
  // we only own the read ⇄ edit toggle. Escape / the × button / blur exits.
  let editingKey = $state<string | null>(null);
  function canEditRow(t: any): boolean {
    return !!renderNode && !!t.edit_node && !t.group;
  }
  // Click / Tab away from the inline editor commits the read⇄edit toggle back
  // to read mode. We defer to the next frame and re-check `activeElement` so
  // focus moving *between* sub-inputs (a vec_field's x/y/z lanes) doesn't
  // collapse the editor mid-edit — only focus genuinely leaving the editor
  // subtree closes it.
  function onEditorFocusOut(e: FocusEvent) {
    const wrap = e.currentTarget as HTMLElement;
    requestAnimationFrame(() => {
      if (!wrap.isConnected) return;
      if (wrap.contains(document.activeElement)) return;
      editingKey = null;
    });
  }
  // `focusout` alone isn't enough: clicking a NON-focusable area (the tree
  // background, a plain group row) doesn't blur the editor's `<input>` /
  // native `<select>`, so the editor would stay open. While a row is being
  // edited we also watch for a pointer-down anywhere outside the editing row
  // and commit back to read mode. Inline editors render native controls (no
  // portals), so the editing row's own subtree is the authoritative bound.
  $effect(() => {
    if (editingKey === null) return;
    const onDown = (e: MouseEvent) => {
      const t = e.target as HTMLElement | null;
      if (t && t.closest('.pf-tree-row-editing')) return;   // inside the open editor row
      editingKey = null;
    };
    window.addEventListener('mousedown', onDown, true);
    return () => window.removeEventListener('mousedown', onDown, true);
  });

  const n     = $derived(node as any);
  const field = $derived(n.name as string);
  const multi = $derived(!!n.multi);
  const lazy  = $derived(!!n.lazy);

  const rowH      = $derived(typeof n.row_height === 'number' ? n.row_height : 24);
  const threshold = $derived(typeof n.virtualize_threshold === 'number' ? n.virtualize_threshold : 400);

  const searchable = $derived(!!n.searchable);
  const reorderable = $derived(!!n.reorderable);

  // ── Selection helpers ────────────────────────────────────────────────────
  const sel = $derived(
    multi
      ? (Array.isArray(ctx.values[field]) ? (ctx.values[field] as string[]) : [])
      : (ctx.values[field] as string),
  );
  function isSelected(v: string): boolean {
    return multi ? (sel as string[]).includes(v) : (sel as string) === v;
  }

  function keyOf(t: FormTreeNode): string {
    return ctx.treeKey(field, t.value);
  }
  function childrenLoaded(t: any): boolean {
    return Array.isArray(t.children) && t.children.length > 0;
  }
  function expandable(t: any): boolean {
    return childrenLoaded(t) || !!t.has_children;
  }

  // ── Local filter (search-highlight, host-resolved, no plugin round-trip) ─
  let filter = $state('');
  const rawFilter = $derived(filter.trim());
  // Path-query mode: opt-in (`path_query`) AND the text starts with `$`. The
  // substring filter/dim is disabled in this mode — the query is parsed into
  // path segments matched against the label hierarchy and the user navigates
  // the hits with F3 / Shift+F3 (+ a results rail), JSONPath-style.
  const pathMode = $derived(!!(n as any).path_query && rawFilter.startsWith('$'));
  // Substring query (drives the existing filter/dim/highlight). Empty in path
  // mode so the tree stays whole while the query navigates instead of filters.
  const q = $derived(pathMode ? '' : rawFilter.toLowerCase());

  // `$.gameplay.rt_engine.Action` → ["gameplay","rt_engine","action"]. A
  // trailing dot (mid-typing) yields an empty segment we discard.
  const pathSegs = $derived.by<string[]>(() => {
    if (!pathMode) return [];
    let body = rawFilter.slice(1);
    if (body.startsWith('.')) body = body.slice(1);
    return body.split('.').map(s => s.trim().toLowerCase()).filter(s => s.length > 0);
  });

  function labelMatches(t: any, q: string): boolean {
    const lbl  = String(t.label ?? '').toLowerCase();
    const desc = String(t.description ?? '').toLowerCase();
    return lbl.includes(q) || desc.includes(q);
  }
  // Returns true if any node in the subtree (including self) matches.
  function subtreeMatches(t: any, q: string): boolean {
    if (labelMatches(t, q)) return true;
    const kids = Array.isArray(t.children) ? t.children : [];
    for (const k of kids) if (subtreeMatches(k, q)) return true;
    return false;
  }

  // Effective expanded map: persistent user state from ctx.treeExpanded, plus
  // a temporary auto-expand for every ancestor of a match while a filter is
  // active (without mutating the persistent state — searches don't leak into
  // the user's manual expansion).
  const effectiveExpanded = $derived.by(() => {
    if (!q) return ctx.treeExpanded;
    const out: Record<string, boolean> = { ...ctx.treeExpanded };
    const walk = (list: any[] | undefined): boolean => {
      if (!Array.isArray(list)) return false;
      let any = false;
      for (const t of list) {
        const self = labelMatches(t, q);
        const kids = walk(t.children);
        if (kids) out[keyOf(t)] = true;
        if (self || kids) any = true;
      }
      return any;
    };
    walk(n.nodes);
    return out;
  });

  // ── Flatten the currently-visible (expanded) tree into a row list ─────────
  interface Row {
    tnode:      FormTreeNode;
    depth:      number;
    key:        string;
    path:       string[];      // ancestor values incl. self
    expandable: boolean;
    expanded:   boolean;
    loading:    boolean;       // synthetic spinner placeholder row?
    matched:    boolean;       // self matches the active filter (for highlight + visibility)
    descendantMatch: boolean;  // a descendant matches (kept visible to show ancestry)
  }

  const rows = $derived.by<Row[]>(() => {
    const out: Row[] = [];
    const walk = (list: FormTreeNode[] | undefined, depth: number, parent: string[]) => {
      if (!Array.isArray(list)) return;
      for (const t of list) {
        const path = [...parent, t.value];
        const exp  = expandable(t);
        const open = exp && !!effectiveExpanded[keyOf(t)];
        // Visibility filter: when a search is active, hide rows that neither
        // match nor have a matching descendant. Ancestors of matches stay
        // visible so the user can see the path; siblings without matches are
        // pruned to keep the result list tight.
        const selfMatch = q ? labelMatches(t, q) : false;
        const descMatch = q ? subtreeMatches(t, q) && !selfMatch : false;
        if (q && !selfMatch && !descMatch) continue;
        out.push({
          tnode: t, depth, key: keyOf(t), path,
          expandable: exp, expanded: open, loading: false,
          matched: selfMatch, descendantMatch: descMatch,
        });
        if (open) {
          if (childrenLoaded(t)) {
            walk(t.children, depth + 1, path);
          } else if ((t as any).loading || (lazy && (t as any).has_children)) {
            out.push({
              tnode: t, depth: depth + 1, key: keyOf(t) + '::__loading',
              path, expandable: false, expanded: false, loading: true,
              matched: false, descendantMatch: false,
            });
          }
        }
      }
    };
    walk(n.nodes, 0, []);
    return out;
  });

  // Honour `expanded = true` ("expand the whole tree on open"): seed every
  // expandable row into the persistent expansion map once on mount. Callers
  // that want runtime expand-all / collapse-all re-mount the tree with a fresh
  // `name` (→ fresh keys) so this re-runs against a clean slate. We only act on
  // an explicit `true` so plugins that never set `expanded` keep their own
  // persistent expansion behaviour untouched.
  let lastSeedName: string | undefined = undefined;
  $effect(() => {
    const nm = String(n.name ?? '');
    if (nm === lastSeedName) return;          // only when the field name changes
    lastSeedName = nm;
    if (n.expanded !== true) return;          // collapse path relies on fresh keys
    untrack(() => {
      const walk = (list: any[] | undefined) => {
        for (const t of list ?? []) {
          if (expandable(t)) ctx.treeExpanded[keyOf(t)] = true;
          walk((t as any).children);
        }
      };
      walk(n.nodes);
    });
  });

  // Don't re-fire on_expand for a row whose children are already in flight.
  const firedExpand = new Set<string>();

  function fireExpand(t: any, path: string[]) {
    const key = keyOf(t);
    if (firedExpand.has(key) || !n.on_expand) return;
    firedExpand.add(key);
    ctx.handleScopedDispatch(
      n.id, 'expand', n.on_expand,
      { id: t.id ?? t.value, value: t.value, path },
      { stateKeys: n.scope_state },
    );
  }

  function toggle(row: Row) {
    const t = row.tnode as any;
    const key = keyOf(t);
    const next = !ctx.treeExpanded[key];
    ctx.treeExpanded[key] = next;
    if (next && lazy && t.has_children && !childrenLoaded(t)) fireExpand(t, row.path);
  }

  function activate(row: Row) {
    const t = row.tnode as any;
    if (t.group) {                       // group header → toggle instead of select
      if (row.expandable) toggle(row);
      return;
    }
    // Editable leaf → enter inline edit instead of plain selection.
    if (canEditRow(t)) {
      editingKey = row.key;
      ctx.values[field] = t.value;       // keep selection in sync with the edited row
      return;
    }
    if (multi) {
      const arr = sel as string[];
      ctx.values[field] = arr.includes(t.value)
        ? arr.filter(v => v !== t.value)
        : [...arr, t.value];
    } else {
      ctx.values[field] = t.value;
    }
    ctx.notifyChange(field, ctx.values[field]);
    // Scoped on_select wins; otherwise the legacy whole-form change_action
    // (single-select only — the multi selection shape differs).
    if (n.on_select) {
      ctx.handleScopedDispatch(n.id, 'select', n.on_select, ctx.values[field], { stateKeys: n.scope_state });
    } else if (!multi && n.change_action) {
      ctx.handleButtonAction(n.change_action, false, { value: t.value });
    }
  }

  // ── Path query: hits + F3 navigation + results rail ──────────────────────
  interface PathHit { key: string; labels: string[]; path: string[]; }
  function segPrefix(label: string, seg: string): boolean {
    return label.toLowerCase().startsWith(seg);
  }
  // Are the prefix segments an ordered subsequence (prefix-match) of the
  // ancestor labels? This makes the path forgiving: `$.rt_engine.Action`
  // matches even though `rt_engine` sits under a `Gameplay` category.
  function ancestorsCover(prefixSegs: string[], ancestorLabels: string[]): boolean {
    let i = 0;
    for (const lbl of ancestorLabels) {
      if (i < prefixSegs.length && segPrefix(lbl, prefixSegs[i])) i++;
    }
    return i === prefixSegs.length;
  }
  const pathHits = $derived.by<PathHit[]>(() => {
    if (!pathMode || pathSegs.length === 0) return [];
    const last   = pathSegs[pathSegs.length - 1];
    const prefix = pathSegs.slice(0, -1);
    const out: PathHit[] = [];
    const walk = (list: any[] | undefined, vals: string[], labels: string[]) => {
      if (!Array.isArray(list)) return;
      for (const t of list) {
        const lbl = String(t.label ?? '');
        const nv  = [...vals, t.value];
        const nl  = [...labels, lbl];
        if (segPrefix(lbl, last) && ancestorsCover(prefix, labels)) {
          out.push({ key: keyOf(t), labels: nl, path: nv });
        }
        walk((t as any).children, nv, nl);
      }
    };
    walk(n.nodes, [], []);
    return out;
  });
  const showRail = $derived(pathMode && pathHits.length > 0);

  let currentHit    = $state(0);
  let currentHitKey = $state<string | null>(null);
  // Reset the cursor whenever a fresh hit set arrives.
  $effect(() => { void pathHits.length; untrack(() => { currentHit = 0; }); });

  function revealHit(hit: PathHit | undefined) {
    if (!hit) return;
    // Expand every ancestor so the target row is materialised + visible.
    for (let d = 0; d < hit.path.length - 1; d++) {
      ctx.treeExpanded[ctx.treeKey(field, hit.path[d])] = true;
    }
    currentHitKey = hit.key;
    // Rows recompute after the expansion mutation flushes — scroll next frame.
    requestAnimationFrame(() => {
      const idx = rows.findIndex(r => r.key === hit.key);
      if (idx >= 0) { activeIdx = idx; ensureVisible(idx); }
    });
  }
  function navigateHit(delta: number) {
    const nh = pathHits.length;
    if (nh === 0) return;
    currentHit = ((currentHit + delta) % nh + nh) % nh;
    revealHit(pathHits[currentHit]);
  }
  function jumpToHit(i: number) {
    if (i < 0 || i >= pathHits.length) return;
    currentHit = i;
    revealHit(pathHits[i]);
  }
  // F3 / Shift+F3 navigate hits even when focus is outside the tree (in the
  // query input or elsewhere in the modal). Gated on path mode so we only
  // hijack the key when a query is actually running.
  $effect(() => {
    if (!pathMode || pathHits.length === 0) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'F3') { e.preventDefault(); navigateHit(e.shiftKey ? -1 : 1); }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  // ── Virtualization (fixed-height rows, windowed like VirtualHunk) ─────────
  let scrollEl  = $state<HTMLDivElement | null>(null);
  let scrollTop = $state(0);
  let viewportH = $state(0);
  const overscan = 8;

  const virtual = $derived(rows.length > threshold);

  $effect(() => {
    const el = scrollEl;
    if (!el || !virtual) return;
    const ro = new ResizeObserver((entries) => {
      for (const e of entries) viewportH = e.contentRect.height;
    });
    ro.observe(el);
    return () => ro.disconnect();
  });

  const startIdx = $derived(virtual ? Math.max(0, Math.floor(scrollTop / rowH) - overscan) : 0);
  const endIdx   = $derived(virtual ? Math.min(rows.length, Math.ceil((scrollTop + viewportH) / rowH) + overscan) : rows.length);
  const topPad   = $derived(virtual ? startIdx * rowH : 0);
  const botPad   = $derived(virtual ? Math.max(0, (rows.length - endIdx) * rowH) : 0);
  const visible  = $derived(rows.slice(startIdx, endIdx));

  // Optional scoped range hint for plugins that fetch by window.
  function onScroll(e: Event) {
    if (virtual) scrollTop = (e.currentTarget as HTMLDivElement).scrollTop;
    if (n.on_scroll_range) {
      ctx.handleScopedDispatch(n.id, 'scroll_range', n.on_scroll_range,
        { start: startIdx, end: endIdx, total: rows.length }, { stateKeys: n.scope_state });
    }
  }

  // ── Keyboard navigation (roving) ─────────────────────────────────────────
  let activeIdx = $state(0);

  // Keep the active index in range as the row list changes.
  $effect(() => {
    if (activeIdx > rows.length - 1) activeIdx = Math.max(0, rows.length - 1);
  });

  function ensureVisible(i: number) {
    const el = scrollEl;
    if (!el) return;
    const top = i * rowH;
    const bot = top + rowH;
    if (top < el.scrollTop) el.scrollTop = top;
    else if (bot > el.scrollTop + el.clientHeight) el.scrollTop = bot - el.clientHeight;
  }
  function move(to: number) {
    activeIdx = Math.max(0, Math.min(rows.length - 1, to));
    ensureVisible(activeIdx);
  }
  function parentIndex(i: number): number {
    const d = rows[i]?.depth ?? 0;
    for (let j = i - 1; j >= 0; j--) if (rows[j].depth < d) return j;
    return i;
  }

  function onKeydown(e: KeyboardEvent) {
    const row = rows[activeIdx];
    switch (e.key) {
      case 'ArrowDown': e.preventDefault(); move(activeIdx + 1); break;
      case 'ArrowUp':   e.preventDefault(); move(activeIdx - 1); break;
      case 'Home':      e.preventDefault(); move(0); break;
      case 'End':       e.preventDefault(); move(rows.length - 1); break;
      case 'ArrowRight':
        e.preventDefault();
        if (row?.expandable && !row.expanded) toggle(row);
        else move(activeIdx + 1);
        break;
      case 'ArrowLeft':
        e.preventDefault();
        if (row?.expandable && row.expanded) toggle(row);
        else move(parentIndex(activeIdx));
        break;
      case 'Enter':
      case ' ':
        // When a row button has focus its native click already activates it;
        // only the container itself (e.g. just Tab-focused) activates via key,
        // so Enter never double-fires (which would no-op a multi toggle).
        if (document.activeElement === scrollEl && row && !row.loading) {
          e.preventDefault();
          activate(row);
        }
        break;
    }
  }

  // ── Drag-drop reorder ────────────────────────────────────────────────────
  // The plugin owns the data model; the host only computes the drop intent
  // and fires `on_reorder` with `{ source, target, position }`. `position`
  // resolves the cursor's vertical zone over the target row.
  let dragKey   = $state<string | null>(null);
  let dragPath  = $state<string[] | null>(null);
  let dragValue = $state<string | null>(null);
  let dragId    = $state<string | null>(null);
  let dropKey   = $state<string | null>(null);
  let dropZone  = $state<'before' | 'inside' | 'after' | null>(null);

  function rowDraggable(t: any): boolean {
    if (typeof t.draggable === 'boolean') return t.draggable;
    if (!reorderable) return false;
    return !t.group;            // groups (expansion headers) are not movable by default
  }
  function rowDroppable(t: any): boolean {
    if (typeof t.drop_target === 'boolean') return t.drop_target;
    return reorderable;
  }

  function onDragStart(row: Row, e: DragEvent) {
    const t = row.tnode as any;
    if (!rowDraggable(t)) { e.preventDefault(); return; }
    dragKey   = row.key;
    dragPath  = row.path;
    dragValue = String(t.value);
    dragId    = t.id ?? null;
    try {
      e.dataTransfer?.setData('text/plain', row.key);
      if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move';
    } catch { /* noop */ }
  }
  function onDragEnd() {
    dragKey = null; dragPath = null; dragValue = null; dragId = null;
    dropKey = null; dropZone = null;
  }
  function isDescendantOfDrag(row: Row): boolean {
    if (!dragPath) return false;
    if (row.path.length <= dragPath.length) return false;
    for (let i = 0; i < dragPath.length; i++) {
      if (row.path[i] !== dragPath[i]) return false;
    }
    return true;
  }
  function zoneFor(row: Row, e: DragEvent): 'before' | 'inside' | 'after' {
    const t = row.tnode as any;
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const rel  = (e.clientY - rect.top) / Math.max(1, rect.height);
    // Group rows + expandable parents accept "inside" in the middle third.
    const canHostInside = !!t.group || row.expandable;
    if (canHostInside) {
      if (rel < 1 / 3) return 'before';
      if (rel > 2 / 3) return 'after';
      return 'inside';
    }
    return rel < 0.5 ? 'before' : 'after';
  }
  function onDragOver(row: Row, e: DragEvent) {
    if (!dragKey) return;
    if (row.loading) return;
    const t = row.tnode as any;
    if (!rowDroppable(t)) return;
    if (row.key === dragKey) return;            // no self-drop
    if (isDescendantOfDrag(row)) return;        // no drop into own subtree
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = 'move';
    const z = zoneFor(row, e);
    if (dropKey !== row.key) dropKey = row.key;
    if (dropZone !== z)      dropZone = z;
  }
  function onDragLeave(row: Row) {
    if (dropKey === row.key) { dropKey = null; dropZone = null; }
  }
  function onDrop(row: Row, e: DragEvent) {
    if (!dragKey || !dragPath || !dragValue) return;
    if (row.loading) return;
    const t = row.tnode as any;
    if (!rowDroppable(t)) return;
    if (row.key === dragKey || isDescendantOfDrag(row)) return;
    e.preventDefault();
    e.stopPropagation();
    const z = zoneFor(row, e);
    const sourcePath = dragPath;
    const sourceValue = dragValue;
    const sourceId = dragId;
    onDragEnd();
    if (!n.on_reorder) return;
    ctx.handleScopedDispatch(
      n.id, 'reorder', n.on_reorder,
      {
        source: { id: sourceId ?? undefined, value: sourceValue, path: sourcePath },
        target: { id: t.id ?? undefined, value: t.value, path: row.path },
        position: z,
      },
      { stateKeys: n.scope_state },
    );
  }

  // ── Context menu ─────────────────────────────────────────────────────────
  let menuOpen = $state(false);
  let menuX    = $state(0);
  let menuY    = $state(0);
  let menuRow  = $state<Row | null>(null);
  let menuItems = $state<MenuItem[]>([]);
  // Resolve picked id → the originating tree menu item (kept off the MenuItem
  // shape so we don't leak host-only fields into the tree node payload).
  let menuMeta = $state<Record<string, any>>({});

  function rowMenuItems(t: any): any[] {
    if (Array.isArray(t.menu_items)) return t.menu_items;
    if (Array.isArray(n.menu_items)) return n.menu_items;
    return [];
  }
  function hasContextMenu(t: any): boolean {
    return rowMenuItems(t).length > 0;
  }

  function buildMenu(row: Row): { items: MenuItem[]; meta: Record<string, any> } {
    const t = row.tnode as any;
    const raw = rowMenuItems(t);
    const items: MenuItem[] = [];
    const meta: Record<string, any> = {};
    raw.forEach((it: any, i: number) => {
      const id = String(it.id ?? `__item_${i}`);
      const Icon = it.icon ? (PLUGIN_ICONS as any)[it.icon] : undefined;
      if (it.separator) {
        items.push({ id: `__sep_${i}`, label: '', separator: true });
        return;
      }
      if (it.header) {
        items.push({ id: `__hdr_${i}`, label: String(it.label ?? ''), header: true });
        return;
      }
      items.push({
        id,
        label: String(it.label ?? id),
        icon: Icon,
        danger: !!it.danger,
        disabled: !!it.disabled,
      });
      meta[id] = it;
    });
    return { items, meta };
  }

  function onContextMenu(row: Row, e: MouseEvent) {
    const t = row.tnode as any;
    if (!hasContextMenu(t)) return;
    e.preventDefault();
    e.stopPropagation();
    const built = buildMenu(row);
    if (built.items.length === 0) return;
    menuItems = built.items;
    menuMeta  = built.meta;
    menuRow   = row;
    menuX     = e.clientX;
    menuY     = e.clientY;
    menuOpen  = true;
  }

  function handleMenuSelect(id: string) {
    const item = menuMeta[id];
    const row = menuRow;
    menuOpen = false;
    menuRow = null;
    if (!item || !row) return;
    const t = row.tnode as any;
    const payload = {
      item_id: item.id ?? id,
      value: t.value,
      path: row.path,
    };
    // Per-item dispatch wins; falls back to per-item action; finally to the
    // tree-level `on_context_menu` slot with the same payload.
    if (item.dispatch) {
      ctx.handleScopedDispatch(n.id, 'context_menu', item.dispatch, payload, { stateKeys: n.scope_state });
    } else if (typeof item.action === 'string') {
      ctx.firePluginAction(ctx.pluginName, item.action, JSON.stringify(payload));
    } else if (n.on_context_menu) {
      ctx.handleScopedDispatch(n.id, 'context_menu', n.on_context_menu, payload, { stateKeys: n.scope_state });
    }
  }

  // ── Match highlight — split label/desc around the active query ───────────
  interface HlPart { text: string; match: boolean }
  function hlSplit(text: string, q: string): HlPart[] {
    if (!q) return [{ text, match: false }];
    const out: HlPart[] = [];
    const lower = text.toLowerCase();
    let i = 0;
    while (i < text.length) {
      const next = lower.indexOf(q, i);
      if (next === -1) { out.push({ text: text.slice(i), match: false }); break; }
      if (next > i) out.push({ text: text.slice(i, next), match: false });
      out.push({ text: text.slice(next, next + q.length), match: true });
      i = next + q.length;
    }
    return out;
  }

  const treeId = $derived(n.id as string);
</script>

<div
  class="pf-field {n.class ?? ''}"
  class:pf-field-highlight={n.highlight}
  class:pf-field-fill={n.fill}
  style={n.style}
>
  {#if n.label}
    <!-- svelte-ignore a11y_label_has_associated_control -->
    <label class="pf-label">
      {n.label}
      {#if n.required}<span class="pf-required" aria-hidden="true">*</span>{/if}
    </label>
  {/if}

  {#if searchable}
    <div class="pf-tree-search" class:pf-tree-search-path={pathMode}>
      <Search size={11} class="pf-tree-search-icon" />
      <input
        type="text"
        class="pf-tree-search-input"
        placeholder={n.search_placeholder ?? 'Filter…'}
        bind:value={filter}
        onkeydown={(e) => {
          if (!pathMode) return;
          if (e.key === 'F3')        { e.preventDefault(); navigateHit(e.shiftKey ? -1 : 1); }
          else if (e.key === 'Enter') { e.preventDefault(); navigateHit(1); }
          else if (e.key === 'ArrowDown' && pathHits.length) { e.preventDefault(); navigateHit(1); }
          else if (e.key === 'ArrowUp'   && pathHits.length) { e.preventDefault(); navigateHit(-1); }
        }}
      />
      {#if pathMode && pathSegs.length > 0}
        <span class="pf-tree-hits" class:pf-tree-hits-empty={pathHits.length === 0}>
          {#if pathHits.length}{currentHit + 1}/{pathHits.length}{:else}0{/if}
        </span>
        <button type="button" class="pf-tree-hit-nav" disabled={!pathHits.length}
          onclick={() => navigateHit(-1)}
          use:tooltip={{ content: 'Previous match', shortcut: 'Shift+F3' }} aria-label="Previous match"
        ><ArrowUp size={11} /></button>
        <button type="button" class="pf-tree-hit-nav" disabled={!pathHits.length}
          onclick={() => navigateHit(1)}
          use:tooltip={{ content: 'Next match', shortcut: 'F3' }} aria-label="Next match"
        ><ArrowDown size={11} /></button>
      {/if}
      {#if filter}
        <button
          type="button"
          class="pf-tree-search-clear"
          onclick={() => filter = ''}
          use:tooltip={'Clear filter'}
          aria-label="Clear filter"
        >×</button>
      {/if}
    </div>
  {/if}

  <div
    class="pf-tree-body-row"
    class:pf-tree-body-row-split={showRail}
    class:pf-tree-body-row-fill={n.fill}
  >
  <div
    class="pf-tree pf-tree-dyn"
    class:pf-tree-bordered={n.bordered}
    class:pf-tree-fill={n.fill}
    style={n.fill ? '' : (n.max_height ? `max-height:${n.max_height}` : (n.height ? `max-height:${typeof n.height === 'number' ? n.height + 'px' : n.height}` : ''))}
    role="tree"
    tabindex="0"
    aria-multiselectable={multi}
    aria-activedescendant={rows[activeIdx] ? `${treeId}__${rows[activeIdx].key}` : undefined}
    bind:this={scrollEl}
    onscroll={onScroll}
    onkeydown={onKeydown}
  >
    {#if topPad}<div style="height:{topPad}px"></div>{/if}
    {#each visible as row, i (row.key)}
      {@const idx = startIdx + i}
      {@const t = row.tnode as any}
      {#if row.loading}
        <div class="pf-tree-row pf-tree-loading" style="padding-left:{row.depth * 14 + 4}px; height:{rowH}px">
          <Loader2 size={11} class="pf-tree-spin" />
          <span class="pf-tree-loading-text">Loading…</span>
        </div>
      {:else}
        {@const selected = isSelected(t.value)}
        {@const isDraggableRow = rowDraggable(t)}
        {@const isDropTarget = dropKey === row.key}
        {@const editing = canEditRow(t) && editingKey === row.key}
        <div
          id="{treeId}__{row.key}"
          class="pf-tree-row"
          class:pf-tree-row-editing={editing}
          class:pf-tree-row-active={idx === activeIdx}
          class:pf-tree-row-hit={currentHitKey === row.key}
          class:pf-tree-row-dim={!!q && !row.matched}
          class:pf-tree-drop-before={isDropTarget && dropZone === 'before'}
          class:pf-tree-drop-after={isDropTarget && dropZone === 'after'}
          class:pf-tree-drop-inside={isDropTarget && dropZone === 'inside'}
          style="padding-left:{row.depth * 14 + 4}px; height:{rowH}px"
          role="treeitem"
          tabindex={idx === activeIdx ? 0 : -1}
          aria-level={row.depth + 1}
          aria-expanded={row.expandable ? row.expanded : undefined}
          aria-selected={selected}
          draggable={isDraggableRow}
          ondragstart={(e) => onDragStart(row, e)}
          ondragend={onDragEnd}
          ondragover={(e) => onDragOver(row, e)}
          ondragleave={() => onDragLeave(row)}
          ondrop={(e) => onDrop(row, e)}
          oncontextmenu={(e) => onContextMenu(row, e)}
        >
          {#if row.expandable}
            <button
              class="pf-tree-chev"
              type="button"
              tabindex="-1"
              aria-label={row.expanded ? 'Collapse' : 'Expand'}
              onclick={() => { activeIdx = idx; toggle(row); }}
            ><ChevronDown size={10} class={row.expanded ? '' : 'pf-chev-collapsed'} /></button>
          {:else}
            <span class="pf-tree-chev-spacer"></span>
          {/if}
          {#if t.icon}
            <span class="pf-tree-icon" style={t.icon_color ? `color:${t.icon_color}` : ''}><PluginIcon name={t.icon} size={11} /></span>
          {/if}
          {#if editing && t.edit_node && renderNode}
            <!-- Inline editor — delegated to the normal node dispatcher so all
                 existing editors (number / text / select / vec / color) work
                 verbatim. The editor's own dispatch fires the mutation; this
                 row just hosts it. -->
            <span class="pf-tree-label-text pf-tree-editlabel"><span>{t.label}</span></span>
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div
              class="pf-tree-editor"
              onfocusout={onEditorFocusOut}
              onkeydown={(e) => { if (e.key === 'Escape') { e.stopPropagation(); editingKey = null; } }}
            >
              {@render renderNode(t.edit_node as FormNode)}
            </div>
            <button
              class="pf-tree-editclose"
              type="button"
              tabindex="-1"
              use:tooltip={'Done'}
              aria-label="Done editing"
              onclick={(e) => { e.stopPropagation(); editingKey = null; }}
            ><X size={11} /></button>
          {:else}
            <button
              class="pf-tree-label"
              class:pf-tree-label-group={t.group}
              class:pf-tree-label-selected={selected}
              type="button"
              tabindex="-1"
              disabled={t.group && !row.expandable}
              onclick={() => { activeIdx = idx; activate(row); }}
            >
              {#if multi && !t.group}
                <span class="pf-tree-cb" class:checked={selected}>
                  {#if selected}<Check size={9} />{/if}
                </span>
              {/if}
              <span class="pf-tree-label-text">
                <span>
                  {#if q && row.matched}
                    {#each hlSplit(String(t.label ?? ''), q) as part}
                      {#if part.match}<mark class="pf-tree-mark">{part.text}</mark>{:else}{part.text}{/if}
                    {/each}
                  {:else}
                    {t.label}
                  {/if}
                </span>
                {#if t.description}
                  <span class="pf-tree-desc">
                    {#if q && row.matched}
                      {#each hlSplit(String(t.description ?? ''), q) as part}
                        {#if part.match}<mark class="pf-tree-mark">{part.text}</mark>{:else}{part.text}{/if}
                      {/each}
                    {:else}
                      {t.description}
                    {/if}
                  </span>
                {/if}
              </span>
              {#if t.loading}
                <Loader2 size={10} class="pf-tree-spin" />
              {/if}
              {#if t.value_display !== undefined && t.value_display !== null && t.value_display !== ''}
                <span class="pf-tree-val pf-tree-tone-{t.value_tone ?? 'default'}" use:tooltip={canEditRow(t) ? 'Click to edit' : ''}>{t.value_display}</span>
              {/if}
              {#if t.pill}
                <span class="pf-tree-pill"><TypePill label={t.pill} kind={t.pill_kind ?? t.pill} tone={t.pill_tone} /></span>
              {/if}
              {#if t.tag}
                <span class="pf-cfg-tag pf-cfg-tag-{t.tag_variant ?? 'neutral'} pf-tree-tag">{t.tag}</span>
              {/if}
            </button>
          {/if}
        </div>
      {/if}
    {/each}
    {#if botPad}<div style="height:{botPad}px"></div>{/if}

    {#if q && rows.length === 0}
      <div class="pf-tree-empty">No matches for <em>{filter}</em></div>
    {:else if pathMode && pathSegs.length > 0 && pathHits.length === 0}
      <div class="pf-tree-empty">No path matches for <em>{filter}</em></div>
    {/if}
  </div>

  {#if showRail}
    <aside class="pf-tree-results" aria-label="Query results">
      <div class="pf-tree-results-head">{pathHits.length} match{pathHits.length === 1 ? '' : 'es'}</div>
      <div class="pf-tree-results-list">
        {#each pathHits as hit, i (hit.key)}
          <button
            type="button"
            class="pf-tree-result"
            class:active={i === currentHit}
            onclick={() => jumpToHit(i)}
            title={hit.labels.join(' › ')}
          >
            <span class="pf-tree-result-path">
              {#each hit.labels as lbl, j}
                {#if j > 0}<span class="pf-tree-result-sep">›</span>{/if}<span
                  class="pf-tree-result-seg"
                  class:leaf={j === hit.labels.length - 1}>{lbl}</span>
              {/each}
            </span>
          </button>
        {/each}
      </div>
    </aside>
  {/if}
  </div>

  {#if ctx.validationErrors[field]}
    <span class="pf-validation-error">{ctx.validationErrors[field]}</span>
  {/if}
  {#if n.hint}
    <span class="pf-hint">{n.hint}</span>
  {/if}
  {#if n.pill}
    <TypePill label={n.pill} kind={n.pill_kind ?? n.pill} tooltip={n.pill_tooltip} />
  {/if}
</div>

{#if menuOpen}
  <ContextMenu
    x={menuX}
    y={menuY}
    items={menuItems}
    onSelect={handleMenuSelect}
    onClose={() => { menuOpen = false; menuRow = null; }}
  />
{/if}

<style>
  /* Dynamic tree adds a scroll viewport + fixed-height rows on top of the
     shared pf-tree-* visuals (form-node-styles.css). */
  .pf-tree-dyn {
    overflow: auto;
    outline: none;
    position: relative;
  }
  /* `fill` — the tree grows to fill its parent flex column and owns the only
     scroll region (the field wrapper becomes a flex column too). Lets a flush
     modal body host a single, full-height tree without a second scrollbar. */
  .pf-field-fill {
    flex: 1 1 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .pf-tree-fill {
    flex: 1 1 0;
    min-height: 0;
    min-width: 0;
  }

  /* Body row wraps the tree + the (optional) query results rail. Transparent
     to layout (`display: contents`) until a path query produces hits, when it
     becomes a flex row so the rail sits to the right of the tree. */
  .pf-tree-body-row { display: contents; }
  .pf-tree-body-row-split {
    display: flex;
    flex-direction: row;
    gap: 6px;
    min-height: 0;
  }
  .pf-tree-body-row-split.pf-tree-body-row-fill { flex: 1 1 0; min-width: 0; }

  /* ── Query results rail (path-query mode) ─────────────────────────────── */
  .pf-tree-results {
    flex: 0 0 clamp(220px, 28%, 340px);
    min-height: 0;
    display: flex;
    flex-direction: column;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm, 4px);
    background: var(--bg-base);
    overflow: hidden;
  }
  .pf-tree-results-head {
    flex-shrink: 0;
    padding: 5px 8px;
    font-size: var(--font-size-2xs);
    font-weight: 600;
    letter-spacing: 0.3px;
    text-transform: uppercase;
    color: var(--text-secondary);
    border-bottom: 1px solid var(--border-subtle);
  }
  .pf-tree-results-list { flex: 1 1 0; min-height: 0; overflow-y: auto; padding: 3px; }
  .pf-tree-result {
    display: flex;
    width: 100%;
    text-align: left;
    align-items: center;
    padding: 3px 6px;
    border: none;
    background: transparent;
    border-radius: 3px;
    cursor: pointer;
    font-family: var(--font-mono);
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
  }
  .pf-tree-result:hover { background: var(--bg-hover); }
  .pf-tree-result.active { background: color-mix(in srgb, var(--accent) 16%, transparent); }
  .pf-tree-result-path { min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .pf-tree-result-sep { color: var(--text-disabled); margin: 0 3px; }
  .pf-tree-result-seg.leaf { color: var(--text-primary); font-weight: 600; }
  .pf-tree-result.active .pf-tree-result-seg.leaf { color: var(--accent); }

  /* Current path hit in the tree itself. */
  .pf-tree-row-hit {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    border-radius: var(--radius-sm, 4px);
    box-shadow: inset 2px 0 0 0 var(--accent);
  }

  /* Path-query hit counter + prev/next nav inside the search row. */
  .pf-tree-search-path { border-color: color-mix(in srgb, var(--accent) 45%, var(--border)); }
  .pf-tree-hits {
    flex-shrink: 0;
    font-family: var(--font-mono);
    font-size: var(--font-size-2xs);
    color: var(--accent);
    padding: 0 2px;
  }
  .pf-tree-hits-empty { color: var(--text-disabled); }
  .pf-tree-hit-nav {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    width: 18px;
    height: 18px;
    border: none;
    background: transparent;
    color: var(--text-muted);
    border-radius: var(--radius-sm, 4px);
    cursor: pointer;
  }
  .pf-tree-hit-nav:hover:not(:disabled) { color: var(--text-primary); background: var(--bg-hover); }
  .pf-tree-hit-nav:disabled { opacity: 0.4; cursor: default; }
  .pf-tree-dyn:focus-visible {
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 45%, transparent);
    border-radius: var(--radius-sm, 4px);
  }
  .pf-tree-row-active {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    border-radius: var(--radius-sm, 4px);
  }
  /* Non-matching ancestors stay visible (to show the path to matches) but
     dim, so the matched leaves still pop. */
  .pf-tree-row-dim { opacity: 0.55; }
  .pf-tree-row.pf-tree-loading {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--text-muted);
    font-size: var(--font-size-xs);
  }
  .pf-tree-loading-text {
    font-style: italic;
  }
  :global(.pf-tree-spin) {
    animation: pf-tree-spin 1s linear infinite;
    color: var(--accent);
    flex-shrink: 0;
  }
  @keyframes pf-tree-spin {
    to { transform: rotate(360deg); }
  }

  /* ── Leaf value (source-tree "key: value" look) ───────────────────────── */
  .pf-tree-val {
    margin-left: auto;
    font-family: var(--font-mono);
    font-size: var(--font-size-xs);
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 55%;
    flex-shrink: 1;
  }
  /* +tag after value → value no longer pushes fully right. */
  .pf-tree-val + .pf-tree-tag { margin-left: 6px; }
  .pf-tree-pill { flex-shrink: 0; margin-left: 8px; display: inline-flex; }
  /* When there's no value_display, the pill still sits to the right. */
  .pf-tree-label > .pf-tree-pill { margin-left: auto; }
  .pf-tree-val + .pf-tree-pill { margin-left: 8px; }
  .pf-tree-tone-default { color: var(--text-primary); }
  .pf-tree-tone-number  { color: #62a0ea; }
  .pf-tree-tone-string  { color: #e0a458; }
  .pf-tree-tone-enum    { color: #74c69d; }
  .pf-tree-tone-bool    { color: #b288f0; }
  .pf-tree-tone-entity  { color: #f08c54; }
  .pf-tree-tone-handle  { color: #c98fe5; }
  .pf-tree-tone-accent  { color: var(--accent); }
  .pf-tree-tone-warn    { color: var(--warning); }
  .pf-tree-tone-muted   { color: var(--text-disabled); }

  /* ── Inline editor ────────────────────────────────────────────────────── */
  .pf-tree-row-editing {
    background: color-mix(in srgb, var(--accent) 8%, transparent);
    border-radius: var(--radius-sm);
    /* The editor needs vertical room beyond the fixed row height. */
    height: auto !important;
    min-height: 26px;
    padding-top: 2px;
    padding-bottom: 2px;
  }
  .pf-tree-editlabel { flex: 0 0 auto; max-width: 40%; margin-right: 8px; }
  .pf-tree-editor {
    flex: 1 1 auto;
    min-width: 0;
    /* Don't let a single text/number editor stretch across an ultra-wide
       modal row — keep it tidy and left-aligned. */
    max-width: 460px;
  }
  /* The embedded editor brings its own `.pf-field` chrome; strip margins +
     its label so it sits flush in the row. */
  .pf-tree-editor :global(.pf-field) { margin: 0; }
  .pf-tree-editor :global(.pf-label) { display: none; }
  .pf-tree-editclose {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    flex-shrink: 0;
    margin-left: 6px;
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    background: transparent;
    border: none;
    cursor: pointer;
  }
  .pf-tree-editclose:hover { color: var(--text-primary); background: var(--bg-hover); }

  /* ── Search input ──────────────────────────────────────────────────────── */
  .pf-tree-search {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 8px;
    margin-bottom: 4px;
    background: var(--bg-base);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  :global(.pf-tree-search-icon) { color: var(--text-muted); flex-shrink: 0; }
  .pf-tree-search-input {
    flex: 1;
    background: transparent;
    border: none;
    outline: none;
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    color: var(--text-primary);
    min-width: 0;
  }
  .pf-tree-search-input::placeholder { color: var(--text-muted); }
  .pf-tree-search-clear {
    background: transparent;
    border: none;
    color: var(--text-muted);
    cursor: pointer;
    padding: 0 4px;
    font-size: var(--font-size-lg);
    line-height: 1;
    border-radius: var(--radius-sm);
  }
  .pf-tree-search-clear:hover { background: var(--bg-hover); color: var(--text-primary); }

  /* ── Match highlight ──────────────────────────────────────────────────── */
  .pf-tree-mark {
    background: color-mix(in srgb, var(--accent) 30%, transparent);
    color: inherit;
    border-radius: 2px;
    padding: 0 1px;
  }

  /* ── Drop indicators ──────────────────────────────────────────────────── */
  .pf-tree-drop-before {
    box-shadow: inset 0 2px 0 0 var(--accent);
  }
  .pf-tree-drop-after {
    box-shadow: inset 0 -2px 0 0 var(--accent);
  }
  .pf-tree-drop-inside {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    border-radius: var(--radius-sm);
  }

  /* ── Empty / no-matches ───────────────────────────────────────────────── */
  .pf-tree-empty {
    padding: 12px 8px;
    color: var(--text-muted);
    font-size: var(--font-size-sm);
    text-align: center;
  }
</style>
