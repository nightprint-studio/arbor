<script lang="ts">
  /**
   * BennuWorkspaceManagerModal — see every workspace at once and rearrange them.
   *
   * A **board**: one column per workspace, one card per project. Moving a project to another
   * workspace is a drag from one column to the other (or <kbd>Alt</kbd>+<kbd>←</kbd>/<kbd>→</kbd>);
   * reordering projects or workspaces is the same drag within a column or by the header. The
   * board itself is `workspaces/BennuWorkspaceBoard.svelte`.
   *
   * The panel on the right is the selected workspace's own settings — name, colour, switching to
   * it, deleting it — which are about one workspace and so do not belong on the board.
   *
   * All state lives in `workspacesStore`; this modal is wiring. Adding a project routes through the
   * store (which switches to the target workspace first so the add goes through projectStore's live
   * open/index path). Deleting asks via the shared ConfirmModal.
   */
  import { ArrowRightLeft, FolderCode, Trash2, MousePointer2 } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Monogram from '$lib/components/shared/ui/Monogram.svelte';
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import BennuWorkspaceBoard from './workspaces/BennuWorkspaceBoard.svelte';
  import { workspacesStore, wsColorVar, WS_COLOR_COUNT } from '$lib/stores/bennu/workspaces.svelte';

  let { onClose }: { onClose: () => void } = $props();

  const colorIndices = Array.from({ length: WS_COLOR_COUNT }, (_, i) => i);

  // Which workspace's settings are shown. Defaults to the active one; falls back to the first when
  // the selection is deleted.
  let selectedId = $state<string>(workspacesStore.activeId);
  const selected = $derived(
    workspacesStore.workspaces.find((w) => w.id === selectedId) ?? workspacesStore.workspaces[0] ?? null,
  );

  /** Which workspace the folder picker is adding to; null when it is closed. */
  let pickerFor = $state<string | null>(null);
  let confirmDeleteId = $state<string | null>(null);
  const confirmTarget = $derived(
    confirmDeleteId ? workspacesStore.workspaces.find((w) => w.id === confirmDeleteId) ?? null : null,
  );

  async function createWorkspace() {
    selectedId = await workspacesStore.create('New workspace');
  }

  async function switchTo(id: string) {
    await workspacesStore.switchTo(id);
    selectedId = id;
  }

  async function addProject(dir: string) {
    const target = pickerFor;
    pickerFor = null;
    if (target) await workspacesStore.addProjectTo(target, dir);
  }

  function confirmDelete() {
    if (confirmDeleteId) void workspacesStore.remove(confirmDeleteId);
    confirmDeleteId = null;
  }
</script>

<Modal {onClose} width="min(1180px, 96vw)" height="min(620px, 88vh)" padBody={false} ariaLabel="Manage workspaces">
  {#snippet header()}
    <ModalHeader {onClose}>
      <FolderCode size={15} />
      <span class="hdr-title">Workspaces</span>
      <span class="hdr-sub">Drag projects between workspaces · drag to reorder</span>
    </ModalHeader>
  {/snippet}

  <div class="mgr">
    <BennuWorkspaceBoard
      bind:selectedId
      onAddProject={(id) => (pickerFor = id)}
      onNewWorkspace={createWorkspace}
      {onClose}
    />

    <!-- ── Right: the selected workspace's own settings ─────────────────────── -->
    <aside class="detail" aria-label="Workspace settings">
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
              onchange={(name) => workspacesStore.rename(ws.id, name)}
            />
          </div>
        </div>

        {#if isActive}
          <span class="active-pill">Active workspace</span>
        {:else}
          <Button variant="secondary" size="sm" block onclick={() => switchTo(ws.id)}>
            {#snippet iconStart()}<ArrowRightLeft size={13} />{/snippet}
            Switch to this workspace
          </Button>
        {/if}

        <div class="field-label">Colour</div>
        <div class="swatches" role="group" aria-label="Workspace colour">
          {#each colorIndices as i (i)}
            <button
              type="button"
              class="swatch"
              class:on={ws.color_idx === i}
              style="--sw: {wsColorVar(i)}"
              aria-label={`Colour ${i + 1}`}
              aria-pressed={ws.color_idx === i}
              onclick={() => workspacesStore.setColor(ws.id, i)}
            ></button>
          {/each}
        </div>

        <div class="spacer"></div>

        <Button variant="ghost" size="sm" block onclick={() => (confirmDeleteId = ws.id)}>
          {#snippet iconStart()}<Trash2 size={14} />{/snippet}
          Delete workspace
        </Button>
      {:else}
        <div class="detail-empty">
          <MousePointer2 size={26} />
          <p>Create a workspace to group projects and switch between them instantly.</p>
        </div>
      {/if}

      <div class="keys">
        <div class="field-label">Keyboard</div>
        <div class="key-row"><Kbd keys={['Alt', '←', '→']} size="sm" /> <span>move to another workspace</span></div>
        <div class="key-row"><Kbd keys={['Alt', '↑', '↓']} size="sm" /> <span>reorder</span></div>
        <div class="key-row"><Kbd keys={['Enter']} size="sm" /> <span>open</span></div>
        <div class="key-row"><Kbd keys={['Del']} size="sm" /> <span>remove from workspace</span></div>
      </div>
    </aside>
  </div>
</Modal>

{#if pickerFor}
  <FileExplorerModal
    mode="folder"
    title="Add project to workspace"
    onConfirm={addProject}
    onCancel={() => (pickerFor = null)}
    onClose={() => (pickerFor = null)}
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
  .hdr-title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }
  .hdr-sub   { flex: 1; font-size: var(--font-size-xs); color: var(--text-muted); }

  .mgr { display: flex; height: 100%; min-height: 0; font-family: var(--font-ui-sans); background: var(--bg-base); }

  .detail {
    flex: 0 0 250px; display: flex; flex-direction: column; gap: 10px; padding: 16px;
    border-left: 1px solid var(--border); background: var(--bg-elevated); min-width: 0; overflow-y: auto;
  }
  .detail-head { display: flex; align-items: center; gap: 10px; }
  .detail-name { flex: 1; min-width: 0; }
  .active-pill {
    align-self: flex-start; font-size: var(--font-size-2xs); font-weight: 700; text-transform: uppercase;
    letter-spacing: 0.5px; color: var(--accent); background: var(--accent-subtle);
    border: 1px solid color-mix(in srgb, var(--accent) 25%, transparent);
    border-radius: var(--radius-sm); padding: 3px 8px;
  }

  .field-label {
    font-size: var(--font-size-2xs); font-weight: 600; text-transform: uppercase; letter-spacing: 0.7px;
    color: var(--text-disabled); margin: 8px 0 4px;
  }
  .swatches { display: flex; flex-wrap: wrap; gap: 8px; }
  .swatch {
    width: 22px; height: 22px; border-radius: var(--radius-sm); cursor: pointer;
    background: var(--sw); border: 2px solid transparent;
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--sw) 40%, transparent);
    transition: transform var(--transition-fast), border-color var(--transition-fast);
  }
  .swatch:hover { transform: scale(1.12); }
  .swatch.on { border-color: var(--text-primary); }

  .spacer { flex: 1; min-height: 8px; }

  .keys { border-top: 1px solid var(--border-subtle); padding-top: 4px; }
  .key-row { display: flex; align-items: center; gap: 8px; margin: 5px 0; font-size: var(--font-size-2xs); color: var(--text-muted); }

  .detail-empty {
    display: flex; flex-direction: column; align-items: center; gap: 10px; padding: 20px 4px;
    color: var(--text-muted); text-align: center;
  }
  .detail-empty p { margin: 0; font-size: var(--font-size-sm); line-height: 1.5; }
</style>
