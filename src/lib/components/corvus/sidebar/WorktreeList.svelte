<script lang="ts">
  import { Layers, Home, CircleDot, Lock, Plus, Trash2, ExternalLink, Info, ChevronRight, Link2, Folder, FolderOpen, ChevronsDown, ChevronsUp } from 'lucide-svelte';
  import { copyDeepLink } from '$lib/utils/deep-link-builder';
  import type { WorktreeInfo } from '$lib/types/corvus/git';
  import { worktreeStore } from '$lib/stores/corvus/worktree.svelte';
  import { tabsStore } from '$lib/stores/corvus/tabs.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { branchGroupingStore } from '$lib/stores/corvus/branch-grouping.svelte';
  import { branchesConfigStore } from '$lib/stores/corvus/branches-config.svelte';
  import { removeWorktree, openInIde } from '$lib/ipc/corvus/worktree';
  import { switchToWorktree } from '$lib/utils/worktree-switch';
  import ContextMenu, { type MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import SidebarSection from '$lib/components/shared/ui/SidebarSection.svelte';
  import Tree from '$lib/components/shared/ui/Tree.svelte';
  import WorktreeInfoModal from './WorktreeInfoModal.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import AddWorktreeModal from './AddWorktreeModal.svelte';

  // Folder-tree view of worktrees follows the same per-repo toggle as
  // BranchTree (the one tab-scoped switch in `branchGroupingStore`). The
  // grouping key here is the worktree's BRANCH path — `feature/auth/login`
  // splits into `feature → auth → login`. Detached HEAD worktrees have no
  // branch path and stay at the root as leaves regardless of the toggle.
  type WtNode =
    | { kind: 'group'; id: string; path: string; name: string; children: WtNode[] }
    | { kind: 'leaf';  id: string; wt: WorktreeInfo };

  // Folder colour — kept in sync with BranchTree so `feature` is the same
  // tint in both sidebars. Only well-known GitFlow / Conventional Branches
  // prefixes get a semantic colour; everything else stays neutral.
  // NOTE: duplicated with BranchTree — candidate for a shared util once
  // a third consumer shows up (e.g. Tags, Stashes grouping).
  const STANDARD_FOLDER_COLORS: Record<string, string> = {
    feature:    'var(--accent)',
    feat:       'var(--accent)',
    release:    'var(--success)',
    hotfix:     'var(--error)',
    bugfix:     'var(--warning)',
    fix:        'var(--warning)',
    bug:        'var(--warning)',
    experiment: 'var(--color-tag)',
    exp:        'var(--color-tag)',
    spike:      'var(--color-tag)',
    support:    'var(--color-stash)',
    chore:      'var(--text-muted)',
    docs:       'var(--text-muted)',
    doc:        'var(--text-muted)',
    test:       'var(--text-muted)',
    tests:      'var(--text-muted)',
    ci:         'var(--text-muted)',
    refactor:   'var(--text-muted)',
  };

  function folderColor(name: string): string {
    return STANDARD_FOLDER_COLORS[name.toLowerCase()] ?? 'var(--text-secondary)';
  }

  let {
    expanded = $bindable(false),
  }: {
    expanded?: boolean;
  } = $props();

  const tab          = $derived(tabsStore.activeTab);
  const worktrees    = $derived(worktreeStore.worktrees);
  const ideConfig    = $derived(worktreeStore.ideConfig);
  const detectedIdes = $derived(worktreeStore.detectedIdes);

  const groupingEnabled   = $derived(branchGroupingStore.isEnabled(tab?.id));
  const groupingRecursive = $derived(branchesConfigStore.groupingRecursive);

  // ── Context menu ───────────────────────────────────────────────────────────
  type CtxState = { x: number; y: number; worktree: WorktreeInfo };
  let ctxMenu = $state<CtxState | null>(null);
  let infoModal = $state<WorktreeInfo | null>(null);
  let addOpen   = $state(false);

  function buildMenuItems(wt: WorktreeInfo): MenuItem[] {
    const items: MenuItem[] = [];

    if (!wt.is_current) {
      items.push({ id: 'switch', label: 'Switch to this workspace', icon: ChevronRight, iconColor: 'var(--accent)' });
    }
    items.push({ id: 'info', label: 'Workspace info', icon: Info, iconColor: 'var(--text-muted)' });

    // Resolve the effective default IDE for this project type (language > global default).
    const effectiveDefaultId = ideConfig
      ? (ideConfig.language_defaults[wt.project_type] ?? ideConfig.default_ide)
      : undefined;

    const available = detectedIdes.filter(d => d.available);
    const customs   = ideConfig?.custom_ides ?? [];

    if (available.length > 0 || customs.length > 0) {
      items.push({ id: '__sep_ide__', label: '', separator: true });
      items.push({ id: '__hdr_ide__', label: 'Open in IDE', header: true });

      for (const ide of available) {
        const isDefault = effectiveDefaultId === ide.id;
        items.push({
          id:          `ide:${ide.id}`,
          label:       ide.name,
          icon:        ExternalLink,
          iconColor:   '#20b2aa',
          badge:       isDefault ? 'Default' : undefined,
          badgeAccent: isDefault,
        });
      }
      for (const custom of customs) {
        const isDefault = effectiveDefaultId === custom.id;
        items.push({
          id:          `ide:${custom.id}`,
          label:       custom.name,
          icon:        ExternalLink,
          iconColor:   '#20b2aa',
          badge:       isDefault ? 'Default' : undefined,
          badgeAccent: isDefault,
        });
      }
    } else {
      // Detection hasn't completed yet — show a single generic fallback.
      items.push({ id: '__sep_ide__', label: '', separator: true });
      items.push({ id: 'ide:default', label: 'Open in IDE', icon: ExternalLink, iconColor: '#20b2aa' });
    }

    if (wt.branch) {
      items.push({ id: '__sep_dl__', label: '', separator: true });
      items.push({ id: 'copy-deep-link', label: 'Copy arbor:// worktree link', icon: Link2, iconColor: '#20b2aa' });
    }

    if (!wt.is_main) {
      items.push({ id: '__sep_del__', label: '', separator: true });
      items.push({ id: 'remove', label: 'Remove workspace', icon: Trash2, danger: true });
    }

    return items;
  }

  // ── Handlers ──────────────────────────────────────────────────────────────

  function handleContextMenu(e: MouseEvent, wt: WorktreeInfo) {
    e.preventDefault();
    ctxMenu = { x: e.clientX, y: e.clientY, worktree: wt };
  }

  function handleDblClick(wt: WorktreeInfo) {
    if (wt.is_current) return;
    switchTo(wt);
  }

  const switchTo = switchToWorktree;

  async function handleCtxSelect(id: string) {
    if (!ctxMenu) return;
    const wt = ctxMenu.worktree;
    ctxMenu = null;

    if (id === 'switch') {
      switchTo(wt);
    } else if (id === 'info') {
      infoModal = wt;
    } else if (id === 'ide:default') {
      await doOpenInIde(wt.path); // uses backend default
    } else if (id.startsWith('ide:')) {
      await doOpenInIde(wt.path, id.slice(4));
    } else if (id === 'remove') {
      await handleRemove(wt);
    } else if (id === 'copy-deep-link') {
      if (tab && wt.branch) {
        void copyDeepLink({ kind: 'branch_worktree', branch: wt.branch }, tab.id);
      }
    }
  }

  async function doOpenInIde(path: string, ideId?: string) {
    try {
      await openInIde(path, ideId);
    } catch (err) {
      uiStore.showToast(`Failed to open IDE: ${err}`, 'error');
    }
  }

  async function handleRemove(wt: WorktreeInfo) {
    if (!tab) return;
    if (wt.is_main) {
      uiStore.showToast('Cannot remove the main workspace.', 'error');
      return;
    }
    try {
      await removeWorktree(tab.id, wt.path);
      uiStore.showToast(`Removed workspace "${wt.branch ?? wt.path}"`, 'success');
      await worktreeStore.load(tab.id);
    } catch (err) {
      uiStore.showToast(`Remove failed: ${err}`, 'error');
    }
  }

  function handleInfoSwitch() {
    if (infoModal) {
      switchTo(infoModal);
      infoModal = null;
    }
  }

  async function handleInfoIde(ideId?: string) {
    if (infoModal) await doOpenInIde(infoModal.path, ideId);
  }

  const PROJECT_ICON: Record<string, string> = {
    rust:         '🦀',
    node_js:      '🟩',
    java_maven:   '☕',
    java_gradle:  '☕',
    go:           '🐹',
    python:       '🐍',
    dot_net:      '🔷',
    cpp:          '⚙️',
    ruby:         '💎',
    php:          '🐘',
    unknown:      '',
  };

  /** Show just the folder name (last path segment). */
  function folderName(p: string) {
    return p.replace(/\\/g, '/').split('/').filter(Boolean).pop() ?? p;
  }

  // ── Grouping forest ─────────────────────────────────────────────
  function splitSegments(name: string, recursive: boolean): string[] {
    if (recursive) return name.split('/');
    const i = name.indexOf('/');
    return i < 0 ? [name] : [name.slice(0, i), name.slice(i + 1)];
  }

  function buildForest(list: WorktreeInfo[], recursive: boolean): WtNode[] {
    const root: WtNode[] = [];
    const groups = new Map<string, WtNode & { kind: 'group' }>();

    for (const wt of list) {
      // Detached HEAD or no branch → leaf at root (folder name as label).
      if (!wt.branch) {
        root.push({ kind: 'leaf', id: `wt:${wt.path}`, wt });
        continue;
      }
      const segs = splitSegments(wt.branch, recursive);
      if (segs.length === 1) {
        root.push({ kind: 'leaf', id: `wt:${wt.path}`, wt });
        continue;
      }
      let bucket = root;
      let pathSoFar = '';
      for (let i = 0; i < segs.length - 1; i++) {
        pathSoFar = pathSoFar ? `${pathSoFar}/${segs[i]}` : segs[i];
        let g = groups.get(pathSoFar);
        if (!g) {
          g = { kind: 'group', id: `g:${pathSoFar}`, path: pathSoFar, name: segs[i], children: [] };
          groups.set(pathSoFar, g);
          bucket.push(g);
        }
        bucket = g.children;
      }
      bucket.push({ kind: 'leaf', id: `wt:${wt.path}`, wt });
    }

    sortNodes(root);
    return root;
  }

  function sortNodes(nodes: WtNode[]) {
    nodes.sort((a, b) => {
      if (a.kind !== b.kind) return a.kind === 'group' ? -1 : 1;
      const an = a.kind === 'group' ? a.name : (a.wt.branch ?? folderName(a.wt.path));
      const bn = b.kind === 'group' ? b.name : (b.wt.branch ?? folderName(b.wt.path));
      return an.localeCompare(bn);
    });
    for (const n of nodes) if (n.kind === 'group') sortNodes(n.children);
  }

  const forest = $derived(groupingEnabled ? buildForest(worktrees, groupingRecursive) : []);

  function allGroupPaths(nodes: WtNode[], acc: Set<string> = new Set()): Set<string> {
    for (const n of nodes) {
      if (n.kind === 'group') { acc.add(n.path); allGroupPaths(n.children, acc); }
    }
    return acc;
  }

  const expandedIds = $derived.by(() => {
    const ids = new Set<string>();
    const collapsed = branchGroupingStore.collapsedGroups(tab?.id);
    for (const p of allGroupPaths(forest)) if (!collapsed.has(p)) ids.add(`g:${p}`);
    return ids;
  });

  function onExpandToggle(id: string, next: boolean) {
    if (!tab || !id.startsWith('g:')) return;
    branchGroupingStore.setCollapsed(tab.id, id.slice(2), !next);
  }

  // ── Group context menu (Expand all / Collapse all from this folder) ────
  type GroupCtx = { x: number; y: number; node: WtNode & { kind: 'group' } };
  let groupCtxMenu = $state<GroupCtx | null>(null);

  function descendantGroupPaths(node: WtNode): string[] {
    if (node.kind !== 'group') return [];
    const out = [node.path];
    for (const c of node.children) {
      if (c.kind === 'group') out.push(...descendantGroupPaths(c));
    }
    return out;
  }

  // Tree.svelte's `onContextMenu` is fired for every row — route leaves
  // to the existing per-worktree handler and groups to the new folder
  // menu. Tree has already preventDefault'd / stopPropagation'd so the
  // leaf row's own oncontextmenu inside `wtBody` no longer fires.
  function handleTreeCtx(node: WtNode, e: MouseEvent) {
    if (node.kind === 'leaf') {
      handleContextMenu(e, node.wt);
      return;
    }
    groupCtxMenu = { x: e.clientX, y: e.clientY, node };
  }

  function groupMenuItems(): MenuItem[] {
    return [
      { id: 'expand-all',   label: 'Expand all',   icon: ChevronsDown, iconColor: 'var(--accent)' },
      { id: 'collapse-all', label: 'Collapse all', icon: ChevronsUp,   iconColor: 'var(--text-muted)' },
    ];
  }

  function handleGroupCtxSelect(id: string) {
    if (!groupCtxMenu || !tab) return;
    const node = groupCtxMenu.node;
    groupCtxMenu = null;
    const paths = descendantGroupPaths(node);
    if (id === 'expand-all') {
      branchGroupingStore.setCollapsedMany(tab.id, paths, false);
    } else if (id === 'collapse-all') {
      branchGroupingStore.setCollapsedMany(tab.id, paths, true);
    }
  }

  const getChildren = (n: WtNode) => (n.kind === 'group' ? n.children : undefined);
  const getId       = (n: WtNode) => n.id;
  const hasChildren = (n: WtNode) => n.kind === 'group';

  function countLeaves(node: WtNode): number {
    if (node.kind === 'leaf') return 1;
    let n = 0;
    for (const c of node.children) n += countLeaves(c);
    return n;
  }
</script>

<!-- Declared at template root (not as a `<SidebarSection>` child) so it stays a
     local renderable snippet rather than being hoisted into SidebarSection's
     props; it's still lexically in scope for the `{@render wtBody(...)}` calls
     inside the section below. -->
{#snippet wtBody(wt: WorktreeInfo, leafLabel: string, withLeftPad: boolean)}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div
      class="wt-row"
      class:is-current={wt.is_current}
      class:flat={withLeftPad}
      role="button"
      tabindex="0"
      use:tooltip={wt.path}
      ondblclick={() => handleDblClick(wt)}
      oncontextmenu={(e) => handleContextMenu(e, wt)}
      onkeydown={(e) => {
        if (e.key === 'Enter') handleDblClick(wt);
      }}
    >
      <span class="wt-project-icon" aria-hidden="true">
        {PROJECT_ICON[wt.project_type] ?? ''}
      </span>

      <span class="wt-name">
        {#if wt.branch}
          {leafLabel}
        {:else}
          <span class="wt-detached" use:tooltip={'Detached HEAD'}>{folderName(wt.path)}</span>
        {/if}
      </span>

      <span class="wt-badges">
        {#if wt.is_main}
          <span class="wt-badge wt-badge-main" use:tooltip={{ content: 'Main worktree', description: 'Cannot be removed' }}>
            <Home size={9} />
          </span>
        {/if}
        {#if wt.is_current}
          <span class="wt-badge wt-badge-current" use:tooltip={'Currently open'}>
            <CircleDot size={9} />
          </span>
        {/if}
        {#if wt.is_locked}
          <span class="wt-badge wt-badge-locked" use:tooltip={'Locked'}>
            <Lock size={9} />
          </span>
        {/if}
      </span>

      <button
        class="wt-info-btn"
        use:tooltip={'Info'}
        onclick={(e) => { e.stopPropagation(); infoModal = wt; }}
      >
        <Info size={11} />
      </button>

      {#if !wt.is_current}
        <span class="wt-actions">
          <button
            class="wt-action-btn"
            use:tooltip={{ content: 'Switch here', description: 'Double-click' }}
            onclick={(e) => { e.stopPropagation(); switchTo(wt); }}
          >
            <ChevronRight size={11} />
          </button>
        </span>
      {/if}
    </div>
  {/snippet}

<SidebarSection
  label="Worktrees"
  iconColor="var(--accent)"
  badge={worktrees.length || null}
  badgeColor="var(--accent)"
  bind:expanded
>
  {#snippet icon()}<Layers size={13} />{/snippet}
  {#snippet actions()}
    <button class="add-btn" use:tooltip={'Add linked worktree'} onclick={() => addOpen = true}>
      <Plus size={11} />
    </button>
  {/snippet}

  {#if worktreeStore.loading}
    <div class="empty-msg">Loading…</div>
  {:else if worktreeStore.error}
    <div class="empty-msg error-msg">{worktreeStore.error}</div>
  {:else if worktrees.length === 0}
    <div class="empty-msg">No additional worktrees found.</div>
  {:else if !groupingEnabled}
    {#each worktrees as wt (wt.path)}
      {@render wtBody(wt, wt.branch ?? folderName(wt.path), true)}
    {/each}
  {:else}
    <Tree
      nodes={forest}
      {getChildren}
      {getId}
      {hasChildren}
      {expandedIds}
      {onExpandToggle}
      onContextMenu={handleTreeCtx}
      rowHeight={24}
      indentSize={12}
      basePadding={4}
      showChevron={true}
      ariaLabel="Worktrees grouped by branch path"
    >
      {#snippet row(ctx)}
        {#if ctx.node.kind === 'group'}
          {@const tint = folderColor(ctx.node.name)}
          {@const total = countLeaves(ctx.node)}
          <span class="wt-group-icon" style="color: {tint}">
            {#if ctx.expanded}<FolderOpen size={12} />{:else}<Folder size={12} />{/if}
          </span>
          <span class="wt-group-name truncate">{ctx.node.name}</span>
          <span class="wt-group-count" use:tooltip={`${total} worktree${total === 1 ? '' : 's'}`}>{total}</span>
        {:else}
          {@const wt = ctx.node.wt}
          {@const lastSeg = wt.branch ? (splitSegments(wt.branch, groupingRecursive).at(-1) ?? wt.branch) : folderName(wt.path)}
          {@render wtBody(wt, lastSeg, false)}
        {/if}
      {/snippet}
    </Tree>
  {/if}
</SidebarSection>

<!-- ── Context menu ── -->
{#if ctxMenu}
  <ContextMenu
    x={ctxMenu.x}
    y={ctxMenu.y}
    items={buildMenuItems(ctxMenu.worktree)}
    onSelect={handleCtxSelect}
    onClose={() => ctxMenu = null}
  />
{/if}

{#if groupCtxMenu}
  <ContextMenu
    x={groupCtxMenu.x}
    y={groupCtxMenu.y}
    items={groupMenuItems()}
    onSelect={handleGroupCtxSelect}
    onClose={() => groupCtxMenu = null}
  />
{/if}

<!-- ── Info modal ── -->
{#if infoModal}
  <WorktreeInfoModal
    worktree={infoModal}
    onClose={() => infoModal = null}
    onSwitch={handleInfoSwitch}
    onOpenInIde={handleInfoIde}
  />
{/if}

<!-- ── Add worktree modal ── -->
{#if addOpen && tab}
  <AddWorktreeModal
    tabId={tab.id}
    onClose={() => addOpen = false}
    onAdded={() => { addOpen = false; tab && worktreeStore.load(tab.id); }}
  />
{/if}

<style>
  /* "Add linked worktree" — hover-reveal handled by SidebarSection's
     .section-actions wrapper. */
  .add-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    flex-shrink: 0;
    background: none;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    cursor: pointer;
    transition: background 0.12s, color 0.12s;
  }
  .add-btn:hover { background: var(--bg-hover); color: var(--accent) !important; }

  .empty-msg {
    padding: 6px 16px;
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    font-style: italic;
  }
  .error-msg { color: var(--diff-del-bg-strong, #ff5555); font-style: normal; }

  /* ── Worktree row ── */
  .wt-row {
    display: flex;
    flex: 1;
    align-items: center;
    gap: 5px;
    padding: 3px 8px 3px 4px;
    cursor: pointer;
    border-radius: var(--radius-sm);
    min-height: 24px;
    transition: background 0.1s;
    position: relative;
  }
  /* Flat view keeps the legacy 22 px left pad so the rows align with the
     icon column shared by other Sidebar sections; the grouped view leaves
     it to Tree.svelte's indent calculation. */
  .wt-row.flat { padding-left: 22px; }
  .wt-row:hover { background: var(--bg-hover); }
  .wt-row:focus-visible { outline: 1px solid var(--accent); }

  .wt-row.is-current {
    background: var(--accent-subtle);
  }
  .wt-row.is-current .wt-name {
    color: var(--accent);
    font-weight: 600;
  }

  .wt-project-icon {
    font-size: var(--font-size-sm);
    line-height: 1;
    flex-shrink: 0;
    width: 16px;
    text-align: center;
  }

  .wt-name {
    font-size: var(--font-size-sm);
    color: var(--text-primary);
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .wt-detached {
    color: var(--text-secondary);
    font-style: italic;
  }

  /* Badges */
  .wt-badges {
    display: flex;
    align-items: center;
    gap: 3px;
    flex-shrink: 0;
  }
  .wt-badge {
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 1px 3px;
    border-radius: var(--radius-sm);
    font-size: var(--font-size-3xs);
  }
  .wt-badge-main {
    color: var(--color-stash);
    background: color-mix(in srgb, var(--color-stash) 18%, transparent);
  }
  .wt-badge-current {
    color: var(--accent);
    background: var(--accent-subtle);
  }
  .wt-badge-locked {
    color: var(--text-muted);
    background: var(--bg-overlay);
  }

  /* Always-visible info button */
  .wt-info-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    flex-shrink: 0;
    background: none;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-disabled);
    cursor: pointer;
    transition: background 0.1s, color 0.1s;
  }
  .wt-row:hover .wt-info-btn,
  .wt-row.is-current .wt-info-btn { color: var(--text-muted); }
  .wt-info-btn:hover { background: var(--bg-overlay); color: var(--text-primary) !important; }

  /* Switch action (hover only) */
  .wt-actions {
    display: flex;
    align-items: center;
    gap: 2px;
    opacity: 0;
    transition: opacity 0.12s;
    flex-shrink: 0;
  }
  .wt-row:hover .wt-actions { opacity: 1; }

  .wt-action-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    background: none;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    cursor: pointer;
    transition: background 0.1s, color 0.1s;
  }
  .wt-action-btn:hover { background: var(--bg-overlay); color: var(--text-primary); }

  /* ── Grouped view (Tree.svelte) ── */
  .wt-group-icon {
    display: inline-flex;
    align-items: center;
    flex-shrink: 0;
  }
  .wt-group-name {
    flex: 1;
    min-width: 0;
    font-size: var(--font-size-sm);
    color: var(--text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .wt-group-count {
    flex-shrink: 0;
    font-family: var(--font-code);
    font-size: var(--font-size-2xs);
    line-height: 1;
    padding: 2px 6px;
    border-radius: 999px;
    color: var(--text-muted);
    background: var(--bg-overlay);
  }
</style>
