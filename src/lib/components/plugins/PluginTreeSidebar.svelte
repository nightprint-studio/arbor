<script lang="ts">
  /**
   * Renders a `kind="tree"` plugin sidebar.
   *
   * The host plugin owns the tree data — it pushes nodes via
   * `arbor.ui.tree.set(sidebar_id, nodes)` and exposes contribution points
   * that *any* plugin can extend. The component reads everything from the
   * stores (no IPC of its own beyond the hook fire on open).
   *
   * Contribution-point naming convention (documented in DocsPanel):
   *
   *   `<plugin>:<sidebar_id>:toolbar`             — buttons in the header toolbar
   *   `<plugin>:<sidebar_id>:node_action`         — hover-reveal buttons per row
   *   `<plugin>:<sidebar_id>:node_decorator`      — always-on badge / icon per row
   *   `<plugin>:<sidebar_id>:context_menu`        — right-click items per row
   *   `<plugin>:<sidebar_id>:dependency_provider` — opens DependencyTreeModal
   *   `<plugin>:<sidebar_id>:footer`              — buttons / text in the footer
   *
   * Each contribution can include a `when` clause filtering by node kind /
   * a single `data` field — keeps the same payload usable across many nodes
   * without exploding into one contribution per node.
   *
   * After the Phase 4 god-object refactor the per-row renderer, the toolbar
   * contribution loop and the footer contribution loop all live under
   * `tree-sidebar/`. The dispatcher keeps the orchestration logic
   * (selection state, context menu, dependency modal, search mode) and
   * delegates rendering.
   */
  import { untrack } from 'svelte';
  import { Search, Filter, Globe } from 'lucide-svelte';
  import { contributionStore } from '$lib/stores/contribution.svelte';
  import { pluginStore } from '$lib/stores/plugin.svelte';
  import { firePluginAction }  from '$lib/ipc/plugin';
  import { whenMatches }       from '$lib/contributions/when';
  import { SIDEBAR_POINT, parseSidebarSection } from '$lib/contributions/sidebar';
  import PluginIcon      from './PluginIcon.svelte';
  import PanelShell      from '$lib/components/shared/ui/PanelShell.svelte';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import Tree            from '$lib/components/shared/ui/Tree.svelte';
  import Breadcrumb, { type BreadcrumbSegment as BCSeg } from '$lib/components/shared/ui/Breadcrumb.svelte';
  import ContextMenu, { type MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import DependencyTreeModal from '$lib/components/shared/DependencyTreeModal.svelte';
  import type { TreeNode, PluginContribution } from '$lib/types/contribution';
  import { tooltip } from '$lib/actions/tooltip';

  import PluginTreeToolbar from './tree-sidebar/PluginTreeToolbar.svelte';
  import PluginTreeNode    from './tree-sidebar/PluginTreeNode.svelte';
  import PluginTreeFooter  from './tree-sidebar/PluginTreeFooter.svelte';
  import './tree-sidebar/tree-sidebar-styles.css';

  interface Props {
    pluginName: string;
    panelId:    string;
    /**
     * When mounted as a bottom-docked panel, render a `BottomPanelHeader`
     * above the tree (and suppress the default `PanelShell` header) so the
     * close X is integrated into the same standardized chrome bar.
     */
    bottomMode?: boolean;
  }
  let { pluginName, panelId, bottomMode = false }: Props = $props();

  const ns = $derived(`${pluginName}:${panelId}`);

  // ── Section metadata (for header label / icon / tooltip) ──────────────────
  const section = $derived(
    contributionStore.forPoint(SIDEBAR_POINT)
      .filter(c => !pluginStore.disabledPlugins.has(c.plugin_name))
      .map(parseSidebarSection)
      .find(s => s.plugin_name === pluginName && s.id === panelId) ?? null
  );

  // ── Tree snapshot ─────────────────────────────────────────────────────────
  const snapshot = $derived(contributionStore.tree(pluginName, panelId));
  const title    = $derived(snapshot?.title ?? section?.label ?? '');
  const baseNodes = $derived(snapshot?.nodes ?? []);
  // Breadcrumb band: pushed alongside the nodes via `arbor.ui.tree.set`.
  // Mapped to the shared Breadcrumb widget shape. Segments with empty
  // `action` render as non-interactive (last/current).
  type BcPayload = { action?: string | null; data?: unknown };
  const breadcrumbSegments = $derived<BCSeg<BcPayload>[]>(
    (snapshot?.breadcrumb ?? []).map(s => ({
      label:       s.label,
      icon:        s.icon ?? null,
      badge:       s.badge ?? null,
      tooltip:     s.tooltip ?? null,
      interactive: !!(s.action && s.action.length > 0),
      value:       { action: s.action ?? null, data: s.data },
    }))
  );
  function handleBreadcrumbSelect(v: BcPayload | undefined) {
    if (!v?.action) return;
    firePluginAction(pluginName, v.action, JSON.stringify({ data: v.data })).catch(() => {});
  }

  // ── Breadcrumb edit-in-place ─────────────────────────────────────────────
  // Plugins opt-in by pushing `breadcrumb_edit_action` (and optionally a
  // `breadcrumb_edit_placeholder`) through `arbor.ui.tree.set`. On commit we
  // fire the action with `{ path }` in ctx; the plugin parses the path,
  // updates its state and re-pushes a fresh snapshot — same code path as
  // clicking a breadcrumb segment.
  const breadcrumbEditAction      = $derived(snapshot?.breadcrumb_edit_action      ?? null);
  const breadcrumbEditPlaceholder = $derived(snapshot?.breadcrumb_edit_placeholder ?? undefined);
  // Reconstruct the current path string from the segments — skip the first
  // (root) chip since it represents the bucket itself, not a folder. Each
  // segment's payload carries the absolute prefix in `data.prefix` for the
  // cloud-storage plugin; for other plugins we fall back to the label trail.
  const breadcrumbEditValue = $derived.by(() => {
    const segs = snapshot?.breadcrumb ?? [];
    // Prefer the data.prefix of the LAST segment if any segment exposes it.
    for (let i = segs.length - 1; i >= 0; i--) {
      const d = segs[i]?.data as { prefix?: string } | undefined;
      if (d && typeof d.prefix === 'string') return d.prefix;
    }
    // Generic fallback: glue labels with '/', skipping the root.
    return segs.slice(1).map(s => s.label).join('/');
  });
  function handleBreadcrumbCommit(path: string) {
    if (!breadcrumbEditAction) return;
    firePluginAction(pluginName, breadcrumbEditAction, JSON.stringify({ path })).catch(() => {});
  }
  // Top-level sections injected by other plugins via the `tree.section`
  // contribution point. Each contribution carries `{ section: <TreeNode> }`
  // and is appended in priority order. The host plugin's own nodes always
  // come first; contributions stack behind them.
  const sectionContribs = $derived(contributionStore.forPoint(`${ns}:tree.section`));
  const nodes = $derived([
    ...baseNodes,
    ...sectionContribs
      .map(c => (c.payload as any)?.section)
      .filter((s): s is TreeNode => s != null && typeof s === 'object'),
  ]);

  // Ensure we have a snapshot — fire `panel:open:<id>` exactly like form-kind
  // panels do. The host responds by calling `arbor.ui.tree.set`, which writes
  // into the contribution registry under `"arbor:tree-state"` and the store
  // picks it up on the coalesced `arbor://contributions-changed` event.
  $effect(() => {
    const pn = pluginName;
    const pid = panelId;
    untrack(() => {
      contributionStore.ensureTree(pn, pid);
      firePluginAction(pn, `panel:open:${pid}`, '{}').catch(() => {});
    });
  });


  // ── Selection + filter ────────────────────────────────────────────────────
  // `selectedId` drives the Tree's own visual "current row" highlight (single
  // row, kept in sync with `selectedIds`'s most recent addition). `selectedIds`
  // is the multi-select set populated via Ctrl/Cmd+click and Shift+click range.
  // Esc on the panel clears everything.
  let selectedId    = $state<string | null>(null);
  let selectedIds   = $state(new Set<string>());
  let lastClickedId = $state<string | null>(null);
  let filter        = $state('');
  let filterFocused = $state(false);

  // ── Search-row mode (local filter vs. remote backend search) ──────────────
  // When the plugin declares `search.modes`, we expose a toggle. The default
  // mode comes from the contribution; switching is purely local UI state and
  // doesn't persist across sidebar opens — keeps the model dead simple.
  const searchConfig    = $derived(section?.search);
  const searchHasRemote = $derived(!!searchConfig?.modes?.includes('remote'));
  const searchHasLocal  = $derived(!searchConfig || searchConfig.modes.includes('local'));
  // Once the user has explicitly toggled, we stop seeding from the config:
  // otherwise every `arbor.ui.tree.set` re-derives `section`, the effect
  // re-runs, and the user's choice gets stomped back to `config.default`.
  let searchMode          = $state<'local' | 'remote'>('local');
  let searchModeUserPick  = $state(false);
  $effect(() => {
    if (searchModeUserPick) return;
    const def = (searchConfig?.default ?? 'local') as 'local' | 'remote';
    if (searchMode !== def) searchMode = def;
  });
  // The filter prop passed to Tree: only honoured in local mode, otherwise
  // we pass an empty string so the tree shows everything and the backend's
  // returned rows aren't double-filtered.
  const effectiveFilter = $derived(searchMode === 'local' ? filter : '');
  const placeholder = $derived(
    searchMode === 'remote'
      ? (searchConfig?.placeholder_remote ?? 'Search… (Enter to run)')
      : (searchConfig?.placeholder_local  ?? 'Filter…')
  );
  // Local-mode wildcard detection. We show a small hint chip when the user
  // types `*` or `?` so they discover the remote-search switch. Suppressed
  // once dismissed for this sidebar lifetime — annoying twice in a row is
  // worse than missing it the first time.
  let wildcardHintDismissed = $state(false);
  const showWildcardHint = $derived(
    searchMode === 'local'
    && searchHasRemote
    && !!searchConfig?.wildcard_hint
    && /[*?]/.test(filter)
    && !wildcardHintDismissed
  );

  function toggleSearchMode() {
    if (!searchHasRemote || !searchHasLocal) return;
    searchMode = searchMode === 'local' ? 'remote' : 'local';
    searchModeUserPick = true;
    wildcardHintDismissed = false;
  }

  function submitRemoteSearch() {
    if (searchMode !== 'remote') return;
    const action = searchConfig?.remote_action;
    if (!action || !filter.trim()) return;
    firePluginAction(pluginName, action, JSON.stringify({ pattern: filter })).catch(() => {});
  }

  function onSearchKeydown(e: KeyboardEvent) {
    if (e.key !== 'Enter') return;
    if (searchMode === 'remote') {
      e.preventDefault();
      submitRemoteSearch();
    }
  }

  function promoteWildcardSearchToRemote() {
    if (!searchHasRemote) return;
    searchMode = 'remote';
    searchModeUserPick = true;
    submitRemoteSearch();
  }

  // Flat list of currently-visible, selectable node ids in render order. We
  // recompute on every $derived read; the cost is O(N) over the in-memory
  // tree, which is small even at 10k items (Tree's own DFS does the same).
  // Only nodes with `selectable === true` participate in multi-select.
  const flatSelectableIds = $derived.by(() => {
    const out: string[] = [];
    const walk = (ns: TreeNode[]) => {
      for (const n of ns) {
        if (n.selectable) out.push(n.id);
        if (n.children && n.children.length > 0) walk(n.children);
      }
    };
    walk(nodes);
    return out;
  });

  // Map id → TreeNode for resolving selectedIds → node objects on action fire.
  const nodeById = $derived.by(() => {
    const m = new Map<string, TreeNode>();
    const walk = (ns: TreeNode[]) => {
      for (const n of ns) {
        m.set(n.id, n);
        if (n.children && n.children.length > 0) walk(n.children);
      }
    };
    walk(nodes);
    return m;
  });

  function setSingleSelection(id: string | null) {
    selectedId    = id;
    selectedIds   = id ? new Set([id]) : new Set();
    lastClickedId = id;
  }

  /** Fire the node's `selection_action` (if any) after the local selection
   *  state has been updated. Independent from `default_action` — both may
   *  be set on the same node. Errors are swallowed because a misconfigured
   *  action shouldn't break selection feedback. */
  function fireSelectionAction(node: TreeNode) {
    if (!node.selection_action) return;
    firePluginAction(pluginName, node.selection_action, JSON.stringify({
      node_id: node.id, data: node.data,
    })).catch(() => {});
  }

  function handleSelect(node: TreeNode, e: MouseEvent | KeyboardEvent) {
    if (!node.selectable) return;
    const id = node.id;
    const ctrl  = e && ('ctrlKey'  in e) && (e.ctrlKey || (e as MouseEvent).metaKey);
    const shift = e && ('shiftKey' in e) &&  e.shiftKey;

    if (shift && lastClickedId !== null && lastClickedId !== id) {
      // Range select between lastClickedId and id (inclusive). We replace the
      // selection rather than additive-extend so the behaviour matches what
      // most file managers do; Ctrl+Shift would extend, but we keep it simple.
      const all = flatSelectableIds;
      const a = all.indexOf(lastClickedId);
      const b = all.indexOf(id);
      if (a >= 0 && b >= 0) {
        const [lo, hi] = a <= b ? [a, b] : [b, a];
        const next = new Set<string>();
        for (let i = lo; i <= hi; i++) next.add(all[i]);
        selectedIds = next;
        selectedId  = id;
        fireSelectionAction(node);
        // Do NOT update lastClickedId — keeps the anchor for further Shift+clicks.
        return;
      }
    }

    if (ctrl) {
      // Toggle membership.
      const next = new Set(selectedIds);
      if (next.has(id)) {
        next.delete(id);
        selectedId = next.size > 0 ? Array.from(next).pop()! : null;
      } else {
        next.add(id);
        selectedId = id;
      }
      selectedIds  = next;
      lastClickedId = id;
      // `selection_action` fires only when the row becomes (or remains) the
      // current selection — not on ctrl-deselect, which leaves either a
      // different row current or nothing at all.
      if (selectedId === id) fireSelectionAction(node);
      return;
    }

    // Plain click → single selection.
    setSingleSelection(id);
    fireSelectionAction(node);
  }

  function clearSelection() {
    selectedIds   = new Set();
    selectedId    = null;
    lastClickedId = null;
  }

  function handleActivate(node: TreeNode) {
    if (node.default_action) {
      firePluginAction(pluginName, node.default_action, JSON.stringify({
        node_id: node.id, data: node.data,
      })).catch(() => {});
    }
  }

  // ── Context menu ──────────────────────────────────────────────────────────
  let menuOpen = $state(false);
  let menuX    = $state(0);
  let menuY    = $state(0);
  let menuTargetNode = $state<TreeNode | null>(null);
  // Per-menu lookup: ContextMenu's onSelect callback only gives us the id —
  // we resolve it to the originating contribution via this map (rebuilt on
  // every right-click so stale entries can't survive).
  let menuMeta = $state<Record<string, { kind: 'action'; pluginName: string; action: string } | { kind: 'deps'; provider: PluginContribution; node: TreeNode }>>({});

  // Items for the currently open menu. Built once per right-click in
  // `handleContextMenu` (NOT in the template) so the side-effect of populating
  // `menuMeta` doesn't run inside a derived/template expression — Svelte 5
  // forbids that and throws state_unsafe_mutation.
  let menuItems = $state<MenuItem[]>([]);

  // Track whether the *current* context menu is in multi-select mode. Computed
  // at right-click time and frozen until the menu closes — the menu doesn't
  // react to selection changes while open.
  let menuIsMulti = $state(false);

  function handleContextMenu(node: TreeNode, e: MouseEvent) {
    // If the right-clicked node isn't already part of the multi-selection,
    // collapse selection to it (file-manager behaviour: right-click on an
    // unselected file targets that single file, not the prior selection).
    if (!selectedIds.has(node.id)) {
      setSingleSelection(node.id);
    }

    menuTargetNode = node;
    menuX = e.clientX;
    menuY = e.clientY;
    menuIsMulti = selectedIds.size > 1;
    // Build BEFORE flipping menuOpen so the first render of <ContextMenu>
    // already sees the populated items / meta.
    menuItems = buildContextMenuFor(node, menuIsMulti);
    menuOpen = true;
  }

  // Build the merged context menu for the right-clicked node:
  //   1. context_menu contributions whose `when` matches (incl. `when.multi`)
  //   2. auto-injected "Show dependencies" when a dependency_provider matches
  // Side-effect: rebuilds `menuMeta` so the id-based onSelect callback can
  // resolve back to the originating contribution.
  function buildContextMenuFor(node: TreeNode | null, isMulti: boolean): MenuItem[] {
    if (!node) return [];
    const items: MenuItem[] = [];
    const meta: typeof menuMeta = {};

    // The matcher reads `__isMulti` from the context to evaluate `when.multi`.
    const matcherCtx = { ...node, __isMulti: isMulti };

    for (const c of contributionStore.forPoint(`${ns}:context_menu`)) {
      const p = c.payload as any;
      if (!whenMatches(c.when, matcherCtx)) continue;
      if (c.disabled) continue;
      const id = `ctx::${c.plugin_name}::${c.item_id}`;
      items.push({
        id,
        label:     p?.label ?? c.item_id,
        danger:    !!p?.danger,
        separator: !!p?.separator,
      });
      if (p?.action) {
        meta[id] = { kind: 'action', pluginName: c.plugin_name, action: p.action };
      }
    }

    // Auto-injected "Show dependencies" when at least one provider matches —
    // single-row only, depends-modal doesn't multi-select.
    if (!isMulti) {
      const providers = contributionStore.forPoint(`${ns}:dependency_provider`)
        .filter(c => !c.disabled && whenMatches(c.when, node));
      if (providers.length > 0) {
        if (items.length > 0) items.push({ id: 'sep-deps', label: '', separator: true });
        const id = 'show-dependencies';
        items.push({ id, label: 'Show dependencies' });
        meta[id] = { kind: 'deps', provider: providers[0], node };
      }
    }
    menuMeta = meta;
    return items;
  }

  // Build the action ctx that gets fired for a context-menu pick. Always
  // includes both single-row fields (`node_id` / `data`, the right-clicked
  // primary) AND multi fields (`node_ids` / `nodes`). Single-target plugins
  // can keep using the existing fields; multi-target plugins use the new
  // arrays. Backward compatible.
  function buildActionCtx(): Record<string, unknown> {
    const ids   = Array.from(selectedIds);
    const nodes = ids.map(id => {
      const n = nodeById.get(id);
      return n ? { id: n.id, label: n.label, kind: n.kind, data: n.data } : { id };
    });
    return {
      node_id:  menuTargetNode?.id,
      data:     menuTargetNode?.data,
      node_ids: ids,
      nodes,
    };
  }

  function handleMenuSelect(id: string) {
    const meta = menuMeta[id];
    menuOpen = false;
    if (!meta) return;
    if (meta.kind === 'deps') {
      depModalNode = meta.node;
      depModalProvider = meta.provider;
      depModalOpen = true;
      return;
    }
    firePluginAction(meta.pluginName, meta.action, JSON.stringify(buildActionCtx()))
      .catch(() => {});
  }

  // ── Dependency modal state ────────────────────────────────────────────────
  let depModalOpen = $state(false);
  let depModalNode = $state<TreeNode | null>(null);
  let depModalProvider = $state<PluginContribution | null>(null);

  const hasFooter = $derived(contributionStore.forPoint(`${ns}:footer`).length > 0);
</script>

<!-- Outer wrapper (always present so the template tree stays balanced).
     In bottom-mode it gains the `.bottom-stack` flex column and renders a
     `BottomPanelHeader` above the header-less `PanelShell`, giving a single
     standardized chrome bar shared with every other bottom panel. Toolbar
     contributions move into the BottomPanelHeader actions slot so plugins
     keep the same affordances regardless of dock side. -->
<div class="plugin-tree-root" class:bottom-stack={bottomMode}>
{#if bottomMode}
  <BottomPanelHeader title={title || 'Plugin'}>
    {#snippet icon()}
      {#if section?.icon}
        <PluginIcon name={section.icon} size={13} />
      {/if}
    {/snippet}
    {#snippet actions()}
      <PluginTreeToolbar {ns} />
    {/snippet}
  </BottomPanelHeader>
{/if}

<PanelShell
  title={title || 'Plugin'}
  class="plugin-tree-sidebar"
  hideHeader={bottomMode}
>
  {#snippet icon()}
    {#if section?.icon}
      <PluginIcon name={section.icon} size={13} />
    {/if}
  {/snippet}

  {#snippet actions()}
    <PluginTreeToolbar {ns} />
  {/snippet}

  {#snippet toolbar()}
    {#if breadcrumbSegments.length > 0}
      <Breadcrumb
        segments={breadcrumbSegments}
        onSelect={handleBreadcrumbSelect}
        editable={!!breadcrumbEditAction}
        editValue={breadcrumbEditValue}
        editPlaceholder={breadcrumbEditPlaceholder}
        onCommit={handleBreadcrumbCommit}
      />
    {/if}
    <div class="plugin-tree-search-row" class:focused={filterFocused} class:remote-mode={searchMode === 'remote'}>
      <Search size={12} class="search-icon" />
      <input
        type="text"
        placeholder={placeholder}
        bind:value={filter}
        onfocus={() => filterFocused = true}
        onblur={() => filterFocused = false}
        onkeydown={onSearchKeydown}
      />
      {#if filter}
        <button class="search-clear" onclick={() => filter = ''} use:tooltip={'Clear'}>×</button>
      {/if}
      {#if searchHasRemote && searchHasLocal}
        <button
          class="search-mode-toggle"
          class:active={searchMode === 'remote'}
          onclick={toggleSearchMode}
          use:tooltip={searchMode === 'remote'
            ? 'Remote search — Enter runs a backend wildcard search. Click to switch to local filter.'
            : 'Local filter — only matches already-loaded rows. Click to switch to remote search.'}
        >
          {#if searchMode === 'remote'}
            <Globe size={11} />
          {:else}
            <Filter size={11} />
          {/if}
        </button>
      {/if}
    </div>
    {#if showWildcardHint}
      <div class="plugin-tree-search-hint" role="status">
        <span>Wildcards apply to loaded rows only.</span>
        <button class="hint-action" onclick={promoteWildcardSearchToRemote}>Search remote</button>
        <button class="hint-dismiss" onclick={() => wildcardHintDismissed = true} use:tooltip={'Dismiss'}>×</button>
      </div>
    {/if}
  {/snippet}

  <Tree
    {nodes}
    {selectedId}
    filter={effectiveFilter}
    selectable={(n: TreeNode) => !!n.selectable}
    rowClass={(ctx) => {
      const cls = [];
      if (ctx.node.kind === 'section') cls.push('tree-row-section');
      if (selectedIds.has(ctx.node.id)) cls.push('tree-row-multi');
      return cls.join(' ') || undefined;
    }}
    rowTitle={(n: TreeNode) => n.tooltip ?? n.label}
    onSelect={(n: TreeNode, e: MouseEvent | KeyboardEvent) => {
      if (n.selectable) handleSelect(n, e);
    }}
    onActivate={handleActivate}
    onContextMenu={handleContextMenu}
    draggable={(n: TreeNode) => !!n.draggable && !!snapshot?.drop_action}
    dropTarget={(n: TreeNode) => !!n.drop_target && !!snapshot?.drop_action}
    onDropOnNode={(src: TreeNode, dst: TreeNode) => {
      const act = snapshot?.drop_action;
      if (!act) return;
      firePluginAction(pluginName, act, JSON.stringify({
        source_id:   src.id, source_data: src.data,
        target_id:   dst.id, target_data: dst.data,
      })).catch(() => {});
    }}
  >
    {#snippet row({ node }: { node: TreeNode })}
      <PluginTreeNode {ns} {node} />
    {/snippet}
  </Tree>

  {#snippet footer()}
    {#if hasFooter}
      <PluginTreeFooter {ns} />
    {/if}
  {/snippet}
</PanelShell>
</div>

<svelte:window
  onkeydown={(e) => {
    // Esc clears the multi-selection — only when no modal is in front and no
    // input has focus (don't fight typing in forms / search fields).
    if (e.key !== 'Escape') return;
    if (selectedIds.size === 0) return;
    const t = e.target as HTMLElement | null;
    if (t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable)) return;
    clearSelection();
  }}
/>

{#if menuOpen && menuTargetNode}
  <ContextMenu
    x={menuX}
    y={menuY}
    items={menuItems}
    onSelect={handleMenuSelect}
    onClose={() => { menuOpen = false; menuTargetNode = null; }}
  />
{/if}

{#if depModalOpen && depModalNode && depModalProvider}
  <DependencyTreeModal
    pluginName={depModalProvider.plugin_name}
    providerAction={(depModalProvider.payload as any)?.action ?? ''}
    node={depModalNode}
    title={(depModalProvider.payload as any)?.label ?? 'Dependencies'}
    onClose={() => { depModalOpen = false; depModalNode = null; depModalProvider = null; }}
  />
{/if}
