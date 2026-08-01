<script lang="ts">
  /**
   * The vault's folders and notes, on the shared `Tree` widget.
   *
   * `Tree` does everything structural — flattening, virtualisation, indentation,
   * the chevron, selection visuals, `Enter` / `←` / `→` — and this file supplies
   * the two things it has no opinion about: what a row looks like, and the
   * keyboard verbs it deliberately leaves alone.
   *
   * **What the widget could not do, and why it is here rather than in it.**
   * `Tree` leaves `ArrowUp` / `ArrowDown` to the browser's tab order on purpose,
   * and has no notion of typing to jump. Both are added through `onRowKeydown`,
   * which is the extension point the widget documents for exactly this ("calling
   * `preventDefault()` … is how a consumer binds keys the tree doesn't own"), so
   * nothing here forks or reaches inside it.
   *
   * Typing a letter does not jump to the first match: it moves the caret into the
   * filter box above and seeds it. That is IntelliJ's speed-search, and on a tree
   * where the answer is usually several folders deep it beats jump-to-next —
   * filtering already reveals matches wherever they are (`Tree` force-expands
   * ancestors of a match), and a second keystroke narrows instead of cycling.
   */
  import { tick } from 'svelte';
  import { Folder, FolderOpen } from 'lucide-svelte';
  import Tree, { type RowSnippetCtx } from '$lib/components/shared/ui/Tree.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import NoteRowContent from './NoteRowContent.svelte';
  import { allFolderIds, ancestorFolderIds, type NoteTreeNode } from './note-tree';
  import { moveRowFocus } from './row-focus';
  import { garrulusNotesStore } from '$lib/stores/garrulus/notes.svelte';

  interface Props {
    nodes: NoteTreeNode[];
    /** The note on screen, drawn as the selected row. */
    activePath?: string | null;
    /** The filter box's text. Matching subtrees expand themselves. */
    filter?: string;
    onOpen: (path: string) => void;
    /** A printable character was typed on a row — the panel routes it to the
     *  filter box. */
    onTypeAhead?: (char: string) => void;
    /** `↑` walked off the first row: the panel puts focus back in the filter. */
    onLeaveTop?: () => void;
  }

  let { nodes, activePath = null, filter = '', onOpen, onTypeAhead, onLeaveTop }: Props = $props();

  /** Controlled expansion, so "expand all" / "reveal" are one set to write to
   *  rather than a message the widget has to be asked to obey. A fresh `Set` on
   *  every change: `$state` does not track mutations of a built-in Set. */
  let expanded = $state<Set<string>>(new Set());
  let treeRef = $state<{ scrollToId: (id: string, block?: 'center' | 'nearest') => void } | null>(
    null,
  );
  let hostEl = $state<HTMLDivElement | undefined>(undefined);

  function setExpanded(id: string, next: boolean) {
    const copy = new Set(expanded);
    if (next) copy.add(id);
    else copy.delete(id);
    expanded = copy;
  }

  export function expandAll() {
    expanded = new Set(allFolderIds(nodes));
  }

  export function collapseAll() {
    expanded = new Set();
  }

  /** Open the folders on the way to `path` and scroll its row into view. The
   *  `tick` lets the newly expanded rows reach the widget's flat list before it
   *  is asked where the row is. */
  export async function reveal(path: string) {
    const copy = new Set(expanded);
    for (const id of ancestorFolderIds(path)) copy.add(id);
    expanded = copy;
    await tick();
    treeRef?.scrollToId(path);
  }

  export function focusFirst() {
    hostEl?.querySelector<HTMLElement>('.tree-row')?.focus();
  }

  /** The row's hover text: where the note lives, and what the colour dot means —
   *  the only place the type's *name* appears next to its accent. */
  function rowTitle(node: NoteTreeNode): string {
    if (node.kind === 'folder') return node.path;
    if (!node.path) return 'This note declares a frontmatter uid, so it has no path to open by.';
    const type = garrulusNotesStore.typeName(node.note.typeId);
    return type ? `${node.path} · ${type}` : node.path;
  }

  function match(node: NoteTreeNode, q: string): boolean {
    if (node.name.toLowerCase().includes(q)) return true;
    return node.kind === 'note' && (node.path?.toLowerCase().includes(q) ?? false);
  }

  function onRowKeydown(node: NoteTreeNode, e: KeyboardEvent) {
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      const moved = moveRowFocus(hostEl, e.currentTarget as HTMLElement, e.key === 'ArrowDown' ? 1 : -1);
      if (!moved && e.key === 'ArrowUp') onLeaveTop?.();
      return;
    }
    // A bare printable character is the speed-search; anything with a modifier
    // belongs to the window, and Backspace/Escape belong to the filter box. A
    // leading space is left to the widget, where it toggles a folder — no search
    // starts with one, and Space is the only key that means two things here.
    if (e.key === ' ' && !filter) return;
    if (onTypeAhead && e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) {
      e.preventDefault();
      onTypeAhead(e.key);
    }
  }
</script>

<div class="gn-tree" bind:this={hostEl}>
  <Tree
    bind:this={treeRef}
    {nodes}
    {filter}
    {match}
    getId={(n: NoteTreeNode) => n.id}
    getChildren={(n: NoteTreeNode) => (n.kind === 'folder' ? n.children : undefined)}
    selectable={(n: NoteTreeNode) => n.kind === 'note' && n.path != null}
    selectedId={activePath}
    expandedIds={expanded}
    onExpandToggle={setExpanded}
    {onRowKeydown}
    onSelect={(n: NoteTreeNode) => {
      if (n.kind === 'note' && n.path) onOpen(n.path);
    }}
    rowTitle={rowTitle}
    rowHeight={22}
    ariaLabel="Vault notes"
  >
    {#snippet row(ctx: RowSnippetCtx<NoteTreeNode>)}
      {#if ctx.node.kind === 'folder'}
        <span class="tree-icon">
          {#if ctx.expanded}<FolderOpen size={13} />{:else}<Folder size={13} />{/if}
        </span>
        <span class="tree-label gn-folder">{ctx.node.name}</span>
        <span class="tree-badge tree-badge-muted">{ctx.node.count}</span>
      {:else}
        <NoteRowContent
          title={ctx.node.name}
          accent={garrulusNotesStore.accentFor(ctx.node.note.typeId)}
          pinned={ctx.node.note.pinned}
          dirty={ctx.node.path != null && garrulusNotesStore.isDirty(ctx.node.path)}
          muted={ctx.node.path == null}
        />
      {/if}
    {/snippet}

    {#snippet emptyState()}
      <EmptyState
        message={filter ? 'No note matches that.' : 'This vault has no notes yet.'}
        compact
      />
    {/snippet}
  </Tree>
</div>

<style>
  .gn-tree {
    /* The tree measures itself against the nearest scrolling ancestor, which is
       the panel body — nothing to add here beyond letting it fill the width. */
    width: 100%;
  }

  .gn-folder { font-weight: 500; }
</style>
