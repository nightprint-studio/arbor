<script lang="ts">
  /**
   * BennuWorkspaceManagerModal — create / rename / recolor / delete workspaces and manage their
   * member projects. The Corvus-light equivalent of `WorkspaceManagementModal`: a two-pane layout
   * (workspace list on the left, selected-workspace detail on the right), no git-specific parts
   * (groups, health scan, fetch/pull, import/export).
   *
   * All state lives in `workspacesStore`; this modal is pure presentation + wiring. Adding a project
   * routes through the store (which switches to the target workspace first so the add goes through
   * projectStore's live open/index path). Deleting asks via the shared ConfirmModal.
   */
  import { Plus, FolderPlus, X, Check, Trash2, ArrowRightLeft, FolderCode } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Monogram from '$lib/components/shared/ui/Monogram.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { workspacesStore, wsColorVar, WS_COLOR_COUNT } from '$lib/stores/bennu/workspaces.svelte';

  let { onClose }: { onClose: () => void } = $props();

  const colorIndices = Array.from({ length: WS_COLOR_COUNT }, (_, i) => i);

  // Which workspace's detail is shown on the right. Defaults to the active one; falls back to the
  // first when the selection is deleted.
  let selectedId = $state<string>(workspacesStore.activeId);
  const selected = $derived(
    workspacesStore.workspaces.find((w) => w.id === selectedId) ?? workspacesStore.workspaces[0] ?? null,
  );

  let pickerOpen = $state(false);
  let confirmDeleteId = $state<string | null>(null);
  const confirmTarget = $derived(
    confirmDeleteId ? workspacesStore.workspaces.find((w) => w.id === confirmDeleteId) ?? null : null,
  );

  function basename(path: string): string {
    const parts = path.split(/[\\/]/).filter(Boolean);
    return parts[parts.length - 1] ?? path;
  }

  async function createWorkspace() {
    const id = await workspacesStore.create('New workspace');
    selectedId = id;
  }

  async function switchTo(id: string) {
    await workspacesStore.switchTo(id);
    selectedId = id;
  }

  function renameSelected(name: string) {
    if (selected) workspacesStore.rename(selected.id, name);
  }

  async function addProject(dir: string) {
    pickerOpen = false;
    if (selected) await workspacesStore.addProjectTo(selected.id, dir);
  }

  function confirmDelete() {
    if (confirmDeleteId) void workspacesStore.remove(confirmDeleteId);
    confirmDeleteId = null;
  }
</script>

<Modal {onClose} width="720px" height="520px" padBody={false} ariaLabel="Manage workspaces">
  {#snippet header()}
    <ModalHeader {onClose}>
      <FolderCode size={15} />
      <span class="hdr-title">Workspaces</span>
      <span class="hdr-sub">Group projects · quick-switch · cross-project search</span>
    </ModalHeader>
  {/snippet}

  <div class="mgr">
    <!-- ── Left: workspace list ─────────────────────────────────────────────── -->
    <aside class="list" aria-label="Workspaces">
      <div class="list-scroll">
        {#each workspacesStore.workspaces as ws (ws.id)}
          {@const isActive = ws.id === workspacesStore.activeId}
          <button
            type="button"
            class="ws-row"
            class:selected={ws.id === selected?.id}
            onclick={() => (selectedId = ws.id)}
          >
            <Monogram name={ws.name || 'Workspace'} color={wsColorVar(ws.color_idx)} size={22} />
            <span class="ws-name">{ws.name || 'Workspace'}</span>
            {#if isActive}
              <Check size={13} class="ws-active-check" />
            {/if}
            <span class="ws-count">{ws.projects.length}</span>
          </button>
        {/each}
        {#if workspacesStore.workspaces.length === 0}
          <div class="list-empty">No workspaces yet.</div>
        {/if}
      </div>
      <div class="list-foot">
        <Button variant="ghost" size="sm" block onclick={createWorkspace}>
          {#snippet iconStart()}<Plus size={14} />{/snippet}
          New workspace
        </Button>
      </div>
    </aside>

    <!-- ── Right: selected workspace detail ─────────────────────────────────── -->
    <section class="detail">
      {#if selected}
        {@const ws = selected}
        {@const isActive = ws.id === workspacesStore.activeId}
        <div class="detail-head">
          <Monogram name={ws.name || 'Workspace'} color={wsColorVar(ws.color_idx)} size={34} />
          <div class="detail-name">
            <Input
              value={ws.name}
              placeholder="Workspace name"
              ariaLabel="Workspace name"
              onchange={renameSelected}
            />
          </div>
          {#if isActive}
            <span class="active-pill">Active</span>
          {:else}
            <Button variant="secondary" size="sm" onclick={() => switchTo(ws.id)}>
              {#snippet iconStart()}<ArrowRightLeft size={13} />{/snippet}
              Switch to
            </Button>
          {/if}
        </div>

        <!-- Color swatches -->
        <div class="field-label">Color</div>
        <div class="swatches" role="group" aria-label="Workspace color">
          {#each colorIndices as i (i)}
            <button
              type="button"
              class="swatch"
              class:on={ws.color_idx === i}
              style="--sw: {wsColorVar(i)}"
              aria-label={`Color ${i + 1}`}
              aria-pressed={ws.color_idx === i}
              onclick={() => workspacesStore.setColor(ws.id, i)}
            ></button>
          {/each}
        </div>

        <!-- Member projects -->
        <div class="field-label projects-label">
          Projects <span class="count-badge">{ws.projects.length}</span>
        </div>
        <div class="projects">
          {#each ws.projects as p (p.root)}
            <div class="proj-row">
              <FolderCode size={14} class="proj-icon" />
              <span class="proj-name">{basename(p.root)}</span>
              <span class="proj-path" use:tooltip={p.root}>{p.root}</span>
              <button
                type="button"
                class="proj-remove"
                aria-label={`Remove ${basename(p.root)}`}
                use:tooltip={'Remove from workspace'}
                onclick={() => workspacesStore.removeProjectFrom(ws.id, p.root)}
              >
                <X size={13} />
              </button>
            </div>
          {/each}
          {#if ws.projects.length === 0}
            <div class="proj-empty">No projects. Add one to build this workspace.</div>
          {/if}
        </div>

        <div class="detail-foot">
          <Button variant="ghost" size="sm" onclick={() => (pickerOpen = true)}>
            {#snippet iconStart()}<FolderPlus size={14} />{/snippet}
            Add project…
          </Button>
          <span class="spacer"></span>
          <Button
            variant="ghost"
            size="sm"
            title="Delete workspace"
            onclick={() => (confirmDeleteId = ws.id)}
          >
            {#snippet iconStart()}<Trash2 size={14} />{/snippet}
            Delete
          </Button>
        </div>
      {:else}
        <div class="detail-empty">
          <FolderCode size={30} />
          <p>Create a workspace to group projects and switch between them instantly.</p>
          <Button variant="primary" size="sm" onclick={createWorkspace}>
            {#snippet iconStart()}<Plus size={14} />{/snippet}
            New workspace
          </Button>
        </div>
      {/if}
    </section>
  </div>
</Modal>

{#if pickerOpen}
  <FileExplorerModal
    mode="folder"
    title="Add project to workspace"
    onConfirm={addProject}
    onCancel={() => (pickerOpen = false)}
    onClose={() => (pickerOpen = false)}
  />
{/if}

{#if confirmTarget}
  <ConfirmModal
    title="Delete workspace"
    message={`Delete “${confirmTarget.name || 'Workspace'}”?`}
    detail="The projects stay on disk — only this workspace grouping is removed."
    variant="danger"
    confirmLabel="Delete"
    zIndex="var(--z-menu)"
    onConfirm={confirmDelete}
    onCancel={() => (confirmDeleteId = null)}
  />
{/if}

<style>
  .hdr-title { font-size: 13px; font-weight: 600; color: var(--text-primary); }
  .hdr-sub   { flex: 1; font-size: 11px; color: var(--text-muted); }

  .mgr { display: flex; height: 100%; font-family: var(--font-ui-sans); }

  /* ── Left list ────────────────────────────────────────────────────────── */
  .list {
    flex: 0 0 240px; display: flex; flex-direction: column;
    border-right: 1px solid var(--border); background: var(--bg-elevated);
  }
  .list-scroll { flex: 1; overflow-y: auto; padding: 8px; display: flex; flex-direction: column; gap: 2px; }
  .list-foot { padding: 8px; border-top: 1px solid var(--border); }
  .ws-row {
    display: flex; align-items: center; gap: 10px; width: 100%;
    padding: 8px 10px; background: transparent; border: 1px solid transparent;
    border-radius: var(--radius-sm); cursor: pointer; text-align: left;
    color: var(--text-primary); transition: background var(--transition-fast);
  }
  .ws-row:hover { background: var(--bg-hover); }
  .ws-row.selected { background: var(--bg-hover); border-color: var(--border-subtle); }
  .ws-name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--font-size-sm); }
  :global(.ws-row .ws-active-check) { color: var(--accent); flex-shrink: 0; }
  .ws-count {
    font-size: 10px; color: var(--text-muted); background: var(--bg-overlay);
    padding: 1px 7px; border-radius: 9px; font-variant-numeric: tabular-nums; flex-shrink: 0;
  }
  .list-empty { padding: 20px 10px; text-align: center; font-size: 11px; color: var(--text-muted); }

  /* ── Right detail ─────────────────────────────────────────────────────── */
  .detail { flex: 1; display: flex; flex-direction: column; padding: 16px 18px; min-width: 0; }
  .detail-head { display: flex; align-items: center; gap: 12px; }
  .detail-name { flex: 1; min-width: 0; }
  .active-pill {
    font-size: 10px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.5px;
    color: var(--accent); background: var(--accent-subtle);
    border: 1px solid color-mix(in srgb, var(--accent) 25%, transparent);
    border-radius: var(--radius-sm); padding: 3px 8px; flex-shrink: 0;
  }

  .field-label {
    font-size: 10px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.7px;
    color: var(--text-disabled); margin: 16px 0 8px;
  }
  .projects-label { display: flex; align-items: center; gap: 8px; }
  .count-badge {
    font-size: 10px; color: var(--text-muted); background: var(--bg-overlay);
    padding: 1px 6px; border-radius: 8px; letter-spacing: 0;
  }

  .swatches { display: flex; flex-wrap: wrap; gap: 8px; }
  .swatch {
    width: 22px; height: 22px; border-radius: var(--radius-sm); cursor: pointer;
    background: var(--sw); border: 2px solid transparent;
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--sw) 40%, transparent);
    transition: transform var(--transition-fast), border-color var(--transition-fast);
  }
  .swatch:hover { transform: scale(1.1); }
  .swatch.on { border-color: var(--text-primary); }

  .projects { flex: 1; overflow-y: auto; display: flex; flex-direction: column; gap: 2px; min-height: 60px; }
  .proj-row {
    display: flex; align-items: center; gap: 8px; padding: 6px 8px;
    border-radius: var(--radius-sm); transition: background var(--transition-fast);
  }
  .proj-row:hover { background: var(--bg-hover); }
  :global(.proj-row .proj-icon) { color: var(--text-muted); flex-shrink: 0; }
  .proj-name { font-size: 12px; color: var(--text-primary); flex-shrink: 0; }
  .proj-path {
    flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-size: 10.5px; color: var(--text-muted); font-family: var(--font-code);
  }
  .proj-remove {
    display: flex; align-items: center; justify-content: center; flex-shrink: 0;
    width: 22px; height: 22px; border: none; background: transparent; cursor: pointer;
    color: var(--text-muted); border-radius: var(--radius-sm);
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .proj-remove:hover { background: var(--danger-subtle); color: var(--danger); }
  .proj-empty { padding: 16px 8px; font-size: 11px; color: var(--text-muted); }

  .detail-foot { display: flex; align-items: center; gap: 8px; padding-top: 12px; border-top: 1px solid var(--border); margin-top: 8px; }
  .spacer { flex: 1; }

  .detail-empty {
    flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center;
    gap: 12px; color: var(--text-muted); text-align: center;
  }
  .detail-empty p { margin: 0; font-size: 12px; max-width: 280px; line-height: 1.5; }
</style>
