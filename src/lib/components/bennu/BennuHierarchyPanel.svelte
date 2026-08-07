<script lang="ts">
  /**
   * The Hierarchy tool window (bottom dock) — the call hierarchy and the type hierarchy.
   *
   * In the bottom dock rather than in a side rail because of what a row *is*: a name, the file it is
   * in, and the line of source around it. That is wide, horizontal data, the same shape the Problems
   * and TODO lists have, and squeezing it into a 260px column would cost the preview — which is the
   * part that lets you recognise a caller without opening it.
   *
   * One panel for both hierarchies, and a direction toggle inside it. They are the same tree with the
   * same rows and the same lazy expansion; only the question differs, and a second panel would be a
   * second copy of all of it. See `hierarchy.svelte.ts` for why a level is fetched at a time.
   *
   * Keyboard: the shared Tree owns ↑/↓/←/→ and Enter, so navigating the tree and jumping from a row
   * need no mouse. The direction chips are reachable by Tab.
   */
  import { tick } from 'svelte';
  import { Network, RefreshCw } from 'lucide-svelte';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import ChipBar, { type ChipItem } from '$lib/components/shared/ui/ChipBar.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Tree, { type RowSnippetCtx } from '$lib/components/shared/ui/Tree.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { bennuContextMenuStore } from '$lib/stores/bennu/contextmenu.svelte';
  import { bennuHierarchyStore, type HierarchyRow } from '$lib/stores/bennu/hierarchy.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import type { MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import { ArrowRight, Copy } from 'lucide-svelte';
  import { baseName } from '$lib/utils/paths';
  import { kindGlyph } from './symbol-kind-glyph';
  import type { LspHierarchyDirection } from '$lib/ipc/bennu/lsp';

  const kind = $derived(bennuHierarchyStore.kind);
  const rows = $derived(bennuHierarchyStore.roots);
  const subject = $derived(bennuHierarchyStore.subject);

  const title = $derived(kind === 'calls' ? 'Call hierarchy' : 'Type hierarchy');

  const chips = $derived<ChipItem[]>(
    bennuHierarchyStore.directions.map((d) => ({ id: d.id, label: d.label })),
  );

  /** Every row can have children until the server has said otherwise — which is what makes the tree
   *  lazy. `exhausted` is the server's "there are none", and only then does the chevron go. */
  function hasChildren(row: HierarchyRow): boolean {
    return !row.exhausted;
  }

  function jump(row: HierarchyRow) {
    const node = row.node;
    // A caller row jumps to the CALL, not to the head of the function containing it — that is the
    // difference between one hop and reading a body to find the line you were told about. Falls back
    // to the declaration for a type hierarchy, which has no call sites.
    const site = node.call_sites[0];
    const line = site?.line ?? node.line;
    void projectStore.openFile(node.file).then(() => bennuUiStore.requestGoto(line));
  }

  /**
   * Take the keyboard when a hierarchy is built.
   *
   * The panel is opened by a shortcut, and reaching for the mouse to walk the tree that shortcut
   * just built is exactly the gap the keyboard-first rule forbids. Focusing the first row hands ↑↓←→
   * and Enter straight to the Tree.
   *
   * Keyed on the nonce rather than on the subject: asking about the same function twice is two
   * openings, and a name cannot tell them apart.
   */
  let bodyEl = $state<HTMLElement | null>(null);
  $effect(() => {
    void bennuHierarchyStore.openNonce;
    if (rows.length === 0) return;
    // After the tree has rendered its rows — they are virtualised, so there is nothing to focus in
    // the same frame the roots arrive.
    void tick().then(() => bodyEl?.querySelector<HTMLElement>('.tree-row')?.focus());
  });

  function copyText(text: string) {
    // Best-effort — clipboard can be denied (permission / focus); swallow.
    void navigator.clipboard?.writeText(text).catch(() => { /* clipboard denied — ignore */ });
  }

  function onRowContextMenu(row: HierarchyRow, e: MouseEvent) {
    e.preventDefault();
    const items: MenuItem[] = [
      { id: 'goto', label: 'Go to', icon: ArrowRight },
      { id: 'copy-name', label: 'Copy name', icon: Copy },
    ];
    bennuContextMenuStore.show(e.clientX, e.clientY, items, (id) => {
      switch (id) {
        case 'goto': jump(row); break;
        case 'copy-name': copyText(row.node.name); break;
      }
    });
  }
</script>

<div class="hier">
  <BottomPanelHeader
    {title}
    onClose={() => bennuUiStore.closeBottom()}
  >
    {#snippet icon()}<Network size={13} />{/snippet}
    {#snippet children()}
      {#if subject}<span class="hier-subject">{subject}</span>{/if}
    {/snippet}
    {#snippet actions()}
      <button
        class="ps-btn"
        type="button"
        use:tooltip={'Re-ask every open level'}
        aria-label="Refresh hierarchy"
        disabled={bennuHierarchyStore.loading || rows.length === 0}
        onclick={() => void bennuHierarchyStore.refresh()}
      >
        <RefreshCw size={13} />
      </button>
    {/snippet}
  </BottomPanelHeader>

  {#if rows.length > 0}
    <div class="hier-bar">
      <ChipBar
        items={chips}
        selected={bennuHierarchyStore.direction}
        size="sm"
        onSelect={(sel) => bennuHierarchyStore.setDirection(sel as LspHierarchyDirection)}
      />
    </div>
  {/if}

  {#if bennuHierarchyStore.loading && rows.length === 0}
    <div class="state"><Spinner size={13} /> Building…</div>
  {:else if rows.length === 0}
    <div class="hier-empty">
      <Network size={20} />
      <EmptyState
        message={bennuHierarchyStore.message
          ?? 'Put the caret on a function or a type and open its hierarchy.'}
      />
    </div>
  {:else}
    <div class="hier-body" bind:this={bodyEl}>
      <Tree
        nodes={rows}
        {hasChildren}
        expandedIds={bennuHierarchyStore.expanded}
        onExpandToggle={(id, next) => bennuHierarchyStore.toggle(id, next)}
        onActivate={jump}
        onContextMenu={onRowContextMenu}
        rowTitle={(row) => `${row.node.file}:${row.node.line}`}
        guides
        ariaLabel={title}
      >
        {#snippet row({ node }: RowSnippetCtx<HierarchyRow>)}
          {@const visual = kindGlyph(node.node.kind)}
          {@const Icon = visual.icon}
          {@const sites = node.node.call_sites.length}
          <Icon size={13} color={visual.color} />
          <span class="r-name">{node.node.name}</span>
          {#if node.node.detail}<span class="r-detail">{node.node.detail}</span>{/if}
          <!-- Several calls to the same thing inside one function: the row jumps to the first, and
               the count says the others are there. -->
          {#if sites > 1}<span class="r-sites">{sites}×</span>{/if}
          <span class="r-spacer"></span>
          {#if node.loading}<Spinner size={11} />{/if}
          <span class="r-where">{baseName(node.node.file)}:{node.node.line}</span>
        {/snippet}
      </Tree>
    </div>
  {/if}
</div>

<style>
  .hier {
    display: flex; flex-direction: column;
    height: 100%; width: 100%; min-height: 0;
    background: var(--bg-base);
  }
  /* What the tree is about, beside the panel title — a hierarchy with no subject named is a tree
     of rows with no question. */
  .hier-subject {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-secondary);
    padding: 0 2px;
  }
  .hier-bar {
    display: flex; align-items: center;
    padding: 4px 8px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .hier-body { flex: 1; min-height: 0; overflow: auto; }
  .state {
    display: flex; align-items: center; gap: 6px;
    padding: 8px 10px;
    font-size: 11px;
    color: var(--text-muted);
  }
  .hier-empty {
    flex: 1;
    display: flex; flex-direction: column; align-items: center; justify-content: center;
    gap: 6px;
    color: var(--text-disabled);
  }
  .r-name {
    font-family: var(--font-mono);
    font-size: 11.5px;
    color: var(--text-primary);
    white-space: nowrap;
  }
  .r-detail {
    font-size: 10.5px;
    color: var(--text-muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    max-width: 40%;
  }
  .r-sites {
    font-size: 10px;
    color: var(--text-secondary);
    background: var(--bg-elevated);
    border-radius: var(--radius-sm);
    padding: 0 4px;
  }
  .r-spacer { flex: 1; }
  .r-where {
    font-size: 10.5px;
    color: var(--text-disabled);
    white-space: nowrap;
  }
</style>
