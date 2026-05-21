<script lang="ts">
  import { Archive, Play, CornerDownLeft, Trash2, Pencil, Crosshair } from 'lucide-svelte';
  import type { StashEntry } from '$lib/types/git';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { graphStore } from '$lib/stores/graph.svelte';
  import { diffStore } from '$lib/stores/diff.svelte';
  import { stashRename } from '$lib/ipc/branch';
  import { getCommitDiff } from '$lib/ipc/diff';
  import ContextMenu, { type MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import InlineEdit from '$lib/components/shared/ui/InlineEdit.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { applyStashAction, popStashAction, dropStashAction } from '$lib/utils/stash-actions';
  import { tooltip } from '$lib/actions/tooltip';

  let {
    stashes,
    onRefresh,
  }: { stashes: StashEntry[]; onRefresh: () => void } = $props();

  const tab = $derived(tabsStore.activeTab);

  async function selectStash(stash: StashEntry) {
    if (!tab) return;
    graphStore.setSelectedStash(stash);
    uiStore.setActiveBottomSection('detail');
    try {
      const files = await getCommitDiff(tab.id, stash.oid);
      diffStore.setFiles(files);
    } catch (err) {
      uiStore.showToast(`Failed to load stash diff: ${err}`, 'error');
    }
  }

  async function apply(stash: StashEntry, e?: MouseEvent) {
    e?.stopPropagation();
    if (!tab) return;
    await applyStashAction(tab.id, stash, onRefresh);
  }

  async function pop(stash: StashEntry, e?: MouseEvent) {
    e?.stopPropagation();
    if (!tab) return;
    await popStashAction(tab.id, stash, onRefresh);
  }

  async function drop(stash: StashEntry, e?: MouseEvent) {
    e?.stopPropagation();
    if (!tab) return;
    await dropStashAction(tab.id, stash, onRefresh);
  }

  // ── Context menu ──────────────────────────────────────────────────────────
  type StashCtx = { x: number; y: number; stash: StashEntry };
  let ctxMenu = $state<StashCtx | null>(null);

  const ctxItems: MenuItem[] = [
    { id: 'apply',  label: 'Apply',  icon: Play,           iconColor: 'var(--success)' },
    { id: 'pop',    label: 'Pop',    icon: CornerDownLeft, iconColor: 'var(--accent)' },
    { id: 'sep1',   label: '', separator: true },
    { id: 'goto',   label: 'Vai allo stash', icon: Crosshair, iconColor: 'var(--color-stash)' },
    { id: 'rename', label: 'Rename', icon: Pencil,         iconColor: '#ffc66d' },
    { id: 'sep2',   label: '', separator: true },
    { id: 'drop',   label: 'Drop',   icon: Trash2, danger: true },
  ];

  function openCtx(e: MouseEvent, stash: StashEntry) {
    e.preventDefault();
    e.stopPropagation();
    ctxMenu = { x: e.clientX, y: e.clientY, stash };
  }

  async function handleCtxSelect(id: string) {
    if (!ctxMenu) return;
    const { stash } = ctxMenu;
    ctxMenu = null;
    if      (id === 'apply')  await apply(stash);
    else if (id === 'pop')    await pop(stash);
    else if (id === 'drop')   await drop(stash);
    else if (id === 'rename') startRename(stash);
    else if (id === 'goto')   gotoStash(stash);
  }

  /** Focus the graph on the commit the stash was created from. Resolves
   *  the parent OID via graphStore's stash-ref list (populated by the
   *  backend `get_graph` response). If the parent commit is beyond the
   *  currently paginated page, the graph's scroll effect transparently
   *  loads the full history before scrolling. Only fails when the stash
   *  ref itself is missing from graph data — which only happens if the
   *  graph for this tab hasn't been loaded yet. */
  async function gotoStash(stash: StashEntry) {
    const ref = graphStore.graphData?.stashes?.find(s => s.index === stash.index);
    // Load the diff panel alongside the scroll so the user lands on the
    // parent commit WITH the stash contents visible in the detail pane.
    await selectStash(stash);
    if (ref?.parentOid) {
      graphStore.scrollToCommit(ref.parentOid);
    } else {
      uiStore.showToast(
        'Grafo non ancora caricato — riprova tra un attimo',
        'warning',
      );
    }
  }

  // ── Inline rename ─────────────────────────────────────────────────────────
  let renamingIndex = $state<number | null>(null);
  let renameValue   = $state('');

  function startRename(stash: StashEntry) {
    renamingIndex = stash.index;
    renameValue   = stash.message;
  }

  function cancelRename() {
    renamingIndex = null;
    renameValue   = '';
  }

  async function confirmRename(stash: StashEntry, msg: string) {
    if (!tab) return;
    renamingIndex = null;
    renameValue   = '';
    if (!msg || msg === stash.message) return;
    try {
      await stashRename(tab.id, stash.index, msg);
      uiStore.showToast('Stash renamed', 'success');
      onRefresh();
    } catch (err) {
      uiStore.showToast(`Rename failed: ${err}`, 'error');
    }
  }
</script>

<div class="stash-list">
  {#each stashes as stash (stash.index)}
    <div
      class="stash-item"
      class:selected={graphStore.selectedStash?.index === stash.index}
      onclick={() => renamingIndex !== stash.index && selectStash(stash)}
      oncontextmenu={(e) => openCtx(e, stash)}
      role="button"
      tabindex="0"
      onkeydown={(e) => e.key === 'Enter' && renamingIndex !== stash.index && selectStash(stash)}
    >
      <Archive size={11} class="stash-icon" />

      <div class="stash-info">
        {#if renamingIndex === stash.index}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div role="none" onclick={(e) => e.stopPropagation()}>
            <InlineEdit bind:value={renameValue} onconfirm={(v) => confirmRename(stash, v)} oncancel={cancelRename} />
          </div>
        {:else}
          <span class="stash-message truncate">{stash.message || `stash@{${stash.index}}`}</span>
          <span class="stash-id">stash@{'{' + stash.index + '}'}</span>
        {/if}
      </div>

      {#if renamingIndex !== stash.index}
        <div class="stash-actions">
          <button class="action-btn" use:tooltip={'Apply stash'} onclick={(e) => apply(stash, e)}>
            <Play size={10} />
          </button>
          <button class="action-btn" use:tooltip={'Pop stash'} onclick={(e) => pop(stash, e)}>
            <CornerDownLeft size={10} />
          </button>
          <button class="action-btn" use:tooltip={'Rename stash'} onclick={(e) => { e.stopPropagation(); startRename(stash); }}>
            <Pencil size={10} />
          </button>
          <button class="action-btn danger" use:tooltip={'Drop stash'} onclick={(e) => drop(stash, e)}>
            <Trash2 size={10} />
          </button>
        </div>
      {/if}
    </div>
  {:else}
    <EmptyState message="No stashes" />
  {/each}
</div>

{#if ctxMenu}
  <ContextMenu
    x={ctxMenu.x}
    y={ctxMenu.y}
    items={ctxItems}
    onSelect={handleCtxSelect}
    onClose={() => ctxMenu = null}
  />
{/if}

<style>
  .stash-list { padding: 2px 0; }

  .stash-item {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 8px 3px 20px;
    font-size: var(--font-size-sm);
    color: var(--text-secondary);
    border-radius: 0;
    margin: 0;
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
    outline: none;
  }

  .stash-item:hover { background: rgba(255,255,255,0.05); color: var(--text-primary); }
  .stash-item.selected { background: rgba(77,120,204,0.18); color: var(--text-primary); }
  .stash-item:hover .stash-actions { opacity: 1; pointer-events: auto; }
  .stash-item.selected .stash-actions { opacity: 1; pointer-events: auto; }

  :global(.stash-icon) { flex-shrink: 0; color: var(--warning); }

  .stash-info {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 1px;
    overflow: hidden;
    min-width: 0;
  }

  .stash-message {
    font-size: var(--font-size-xs);
    color: var(--text-primary);
    font-weight: 400;
  }

  .stash-id {
    font-size: 10px;
    color: var(--text-disabled);
    font-family: var(--font-code);
  }

  .stash-actions {
    display: flex;
    align-items: center;
    gap: 2px;
    opacity: 0;
    pointer-events: none;
    transition: opacity var(--transition-fast);
    flex-shrink: 0;
  }

  .action-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 20px; height: 20px;
    border: none;
    background: transparent;
    color: var(--text-muted);
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .action-btn:hover { background: var(--bg-overlay); color: var(--text-primary); }
  .action-btn.danger:hover { color: var(--error); background: var(--error-subtle); }

</style>
