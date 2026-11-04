<script lang="ts">
  /**
   * WorkspacesModal — workspace management, laid out like Corvus's: a single column of workspace cards, each
   * folded to a header (colour, name, "active", how many projects) with its actions, and unfolding to the
   * projects it holds. A workspace is only a list; deleting one never touches a project. Changes are applied
   * as they are made, so there is nothing to save.
   */
  import { ArrowRightLeft, FolderPlus, Layers, Plus, Trash2, X } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Collapsible from '$lib/components/shared/ui/Collapsible.svelte';
  import ColorPalettePicker from '$lib/components/shared/ui/ColorPalettePicker.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Monogram from '$lib/components/shared/ui/Monogram.svelte';
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
  import type { Workspace } from '$lib/types/hirundo';
  import { WS_COLOR_COUNT, wsColor } from '../request-model';

  let { onClose }: { onClose: () => void } = $props();

  let addingTo = $state<string | null>(null);
  let deleting = $state<Workspace | null>(null);
  const colors = Array.from({ length: WS_COLOR_COUNT }, (_, i) => wsColor(i));
  const base = (p: string) => p.split(/[\\/]/).filter(Boolean).pop() ?? p;
  const activeId = $derived(hirundoStore.config.active_workspace);

  async function rename(w: Workspace, name: string) {
    if (name.trim() && name.trim() !== w.name) await hirundoStore.updateWorkspace(w.id, name.trim(), w.color_idx);
  }
</script>

<Modal {onClose} width="900px" height="78vh" padBody={false} ariaLabel="Workspaces">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Layers size={14} />
      <span class="modal-title">Workspaces</span>
    </ModalHeader>
  {/snippet}

  <div class="wm">
    <div class="wm-toolbar">
      <span class="wm-count">{hirundoStore.workspaces.length} {hirundoStore.workspaces.length === 1 ? 'workspace' : 'workspaces'}</span>
      <Button variant="primary" size="sm" onclick={() => void hirundoStore.createWorkspace('New workspace')}>{#snippet iconStart()}<Plus size={13} />{/snippet}New workspace</Button>
    </div>

    <div class="wm-list">
      {#if hirundoStore.workspaces.length === 0}
        <EmptyState message="No workspaces" description="Open a project and a workspace is created for it, or make one here." />
      {/if}
      {#each hirundoStore.workspaces as w (w.id)}
        <div class="wm-card">
          <Collapsible open={w.id === activeId} chevron>
            {#snippet header()}
              <span class="wm-head">
                <Monogram name={w.name || 'Workspace'} size={20} color={wsColor(w.color_idx)} />
                <b class="wm-name">{w.name || 'Workspace'}</b>
                {#if w.id === activeId}<Badge variant="tone" tone="accent" size="sm" label="active" />{/if}
                <span class="wm-muted">{w.projects.length} {w.projects.length === 1 ? 'project' : 'projects'}</span>
                <span class="wm-spacer"></span>
                <span class="wm-actions" role="presentation" onclick={(e) => e.stopPropagation()}>
                  {#if w.id !== activeId}<IconButton tooltip="Switch to this workspace" size={24} onclick={() => void hirundoStore.switchWorkspace(w.id)}><ArrowRightLeft size={13} /></IconButton>{/if}
                  <IconButton tooltip="Delete the workspace (its projects stay)" size={24} variant="danger" onclick={() => (deleting = w)}><Trash2 size={13} /></IconButton>
                </span>
              </span>
            {/snippet}

            <div class="wm-body">
              <div class="wm-settings">
                <FormField label="Name"><Input value={w.name} onchange={(v) => void rename(w, v)} /></FormField>
                <FormField label="Colour"><ColorPalettePicker {colors} value={w.color_idx} labelPrefix="Colour" ariaLabel="Workspace colour" size={22} onChange={(i) => void hirundoStore.updateWorkspace(w.id, w.name, i)} /></FormField>
              </div>
              {#if w.projects.length === 0}
                <p class="wm-empty">No projects in this workspace.</p>
              {:else}
                <div class="wm-projects">
                  {#each w.projects as root (root)}
                    <SidebarItem selected={root === hirundoStore.project} onclick={() => void hirundoStore.switchToProject(w.id, root)}>
                      <b>{base(root)}</b>
                      <span class="wm-muted wm-path">{root}</span>
                      {#snippet actions()}<IconButton tooltip="Remove from this workspace" size={20} onclick={(e) => { e.stopPropagation(); void hirundoStore.removeProjectFrom(w.id, root); }}><X size={12} /></IconButton>{/snippet}
                    </SidebarItem>
                  {/each}
                </div>
              {/if}
              <div class="wm-add"><Button variant="outline" size="sm" onclick={() => (addingTo = w.id)}>{#snippet iconStart()}<FolderPlus size={12} />{/snippet}Add project…</Button></div>
            </div>
          </Collapsible>
        </div>
      {/each}
    </div>
  </div>

  {#snippet footer()}
    <ModalFooter><Button variant="primary" onclick={onClose}>Close</Button></ModalFooter>
  {/snippet}
</Modal>

{#if addingTo}
  {@const target = addingTo}
  <FileExplorerModal mode="folder" title="Choose a project folder" onConfirm={(p: string) => { addingTo = null; void hirundoStore.addProjectTo(target, p); }} onCancel={() => (addingTo = null)} onClose={() => (addingTo = null)} />
{/if}
{#if deleting}
  {@const doomed = deleting}
  <ConfirmModal title="Delete workspace" message={`Delete “${doomed.name}”?`} detail="Only the grouping is removed: the projects and their requests stay where they are." variant="danger" confirmLabel="Delete" onConfirm={() => { deleting = null; void hirundoStore.deleteWorkspace(doomed.id); }} onCancel={() => (deleting = null)} />
{/if}

<style>
  .wm { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .wm-toolbar { display: flex; align-items: center; justify-content: space-between; padding: 10px 16px; border-bottom: 1px solid var(--border-subtle); }
  .wm-count { font-size: var(--font-size-xs); color: var(--text-muted); }
  .wm-list { flex: 1; min-height: 0; overflow: auto; padding: 12px 16px; display: flex; flex-direction: column; gap: 8px; }
  .wm-card { border: 1px solid var(--border-subtle); border-radius: var(--radius-md); background: var(--bg-elevated); padding: 4px 8px; }
  .wm-head { display: flex; align-items: center; gap: 9px; flex: 1; min-width: 0; padding: 4px 0; }
  .wm-name { font-size: var(--font-size-sm); font-weight: 500; color: var(--text-primary); }
  .wm-muted { font-size: var(--font-size-xs); color: var(--text-muted); }
  .wm-spacer { flex: 1; }
  .wm-actions { display: inline-flex; gap: 2px; }
  .wm-body { padding: 4px 4px 10px; display: flex; flex-direction: column; gap: 8px; }
  .wm-settings { display: grid; grid-template-columns: 1fr auto; gap: 16px; align-items: end; }
  .wm-projects { display: flex; flex-direction: column; border: 1px solid var(--border-subtle); border-radius: var(--radius-sm); padding: 2px; background: var(--bg-base); }
  .wm-path { margin-left: 8px; }
  .wm-empty { margin: 0; font-size: var(--font-size-xs); color: var(--text-muted); }
  .wm-add { display: flex; }
</style>
