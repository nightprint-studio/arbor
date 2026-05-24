<script lang="ts">
  import { GitBranch, ArrowUp, ArrowDown, RotateCcw, ArrowUpToLine, Trash2, ExternalLink, Pencil, GitMerge, ArrowLeftRight, Link2, Combine, FastForward } from 'lucide-svelte';
  import type { MergeStrategy } from '$lib/ipc/branch';
  import { copyDeepLink } from '$lib/utils/deep-link-builder';
  import type { BranchInfo } from '$lib/types/git';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { graphStore } from '$lib/stores/graph.svelte';
  import { repoStore } from '$lib/stores/repo.svelte';
  import { checkoutBranchSafe, checkoutRemoteAsLocalSafe, deleteBranch, deleteRemoteBranches, mergeBranch } from '$lib/ipc/branch';
  import { applyPostCheckout } from '$lib/utils/applyPostCheckout';
  import { handleCheckoutResult } from '$lib/utils/checkoutResultHandler';
  import { pushBranch, openInBrowser } from '$lib/ipc/remote';
  import ContextMenu, { type MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import DeleteRemoteBranchModal from './DeleteRemoteBranchModal.svelte';
  import RemoteBranchRenameModal from './RemoteBranchRenameModal.svelte';
  import BranchCompareModal from './BranchCompareModal.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  let {
    branches,
    type,
    onRename,
    onCreateBranch,
  }: { branches: BranchInfo[]; type: 'local' | 'remote'; onRename?: (branch: BranchInfo) => void; onCreateBranch?: (branch: BranchInfo) => void } = $props();

  const tab = $derived(tabsStore.activeTab);

  const LANE_COLORS = [
    '#4d78cc', '#cc7832', '#6a9956', '#9876aa',
    '#c75450', '#20b2aa', '#ffc66d', '#e08060',
  ];

  function branchColor(name: string): string {
    let h = 0;
    for (let i = 0; i < name.length; i++) h = (h * 31 + name.charCodeAt(i)) >>> 0;
    return LANE_COLORS[h % LANE_COLORS.length];
  }

  function handleClick(branch: BranchInfo) {
    graphStore.scrollToBranch(branch.name);
  }

  async function handleCheckout(branch: BranchInfo, e: MouseEvent) {
    e.stopPropagation();
    if (!tab || type !== 'local' || branch.is_head) return;
    try {
      const result = await checkoutBranchSafe(tab.id, branch.name);
      handleCheckoutResult(result, {
        targetLabel:    branch.name,
        successMessage: `Checked out ${branch.name}`,
      });
      // Light refresh — checkout doesn't change graph topology, only HEAD.
      // Skips the expensive `getGraph` lane re-assignment.
      await applyPostCheckout(tab.id);
    } catch (err) {
      // Backend reject — refresh anyway because the failure could have
      // partially mutated the workdir (Windows file-lock during checkout_tree,
      // hook side-effect) and the abnormal-state alert in repoStore will
      // surface it.
      await applyPostCheckout(tab.id).catch(() => { /* best-effort */ });
      uiStore.showToast(`${err}`, 'error');
    }
  }

  // ── Context menu ────────────────────────────────────────────────
  type BranchCtx = { x: number; y: number; branch: BranchInfo };
  let ctxMenu = $state<BranchCtx | null>(null);
  let confirmRemoteDelete = $state<BranchInfo | null>(null);
  let renameRemoteTarget  = $state<BranchInfo | null>(null);

  function openBranchCtx(e: MouseEvent, branch: BranchInfo) {
    e.preventDefault();
    e.stopPropagation();
    graphStore.setHighlightedBranch(branch.name);
    ctxMenu = { x: e.clientX, y: e.clientY, branch };
  }

  function branchMenuItems(branch: BranchInfo): MenuItem[] {
    if (type === 'remote') {
      return [
        { id: 'checkout',       label: 'Checkout as local branch', icon: RotateCcw },
        { id: 'sep',            label: '',                          separator: true },
        { id: 'rename-remote',  label: 'Rename…',                  icon: Pencil },
        { id: 'sep1',           label: '',                          separator: true },
        { id: 'open-browser',   label: 'Open in browser',          icon: ExternalLink },
        { id: 'copy-deep-link', label: 'Copy arbor:// checkout link', icon: Link2 },
        { id: 'sep2',           label: '',                          separator: true },
        { id: 'delete-remote',  label: 'Delete remote branch',     icon: Trash2, danger: true },
      ];
    }
    return [
      { id: 'checkout',      label: 'Checkout',              icon: RotateCcw,   disabled: branch.is_head },
      { id: 'push',          label: 'Push to remote',        icon: ArrowUpToLine, action: branch.is_head ? 'push' : undefined },
      { id: 'create-branch', label: 'Create branch from here…', icon: GitBranch, action: 'new_branch' },
      { id: 'sep',           label: '',                      separator: true },
      { id: 'rename',        label: 'Rename…',               icon: Pencil },
      { id: 'sep1',         label: '',               separator: true },
      { id: 'open-browser', label: 'Open in browser', icon: ExternalLink },
      { id: 'copy-deep-link', label: 'Copy arbor:// checkout link', icon: Link2 },
      { id: 'sep2',         label: '',               separator: true },
      { id: 'delete',       label: 'Delete Branch',  icon: Trash2, danger: true, disabled: branch.is_head },
    ];
  }

  /** Strip the `<remote>/` prefix from a remote branch name so the deep
   *  link references the local short name the recipient will check out. */
  function shortBranchName(b: BranchInfo): string {
    if (type !== 'remote') return b.name;
    const slash = b.name.indexOf('/');
    return slash >= 0 ? b.name.slice(slash + 1) : b.name;
  }

  async function handleBranchCtxSelect(id: string) {
    if (!ctxMenu || !tab) return;
    const { branch } = ctxMenu;
    ctxMenu = null;

    if (id === 'checkout') {
      try {
        const isRemote = type === 'remote';
        const result = isRemote
          ? await checkoutRemoteAsLocalSafe(tab.id, branch.name)
          : await checkoutBranchSafe(tab.id, branch.name);
        // Use the resolved local name (for remote-as-local it may differ from
        // the full `origin/foo` we asked for) when displaying the toast.
        const label = result.resolved_local_name ?? branch.name;
        handleCheckoutResult(result, {
          targetLabel:    label,
          successMessage: `Checked out ${label}`,
        });
        // Remote-as-local creates a brand-new ref that needs to appear on
        // the graph node — full refresh required.  For an existing local
        // branch only HEAD moves, so the light path is enough.
        if (isRemote) graphStore.refresh();
        else          await applyPostCheckout(tab.id);
      } catch (err) {
        await applyPostCheckout(tab.id).catch(() => { /* best-effort */ });
        uiStore.showToast(`${err}`, 'error');
      }
    } else if (id === 'push') {
      try {
        await pushBranch(tab.id, 'origin', `refs/heads/${branch.name}`);
        uiStore.showToast(`Pushed ${branch.name}`, 'success');
        graphStore.refresh();
      } catch (err) { uiStore.showToast(`Push failed: ${err}`, 'error'); }
    } else if (id === 'delete') {
      try {
        await deleteBranch(tab.id, branch.name);
        uiStore.showToast(`Deleted branch "${branch.name}"`, 'success');
        graphStore.refresh();
      } catch (err) { uiStore.showToast(`${err}`, 'error'); }
    } else if (id === 'rename') {
      onRename?.(branch);
    } else if (id === 'create-branch') {
      onCreateBranch?.(branch);
    } else if (id === 'open-browser') {
      try {
        await openInBrowser(tab.id, `branch:${branch.name}`);
      } catch (err) { uiStore.showToast(`${err}`, 'error'); }
    } else if (id === 'delete-remote') {
      confirmRemoteDelete = branch;
    } else if (id === 'rename-remote') {
      renameRemoteTarget = branch;
    } else if (id === 'copy-deep-link') {
      void copyDeepLink({ kind: 'branch_checkout', branch: shortBranchName(branch) }, tab.id);
    }
  }

  async function executeRemoteDelete() {
    const branch = confirmRemoteDelete;
    confirmRemoteDelete = null;
    if (!branch || !tab) return;
    try {
      const failed = await deleteRemoteBranches(tab.id, [branch.name]);
      if (failed.length === 0) {
        uiStore.showToast(`Deleted remote branch "${branch.name}"`, 'success');
        graphStore.refresh();
      } else {
        uiStore.showToast(`Failed to delete "${branch.name}" from remote`, 'error');
      }
    } catch (err) { uiStore.showToast(`${err}`, 'error'); }
  }

  // ── Drag and drop (mouse-event based — avoids WebView2 DnD issues) ──────────
  type DropMenuState = {
    x: number;
    y: number;
    source: { name: string; listType: 'local' | 'remote' };
    target: { name: string; is_head: boolean; listType: 'local' | 'remote' };
  };
  let dropMenu    = $state<DropMenuState | null>(null);
  let compareModal = $state<{ fromRef: string; toRef: string } | null>(null);

  // Ghost label shown while dragging (fixed-position, follows cursor)
  let ghost = $state<{ name: string; x: number; y: number } | null>(null);

  // The branch item currently highlighted as a drop target (direct DOM ref)
  let hoverEl: HTMLElement | null = null;
  const HOVER_CLASS = 'drag-over';
  const MIN_DRAG_PX = 5;

  function clearHover() {
    hoverEl?.classList.remove(HOVER_CLASS);
    hoverEl = null;
  }

  function branchElAt(x: number, y: number): HTMLElement | null {
    // elementFromPoint returns the topmost element; walk up to find the branch item
    const el = document.elementFromPoint(x, y) as HTMLElement | null;
    return el?.closest<HTMLElement>('[data-bname]') ?? null;
  }

  function startDrag(e: MouseEvent, branch: BranchInfo) {
    // Only react to primary button; ignore if a menu is open
    if (e.button !== 0 || ctxMenu || dropMenu) return;

    const startX = e.clientX;
    const startY = e.clientY;
    const source = { name: branch.name, listType: type };
    let dragging = false;

    function onMove(ev: MouseEvent) {
      if (!dragging) {
        const dx = ev.clientX - startX;
        const dy = ev.clientY - startY;
        if (Math.hypot(dx, dy) < MIN_DRAG_PX) return;
        dragging = true;
        // Change cursor globally while dragging
        document.body.style.cursor = 'grabbing';
      }

      ghost = { name: branch.name, x: ev.clientX + 14, y: ev.clientY - 10 };

      // Hover highlight
      clearHover();
      const el = branchElAt(ev.clientX, ev.clientY);
      if (el) {
        const tname = el.dataset.bname!;
        const ttype = el.dataset.btype as string;
        if (!(tname === source.name && ttype === source.listType)) {
          el.classList.add(HOVER_CLASS);
          hoverEl = el;
        }
      }
    }

    function onUp(ev: MouseEvent) {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
      document.body.style.cursor = '';
      clearHover();
      ghost = null;

      if (!dragging) return; // was just a click

      const el = branchElAt(ev.clientX, ev.clientY);
      if (!el) return;

      const targetName   = el.dataset.bname!;
      const targetType   = el.dataset.btype as 'local' | 'remote';
      const targetIsHead = el.dataset.bhead === 'true';

      if (targetName === source.name && targetType === source.listType) return;

      const menuData: DropMenuState = {
        x: ev.clientX,
        y: ev.clientY,
        source,
        target: { name: targetName, is_head: targetIsHead, listType: targetType },
      };

      // Defer by one event-loop tick: the mouseup→click sequence must finish
      // before the ContextMenu mounts, otherwise its <svelte:window onclick>
      // handler fires on that same click and immediately closes the menu.
      setTimeout(() => { dropMenu = menuData; }, 0);
    }

    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
  }

  function dropMenuItems(menu: DropMenuState): MenuItem[] {
    const items: MenuItem[] = [];
    // Merge family: local→local only, and target must be current branch (no checkout needed)
    if (menu.target.listType === 'local' && menu.target.is_head && menu.source.listType === 'local') {
      const src = menu.source.name;
      const tgt = menu.target.name;
      items.push({
        id: 'merge',
        label: `Merge "${src}" into "${tgt}"`,
        icon: GitMerge,
        iconColor: 'var(--accent)',
      });
      items.push({
        id: 'merge-no-ff',
        label: `Merge "${src}" into "${tgt}" (no fast-forward)`,
        icon: GitMerge,
        iconColor: 'var(--color-tag)',
      });
      items.push({
        id: 'merge-squash',
        label: `Squash merge "${src}" into "${tgt}"`,
        icon: Combine,
        iconColor: 'var(--color-stash)',
      });
      items.push({
        id: 'merge-ff-only',
        label: `Fast-forward "${tgt}" to "${src}" only`,
        icon: FastForward,
        iconColor: 'var(--success)',
      });
      items.push({ id: 'sep-dm', label: '', separator: true });
    }
    items.push({
      id: 'compare',
      label: `Compare "${menu.source.name}" ↔ "${menu.target.name}"`,
      icon: ArrowLeftRight,
      iconColor: '#20b2aa',
    });
    return items;
  }

  /** Map drop-menu id to the backend MergeStrategy. */
  function strategyForMenuId(id: string): MergeStrategy | null {
    switch (id) {
      case 'merge':          return 'default';
      case 'merge-no-ff':    return 'no_ff';
      case 'merge-squash':   return 'squash';
      case 'merge-ff-only':  return 'ff_only';
      default:               return null;
    }
  }

  async function handleDropMenuSelect(id: string) {
    if (!dropMenu || !tab) return;
    const { source, target } = dropMenu;
    dropMenu = null;

    const strategy = strategyForMenuId(id);
    if (strategy) {
      try {
        const outcome = await mergeBranch(tab.id, source.name, strategy);
        switch (outcome) {
          case 'already_up_to_date':
            uiStore.showToast(
              `"${target.name}" already contains "${source.name}" — nothing to merge`,
              'info',
            );
            break;
          case 'fast_forward':
            uiStore.showToast(
              `Fast-forwarded "${target.name}" to "${source.name}"`,
              'success',
            );
            break;
          case 'merged':
            uiStore.showToast(`Merged "${source.name}" into "${target.name}"`, 'success');
            break;
          case 'squashed':
            uiStore.showToast(
              `Squashed "${source.name}" into the index — review and commit from Stage`,
              'success',
            );
            break;
        }
        graphStore.refresh();
      } catch (err) {
        const msg = String(err);
        if (msg.includes('CONFLICTS:')) {
          uiStore.showToast('Merge completed with conflicts — resolve them in the Stage area', 'warning');
          graphStore.refresh();
        } else {
          uiStore.showToast(`Merge failed: ${msg}`, 'error');
        }
      }
    } else if (id === 'compare') {
      compareModal = { fromRef: target.name, toRef: source.name };
    }
  }
</script>

<div class="branch-tree" role="list">
  {#each branches as branch (branch.name)}
    {@const color = branchColor(branch.name)}
    <div
      class="branch-item"
      class:current={branch.is_head}
      class:selected={graphStore.highlightedBranchName === branch.name}
      data-bname={branch.name}
      data-btype={type}
      data-bhead={branch.is_head ? 'true' : 'false'}
      onclick={() => handleClick(branch)}
      ondblclick={(e) => handleCheckout(branch, e)}
      oncontextmenu={(e) => openBranchCtx(e, branch)}
      onmousedown={(e) => startDrag(e, branch)}
      use:tooltip={{
        content: branch.name,
        description: branch.head_summary
          ? `${branch.head_summary}\nClick to focus · Double-click to checkout · Right-click for options · Drag to merge/compare`
          : 'Click to focus · Double-click to checkout · Right-click for options · Drag to merge/compare',
      }}
      role="button"
      tabindex="0"
      onkeydown={(e) => e.key === 'Enter' && handleClick(branch)}
    >
      <span class="branch-icon" style="color: {branch.is_head ? 'var(--accent)' : color}">
        <GitBranch size={12} />
      </span>

      <span class="branch-name truncate">{branch.name}</span>

      {#if branch.is_head}
        <span class="current-pill">HEAD</span>
      {/if}

      {#if type === 'local' && !branch.upstream && !repoStore.remoteBranches.some(r => r.name.endsWith('/' + branch.name))}
        <span class="local-only-badge" use:tooltip={{ content: 'No remote tracking branch', description: 'Not pushed yet' }}>local</span>
      {/if}

      {#if branch.ahead > 0}
        <span class="sync-badge ahead" use:tooltip={`${branch.ahead} ahead of remote`}>
          <ArrowUp size={10} />{branch.ahead}
        </span>
      {/if}
      {#if branch.behind > 0}
        <span class="sync-badge behind" use:tooltip={`${branch.behind} behind remote`}>
          <ArrowDown size={10} />{branch.behind}
        </span>
      {/if}
    </div>
  {:else}
    <EmptyState message="No branches" />
  {/each}
</div>

<!-- Drag ghost -->
{#if ghost}
  <div class="drag-ghost" style="left:{ghost.x}px; top:{ghost.y}px">
    <GitBranch size={11} />
    <span>{ghost.name}</span>
  </div>
{/if}

<style>
  .branch-tree { padding: 2px 0; }

  .branch-item {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 3px 8px 3px 4px;
    cursor: pointer;
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    color: var(--text-secondary);
    border-radius: 0;
    margin: 0;
    transition: background var(--transition-fast), color var(--transition-fast);
    overflow: hidden;
    outline: none;
    min-height: 22px;
    user-select: none;
  }

  .branch-item:hover { background: rgba(255,255,255,0.05); color: var(--text-primary); }
  .branch-item:focus-visible { outline: 1px solid var(--border-focus); outline-offset: -1px; }

  .branch-item.current {
    color: var(--text-primary);
    background: rgba(77, 120, 204, 0.16);
    font-weight: 500;
  }
  .branch-item.current:hover { background: rgba(77, 120, 204, 0.22); }

  /* Highlighted when scrolled-to or right-clicked */
  .branch-item.selected {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    color: var(--text-primary);
    outline: 1px solid color-mix(in srgb, var(--accent) 45%, transparent);
    outline-offset: -1px;
  }
  .branch-item.selected:hover { background: color-mix(in srgb, var(--accent) 24%, transparent); }

  /* Applied directly via DOM during mouse drag */
  :global(.branch-item.drag-over) {
    background: color-mix(in srgb, var(--accent) 20%, transparent) !important;
    outline: 1px dashed var(--accent) !important;
    outline-offset: -1px;
    color: var(--text-primary) !important;
  }

  .branch-icon {
    display: flex;
    align-items: center;
    flex-shrink: 0;
    transition: color var(--transition-fast);
  }

  .branch-name { flex: 1; min-width: 0; }

  .current-pill {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.4px;
    color: var(--accent);
    background: rgba(77, 120, 204, 0.18);
    border: 1px solid rgba(77, 120, 204, 0.35);
    padding: 0 4px;
    border-radius: 999px;
    flex-shrink: 0;
  }

  .local-only-badge {
    font-size: 9px;
    font-weight: 600;
    letter-spacing: 0.3px;
    color: var(--color-tag);
    background: color-mix(in srgb, var(--color-tag) 14%, transparent);
    border: 1px solid rgba(152, 118, 170, 0.28);
    padding: 0 4px;
    border-radius: 999px;
    flex-shrink: 0;
    line-height: 14px;
  }

  .sync-badge {
    display: flex;
    align-items: center;
    gap: 1px;
    font-size: 10px;
    flex-shrink: 0;
  }
  .sync-badge.ahead  { color: var(--success); }
  .sync-badge.behind { color: var(--warning); }

  /* Drag ghost — fixed so it escapes overflow:hidden parents */
  .drag-ghost {
    position: fixed;
    pointer-events: none;
    z-index: var(--z-top);
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px 4px 8px;
    background: var(--bg-elevated);
    border: 1px solid var(--accent);
    border-radius: var(--radius-md);
    box-shadow: 0 4px 16px rgba(0,0,0,0.4);
    font-family: var(--font-code);
    font-size: 12px;
    font-weight: 500;
    color: var(--text-primary);
    white-space: nowrap;
    opacity: 0.92;
  }
</style>

{#if ctxMenu}
  <ContextMenu
    x={ctxMenu.x}
    y={ctxMenu.y}
    items={branchMenuItems(ctxMenu.branch)}
    onSelect={handleBranchCtxSelect}
    onClose={() => ctxMenu = null}
  />
{/if}

{#if dropMenu}
  <ContextMenu
    x={dropMenu.x}
    y={dropMenu.y}
    items={dropMenuItems(dropMenu)}
    onSelect={handleDropMenuSelect}
    onClose={() => dropMenu = null}
  />
{/if}

{#if confirmRemoteDelete}
  <DeleteRemoteBranchModal
    branchName={confirmRemoteDelete.name}
    onConfirm={executeRemoteDelete}
    onCancel={() => (confirmRemoteDelete = null)}
  />
{/if}

{#if renameRemoteTarget}
  <RemoteBranchRenameModal
    branch={renameRemoteTarget}
    onClose={() => (renameRemoteTarget = null)}
    onRenamed={() => graphStore.refresh()}
  />
{/if}

{#if compareModal}
  <BranchCompareModal
    fromRef={compareModal.fromRef}
    toRef={compareModal.toRef}
    onClose={() => compareModal = null}
  />
{/if}
