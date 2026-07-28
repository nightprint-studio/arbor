<script lang="ts" module>
  import type { FolderEntry } from '$lib/stores/picus/project.svelte';
  import type { ScriptFile } from '$lib/types/picus';

  /**
   * A row of the repository tree: a real directory, or a file inside one.
   *
   * `id` is the project-relative path in both cases — the identity the backend
   * uses, the thing expansion is keyed by, and (because a path contains every
   * ancestor's name) the string the filter matches against.
   */
  export type ScriptNode =
    | { kind: 'folder'; id: string; name: string; entry: FolderEntry; children: ScriptNode[] }
    | { kind: 'file'; id: string; name: string; file: ScriptFile; entry: FolderEntry; children: ScriptNode[] };
</script>

<script lang="ts">
  /**
   * The repository, as it is on disk.
   *
   * Not a two-level "branch → folder" invention: the real hierarchy, nested to
   * whatever depth the repository has, with the engine and the purpose shown on
   * the folder that has them. A layout like `AGGIORNAMENTO/<version>/ORA` puts
   * the engine at the bottom and the role at the top, and the tree renders that
   * as it is rather than flattening it into a list of eleven rows all called
   * `ORA`.
   *
   * ## What a deep row says
   *
   * Three things carry the ancestry, each earning its place:
   *  • **indent guides** — the cheapest way to read depth, always on;
   *  • **the full path in the row's tooltip** — every row, folder or file;
   *  • **a muted parent prefix, but only on names that repeat**. Eleven folders
   *    called `ORA` are meaningless without their versions; the single `MSQ` is
   *    not, and prefixing it too would be noise on every row to serve some.
   *
   * ## Classifying from here
   *
   * The row menu is the answer to "this `ORA` is Oracle" — right-click, or
   * Shift+F10 / the Menu key on the focused row, which is why this asks the
   * shared Tree for a row-level key hook rather than for a mouse-only one.
   */
  import { ChevronsDownUp, ChevronsUpDown, Copy, FileCode2, Folder, FolderOpen, FolderCog, SquareArrowOutUpRight } from 'lucide-svelte';
  import Tree from '$lib/components/shared/ui/Tree.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import ContextMenu, { type MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import EncodingPill from '$lib/components/shared/internal/EncodingPill.svelte';
  import PicusDialectChip from '../PicusDialectChip.svelte';
  import PicusRoleChip from '../PicusRoleChip.svelte';
  import { folderClassifyItems, runFolderClassifyId } from '../folder-classify';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { picusProjectStore } from '$lib/stores/picus/project.svelte';
  import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';
  import { picusUiStore } from '$lib/stores/picus/ui.svelte';
  import { declaredEngine, folderEngine, type FolderNode } from '$lib/types/picus';

  let { filter = '' }: { filter?: string } = $props();

  /** Marker meaning for the coloured dot on a file row. */
  const STATUS_HINT = {
    modified: 'Modified since the last index',
    new: 'Created by a generation and not yet reviewed',
    error: 'Has a blocking finding',
  } as const;

  // ── The tree, folders before files ─────────────────────────────────────────
  function build(list: FolderNode[]): ScriptNode[] {
    return list.map((node) => {
      const entry = picusProjectStore.entryFor(node.path)!;
      return {
        kind: 'folder' as const,
        id: node.path,
        name: node.name,
        entry,
        children: [
          ...build(node.children),
          ...node.files.map((file): ScriptNode => ({
            kind: 'file' as const,
            id: file.path,
            name: file.name,
            file,
            entry,
            children: [],
          })),
        ],
      };
    });
  }

  const nodes = $derived(build(picusProjectStore.tree));

  /**
   * Folder names that occur more than once in the repository.
   *
   * The disambiguating prefix is spent only on these: it is the difference
   * between a row that means nothing and one that does, and on unique names it
   * would be pure noise.
   */
  const repeatedNames = $derived.by(() => {
    const count = new Map<string, number>();
    for (const e of picusProjectStore.entries) {
      count.set(e.node.name, (count.get(e.node.name) ?? 0) + 1);
    }
    return new Set([...count].filter(([, n]) => n > 1).map(([name]) => name));
  });

  /** The parent's own name — shown before an ambiguous folder name. */
  function parentName(entry: FolderEntry): string {
    if (!entry.parent) return '';
    return entry.parent.slice(entry.parent.lastIndexOf('/') + 1);
  }

  function needsPrefix(node: ScriptNode): boolean {
    return node.kind === 'folder' && !!node.entry.parent && repeatedNames.has(node.name);
  }

  // ── Row menu ───────────────────────────────────────────────────────────────
  let menu = $state<{ x: number; y: number; node: ScriptNode } | null>(null);

  const menuItems = $derived.by<MenuItem[]>(() => {
    const node = menu?.node;
    if (!node) return [];
    const copy: MenuItem = { id: 'copy', label: 'Copy path', icon: Copy };
    if (node.kind === 'file') {
      return [
        { id: 'open', label: 'Open', icon: SquareArrowOutUpRight },
        { id: 'sep', label: '', separator: true },
        // The engine of a file is its folder's, so the correction belongs there —
        // and saying so here is better than leaving the menu silent about it.
        { id: 'classify', label: `Classify ${node.entry.node.name}…`, icon: FolderCog },
        copy,
      ];
    }
    return [
      { id: 'head', label: node.entry.node.path, header: true },
      ...folderClassifyItems(node.entry),
      { id: 'sep2', label: '', separator: true },
      copy,
    ];
  });

  async function onMenuSelect(id: string) {
    const node = menu?.node;
    menu = null;
    if (!node) return;
    if (id === 'copy') {
      try {
        await navigator.clipboard.writeText(node.id);
        toastStore.show(`${node.id} copied.`, 'success');
      } catch {
        toastStore.show('The path could not be copied to the clipboard.', 'error');
      }
      return;
    }
    if (id === 'open' && node.kind === 'file') { openFile(node); return; }
    if (id === 'classify') { picusUiStore.openFolderClassify(node.entry.node.path); return; }
    await runFolderClassifyId(node.entry, id);
  }

  // ── Rows ───────────────────────────────────────────────────────────────────
  function openFile(node: ScriptNode) {
    if (node.kind !== 'file') return;
    picusTabsStore.openFile(node.file.path, node.file.name, folderEngine(node.entry.node));
  }

  function onSelect(node: ScriptNode) {
    if (node.kind === 'file') openFile(node);
  }

  /**
   * Shift+F10 and the Menu key open the row's menu at the row — the keyboard
   * route to everything the right button offers, which is the whole reason the
   * shared Tree grew a row-level key hook.
   */
  function onRowKeydown(node: ScriptNode, e: KeyboardEvent) {
    const wanted = e.key === 'ContextMenu' || (e.key === 'F10' && e.shiftKey);
    if (!wanted) return;
    e.preventDefault();
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    menu = { x: rect.left + 24, y: rect.bottom, node };
  }
</script>

<div class="st-actions">
  <Button variant="icon" size="xs" tooltip="Expand every folder" ariaLabel="Expand every folder"
          onclick={() => picusProjectStore.expandAll()}>
    {#snippet iconStart()}<ChevronsUpDown size={12} />{/snippet}
  </Button>
  <Button variant="icon" size="xs" tooltip="Collapse every folder" ariaLabel="Collapse every folder"
          onclick={() => picusProjectStore.collapseAll()}>
    {#snippet iconStart()}<ChevronsDownUp size={12} />{/snippet}
  </Button>
  <span class="st-count">{picusProjectStore.folderCount} folders · {picusProjectStore.fileCount} files</span>
</div>

<Tree
  {nodes}
  {filter}
  guides
  ariaLabel="Scripts on disk"
  rowHeight={22}
  expandedIds={picusProjectStore.expandedIds}
  onExpandToggle={(id, next) => picusProjectStore.setFolderExpanded(id, next)}
  selectedId={picusTabsStore.active?.file ?? null}
  match={(node, q) => node.id.toLowerCase().includes(q)}
  rowTitle={(node) => node.id}
  {onSelect}
  {onRowKeydown}
  onContextMenu={(node, e) => (menu = { x: e.clientX, y: e.clientY, node })}
>
  {#snippet row({ node, expanded })}
    {#if node.kind === 'folder'}
      {@const folder = node.entry.node}
      <span class="tree-icon">
        {#if expanded}<FolderOpen size={13} />{:else}<Folder size={13} />{/if}
      </span>
      <span class="tree-label st-folder">
        {#if needsPrefix(node)}<span class="st-parent">{parentName(node.entry)}/</span>{/if}{node.name}
      </span>
      <!-- The engine is shown where it decides something: on the folder that
           declares it, and on any folder that actually holds scripts. A pure
           container in between says nothing and stays clean. -->
      {#if declaredEngine(folder) !== null || folder.files.length}
        <PicusDialectChip
          engine={folderEngine(folder)}
          terse
          inherited={declaredEngine(folder) === null}
          from={node.entry.dialectFrom ?? ''}
        />
      {/if}
      {#if folder.role !== null || folder.files.length}
        <PicusRoleChip
          role={folder.effectiveRole}
          terse
          inherited={folder.role === null}
          from={node.entry.roleFrom ?? ''}
        />
      {/if}
      {#if folder.files.length}
        <Badge variant="count" size="sm" label={String(folder.files.length)} />
      {/if}
    {:else}
      <span class="tree-icon"><FileCode2 size={13} /></span>
      <span class="tree-label st-file">{node.name}</span>
      {#if node.file.status}
        <span class="st-mark st-{node.file.status}" title={STATUS_HINT[node.file.status]}></span>
      {/if}
      <EncodingPill
        encoding={node.file.encoding}
        expected={node.file.expectedEncoding}
        eol={node.file.eol}
        compact
      />
    {/if}
  {/snippet}

  {#snippet emptyState()}
    <p class="st-empty">
      {filter ? `No folder or file matches “${filter}”.` : 'This folder holds no SQL scripts.'}
    </p>
  {/snippet}
</Tree>

{#if menu}
  <ContextMenu
    items={menuItems}
    x={menu.x}
    y={menu.y}
    onSelect={(id) => void onMenuSelect(id)}
    onClose={() => (menu = null)}
  />
{/if}

<style>
  .st-actions {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 4px 8px 2px;
  }
  .st-count {
    margin-left: auto;
    font-size: 10px;
    color: var(--text-disabled);
    white-space: nowrap;
  }

  .st-folder { font-size: 12px; }
  /* Only on names that repeat — the version that tells eleven ORA folders apart. */
  .st-parent { color: var(--text-disabled); }

  .st-file { font-family: var(--font-code); font-size: 11.5px; }

  /* Working-copy markers: new / modified / has a blocking finding. */
  .st-mark {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .st-modified { background: var(--warning); }
  .st-new { background: var(--success); }
  .st-error { background: var(--error); }

  .st-empty {
    padding: 18px 12px;
    text-align: center;
    font-size: 11px;
    font-style: italic;
    color: var(--text-muted);
  }
</style>
