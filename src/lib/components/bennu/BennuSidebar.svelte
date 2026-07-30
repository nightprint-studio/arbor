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
    Copy, LocateFixed, ChevronDown, ChevronRight,
    Box, CircleDashed, Rows3, AtSign,
    Braces, Hash, FileCog, FileText, Database, Globe,
  } from 'lucide-svelte';
  import { tick } from 'svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import Tree from '$lib/components/shared/ui/Tree.svelte';
  import type { RowSnippetCtx } from '$lib/components/shared/ui/Tree.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import BennuFilterBar from './BennuFilterBar.svelte';
  import BennuNewFileModal from './BennuNewFileModal.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuIndexStore } from '$lib/stores/bennu/index.svelte';
  import { bennuContextMenuStore } from '$lib/stores/bennu/contextmenu.svelte';
  import type { MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import type { TreeNode } from '$lib/types/bennu';

  let pickerOpen = $state(false);
  let filter = $state('');
  // The Tree instance — for its imperative `scrollToId` (the tree is virtualized, so a
  // DOM `scrollIntoView` on the selected row can't work when it's off-screen).
  let treeRef = $state<{ scrollToId: (id: string, block?: 'center' | 'nearest') => void } | null>(null);

  // The store's tree root is a single dir node; render its children as the top level
  // so the project folder itself isn't an extra nesting level.
  const rootChildren = $derived<TreeNode[]>(projectStore.tree?.children ?? []);

  // ── File-tree icons (IntelliJ-style) ─────────────────────────────────────────
  // Per-`.java`-file type kind (class/interface/enum/record/annotation), from the project's class
  // index. Fetched into a plain map (the class index is async + cached) and refreshed when the index
  // rebuilds. A file that declares several types keys off the one matching the file name (the public
  // top-level type).
  let kindByFile = $state<Map<string, string>>(new Map());
  $effect(() => {
    const root = projectStore.project?.root;
    // Re-fetch when a rebuild lands (the store drops its class cache on rebuild).
    void bennuIndexStore.buildRevision;
    if (!root) { kindByFile = new Map(); return; }
    let cancelled = false;
    void bennuIndexStore.classesForRoot(root).then((classes) => {
      if (cancelled) return;
      const m = new Map<string, string>();
      for (const c of classes) {
        const key = c.file.replace(/\\/g, '/');
        const stem = key.split('/').pop()?.replace(/\.java$/, '') ?? '';
        // Prefer the primary (file-named) type; otherwise keep the first seen.
        if (!m.has(key) || c.simple === stem) m.set(key, c.kind);
      }
      kindByFile = m;
    }).catch(() => {});
    return () => { cancelled = true; };
  });

  /** Icon + color for a Java type kind — distinct glyphs so class/interface/enum/annotation read at
   *  a glance (IntelliJ-style). */
  const KIND_ICON: Record<string, { icon: typeof FileCode2; color: string }> = {
    class:      { icon: Box,          color: 'var(--success)' },
    interface:  { icon: CircleDashed, color: 'var(--info)' },
    enum:       { icon: Rows3,        color: 'var(--warning)' },
    record:     { icon: Box,          color: 'var(--info)' },
    annotation: { icon: AtSign,       color: 'var(--color-tag, #c792ea)' },
  };

  /** Icon + color by file extension (non-Java files) — lucide glyphs tinted with each language's
   *  brand color for IntelliJ-like recognition (JS yellow, CSS blue, HTML orange, JSP server-orange).
   *  Lucide has no official brand LOGOS; these are the closest glyphs + the real brand hues. */
  const EXT_ICON: Record<string, { icon: typeof FileCode2; color: string }> = {
    // JSP / JSP fragments / tag files — Java server pages (server-side orange).
    jsp:  { icon: FileCode2, color: '#e76f00' },
    jspf: { icon: FileCode2, color: '#e76f00' },
    jspx: { icon: FileCode2, color: '#e76f00' },
    tag:  { icon: FileCode2, color: '#e76f00' },
    // JavaScript / TypeScript.
    js:   { icon: Braces, color: '#f7df1e' },
    mjs:  { icon: Braces, color: '#f7df1e' },
    cjs:  { icon: Braces, color: '#f7df1e' },
    ts:   { icon: Braces, color: '#3178c6' },
    // Stylesheets.
    css:  { icon: Hash, color: '#2965f1' },
    scss: { icon: Hash, color: '#cf649a' },
    less: { icon: Hash, color: '#1d365d' },
    // Markup / data.
    html: { icon: Globe, color: '#e34f26' },
    htm:  { icon: Globe, color: '#e34f26' },
    xml:  { icon: FileCode2, color: 'var(--text-muted)' },
    json: { icon: Braces, color: 'var(--warning)' },
    // Config / docs / data.
    properties: { icon: FileCog, color: 'var(--text-muted)' },
    yml:  { icon: FileCog, color: '#cb171e' },
    yaml: { icon: FileCog, color: '#cb171e' },
    sql:  { icon: Database, color: 'var(--info)' },
    md:   { icon: FileText, color: 'var(--text-muted)' },
    txt:  { icon: FileText, color: 'var(--text-muted)' },
  };

  /** The folder-icon tint by source-root role — main (blue) / test (green) / resources (amber) /
   *  webapp (purple), so the tree conveys main/test/resource context at a glance. */
  function folderColor(path: string): string {
    const p = path.replace(/\\/g, '/');
    if (/\/src\/(main|test)\/resources(\/|$)/.test(p)) return 'var(--warning)';
    if (/\/src\/test(\/|$)/.test(p)) return 'var(--success)';
    if (/\/src\/main\/webapp(\/|$)/.test(p)) return 'var(--color-tag, #c792ea)';
    if (/\/src\/main(\/|$)/.test(p)) return 'var(--info)';
    return 'var(--text-muted)';
  }

  /** The icon + color for a tree node: a source-root-tinted folder, a Java kind glyph, or the
   *  default file icon. */
  function iconFor(node: TreeNode): { icon: typeof FileCode2; color: string } {
    if (node.is_dir) return { icon: Folder, color: folderColor(node.path) };
    const path = node.path.replace(/\\/g, '/');
    if (path.endsWith('.java')) {
      const meta = KIND_ICON[kindByFile.get(path) ?? ''];
      // A `.java` whose kind isn't indexed yet (index still building) → a neutral code icon.
      return meta ?? { icon: FileCode2, color: 'var(--info)' };
    }
    const ext = path.split('.').pop()?.toLowerCase() ?? '';
    return EXT_ICON[ext] ?? { icon: FileCode2, color: 'var(--text-muted)' };
  }

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

  /** Select-opened-file: expand the active file's ancestor folders, then scroll its
   *  row into view. `await tick()` lets the newly-expanded rows land in the Tree's
   *  flat list before `scrollToId` computes the row index. */
  async function revealActive() {
    const path = projectStore.activeFilePath;
    if (!path) return;
    bennuUiStore.expandTreeIds(ancestorsOf(path));
    await tick();
    treeRef?.scrollToId(path);
  }

  // The toolbar's Select-opened-file + the palette both bump this relay.
  let lastReveal = 0;
  $effect(() => {
    const n = bennuUiStore.revealNonce;
    if (n !== lastReveal) { lastReveal = n; void revealActive(); }
  });

  /** Reveal a file in the tree: open it (so it becomes the selected row), expand its
   *  ancestor folders, and scroll the selected row into view. */
  async function revealPath(path: string) {
    await projectStore.openFile(path);
    bennuUiStore.expandTreeIds(ancestorsOf(path));
    await tick();
    treeRef?.scrollToId(path);
  }

  // ── Right-click context menu (read-only; FS mutations are a later wave) ──────
  /** `path` relative to the project root, forward slashes. Falls back to the
   *  absolute path when it isn't under the root. */
  function relativePath(path: string): string {
    const root = projectStore.project?.root;
    const fwd = path.replace(/\\/g, '/');
    if (!root) return fwd;
    const rootFwd = root.replace(/\\/g, '/').replace(/\/+$/, '');
    if (fwd === rootFwd) return '.';
    const prefix = rootFwd + '/';
    return fwd.startsWith(prefix) ? fwd.slice(prefix.length) : fwd;
  }

  function copyText(text: string) {
    void navigator.clipboard?.writeText(text).catch(() => { /* clipboard denied — ignore */ });
  }

  function onRowContextMenu(node: TreeNode, e: MouseEvent) {
    // "New file…" creates in this directory (dir node) or the file's directory (file node).
    const newDir = node.is_dir ? node.path : parentDir(node.path);
    const items: MenuItem[] = node.is_dir
      ? [
          { id: 'new',           label: 'New file…',          icon: Plus },
          { separator: true, id: 'sep-new', label: '' },
          { id: 'copy-path',     label: 'Copy path',          icon: Copy },
          { id: 'copy-rel',      label: 'Copy relative path', icon: Copy },
          { separator: true, id: 'sep-dir', label: '' },
          bennuUiStore.isExpanded(node.path)
            ? { id: 'collapse', label: 'Collapse', icon: ChevronRight }
            : { id: 'expand',   label: 'Expand',   icon: ChevronDown },
        ]
      : [
          { id: 'open',          label: 'Open',               icon: FolderOpen },
          { id: 'new',           label: 'New file…',          icon: Plus },
          { separator: true, id: 'sep-file', label: '' },
          { id: 'copy-path',     label: 'Copy path',          icon: Copy },
          { id: 'copy-rel',      label: 'Copy relative path', icon: Copy },
          { id: 'reveal',        label: 'Reveal in Project',  icon: LocateFixed },
        ];
    bennuContextMenuStore.show(e.clientX, e.clientY, items, (id) => {
      switch (id) {
        case 'new':       newFileDir = newDir; break;
        case 'open':      void projectStore.openFile(node.path); break;
        case 'copy-path': copyText(node.path); break;
        case 'copy-rel':  copyText(relativePath(node.path)); break;
        case 'reveal':    void revealPath(node.path); break;
        case 'expand':    bennuUiStore.setExpanded(node.path, true); break;
        case 'collapse':  bennuUiStore.setExpanded(node.path, false); break;
      }
    });
  }

  /** Parent directory of a path (forward-slash aware). */
  function parentDir(p: string): string {
    return p.replace(/[\\/][^\\/]*$/, '') || p;
  }

  /** Where the header / kebab "New file…" creates: the active file's directory, else the root. */
  function defaultNewDir(): string {
    const active = projectStore.activeFilePath;
    return active ? parentDir(active) : (projectStore.project?.root ?? '');
  }

  // The directory a New-file modal is open for (null = closed).
  let newFileDir = $state<string | null>(null);

  // ── Options kebab ────────────────────────────────────────────────────────────
  const optionsMenu: DropdownItem[] = [
    { kind: 'item', id: 'expand',   label: 'Expand all',   icon: ChevronsUpDown,   onclick: expandAll },
    { kind: 'item', id: 'collapse', label: 'Collapse all', icon: ChevronsDownUp,   onclick: collapseAll },
    { kind: 'separator' },
    { kind: 'item', id: 'reveal',   label: 'Select opened file', icon: Crosshair, onclick: revealActive },
    { kind: 'separator' },
    { kind: 'item', id: 'newfile',  label: 'New file…', icon: Plus,
      onclick: () => { newFileDir = defaultNewDir(); } },
  ];
</script>

<PanelShell title="Project">
  {#snippet icon()}<FolderTree size={13} />{/snippet}
  {#snippet actions()}
    <button class="ps-btn" type="button" onclick={() => { newFileDir = defaultNewDir(); }} disabled={!projectStore.project} use:tooltip={'New file'} aria-label="New file">
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
    {#snippet toolbar()}
      <BennuFilterBar bind:query={filter} placeholder="Filter files…" />
    {/snippet}
  {/if}

  {#if projectStore.project}
    <div class="bs-tree">
      <Tree
        bind:this={treeRef}
        nodes={rootChildren}
        getId={(n) => n.path}
        getChildren={(n) => (n.is_dir ? n.children : undefined)}
        selectedId={projectStore.activeFilePath}
        expandedIds={bennuUiStore.treeExpanded}
        {onExpandToggle}
        {filter}
        ariaLabel="Project files"
        onSelect={onRowSelect}
        onContextMenu={onRowContextMenu}
      >
        {#snippet row(ctx: RowSnippetCtx<TreeNode>)}
          {@const meta = iconFor(ctx.node)}
          {@const Icon = meta.icon}
          <span class="tree-icon" style="color: {meta.color}">
            <Icon size={14} />
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

{#if newFileDir !== null}
  <BennuNewFileModal dir={newFileDir} onClose={() => (newFileDir = null)} />
{/if}

<style>
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
    font-size: var(--font-size-sm); cursor: pointer;
    transition: border-color var(--transition-fast), color var(--transition-fast);
  }
  .bs-empty-action:hover { border-color: var(--border-focus, var(--accent)); color: var(--text-primary); }

  .tree-icon { display: flex; align-items: center; color: var(--text-muted); }
  .tree-label { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
