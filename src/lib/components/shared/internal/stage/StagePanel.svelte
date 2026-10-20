<script module lang="ts">
  import type { StageTarget } from '$lib/utils/stage/stage-tree';

  export interface StageActions {
    stage: (t: StageTarget) => void | Promise<void>;
    unstage: (t: StageTarget) => void | Promise<void>;
    /** Called after the user confirmed (when `confirmDiscard`). Omit to hide every discard. */
    discard?: (t: StageTarget) => void | Promise<void>;
    stageAll: () => void | Promise<void>;
    unstageAll: () => void | Promise<void>;
    discardAll?: () => void | Promise<void>;
    /** Omit to hide the stash buttons. */
    stash?: () => void;
  }
</script>

<script lang="ts">
  /**
   * The staging panel, without a repository behind it: Unstaged / Staged accordions (list or
   * folder tree), per-file and per-folder stage / unstage / discard, a context menu, the
   * discard confirmation, and a footer slot for the commit box.
   *
   * Props-driven on purpose. It receives the entries and a set of {@link StageActions} and
   * reports what the user chose; reading git, refreshing and toasting belong to the host. That
   * is what lets Corvus (a tab) and Bennu (a project path) show the very same panel.
   */
  import { untrack, type Snippet } from 'svelte';
  import { Plus, Minus, RotateCcw, Archive, Square, CheckSquare } from 'lucide-svelte';
  import ContextMenu, { type MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import StageFileList from './StageFileList.svelte';
  import DiscardConfirmModal from './DiscardConfirmModal.svelte';
  import {
    buildStageTree, collectDirs, fileTarget, folderTarget, reconcileExpanded,
    type StageTreeNode,
  } from '$lib/utils/stage/stage-tree';
  import type { StatusEntry } from '$lib/types/corvus/git';

  interface Props {
    /** Working-tree changes, untracked files included. */
    unstaged: StatusEntry[];
    staged: StatusEntry[];
    actions: StageActions;
    /** Path highlighted as the one being looked at. */
    selectedPath?: string | null;
    /** Open the diff of a file (`staged` says which side). */
    onOpen: (entry: StatusEntry, staged: boolean) => void;
    viewMode: 'list' | 'tree';
    onViewModeChange: (mode: 'list' | 'tree') => void;
    /** Collapse single-child folder chains in the tree view. */
    compactDirs?: boolean;
    /** Ask before discarding. */
    confirmDiscard?: boolean;
    /** Rendered between the two sections (merge conflicts). */
    middle?: Snippet;
    /** Rendered under the sections (the commit box). */
    footer?: Snippet;
  }

  let {
    unstaged, staged, actions, selectedPath = null, onOpen, viewMode, onViewModeChange,
    compactDirs = false, confirmDiscard = true, middle, footer,
  }: Props = $props();

  // Accordion: unstaged starts open, staged closed; opening one closes the other.
  let unstagedCollapsed = $state(false);
  let stagedCollapsed = $state(true);
  function toggleSection(which: 'unstaged' | 'staged') {
    if (which === 'unstaged') {
      unstagedCollapsed = !unstagedCollapsed;
      if (!unstagedCollapsed) stagedCollapsed = true;
    } else {
      stagedCollapsed = !stagedCollapsed;
      if (!stagedCollapsed) unstagedCollapsed = true;
    }
  }

  const unstagedTree = $derived(buildStageTree(unstaged, compactDirs));
  const stagedTree = $derived(buildStageTree(staged, compactDirs));

  // Expand state per side. Dirs seen on the previous pass tell a brand-new folder (opens) from
  // one the user collapsed (stays collapsed).
  let unstagedExpanded = $state(new Set<string>());
  let stagedExpanded = $state(new Set<string>());
  let unstagedKnown = new Set<string>();
  let stagedKnown = new Set<string>();

  $effect(() => {
    const dirs = collectDirs(unstagedTree);
    untrack(() => {
      unstagedExpanded = reconcileExpanded(dirs, unstagedExpanded, unstagedKnown);
      unstagedKnown = dirs;
    });
  });
  $effect(() => {
    const dirs = collectDirs(stagedTree);
    untrack(() => {
      stagedExpanded = reconcileExpanded(dirs, stagedExpanded, stagedKnown);
      stagedKnown = dirs;
    });
  });

  function toggled(set: Set<string>, path: string): Set<string> {
    const next = new Set(set);
    if (!next.delete(path)) next.add(path);
    return next;
  }

  // ── Discard confirmation ────────────────────────────────────────────────
  type DiscardPending = ({ kind: 'paths' } & StageTarget) | { kind: 'all'; count: number };
  let discardPending = $state<DiscardPending | null>(null);

  function askDiscard(target: StageTarget) {
    if (!actions.discard || target.paths.length === 0) return;
    if (confirmDiscard) discardPending = { kind: 'paths', ...target };
    else void actions.discard(target);
  }

  function askDiscardAll() {
    if (unstaged.length === 0) return;
    discardPending = { kind: 'all', count: unstaged.length };
  }

  async function confirmPending() {
    const pending = discardPending;
    discardPending = null;
    if (!pending) return;
    if (pending.kind === 'paths') await actions.discard?.(pending);
    else await actions.discardAll?.();
  }

  // ── Context menu ────────────────────────────────────────────────────────
  type Ctx = StageTarget & { x: number; y: number; items: MenuItem[] };
  let ctx = $state<Ctx | null>(null);

  function openContext(e: MouseEvent, isStaged: boolean, entry: StatusEntry | null, node: StageTreeNode | null) {
    e.preventDefault();
    e.stopPropagation();
    const folder = node ? folderTarget(node) : null;
    const target = entry ? fileTarget(entry) : { paths: folder!.paths, label: folder!.label };
    const noun = entry ? 'File' : 'Folder';
    const suffix = folder ? ` (${folder.count})` : '';
    const items: MenuItem[] = isStaged
      ? [{ id: 'unstage', label: `Unstage ${noun}${suffix}`, icon: Minus, iconColor: 'var(--warning)' }]
      : [
          { id: 'stage', label: `Stage ${noun}${suffix}`, icon: Plus, iconColor: 'var(--success)' },
          ...(actions.discard ? [{ id: 'discard', label: entry ? 'Discard Changes' : `Discard Folder${suffix}`, icon: RotateCcw, danger: true }] : []),
          ...(folder && actions.stash
            ? [
                { id: 'sep-1', label: '', separator: true },
                { id: 'stash', label: 'Stash All Changes…', icon: Archive, iconColor: 'var(--color-stash)' },
              ]
            : []),
        ];
    ctx = { x: e.clientX, y: e.clientY, ...target, items };
  }

  async function onContextSelect(id: string) {
    if (!ctx) return;
    const target: StageTarget = { paths: ctx.paths, label: ctx.label };
    ctx = null;
    if (id === 'stage') await actions.stage(target);
    else if (id === 'unstage') await actions.unstage(target);
    else if (id === 'discard') askDiscard(target);
    else if (id === 'stash') actions.stash?.();
  }

  const toggleView = () => onViewModeChange(viewMode === 'tree' ? 'list' : 'tree');
</script>

{#snippet unstagedActions()}
  <IconButton size={20} tooltip="Stage all" onclick={() => void actions.stageAll()}><CheckSquare size={13} /></IconButton>
  {#if actions.discardAll}
    <IconButton size={20} variant="danger" tooltip="Discard all unstaged changes" onclick={askDiscardAll}><RotateCcw size={12} /></IconButton>
  {/if}
  {#if actions.stash}
    <IconButton size={20} tooltip="Stash all changes" onclick={() => actions.stash?.()}><Archive size={12} /></IconButton>
  {/if}
{/snippet}

{#snippet stagedActions()}
  <IconButton size={20} tooltip="Unstage all" onclick={() => void actions.unstageAll()}><Square size={13} /></IconButton>
{/snippet}

<div class="files-panel">
  <StageFileList
    kind="unstaged"
    entries={unstaged}
    tree={unstagedTree}
    {viewMode}
    collapsed={unstagedCollapsed}
    expandedPaths={unstagedExpanded}
    {selectedPath}
    onToggleSection={() => toggleSection('unstaged')}
    onToggleView={toggleView}
    onToggleDir={(p) => (unstagedExpanded = toggled(unstagedExpanded, p))}
    onOpen={(e) => onOpen(e, false)}
    onAct={(e) => actions.stage(fileTarget(e))}
    onDiscard={actions.discard ? (e) => askDiscard(fileTarget(e)) : undefined}
    onContext={(e, entry, node) => openContext(e, false, entry, node)}
    actions={unstagedActions}
  />

  {@render middle?.()}

  <StageFileList
    kind="staged"
    entries={staged}
    tree={stagedTree}
    {viewMode}
    collapsed={stagedCollapsed}
    expandedPaths={stagedExpanded}
    {selectedPath}
    onToggleSection={() => toggleSection('staged')}
    onToggleView={toggleView}
    onToggleDir={(p) => (stagedExpanded = toggled(stagedExpanded, p))}
    onOpen={(e) => onOpen(e, true)}
    onAct={(e) => actions.unstage(fileTarget(e))}
    onContext={(e, entry, node) => openContext(e, true, entry, node)}
    actions={stagedActions}
  />

  {#if footer}
    <div class="footer">{@render footer()}</div>
  {/if}
</div>

{#if ctx}
  <ContextMenu x={ctx.x} y={ctx.y} items={ctx.items} onSelect={onContextSelect} onClose={() => (ctx = null)} />
{/if}

{#if discardPending}
  <DiscardConfirmModal
    target={discardPending.kind === 'paths'
      ? discardPending.label
      : `all ${discardPending.count} unstaged file${discardPending.count !== 1 ? 's' : ''}`}
    onConfirm={confirmPending}
    onCancel={() => (discardPending = null)}
  />
{/if}

<style>
  .files-panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg-input);
    overflow: hidden;
  }
  .footer { border-top: 1px solid var(--border); flex-shrink: 0; }
</style>
