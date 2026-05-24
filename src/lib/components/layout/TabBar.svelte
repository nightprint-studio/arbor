<script lang="ts">
  import { Layers, AlertTriangle } from 'lucide-svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import { workspacesStore } from '$lib/stores/workspaces.svelte';
  import { closeRepo } from '$lib/ipc/graph';
  import ContextMenu, { type MenuItem } from '../shared/ContextMenu.svelte';
  import Monogram from '$lib/components/shared/ui/Monogram.svelte';
  import Tabs, { type TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import { workspaceColorVar } from '$lib/types/workspace';
  import { tooltipBottom as tooltip } from '$lib/actions/tooltip';

  let { onOpen }: { onOpen: () => void } = $props();

  type RepoTabPayload = (typeof tabsStore.tabs)[number];

  // ── Items for the shared Tabs widget ────────────────────────────────────
  // Each TabItem carries the underlying repo tab as `data` so the
  // `itemContent` snippet can render workspace monogram / tombstone /
  // worktree icon / branch chip without re-looking-up by id.
  const items = $derived<TabItem[]>(
    tabsStore.tabs.map(t => ({
      id:       t.id,
      label:    t.name,
      title:    sourceWorkspaceTitle(t),
      closable: true,
      data:     t,
    })),
  );

  function sourceWorkspaceTitle(t: RepoTabPayload): string {
    const id = workspacesStore.sourceWsFor(t.id);
    const src = id ? workspacesStore.workspaces.find(w => w.id === id) : null;
    return src ? `${t.path}\nFrom workspace: ${src.name}` : t.path;
  }
  function sourceWorkspace(tabId: string) {
    const id = workspacesStore.sourceWsFor(tabId);
    if (!id) return null;
    return workspacesStore.workspaces.find(w => w.id === id) ?? null;
  }

  // ── Close + select ──────────────────────────────────────────────────
  async function handleClose(tabId: string) {
    try { await closeRepo(tabId); } catch { /* ignore */ }
    workspacesStore.unmarkCrossWs(tabId);
    tabsStore.removeTab(tabId);
  }

  // ── Context menu ────────────────────────────────────────────────────
  type TabCtx = { x: number; y: number; tabId: string };
  let tabCtxMenu = $state<TabCtx | null>(null);

  function openTabCtx(tabId: string, _item: TabItem, e: MouseEvent) {
    tabCtxMenu = { x: e.clientX, y: e.clientY, tabId };
  }

  function tabMenuItems(tabId: string): MenuItem[] {
    const otherCount = tabsStore.tabs.filter(t => t.id !== tabId).length;
    const isCross = workspacesStore.sourceWsFor(tabId) !== null;
    return [
      { id: 'close',        label: 'Close Tab', action: 'close_tab' },
      { id: 'close-others', label: 'Close Other Tabs', disabled: otherCount === 0 },
      { id: 'close-all',    label: 'Close All Tabs' },
      { id: 'sep',          label: '', separator: true },
      ...(isCross ? [{ id: 'add-to-active', label: 'Add to Active Workspace' }] : []),
      { id: 'copy-path',    label: 'Copy Path' },
    ];
  }

  async function handleTabCtxSelect(id: string) {
    if (!tabCtxMenu) return;
    const { tabId } = tabCtxMenu;
    tabCtxMenu = null;

    if (id === 'close') {
      await handleClose(tabId);
    } else if (id === 'close-others') {
      for (const t of tabsStore.tabs.filter(t => t.id !== tabId)) await handleClose(t.id);
    } else if (id === 'close-all') {
      for (const t of [...tabsStore.tabs]) await handleClose(t.id);
    } else if (id === 'copy-path') {
      const tab = tabsStore.tabs.find(t => t.id === tabId);
      if (tab) {
        await copyToClipboard(tab.path, { successToast: 'Path copied' });
      }
    } else if (id === 'add-to-active') {
      const activeWs = workspacesStore.activeId;
      if (activeWs) {
        try {
          await workspacesStore.addRepoTo(activeWs, tabId);
          workspacesStore.unmarkCrossWs(tabId);
          uiStore.showToast('Added to active workspace', 'success');
        } catch (e) {
          uiStore.showToast(`Failed: ${e}`, 'error');
        }
      }
    }
  }
</script>

<div class="tabbar-wrap">
  <Tabs
    {items}
    value={tabsStore.activeTabId}
    variant="panel"
    size="md"
    draggable
    overflow
    closable
    ariaLabel="Repositories"
    addLabel="Open repository (Ctrl+O)"
    onSelect={(id) => tabsStore.setActive(id)}
    onClose={(id) => handleClose(id)}
    onAdd={() => onOpen()}
    onReorder={(from, to) => tabsStore.reorderTabs(from, to)}
    onContextMenu={openTabCtx}
  >
    {#snippet itemContent({ item, active })}
      {@const tab    = item.data as RepoTabPayload}
      {@const source = sourceWorkspace(tab.id)}
      {#if source}
        <Monogram name={source.name} color={workspaceColorVar(source.color_idx)} size={12} />
        <!-- Cross-workspace stripe — reinforces the monogram signal with a
             subtle dashed accent line at the bottom of the tab. Hidden when
             the tab is active because the panel variant's solid underline
             already occupies that edge. -->
        {#if !active}
          <span class="cross-ws-stripe" aria-hidden="true"></span>
        {/if}
      {/if}
      {#if tab.tombstone}
        <span
          class="tab-tombstone-icon"
          use:tooltip={{ content: 'Repository missing', description: tab.tombstone.message }}
        ><AlertTriangle size={10} /></span>
      {:else if tab.isLinkedWorktree}
        <span class="tab-worktree-icon" use:tooltip={'Linked worktree'}>
          <Layers size={10} />
        </span>
      {/if}
      <span class="tab-name" class:tab-name-tombstone={!!tab.tombstone}>{tab.name}</span>
      {#if tab.currentBranch && !tab.tombstone}
        <span class="tab-branch" class:tab-branch-active={tab.id === tabsStore.activeTabId}>
          {tab.currentBranch}
        </span>
      {/if}
    {/snippet}
  </Tabs>
</div>

{#if tabCtxMenu}
  <ContextMenu
    x={tabCtxMenu.x}
    y={tabCtxMenu.y}
    items={tabMenuItems(tabCtxMenu.tabId)}
    onSelect={handleTabCtxSelect}
    onClose={() => tabCtxMenu = null}
  />
{/if}

<style>
  /* The shared <Tabs variant="panel"> handles strip layout, drag, overflow,
     add, close and the active "lift" treatment. This wrapper only fixes the
     bar's height + bg + z-index (it sits above the graph), and styles the
     repo-specific in-tab content (monogram/tombstone/worktree icons, branch
     chip). The chip's accent variant when the tab is active is matched here
     since the snippet receives `active` via the parent class on .tabs-tab. */

  .tabbar-wrap {
    height: 32px;
    background: var(--bg-base);
    flex-shrink: 0;
    position: relative;
    z-index: 50;
    display: flex;
    align-items: stretch;
  }
  .tabbar-wrap :global(.tabs) { width: 100%; }
  /* The strip should bottom-align to give room for the active tab's lift. */
  .tabbar-wrap :global(.tabs-strip) { align-self: flex-end; height: 90%; }

  .tab-name {
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 140px;
    font-size: var(--font-size-sm);
  }
  .tab-name-tombstone {
    color: var(--text-muted);
    font-style: italic;
    text-decoration: line-through;
    text-decoration-color: rgba(204, 167, 58, 0.5);
  }

  .tab-worktree-icon,
  .tab-tombstone-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    pointer-events: auto;
  }
  .tab-worktree-icon  { color: var(--accent); }
  .tab-tombstone-icon { color: var(--warning); }

  .tab-branch {
    font-size: 10px;
    color: var(--text-muted);
    max-width: 90px;
    overflow: hidden;
    text-overflow: ellipsis;
    background: var(--bg-overlay);
    padding: 1px 5px;
    border-radius: 9px;
    letter-spacing: 0.01em;
  }
  .tab-branch-active { color: var(--accent); background: var(--accent-subtle); }

  /* Absolutely-positioned stripe sitting at the bottom edge of the
     parent .tabs-tab (which is `position: relative` in Tabs.svelte). */
  .cross-ws-stripe {
    position: absolute;
    left: 4px;
    right: 4px;
    bottom: 0;
    height: 2px;
    border-radius: 1px 1px 0 0;
    background: repeating-linear-gradient(
      90deg,
      color-mix(in srgb, var(--accent) 45%, transparent) 0 3px,
      transparent 3px 6px
    );
    pointer-events: none;
  }
</style>
