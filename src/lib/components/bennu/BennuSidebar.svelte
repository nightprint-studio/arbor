<script lang="ts">
  /**
   * BennuSidebar — the Project tool window (the Java project file tree).
   *
   * Fed by `bennu_project_tree` via the project store. Uses the shared `Tree` widget
   * with **controlled expansion** (the UI store owns the expanded-id set) so the
   * header toolbar can Collapse-all / Expand-all and Select-opened-file can reveal a
   * path. Clicking a file opens it in the editor.
   *
   * Header actions (keyboard-reachable, all `.ps-btn`): New file (＋), Select opened
   * file (locate), Collapse all / Expand all (chevrons), and an Options kebab. New
   * file + a couple of Options entries are stubs (toast) until bennu-be serves them.
   *
   * Imports only shared/ui + shared FileExplorerModal + bennu-local store.
   */
  import {
    FolderOpen, Folder, FileCode2, FolderTree, Plus, Crosshair,
    ChevronsDownUp, ChevronsUpDown, MoreVertical,
  } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import Tree from '$lib/components/shared/ui/Tree.svelte';
  import type { RowSnippetCtx } from '$lib/components/shared/ui/Tree.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import type { TreeNode } from '$lib/types/bennu';

  let pickerOpen = $state(false);
  let filter = $state('');
  let treeBodyEl = $state<HTMLDivElement | null>(null);

  // The store's tree root is a single dir node; render its children as the top level
  // so the project folder itself isn't an extra nesting level.
  const rootChildren = $derived<TreeNode[]>(projectStore.tree?.children ?? []);

  async function openProject(dir: string) {
    pickerOpen = false;
    try { await projectStore.openProject(dir); }
    catch { /* mock fallback already applied by the store */ }
  }

  function onRowSelect(node: TreeNode) {
    if (!node.is_dir) void projectStore.openFile(node.path);
  }

  // ── Controlled expansion (UI store owns the set) ─────────────────────────────
  function onExpandToggle(id: string, next: boolean) {
    bennuUiStore.setExpanded(id, next);
  }

  /** Every directory path in the (sub)tree — used by Expand-all. */
  function allDirIds(nodes: TreeNode[]): string[] {
    const out: string[] = [];
    const walk = (n: TreeNode) => {
      if (n.is_dir) { out.push(n.path); for (const c of n.children) walk(c); }
    };
    for (const n of nodes) walk(n);
    return out;
  }

  function collapseAll() { bennuUiStore.collapseAllTree(); }
  function expandAll() { bennuUiStore.expandTreeIds(allDirIds(rootChildren)); }

  /** Ancestor directory paths of a file path within the project tree. */
  function ancestorsOf(filePath: string): string[] {
    const out: string[] = [];
    const walk = (n: TreeNode, trail: string[]): boolean => {
      if (n.path === filePath) return true;
      if (!n.is_dir) return false;
      for (const c of n.children) {
        if (walk(c, [...trail, n.path])) { out.push(...trail, n.path); return true; }
      }
      return false;
    };
    for (const n of rootChildren) walk(n, []);
    return out;
  }

  function revealActive() {
    const path = projectStore.activeFilePath;
    if (!path) return;
    bennuUiStore.expandTreeIds(ancestorsOf(path));
    scrollToActive();
  }

  function scrollToActive() {
    queueMicrotask(() => {
      const row = treeBodyEl?.querySelector<HTMLElement>('.tree-row-selected');
      row?.scrollIntoView({ block: 'center' });
    });
  }

  // The toolbar's Select-opened-file + the palette both bump this relay.
  let lastReveal = 0;
  $effect(() => {
    const n = bennuUiStore.revealNonce;
    if (n !== lastReveal) { lastReveal = n; revealActive(); }
  });

  // ── Options kebab ────────────────────────────────────────────────────────────
  const optionsMenu: DropdownItem[] = [
    { kind: 'item', id: 'expand',   label: 'Expand all',   icon: ChevronsUpDown,   onclick: expandAll },
    { kind: 'item', id: 'collapse', label: 'Collapse all', icon: ChevronsDownUp,   onclick: collapseAll },
    { kind: 'separator' },
    { kind: 'item', id: 'reveal',   label: 'Select opened file', icon: Crosshair, onclick: revealActive },
    { kind: 'separator' },
    { kind: 'item', id: 'newfile',  label: 'New file…', icon: Plus,
      onclick: () => toastStore.show("Creating files isn't implemented yet.", 'info') },
  ];
</script>

<PanelShell title="Project">
  {#snippet icon()}<FolderTree size={13} />{/snippet}
  {#snippet actions()}
    <button class="ps-btn" type="button" onclick={() => toastStore.show("Creating files isn't implemented yet.", 'info')} use:tooltip={'New file'} aria-label="New file">
      <Plus size={14} />
    </button>
    <button class="ps-btn" type="button" onclick={revealActive} disabled={!projectStore.activeFilePath} use:tooltip={'Select opened file'} aria-label="Select opened file">
      <Crosshair size={14} />
    </button>
    <button class="ps-btn" type="button" onclick={collapseAll} use:tooltip={'Collapse all'} aria-label="Collapse all">
      <ChevronsDownUp size={14} />
    </button>
    <button class="ps-btn" type="button" onclick={expandAll} use:tooltip={'Expand all'} aria-label="Expand all">
      <ChevronsUpDown size={14} />
    </button>
    <Dropdown items={optionsMenu} position="fixed" direction="down" width="200px">
      {#snippet trigger({ open, toggle })}
        <button class="ps-btn" class:ps-btn-active={open} type="button" onclick={toggle} use:tooltip={'Options'} aria-label="Project options" aria-haspopup="menu" aria-expanded={open}>
          <MoreVertical size={14} />
        </button>
      {/snippet}
    </Dropdown>
  {/snippet}

  {#if projectStore.project}
    <div class="bs-filter">
      <SearchBar bind:query={filter} placeholder="Filter files…" showRegex={false} showCounter={false} />
    </div>
    <div class="bs-tree" bind:this={treeBodyEl}>
      <Tree
        nodes={rootChildren}
        getId={(n) => n.path}
        getChildren={(n) => (n.is_dir ? n.children : undefined)}
        selectedId={projectStore.activeFilePath}
        expandedIds={bennuUiStore.treeExpanded}
        {onExpandToggle}
        {filter}
        ariaLabel="Project files"
        onSelect={onRowSelect}
      >
        {#snippet row(ctx: RowSnippetCtx<TreeNode>)}
          <span class="tree-icon">
            {#if ctx.node.is_dir}
              <Folder size={14} />
            {:else}
              <FileCode2 size={14} />
            {/if}
          </span>
          <span class="tree-label">{ctx.node.name}</span>
        {/snippet}
      </Tree>
    </div>
  {:else}
    <div class="bs-empty">
      <EmptyState message="No project open." />
      <button class="bs-empty-action" type="button" onclick={() => (pickerOpen = true)}>
        <FolderOpen size={14} /> Open project…
      </button>
    </div>
  {/if}
</PanelShell>

{#if pickerOpen}
  <FileExplorerModal
    mode="folder"
    title="Open Java project"
    onConfirm={openProject}
    onCancel={() => (pickerOpen = false)}
    onClose={() => (pickerOpen = false)}
  />
{/if}

<style>
  .bs-filter { padding: 6px 8px 4px; }

  .bs-tree {
    flex: 1; min-height: 0;
    overflow-y: auto;
    padding: 2px 0 6px;
  }

  .bs-empty {
    flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center;
    gap: 10px; min-height: 0; padding: 12px;
  }
  .bs-empty-action {
    display: inline-flex; align-items: center; gap: 6px;
    height: 28px; padding: 0 12px;
    background: var(--bg-input); border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md); color: var(--text-secondary);
    font-size: 12px; cursor: pointer;
    transition: border-color var(--transition-fast), color var(--transition-fast);
  }
  .bs-empty-action:hover { border-color: var(--border-focus, var(--accent)); color: var(--text-primary); }

  .tree-icon { display: flex; align-items: center; color: var(--text-muted); }
  .tree-label { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
