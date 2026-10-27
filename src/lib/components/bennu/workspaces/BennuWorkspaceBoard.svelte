<script lang="ts">
  /**
   * BennuWorkspaceBoard — every workspace as a column, every project as a card in it.
   *
   * The point of a board over a list-and-detail pane is that moving a project is one gesture:
   * grab it, drop it in the other column, where you want it. Reordering is the same gesture within
   * a column, and workspaces reorder by dragging their headers.
   *
   * ## Keyboard (the mouse is a convenience, not the only way)
   *
   * On a project: <kbd>↑</kbd>/<kbd>↓</kbd> move the focus; <kbd>Alt</kbd>+<kbd>↑</kbd>/<kbd>↓</kbd>
   * reorder it; <kbd>Alt</kbd>+<kbd>←</kbd>/<kbd>→</kbd> send it to the neighbouring workspace;
   * <kbd>Enter</kbd> opens it; <kbd>Delete</kbd> removes it from the workspace.
   * On a workspace header: <kbd>Alt</kbd>+<kbd>←</kbd>/<kbd>→</kbd> reorder, <kbd>Enter</kbd> switches to it.
   *
   * The pointer drag lives in `board-drag.svelte.ts`; this file is the board's markup and the
   * mapping from a drop to a store call.
   */
  import { tick, onDestroy } from 'svelte';
  import { flip } from 'svelte/animate';
  import { Plus, GripVertical, Check, X, FolderPlus } from 'lucide-svelte';
  import Monogram from '$lib/components/shared/ui/Monogram.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { workspacesStore, wsColorVar } from '$lib/stores/bennu/workspaces.svelte';
  import { createBoardDrag, type DragItem, type DropTarget } from './board-drag.svelte';

  let {
    selectedId = $bindable(''),
    onAddProject,
    onNewWorkspace,
    onClose,
  }: {
    selectedId?: string;
    onAddProject: (wsId: string) => void;
    onNewWorkspace: () => void;
    onClose: () => void;
  } = $props();

  function basename(path: string): string {
    const parts = path.split(/[\\/]/).filter(Boolean);
    return parts[parts.length - 1] ?? path;
  }

  const workspaces = $derived(workspacesStore.workspaces);

  // ── Drop → store ──────────────────────────────────────────────────────────────
  function drop(item: DragItem, target: DropTarget) {
    if (item.kind === 'workspace' && target.kind === 'workspace') {
      workspacesStore.moveWorkspace(item.wsId, target.index);
    } else if (item.kind === 'project' && target.kind === 'project') {
      void workspacesStore.moveProject(item.wsId, item.root, target.wsId, target.index);
    }
  }
  const drag = createBoardDrag(drop);
  onDestroy(() => drag.cancel());

  const projectDrop = (wsId: string) =>
    drag.active && drag.target?.kind === 'project' && drag.target.wsId === wsId ? drag.target.index : -1;
  const workspaceDrop = $derived(
    drag.active && drag.target?.kind === 'workspace' ? drag.target.index : -1,
  );

  // ── Keyboard ──────────────────────────────────────────────────────────────────
  /** Put the focus back on a card after it has moved — the DOM node it was on may be gone. */
  async function refocus(wsId: string, root: string) {
    await tick();
    document
      .querySelector<HTMLElement>(`[data-proj-key="${CSS.escape(`${wsId}|${root}`)}"]`)
      ?.focus();
  }

  function stepFocus(from: HTMLElement, dir: 1 | -1) {
    const rows = [...(from.closest('[data-ws-col]')?.querySelectorAll<HTMLElement>('[data-proj-row]') ?? [])];
    rows[rows.indexOf(from) + dir]?.focus();
  }

  function onProjectKey(e: KeyboardEvent, wsId: string, root: string, index: number, count: number) {
    const el = e.currentTarget as HTMLElement;
    const wsIndex = workspaces.findIndex((w) => w.id === wsId);

    if (e.altKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) {
      e.preventDefault();
      const to = e.key === 'ArrowUp' ? index - 1 : index + 2; // gaps: before the one above / after the one below
      if (to < 0 || to > count) return;
      void workspacesStore.moveProject(wsId, root, wsId, to).then(() => refocus(wsId, root));
    } else if (e.altKey && (e.key === 'ArrowLeft' || e.key === 'ArrowRight')) {
      e.preventDefault();
      const next = workspaces[wsIndex + (e.key === 'ArrowLeft' ? -1 : 1)];
      if (!next) return;
      const at = Math.min(index, next.projects.length);
      void workspacesStore.moveProject(wsId, root, next.id, at).then(() => refocus(next.id, root));
    } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      stepFocus(el, e.key === 'ArrowDown' ? 1 : -1);
    } else if (e.key === 'Enter') {
      e.preventDefault();
      openProject(wsId, root);
    } else if (e.key === 'Delete') {
      e.preventDefault();
      workspacesStore.removeProjectFrom(wsId, root);
    }
  }

  function onHeaderKey(e: KeyboardEvent, wsId: string, index: number) {
    if (e.altKey && (e.key === 'ArrowLeft' || e.key === 'ArrowRight')) {
      e.preventDefault();
      const to = e.key === 'ArrowLeft' ? index - 1 : index + 2;
      if (to < 0 || to > workspaces.length) return;
      workspacesStore.moveWorkspace(wsId, to);
      void tick().then(() =>
        document.querySelector<HTMLElement>(`[data-ws-head="${CSS.escape(wsId)}"]`)?.focus());
    } else if (e.key === 'Enter') {
      e.preventDefault();
      selectedId = wsId;
    }
  }

  function grabProject(e: PointerEvent, wsId: string, root: string) {
    // The remove button is a click, not a grab.
    if ((e.target as HTMLElement).closest('[data-no-drag]')) return;
    drag.begin(e, { kind: 'project', wsId, root, label: basename(root) });
  }

  function openProject(wsId: string, root: string) {
    onClose();
    void workspacesStore.switchToProject(wsId, root);
  }

  function select(wsId: string) {
    if (!drag.justDragged) selectedId = wsId;
  }
</script>

<div class="board" data-board class:dragging={drag.active} role="presentation">
  <!-- No `{@const}` here: `animate:flip` wants the column to be the only thing in the each body. -->
  {#each workspaces as ws, wi (ws.id)}
    <section
      class="col"
      class:selected={ws.id === selectedId}
      class:ghosted={drag.active && drag.item?.kind === 'workspace' && drag.item.wsId === ws.id}
      style:--ws={wsColorVar(ws.color_idx)}
      data-ws-col={ws.id}
      aria-label={ws.name || 'Workspace'}
      animate:flip={{ duration: 200 }}
    >
      {#if workspaceDrop === wi}<div class="col-line"></div>{/if}

      <div
        class="head"
        tabindex="0"
        role="button"
        aria-label={`${ws.name || 'Workspace'} — ${ws.projects.length} projects`}
        data-ws-head={ws.id}
        onpointerdown={(e) => drag.begin(e, { kind: 'workspace', wsId: ws.id, label: ws.name || 'Workspace' })}
        onclick={() => select(ws.id)}
        onkeydown={(e) => onHeaderKey(e, ws.id, wi)}
      >
        <GripVertical size={13} class="grip" />
        <Monogram name={ws.name || 'Workspace'} color={wsColorVar(ws.color_idx)} size={22} />
        <span class="name">{ws.name || 'Workspace'}</span>
        {#if ws.id === workspacesStore.activeId}<Check size={12} class="active-check" />{/if}
        <span class="count">{ws.projects.length}</span>
      </div>

      <div class="rows" role="listbox" aria-label={`Projects in ${ws.name || 'Workspace'}`}>
        <!-- The insertion line is a class on the row below the gap (`drop-before`), not an element of
             its own: `animate:flip` needs the row to be the only thing in the each body. -->
        {#each ws.projects as p, i (p.root)}
          <div
            class="proj"
            class:ghosted={drag.active && drag.item?.kind === 'project' && drag.item.wsId === ws.id && drag.item.root === p.root}
            class:drop-before={projectDrop(ws.id) === i}
            role="option"
            aria-selected="false"
            tabindex="0"
            data-proj-row
            data-proj-key={`${ws.id}|${p.root}`}
            onpointerdown={(e) => grabProject(e, ws.id, p.root)}
            ondblclick={() => openProject(ws.id, p.root)}
            onkeydown={(e) => onProjectKey(e, ws.id, p.root, i, ws.projects.length)}
            use:tooltip={p.root}
            animate:flip={{ duration: 180 }}
          >
            <Monogram name={basename(p.root)} color={wsColorVar(ws.color_idx)} size={20} />
            <span class="proj-name">{basename(p.root)}</span>
            <button
              type="button"
              class="remove"
              data-no-drag
              aria-label={`Remove ${basename(p.root)} from ${ws.name || 'Workspace'}`}
              onclick={() => workspacesStore.removeProjectFrom(ws.id, p.root)}
            ><X size={12} /></button>
          </div>
        {/each}
        {#if projectDrop(ws.id) === ws.projects.length && ws.projects.length > 0}<div class="line"></div>{/if}
        {#if ws.projects.length === 0}
          <div class="empty" class:drop-target={projectDrop(ws.id) === 0}>Drop a project here</div>
        {/if}
      </div>

      <button type="button" class="add" onclick={() => onAddProject(ws.id)}>
        <FolderPlus size={13} /> Add project…
      </button>
    </section>
  {/each}
  {#if workspaceDrop === workspaces.length}<div class="col-line tail"></div>{/if}

  <button type="button" class="new-col" onclick={onNewWorkspace}>
    <Plus size={16} />
    <span>New workspace</span>
  </button>
</div>

<!-- The card under the pointer. Fixed and pointer-events: none, so the hit test sees what is below. -->
{#if drag.active && drag.item}
  <div class="ghost" style:left="{drag.x + 12}px" style:top="{drag.y + 12}px">
    {drag.item.label}
  </div>
{/if}

<style>
  .board {
    flex: 1; min-width: 0; display: flex; align-items: stretch; gap: 10px;
    padding: 14px; overflow-x: auto; overflow-y: hidden;
    font-family: var(--font-ui-sans);
  }
  .board.dragging { cursor: grabbing; user-select: none; }

  .col {
    position: relative; flex: 0 0 236px; display: flex; flex-direction: column; min-height: 0;
    background: var(--bg-elevated); border: 1px solid var(--border-subtle);
    border-top: 3px solid var(--ws); border-radius: var(--radius-lg);
    transition: border-color var(--transition-fast), box-shadow var(--transition-fast), opacity var(--transition-fast);
  }
  .col.selected { border-color: color-mix(in srgb, var(--ws) 60%, var(--border-subtle)); box-shadow: 0 0 0 1px color-mix(in srgb, var(--ws) 40%, transparent); }
  .col.ghosted { opacity: 0.4; }

  .head {
    display: flex; align-items: center; gap: 8px; padding: 10px 10px 8px; cursor: grab;
    color: var(--text-primary); border-radius: var(--radius-md, 8px) var(--radius-md, 8px) 0 0;
    outline-offset: -2px;
  }
  .head:hover { background: var(--bg-hover); }
  .head:focus-visible, .proj:focus-visible { outline: 2px solid var(--accent); }
  :global(.head .grip) { color: var(--text-disabled); flex-shrink: 0; }
  .name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--font-size-sm); font-weight: 600; }
  :global(.head .active-check) { color: var(--accent); flex-shrink: 0; }
  .count {
    font-size: var(--font-size-2xs); color: var(--text-muted); background: var(--bg-overlay);
    padding: 1px 7px; border-radius: 9px; font-variant-numeric: tabular-nums; flex-shrink: 0;
  }

  .rows { flex: 1; min-height: 48px; overflow-y: auto; padding: 2px 6px 4px; display: flex; flex-direction: column; gap: 3px; }
  .proj {
    display: flex; align-items: center; gap: 8px; padding: 7px 8px; cursor: grab;
    background: var(--bg-base); border: 1px solid var(--border-subtle); border-radius: var(--radius-md, 8px);
    transition: background var(--transition-fast), border-color var(--transition-fast), opacity var(--transition-fast);
    animation: card-in 180ms cubic-bezier(0.2, 0, 0, 1) backwards;
  }
  .proj:hover { background: var(--bg-hover); border-color: var(--border); }
  .proj.ghosted { opacity: 0.35; }
  /* The insertion line above this row — where a drop would land. */
  .proj { position: relative; }
  .proj.drop-before::before {
    content: ''; position: absolute; left: 2px; right: 2px; top: -3px; height: 2px; border-radius: 2px;
    background: var(--accent); box-shadow: 0 0 8px color-mix(in srgb, var(--accent) 60%, transparent);
  }
  .empty.drop-target { border-color: var(--accent); color: var(--accent); background: var(--accent-subtle); }
  .proj-name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--font-size-sm); color: var(--text-primary); }
  .remove {
    display: flex; align-items: center; justify-content: center; width: 20px; height: 20px; flex-shrink: 0;
    border: 0; background: transparent; color: var(--text-muted); border-radius: var(--radius-sm); cursor: pointer;
    opacity: 0; transition: opacity var(--transition-fast), background var(--transition-fast), color var(--transition-fast);
  }
  .proj:hover .remove, .proj:focus-within .remove { opacity: 1; }
  .remove:hover { background: var(--error-subtle); color: var(--error); }
  .empty { padding: 14px 8px; text-align: center; font-size: var(--font-size-xs); color: var(--text-disabled); border: 1px dashed var(--border-subtle); border-radius: var(--radius-md, 8px); }

  /* Insertion lines — where a drop would land. */
  .line, .col-line {
    flex-shrink: 0; background: var(--accent); border-radius: 2px;
    box-shadow: 0 0 8px color-mix(in srgb, var(--accent) 60%, transparent);
    animation: line-in 120ms ease backwards;
  }
  .line { height: 2px; margin: -1px 2px; }
  .col-line { position: absolute; left: -7px; top: 0; bottom: 0; width: 3px; }
  .col-line.tail { position: static; align-self: stretch; margin-left: -7px; margin-right: 4px; }

  .add {
    display: flex; align-items: center; justify-content: center; gap: 6px; margin: 4px 6px 6px; padding: 6px;
    border: 1px dashed var(--border); border-radius: var(--radius-md, 8px); background: transparent;
    color: var(--text-muted); font-size: var(--font-size-xs); cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
  }
  .add:hover { background: var(--bg-hover); color: var(--text-primary); border-color: var(--accent); }

  .new-col {
    flex: 0 0 150px; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px;
    border: 1px dashed var(--border); border-radius: var(--radius-lg); background: transparent;
    color: var(--text-muted); font-size: var(--font-size-sm); cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
  }
  .new-col:hover { background: var(--bg-hover); color: var(--text-primary); border-color: var(--accent); }

  .ghost {
    position: fixed; z-index: 2000; pointer-events: none; padding: 6px 12px;
    background: var(--bg-elevated); border: 1px solid var(--accent); border-radius: var(--radius-md, 8px);
    color: var(--text-primary); font-size: var(--font-size-sm); font-weight: 600;
    box-shadow: 0 10px 28px rgba(0, 0, 0, 0.45); transform: rotate(-2deg) scale(1.03);
  }

  @keyframes card-in { from { opacity: 0; transform: translateY(5px); } to { opacity: 1; transform: none; } }
  @keyframes line-in { from { opacity: 0; transform: scaleX(0.6); } to { opacity: 1; transform: none; } }
  @media (prefers-reduced-motion: reduce) {
    .proj, .line, .col-line { animation: none; }
  }
</style>
