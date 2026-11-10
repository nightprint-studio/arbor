<script lang="ts">
  /**
   * ProjectTree — the "Project" tool window: a lazily-listed file tree over a root folder, with the
   * file-manager verbs every product wants (open, new, rename, duplicate, copy/cut/paste, delete,
   * reveal, copy path, drag a file into a folder), git marks, a filter, and keyboard for all of it.
   *
   * It is backed by the generic filesystem commands (`fs_read_dir` & co.), so it needs no product
   * backend. A product supplies only what is genuinely its own: which entries to show
   * (`entryFilter`), what a file's icon and trailing decoration are, the "New" entries, extra
   * menu entries, and what to do after the tree changed something on disk (`onRenamed`, `onDeleted`,
   * `onMoved`) so its own models (open tabs, caches) follow.
   *
   * Extension points (everything a product needs that is not the file-manager core): `loadDir` (its own
   * listing, with `ignored` / `hidden` / `data` per entry), `transformChildren` (collapse package chains),
   * `expanded` (bindable, to persist it), `actionOverrides` / `hiddenActions` (take over or drop a built-in
   * verb), `extraMenu` (own entries, also for the empty area), `onDrop`, `headerActions`, `rowExtra`,
   * `iconFor`, `gitBadge`, plus `reveal(path)` / `reload()` on the instance.
   *
   * Keyboard: arrows / Enter (Tree), F2 rename, Delete delete, Ctrl+C / Ctrl+X / Ctrl+V,
   * Ctrl+D duplicate, Shift+F10 / Menu key opens the row menu.
   */
  import type { Component, Snippet } from 'svelte';
  import { tick, untrack } from 'svelte';
  import {
    FolderTree, Plus, Crosshair, ChevronsDownUp, ChevronsUpDown, FolderPlus, Copy, Scissors, ClipboardPaste, Trash2,
    FilePen, ExternalLink, CopyPlus, FileText, Link,
  } from 'lucide-svelte';
  import IconifyIconView from '@iconify/svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import Tree from '$lib/components/shared/ui/Tree.svelte';
  import type { RowSnippetCtx, TreeController } from '$lib/components/shared/ui/Tree.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import FilterInput from '$lib/components/shared/ui/FilterInput.svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import ContextMenu, { type MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { getFileIcon, getFolderIcon } from '$lib/utils/file-icons';
  import { GIT_BADGE_COLOR, GIT_BADGE_GLYPH, GIT_BADGE_LABEL } from '$lib/utils/git-badge';
  import { revealFile } from '$lib/utils/reveal';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { fsCopy, fsMove, fsDuplicate, fsRename, fsCreateDir, fsTrash, type FsEntry, type GitBadge } from '$lib/ipc/fs';
  import NamePromptModal from './NamePromptModal.svelte';
  import {
    type ProjectNode, type ProjectEntry, type Listing, type TransformChildren,
    buildNodes, listDir, parentDir, joinPath, ancestorsBelow,
  } from './project-tree-model';

  /** A product-supplied "New" entry. `run` gets the folder the new thing belongs in. */
  export interface ProjectTreeNewItem {
    id: string;
    label: string;
    icon?: Component<any>;
    run: (dir: string) => void;
  }
  /** Built-in verbs a product can take over (`actionOverrides`) or hide (`hiddenActions`). */
  export type ProjectTreeActionId = 'open' | 'new-folder' | 'rename' | 'duplicate' | 'copy' | 'cut' | 'paste' | 'copy-path' | 'reveal' | 'delete';
  /** A product override for a row's icon (a lucide glyph tinted by `color`). */
  export interface ProjectTreeIcon { icon: Component<any>; color?: string }

  interface Props {
    /** The folder the tree is rooted at; `null` shows `emptyMessage`. */
    root: string | null;
    title?: string;
    /** The open file — the row the tree follows and "Select opened file" reveals. */
    activePath?: string | null;
    /** Which entries to list (folders included). Default: everything not hidden. */
    entryFilter?: (e: FsEntry) => boolean;
    /** Replace the filesystem listing with the product's own source (flags like `ignored`, a cap marker…). */
    loadDir?: (dir: string) => Promise<ProjectEntry[]>;
    /** Reshape a folder's children after they are built — e.g. collapse a package chain into one row. */
    transformChildren?: TransformChildren;
    /** Open folders. Bind it to persist expansion per project; left alone the tree keeps its own. */
    expanded?: Set<string>;
    /** Take over a built-in verb (rename with refactor, delete with a usage check…). */
    actionOverrides?: Partial<Record<ProjectTreeActionId, (node: ProjectNode | null) => void>>;
    /** Hide built-in verbs the product cannot honour. */
    hiddenActions?: ProjectTreeActionId[];
    /** A file was dropped on a folder; the product decides (confirm, refactor). Default: plain move. */
    onDrop?: (src: ProjectNode, dst: ProjectNode) => void;
    /** Extra header buttons, after the built-in ones. */
    headerActions?: Snippet;
    /** Change this value to make the tree re-list every folder it has fetched. */
    reloadToken?: unknown;
    onOpen: (path: string) => void;
    newItems?: ProjectTreeNewItem[];
    /** Extra context-menu entries for a row, listed above the generic ones. */
    extraMenu?: (node: ProjectNode | null) => { items: MenuItem[]; run: (id: string) => void } | null;
    iconFor?: (node: ProjectNode) => ProjectTreeIcon | undefined;
    /** Git mark of a row (see `$lib/stores/git-status.svelte`). */
    gitBadge?: (path: string, isDir: boolean) => GitBadge | undefined;
    /** Product rule for a typed rename (e.g. force an extension). */
    renameTransform?: (name: string, node: ProjectNode) => string;
    onRenamed?: (from: string, to: string) => void | Promise<void>;
    onDeleted?: (path: string) => void | Promise<void>;
    onMoved?: (from: string, to: string) => void | Promise<void>;
    emptyMessage?: string;
    /** Trailing per-row decoration (badges, a dimmed subtitle). */
    rowExtra?: Snippet<[ProjectNode]>;
  }

  let {
    root, title = 'Project', activePath = null, entryFilter, loadDir, transformChildren,
    expanded = $bindable(new Set<string>()), actionOverrides, hiddenActions = [], onDrop, headerActions, reloadToken, onOpen, newItems = [], extraMenu,
    iconFor, gitBadge, renameTransform, onRenamed, onDeleted, onMoved,
    emptyMessage = 'No project open.', rowExtra,
  }: Props = $props();

  // ── Listing ──────────────────────────────────────────────────────────────────
  let listing = $state.raw<Listing>({});
  let filter = $state('');
  let treeRef = $state<TreeController | null>(null);

  const nodes = $derived(root ? buildNodes(root, listing, transformChildren) : []);
  const fetchDir = (dir: string) => (loadDir ? loadDir(dir) : listDir(dir, entryFilter));

  async function load(dir: string) {
    const entries = await fetchDir(dir);
    listing = { ...listing, [dir]: entries };
  }
  async function reloadAll() {
    if (!root) return;
    const dirs = Object.keys(listing);
    const fetched = await Promise.all(dirs.map(async (d) => [d, await fetchDir(d)] as const));
    listing = Object.fromEntries(fetched);
  }

  // A new root starts clean; the root listing is the only thing fetched eagerly.
  $effect(() => {
    const r = root;
    untrack(() => { listing = {}; });
    if (r) void load(r);
  });
  // A folder that is open but not listed yet — its expansion was restored by the product — is fetched.
  // Each fetch that lands may reveal the next open folder down, which runs this again.
  $effect(() => {
    if (!root) return;
    for (const dir of expanded) if (dir !== root && listing[dir] === undefined && !loading.has(dir)) {
      loading.add(dir);
      void load(dir).finally(() => loading.delete(dir));
    }
  });
  const loading = new Set<string>();
  $effect(() => { void reloadToken; untrack(() => void reloadAll()); });

  function onExpandToggle(id: string, next: boolean) {
    const s = new Set(expanded);
    if (next) { s.add(id); if (listing[id] === undefined) void load(id); } else s.delete(id);
    expanded = s;
  }

  const revealActive = () => (activePath ? reveal(activePath) : Promise.resolve());
  const collapseAll = () => { expanded = new Set(); };

  /** Open every folder, fetching what is missing, down to a depth that keeps a huge tree from loading whole. */
  async function expandAll(maxDepth = 6) {
    if (!root) return;
    const open = new Set<string>();
    let level = [root];
    for (let depth = 0; depth < maxDepth && level.length; depth++) {
      const next: string[] = [];
      for (const dir of level) {
        if (listing[dir] === undefined) await load(dir);
        for (const e of listing[dir] ?? []) if (e.is_dir) { open.add(e.path); next.push(e.path); }
      }
      level = next;
    }
    expanded = open;
  }

  /** Open the ancestors of `path`, scroll to it and (optionally) put keyboard focus on its row. */
  export async function reveal(path: string, focus = false) {
    if (!root) return;
    const dirs = ancestorsBelow(root, path);
    for (const dir of dirs) if (listing[dir] === undefined) await load(dir);
    expanded = new Set([...expanded, ...dirs]);
    await tick();
    treeRef?.scrollToId(path, 'center');
    if (focus) await treeRef?.focusId(path);
  }
  /** Re-list every folder fetched so far. */
  export const reload = () => reloadAll();

  // ── Clipboard (copy / cut / paste) and moves ─────────────────────────────────
  let clip = $state<{ path: string; cut: boolean } | null>(null);

  /** Re-list the folders touched by an operation (only the ones already fetched). */
  async function afterChange(...dirs: string[]) {
    await Promise.all([...new Set(dirs.filter((d) => listing[d] !== undefined))].map(load));
  }

  async function paste(into: ProjectNode | null) {
    if (!clip || !root) return;
    const dir = into ? (into.is_dir ? into.path : parentDir(into.path)) : root;
    const { path, cut } = clip;
    try {
      const out = cut ? await fsMove([path], dir) : await fsCopy([path], dir);
      if (cut) { clip = null; for (const to of out) await onMoved?.(path, to); }
      await afterChange(dir, parentDir(path));
    } catch (e) { toastStore.show(`Paste failed: ${e}`, 'error'); }
  }

  async function duplicate(node: ProjectNode) {
    try { await fsDuplicate([node.path]); await afterChange(parentDir(node.path)); }
    catch (e) { toastStore.show(`Duplicate failed: ${e}`, 'error'); }
  }

  async function dropOn(src: ProjectNode, dst: ProjectNode) {
    if (parentDir(src.path) === dst.path) return;
    try {
      const out = await fsMove([src.path], dst.path);
      for (const to of out) await onMoved?.(src.path, to);
      expanded = new Set([...expanded, dst.path]);
      await afterChange(dst.path, parentDir(src.path));
    } catch (e) { toastStore.show(`Move failed: ${e}`, 'error'); }
  }

  // ── Rename / new folder / delete ─────────────────────────────────────────────
  let renaming = $state<ProjectNode | null>(null);
  let newFolderIn = $state<string | null>(null);
  let deleting = $state<ProjectNode | null>(null);

  async function doRename(node: ProjectNode, typed: string) {
    const name = renameTransform ? renameTransform(typed, node) : typed;
    const to = joinPath(parentDir(node.path), name);
    if (to === node.path) return;
    await fsRename(node.path, to);
    await onRenamed?.(node.path, to);
    await afterChange(parentDir(node.path));
  }
  async function doNewFolder(dir: string, name: string) {
    await fsCreateDir(joinPath(dir, name));
    expanded = new Set([...expanded, dir]);
    await afterChange(dir);
  }
  async function confirmDelete() {
    const node = deleting;
    if (!node) return;
    deleting = null;
    try {
      await fsTrash([node.path]);
      await onDeleted?.(node.path);
      await afterChange(parentDir(node.path));
    } catch (e) { toastStore.show(`Delete failed: ${e}`, 'error'); }
  }

  // ── Context menu ─────────────────────────────────────────────────────────────
  let menu = $state<{ x: number; y: number; node: ProjectNode | null } | null>(null);
  const targetDir = (node: ProjectNode | null) => (!root ? '' : !node ? root : node.is_dir ? node.path : parentDir(node.path));

  const menuItems = $derived.by<MenuItem[]>(() => {
    if (!menu) return [];
    const node = menu.node;
    const extra = extraMenu ? extraMenu(node) : null;
    const items: MenuItem[] = [
      ...(extra?.items ?? []),
      ...(extra?.items.length ? [{ id: 'sep0', label: '', separator: true }] : []),
      ...(node && !node.is_dir ? [{ id: 'open', label: 'Open', icon: FileText }] : []),
      ...newItems.map((n) => ({ id: `new:${n.id}`, label: `New ${n.label}…`, icon: n.icon ?? Plus })),
      { id: 'new-folder', label: 'New folder…', icon: FolderPlus },
      ...(node ? [
        { id: 'sep1', label: '', separator: true },
        { id: 'rename', label: 'Rename…', icon: FilePen, shortcut: 'F2' },
        { id: 'duplicate', label: 'Duplicate', icon: CopyPlus, shortcut: 'Ctrl+D' },
        { id: 'copy', label: 'Copy', icon: Copy, shortcut: 'Ctrl+C' },
        { id: 'cut', label: 'Cut', icon: Scissors, shortcut: 'Ctrl+X' },
      ] : []),
      { id: 'paste', label: 'Paste', icon: ClipboardPaste, shortcut: 'Ctrl+V', disabled: !clip },
      ...(node ? [
        { id: 'sep2', label: '', separator: true },
        { id: 'copy-path', label: 'Copy path', icon: Link },
        { id: 'reveal', label: 'Reveal in File Explorer', icon: ExternalLink },
        { id: 'sep3', label: '', separator: true },
        { id: 'delete', label: 'Delete…', icon: Trash2, danger: true, shortcut: 'Del' },
      ] : []),
    ];
    return tidy(items.filter((i) => !hiddenActions.includes(i.id as ProjectTreeActionId)));
  });

  /** Drop separators that ended up leading, trailing or doubled once verbs were hidden. */
  function tidy(items: MenuItem[]): MenuItem[] {
    const out: MenuItem[] = [];
    for (const i of items) {
      if (i.separator && (out.length === 0 || out[out.length - 1].separator)) continue;
      out.push(i);
    }
    while (out.length && out[out.length - 1].separator) out.pop();
    return out;
  }

  function runAction(id: string, node: ProjectNode | null) {
    const extra = extraMenu ? extraMenu(node) : null;
    const takeOver = actionOverrides?.[id as ProjectTreeActionId];
    if (takeOver) { takeOver(node); return; }
    if (extra?.items.some((i) => i.id === id)) { extra.run(id); return; }
    if (id.startsWith('new:')) { newItems.find((n) => `new:${n.id}` === id)?.run(targetDir(node)); return; }
    switch (id) {
      case 'open': if (node) onOpen(node.path); break;
      case 'new-folder': newFolderIn = targetDir(node); break;
      case 'rename': if (node) renaming = node; break;
      case 'duplicate': if (node) void duplicate(node); break;
      case 'copy': if (node) clip = { path: node.path, cut: false }; break;
      case 'cut': if (node) clip = { path: node.path, cut: true }; break;
      case 'paste': void paste(node); break;
      case 'copy-path': if (node) void copyToClipboard(node.path); break;
      case 'reveal': if (node) void revealFile(node.path).catch((e) => toastStore.show(`Could not reveal: ${e}`, 'error')); break;
      case 'delete': if (node) deleting = node; break;
    }
  }
  function onMenuSelect(id: string) {
    const node = menu?.node ?? null;
    menu = null;
    runAction(id, node);
  }

  function onRowKeydown(node: ProjectNode, e: KeyboardEvent) {
    const mod = e.ctrlKey || e.metaKey;
    const key = e.key.toLowerCase();
    let id: string | null = null;
    if (e.key === 'F2') id = 'rename';
    else if (e.key === 'Delete') id = 'delete';
    else if (mod && key === 'c') id = 'copy';
    else if (mod && key === 'x') id = 'cut';
    else if (mod && key === 'v') id = 'paste';
    else if (mod && key === 'd') id = 'duplicate';
    else if (e.key === 'ContextMenu' || (e.shiftKey && e.key === 'F10')) {
      e.preventDefault();
      const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
      menu = { x: r.left + 24, y: r.bottom, node };
      return;
    }
    if (!id) return;
    e.preventDefault();
    runAction(id, node);
  }

  // ── Header "New" dropdown ────────────────────────────────────────────────────
  const newDropdown = $derived<DropdownItem[]>([
    ...newItems.map((n) => ({
      kind: 'item' as const, id: n.id, label: n.label, icon: n.icon ?? Plus,
      onclick: () => n.run(root ?? ''),
    })),
    { kind: 'item' as const, id: 'folder', label: 'Folder…', icon: FolderPlus, onclick: () => { if (root) newFolderIn = root; } },
  ]);

  const deleteDetail = $derived(deleting?.is_dir ? 'The folder and everything in it moves to the Recycle Bin.' : 'The file moves to the Recycle Bin.');
</script>

<PanelShell {title}>
  {#snippet icon()}<FolderTree size={13} />{/snippet}
  {#snippet actions()}
    <Dropdown items={newDropdown} position="fixed" direction="down" width="200px">
      {#snippet trigger({ open, toggle })}
        <IconButton tooltip="New…" active={open} ariaHasPopup ariaExpanded={open} disabled={!root} onclick={toggle}><Plus size={14} /></IconButton>
      {/snippet}
    </Dropdown>
    <IconButton tooltip="Select opened file" onclick={revealActive} disabled={!activePath}><Crosshair size={14} /></IconButton>
    <IconButton tooltip="Collapse all" onclick={collapseAll}><ChevronsDownUp size={14} /></IconButton>
    <IconButton tooltip="Expand all" onclick={() => void expandAll()} disabled={!root}><ChevronsUpDown size={14} /></IconButton>
    {@render headerActions?.()}
  {/snippet}
  {#if root}
    {#snippet toolbar()}
      <FilterInput bind:value={filter} placeholder="Filter files…" />
    {/snippet}
  {/if}

  {#if root}
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div
      class="pt-tree"
      role="presentation"
      oncontextmenu={(e) => { if (e.target === e.currentTarget) { e.preventDefault(); menu = { x: e.clientX, y: e.clientY, node: null }; } }}
    >
      <Tree
        bind:this={treeRef}
        {nodes}
        rowHeight={26}
        getId={(n) => n.path}
        getChildren={(n) => (n.is_dir ? n.children : undefined)}
        hasChildren={(n) => n.is_dir}
        selectedId={activePath}
        contextId={menu?.node?.path ?? null}
        expandedIds={expanded}
        {onExpandToggle}
        {filter}
        ariaLabel={title}
        onSelect={(n) => { if (!n.is_dir) onOpen(n.path); }}
        onContextMenu={(n, e) => { e.preventDefault(); e.stopPropagation(); menu = { x: e.clientX, y: e.clientY, node: n }; }}
        {onRowKeydown}
        draggable={(n) => !n.is_dir}
        dropTarget={(n) => n.is_dir}
        onDropOnNode={(s, d) => (onDrop ? onDrop(s, d) : void dropOn(s, d))}
      >
        {#snippet row(ctx: RowSnippetCtx<ProjectNode>)}
          {@const override = iconFor?.(ctx.node)}
          {@const git = gitBadge?.(ctx.node.path, ctx.node.is_dir)}
          {@const faint = ctx.node.ignored ? 'faint-ignored' : ctx.node.hidden ? 'faint-hidden' : ''}
          <span class="pt-icon {faint}">
            {#if override}
              {@const Glyph = override.icon}
              <Glyph size={14} color={override.color} />
            {:else}
              <IconifyIconView
                icon={ctx.node.is_dir ? getFolderIcon(ctx.node.name, ctx.expanded) : getFileIcon(ctx.node.name)}
                width={15} height={15}
              />
            {/if}
          </span>
          <span class="pt-label {faint}" style:color={git && !ctx.node.is_dir ? GIT_BADGE_COLOR[git] : undefined}>{ctx.node.label ?? ctx.node.name}</span>
          {#if git}
            <span class="pt-git" style:color={GIT_BADGE_COLOR[git]}
              use:tooltip={ctx.node.is_dir ? `Contains changes (${GIT_BADGE_LABEL[git].toLowerCase()})` : GIT_BADGE_LABEL[git]}
            >{ctx.node.is_dir ? '●' : GIT_BADGE_GLYPH[git]}</span>
          {/if}
          {@render rowExtra?.(ctx.node)}
        {/snippet}
      </Tree>
    </div>
  {:else}
    <EmptyState message={emptyMessage} />
  {/if}
</PanelShell>

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menuItems} onSelect={onMenuSelect} onClose={() => (menu = null)} />
{/if}

{#if renaming}
  {@const node = renaming}
  <NamePromptModal
    title={node.is_dir ? 'Rename folder' : 'Rename file'}
    label="Name"
    initial={node.name}
    confirmLabel="Rename"
    onSubmit={async (name) => { await doRename(node, name); }}
    onClose={() => (renaming = null)}
  />
{/if}

{#if newFolderIn !== null}
  {@const dir = newFolderIn}
  <NamePromptModal
    title="New folder"
    label="Folder name"
    confirmLabel="Create"
    onSubmit={async (name) => { await doNewFolder(dir, name); }}
    onClose={() => (newFolderIn = null)}
  />
{/if}

{#if deleting}
  <ConfirmModal
    title={deleting.is_dir ? 'Delete this folder?' : 'Delete this file?'}
    message={deleting.name}
    detail={deleteDetail}
    variant="danger"
    confirmLabel="Delete"
    onConfirm={() => void confirmDelete()}
    onCancel={() => (deleting = null)}
  />
{/if}

<style>
  .pt-tree { flex: 1; min-height: 0; overflow-y: auto; padding: 2px 0 6px; }
  .pt-tree :global(.tree-row) { font-size: var(--font-size-md); }
  .pt-icon { display: flex; align-items: center; color: var(--text-muted); font-size: calc(var(--font-size-md) * 1.25); }
  .pt-icon :global(svg) { width: 1em; height: 1em; }
  .pt-label { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .faint-hidden { opacity: 0.62; }
  .faint-ignored { opacity: 0.42; }
  .pt-git { flex: none; margin-left: 5px; font-size: 10px; font-weight: 700; line-height: 1; }
</style>
