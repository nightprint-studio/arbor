<script lang="ts">
  /**
   * Manage nemus **project workspaces** — Arbor-style named groups of `.nemus`
   * projects with a colour, switchable from the title bar. Create / rename /
   * recolour / delete a workspace and manage its member projects (add the open
   * project, or any recent; remove members). Keyboard-first: the new-workspace
   * field auto-focuses, Enter creates; Esc closes (Modal).
   */
  import { Layers, Plus, Trash2, Check, FolderGit2, X, FolderOpen } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { workspaceStore, WORKSPACE_COLORS, workspaceColor } from '../stores/workspace.svelte';
  import { projectStore } from '../stores/project.svelte';

  let { onClose }: { onClose: () => void } = $props();

  let newName = $state('');

  const workspaces = $derived(workspaceStore.workspaces);
  const openPath = $derived(projectStore.project?.path ?? null);

  /** Last path segment, for a readable project label. */
  function basename(path: string): string {
    const parts = path.split(/[\\/]/).filter(Boolean);
    return parts[parts.length - 1] ?? path;
  }

  function create() {
    const name = newName.trim();
    if (!name) return;
    workspaceStore.createWorkspace(name);
    newName = '';
  }
  function onNewKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') { e.preventDefault(); create(); }
  }
</script>

<Modal {onClose} width="640px" height="540px" ariaLabel="Workspaces">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Layers size={14} />
      <span class="modal-title">Workspaces</span>
    </ModalHeader>
  {/snippet}

  <div class="ws">
    <p class="ws-lead">Group your <code>.nemus</code> projects into named, colour-coded sets — switch between them from the title bar.</p>

    <div class="ws-new">
      <Input bind:value={newName} autofocus placeholder="New workspace name…" onkeydown={onNewKeydown} ariaLabel="New workspace name" />
      <Button variant="primary" disabled={!newName.trim()} onclick={create}>
        <Plus size={14} /> Create
      </Button>
    </div>

    {#if workspaces.length === 0}
      <EmptyState message="No workspaces yet. Create one above, then add projects to it." />
    {:else}
      <div class="ws-list">
        {#each workspaces as w (w.id)}
          {@const active = workspaceStore.activeWorkspace === w.id}
          <div class="ws-card" class:active style="--wc: {workspaceColor(w.color_idx)}">
            <div class="ws-card-head">
              <button
                class="ws-active"
                class:on={active}
                onclick={() => workspaceStore.setActiveWorkspace(active ? null : w.id)}
                use:tooltip={active ? 'Active — click to clear' : 'Make active'}
                aria-pressed={active}
                aria-label="Toggle active workspace"
              >
                {#if active}<Check size={12} />{/if}
              </button>
              <input
                class="ws-name"
                value={w.name}
                onchange={(e) => workspaceStore.renameWorkspace(w.id, e.currentTarget.value)}
                aria-label="Workspace name"
              />
              <button class="ws-del" onclick={() => workspaceStore.deleteWorkspace(w.id)} use:tooltip={'Delete workspace'} aria-label="Delete workspace"><Trash2 size={13} /></button>
            </div>

            <div class="ws-colors" role="group" aria-label="Workspace colour">
              {#each WORKSPACE_COLORS as c, i (c)}
                <button
                  class="ws-swatch"
                  class:sel={w.color_idx === i}
                  style="--s: {c}"
                  onclick={() => workspaceStore.setWorkspaceColor(w.id, i)}
                  aria-label="Colour {i + 1}"
                ></button>
              {/each}
            </div>

            <div class="ws-projects">
              {#if w.project_paths.length === 0}
                <span class="ws-empty">No projects yet.</span>
              {:else}
                {#each w.project_paths as p (p)}
                  <div class="ws-proj">
                    <FolderGit2 size={12} />
                    <span class="ws-proj-name" use:tooltip={p}>{basename(p)}</span>
                    <button class="ws-proj-x" onclick={() => workspaceStore.removeProjectFromWorkspace(w.id, p)} aria-label="Remove project"><X size={11} /></button>
                  </div>
                {/each}
              {/if}
            </div>

            {#if openPath && !w.project_paths.includes(openPath)}
              <button class="ws-add" onclick={() => workspaceStore.addProjectToWorkspace(w.id, openPath)}>
                <FolderOpen size={12} /> Add current project
              </button>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  </div>

  {#snippet footer()}
    <Button variant="ghost" onclick={onClose}>Done</Button>
  {/snippet}
</Modal>

<style>
  .modal-title { font-size: 13px; font-weight: 600; color: var(--text-primary); }
  .ws { display: flex; flex-direction: column; gap: 12px; padding: 4px 2px; }
  .ws-lead { font-size: 12px; color: var(--text-muted); margin: 0; line-height: 1.5; }
  .ws-lead code { font-family: var(--font-code); font-size: 11px; }

  .ws-new { display: flex; gap: 8px; align-items: center; }
  /* Let the field (whatever wrapper Input renders) take the row; the button hugs. */
  .ws-new > :global(:first-child) { flex: 1; min-width: 0; }

  .ws-list { display: flex; flex-direction: column; gap: 10px; overflow-y: auto; }
  .ws-card {
    border: 1px solid var(--border-subtle);
    border-left: 3px solid var(--wc);
    border-radius: var(--radius-md);
    padding: 10px 12px;
    background: var(--bg-elevated);
    display: flex; flex-direction: column; gap: 8px;
  }
  .ws-card.active { box-shadow: 0 0 0 1px var(--wc), 0 0 12px color-mix(in srgb, var(--wc) 22%, transparent); }

  .ws-card-head { display: flex; align-items: center; gap: 8px; }
  .ws-active {
    display: flex; align-items: center; justify-content: center;
    width: 20px; height: 20px; flex-shrink: 0;
    border: 1px solid var(--border); border-radius: 50%;
    background: transparent; color: var(--wc); cursor: pointer;
    transition: background var(--transition-fast), border-color var(--transition-fast);
  }
  .ws-active.on { background: var(--wc); border-color: var(--wc); color: #14151a; }
  .ws-name {
    flex: 1; min-width: 0;
    background: transparent; border: none; outline: none;
    color: var(--text-primary); font-size: 13px; font-weight: 600;
    font-family: var(--font-ui-sans);
    padding: 2px 4px; border-radius: var(--radius-sm);
  }
  .ws-name:hover, .ws-name:focus { background: var(--bg-input); }
  .ws-del {
    display: flex; align-items: center; justify-content: center;
    width: 24px; height: 24px; flex-shrink: 0;
    background: transparent; border: none; border-radius: var(--radius-sm);
    color: var(--text-muted); cursor: pointer;
  }
  .ws-del:hover { background: var(--error-subtle); color: var(--error); }

  .ws-colors { display: flex; gap: 5px; }
  .ws-swatch {
    width: 16px; height: 16px; border-radius: 50%;
    border: 2px solid transparent; background: var(--s); cursor: pointer;
    padding: 0;
    transition: transform var(--transition-fast);
  }
  .ws-swatch:hover { transform: scale(1.15); }
  .ws-swatch.sel { border-color: var(--text-primary); }

  .ws-projects { display: flex; flex-wrap: wrap; gap: 5px; }
  .ws-empty { font-size: 11px; color: var(--text-disabled); font-style: italic; }
  .ws-proj {
    display: flex; align-items: center; gap: 5px;
    padding: 3px 4px 3px 7px;
    background: var(--bg-input); border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm); color: var(--text-secondary);
    font-size: 11.5px; max-width: 220px;
  }
  .ws-proj-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ws-proj-x {
    display: flex; align-items: center; justify-content: center;
    width: 16px; height: 16px; flex-shrink: 0;
    background: transparent; border: none; border-radius: 50%;
    color: var(--text-muted); cursor: pointer;
  }
  .ws-proj-x:hover { background: var(--bg-hover); color: var(--text-primary); }

  .ws-add {
    align-self: flex-start;
    display: flex; align-items: center; gap: 5px;
    padding: 4px 8px;
    background: transparent; border: 1px dashed var(--border);
    border-radius: var(--radius-sm); color: var(--text-secondary);
    font-size: 11.5px; cursor: pointer;
    transition: border-color var(--transition-fast), color var(--transition-fast);
  }
  .ws-add:hover { border-color: var(--wc); color: var(--text-primary); }
</style>
