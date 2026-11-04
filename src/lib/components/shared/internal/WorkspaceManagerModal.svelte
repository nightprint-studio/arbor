<script lang="ts">
  /**
   * WorkspaceManagerModal — see every workspace at once and rearrange them. One widget for every product that
   * groups projects in workspaces; it owns no store and no backend, it is given the workspaces and calls back.
   *
   * A **board** (`WorkspaceManagerBoard`): a column per workspace, a card per project. Moving a project to
   * another workspace is a drag (or <kbd>Alt</kbd>+<kbd>←</kbd>/<kbd>→</kbd>); reordering is the same drag inside
   * a column, or by a header. The panel on the right is the selected workspace's own settings — name, colour,
   * switching to it, deleting it.
   *
   * Keyboard, from anywhere in the dialog: <kbd>Ctrl</kbd>+<kbd>F</kbd> filters the projects,
   * <kbd>Ctrl</kbd>+<kbd>N</kbd> makes a workspace, <kbd>Ctrl</kbd>+<kbd>O</kbd> adds a project to the selected one.
   * On a card or a header the board's own keys apply (listed in the panel).
   *
   * The folder picker for "add a project" and the confirmation for deleting are this widget's; the product says
   * only what adding and deleting *do* (onAddProject, onDelete).
   */
  import { ArrowRightLeft, FolderCode, MousePointer2, Trash2 } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import ColorPalettePicker from '$lib/components/shared/ui/ColorPalettePicker.svelte';
  import FilterInput from '$lib/components/shared/ui/FilterInput.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Monogram from '$lib/components/shared/ui/Monogram.svelte';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import Kbd from './Kbd.svelte';
  import WorkspaceManagerBoard from './WorkspaceManagerBoard.svelte';
  import type { WorkspaceManagerItem } from './workspace-manager';

  interface Props {
    workspaces: WorkspaceManagerItem[];
    activeId: string;
    colorCount: number;
    colorVar: (idx: number) => string;
    onClose: () => void;
    /** Make a workspace; resolves with its id so the panel can show it. */
    onCreate: () => string | Promise<string>;
    onSwitchTo: (id: string) => void | Promise<void>;
    onRename: (id: string, name: string) => void;
    onColor: (id: string, idx: number) => void;
    onDelete: (id: string) => void;
    /** A folder was picked for a workspace. */
    onAddProject: (wsId: string, root: string) => void | Promise<void>;
    onRemoveProject: (wsId: string, root: string) => void;
    onOpenProject: (wsId: string, root: string) => void;
    onMoveProject: (fromId: string, root: string, toId: string, index: number) => void | Promise<void>;
    onMoveWorkspace: (id: string, index: number) => void;
  }

  let { workspaces, activeId, colorCount, colorVar, onClose, onCreate, onSwitchTo, onRename, onColor, onDelete, onAddProject, onRemoveProject, onOpenProject, onMoveProject, onMoveWorkspace }: Props = $props();

  const colors = $derived(Array.from({ length: colorCount }, (_, i) => colorVar(i)));

  // svelte-ignore state_referenced_locally
  let selectedId = $state<string>(activeId);
  const selected = $derived(workspaces.find((w) => w.id === selectedId) ?? workspaces[0] ?? null);

  let query = $state('');
  let filterEl = $state<HTMLInputElement | null>(null);
  /** Which workspace the folder picker is adding to; null when it is closed. */
  let pickerFor = $state<string | null>(null);
  let confirmDeleteId = $state<string | null>(null);
  const confirmTarget = $derived(confirmDeleteId ? workspaces.find((w) => w.id === confirmDeleteId) ?? null : null);

  async function create() {
    selectedId = await onCreate();
  }

  async function switchTo(id: string) {
    await onSwitchTo(id);
    selectedId = id;
  }

  function onKey(e: KeyboardEvent) {
    if (pickerFor || confirmDeleteId || !(e.ctrlKey || e.metaKey) || e.shiftKey) return;
    const key = e.key.toLowerCase();
    if (key === 'f') { e.preventDefault(); filterEl?.focus(); }
    else if (key === 'n') { e.preventDefault(); void create(); }
    else if (key === 'o' && selected) { e.preventDefault(); pickerFor = selected.id; }
  }
</script>

<svelte:window onkeydown={onKey} />

<Modal {onClose} width="min(1180px, 96vw)" height="min(620px, 88vh)" padBody={false} ariaLabel="Manage workspaces">
  {#snippet header()}
    <ModalHeader {onClose}>
      <FolderCode size={15} />
      <span class="hdr-title">Workspaces</span>
      <span class="hdr-sub">Drag projects between workspaces · drag to reorder</span>
      <div class="hdr-filter"><FilterInput bind:value={query} bind:element={filterEl} placeholder="Filter projects…" ariaLabel="Filter projects" /></div>
    </ModalHeader>
  {/snippet}

  <div class="mgr">
    <WorkspaceManagerBoard
      {workspaces}
      {activeId}
      {colorVar}
      {query}
      bind:selectedId
      onAddProject={(id) => (pickerFor = id)}
      onNewWorkspace={() => void create()}
      {onOpenProject}
      {onRemoveProject}
      {onMoveProject}
      {onMoveWorkspace}
      {onClose}
    />

    <aside class="detail" aria-label="Workspace settings">
      {#if selected}
        {@const ws = selected}
        <div class="detail-head">
          <Monogram name={ws.name || 'Workspace'} color={colorVar(ws.colorIdx)} size={34} />
          <div class="detail-name">
            <Input value={ws.name} placeholder="Workspace name" ariaLabel="Workspace name" onchange={(name) => onRename(ws.id, name)} />
          </div>
        </div>

        {#if ws.id === activeId}
          <span class="active-pill">Active workspace</span>
        {:else}
          <Button variant="secondary" size="sm" block onclick={() => void switchTo(ws.id)}>
            {#snippet iconStart()}<ArrowRightLeft size={13} />{/snippet}
            Switch to this workspace
          </Button>
        {/if}

        <div class="field-label">Colour</div>
        <ColorPalettePicker {colors} value={ws.colorIdx} labelPrefix="Colour" ariaLabel="Workspace colour" size={22} onChange={(i) => onColor(ws.id, i)} />

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
        <div class="key-row"><Kbd keys={['Ctrl', 'F']} size="sm" /> <span>filter projects</span></div>
        <div class="key-row"><Kbd keys={['Ctrl', 'N']} size="sm" /> <span>new workspace</span></div>
        <div class="key-row"><Kbd keys={['Ctrl', 'O']} size="sm" /> <span>add a project to the selected one</span></div>
        <div class="key-row"><Kbd keys={['Alt', '←', '→']} size="sm" /> <span>move to another workspace</span></div>
        <div class="key-row"><Kbd keys={['Alt', '↑', '↓']} size="sm" /> <span>reorder</span></div>
        <div class="key-row"><Kbd keys={['Enter']} size="sm" /> <span>open</span></div>
        <div class="key-row"><Kbd keys={['Del']} size="sm" /> <span>remove from workspace</span></div>
      </div>
    </aside>
  </div>
</Modal>

{#if pickerFor}
  {@const target = pickerFor}
  <FileExplorerModal
    mode="folder"
    title="Add project to workspace"
    onConfirm={(dir: string) => { pickerFor = null; void onAddProject(target, dir); }}
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
    onConfirm={() => { if (confirmDeleteId) onDelete(confirmDeleteId); confirmDeleteId = null; }}
    onCancel={() => (confirmDeleteId = null)}
  />
{/if}

<style>
  .hdr-title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }
  .hdr-sub   { flex: 1; font-size: var(--font-size-xs); color: var(--text-muted); }
  .hdr-filter { width: 220px; flex: none; }

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

  .spacer { flex: 1; min-height: 8px; }

  .keys { border-top: 1px solid var(--border-subtle); padding-top: 4px; }
  .key-row { display: flex; align-items: center; gap: 8px; margin: 5px 0; font-size: var(--font-size-2xs); color: var(--text-muted); }

  .detail-empty {
    display: flex; flex-direction: column; align-items: center; gap: 10px; padding: 20px 4px;
    color: var(--text-muted); text-align: center;
  }
  .detail-empty p { margin: 0; font-size: var(--font-size-sm); line-height: 1.5; }
</style>
