<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { FolderSearch, Download, RefreshCw } from 'lucide-svelte';
  import FileExplorerModal from '../shared/FileExplorerModal.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { workspacesStore } from '$lib/stores/workspaces.svelte';
  import { relocateRepo, validateRepoPath } from '$lib/ipc/missing';
  import type { RepoRegistryEntry } from '$lib/types/workspace';

  // Prominent, always-visible Locate / Clone actions for a workspace member
  // whose path no longer resolves on disk.  Both routes converge on the same
  // backend `relocate_repo` command, which re-points the EXISTING registry
  // entry at the recovered folder — so the repo id, workspace membership and
  // tab snapshot all survive — and fires `on_project_relocated`.
  interface Props {
    entry:      RepoRegistryEntry;
    /** Called after a successful relink so the parent can refresh health /
     *  path-status and drop the warning state for this row. */
    onResolved: () => void;
  }
  let { entry, onResolved }: Props = $props();

  let picker = $state<null | 'locate' | 'clone-dest'>(null);
  let busy   = $state<null | 'locate' | 'clone'>(null);

  function joinPath(base: string, name: string): string {
    if (!base) return name;
    if (!name) return base;
    const sep = base.includes('\\') ? '\\' : '/';
    return base.replace(/[\\/]+$/, '') + sep + name;
  }

  async function doLocate(newPath: string) {
    picker = null;
    busy = 'locate';
    try {
      const v = await validateRepoPath(newPath);
      if (v.status !== 'ok') {
        uiStore.showToast(v.message || 'The selected folder is not a git repository', 'error');
        return;
      }
      await relocateRepo(entry.id, newPath);
      await workspacesStore.reloadRegistry();
      uiStore.showToast(`Relocated "${entry.display_name}"`, 'success');
      onResolved();
    } catch (e) {
      uiStore.showToast(`Locate failed: ${e}`, 'error');
    } finally {
      busy = null;
    }
  }

  async function doClone(parentDir: string) {
    picker = null;
    if (!entry.remote_url) return;
    // Clone into <parent>/<display-name>, mirroring CloneRepoModal's leaf
    // convention, then re-point the existing entry at the fresh checkout.
    const dest = joinPath(parentDir.replace(/[\\/]+$/, ''), entry.display_name);
    busy = 'clone';
    try {
      await invoke<string>('clone_repo', {
        opts: {
          url: entry.remote_url,
          dest_path: dest,
          branch: null,
          shallow: false,
          recurse_submodules: false,
        },
      });
      await relocateRepo(entry.id, dest);
      await workspacesStore.reloadRegistry();
      uiStore.showToast(`Cloned "${entry.display_name}"`, 'success');
      onResolved();
    } catch (e) {
      uiStore.showToast(`Clone failed: ${e}`, 'error');
    } finally {
      busy = null;
    }
  }
</script>

<div class="relink-actions">
  <button class="relink-btn locate" onclick={() => picker = 'locate'} disabled={busy !== null}>
    {#if busy === 'locate'}<RefreshCw size={12} class="spin" />{:else}<FolderSearch size={12} />{/if}
    <span>Locate…</span>
  </button>
  {#if entry.remote_url}
    <button class="relink-btn clone" onclick={() => picker = 'clone-dest'} disabled={busy !== null} use:tooltip={`Clone from ${entry.remote_url}`}>
      {#if busy === 'clone'}<RefreshCw size={12} class="spin" />{:else}<Download size={12} />{/if}
      <span>Clone…</span>
    </button>
  {/if}
</div>

{#if picker === 'locate'}
  <FileExplorerModal
    mode="folder"
    title={`Locate "${entry.display_name}"`}
    initialPath={entry.path}
    onConfirm={doLocate}
    onCancel={() => picker = null}
  />
{:else if picker === 'clone-dest'}
  <FileExplorerModal
    mode="folder"
    title={`Clone destination for "${entry.display_name}"`}
    onConfirm={doClone}
    onCancel={() => picker = null}
  />
{/if}

<style>
  .relink-actions {
    display: flex;
    gap: 6px;
    flex-shrink: 0;
  }
  .relink-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 4px 10px;
    border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans);
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
    transition: background var(--transition-fast), border-color var(--transition-fast),
                color var(--transition-fast), filter var(--transition-fast);
  }
  .relink-btn:disabled { opacity: 0.6; cursor: not-allowed; }
  .relink-btn.locate {
    background: var(--accent);
    border: 1px solid var(--accent);
    color: var(--text-on-accent);
  }
  .relink-btn.locate:hover:not(:disabled) { filter: brightness(1.08); }
  .relink-btn.clone {
    background: transparent;
    border: 1px solid var(--border);
    color: var(--text-primary);
  }
  .relink-btn.clone:hover:not(:disabled) { background: var(--bg-hover); border-color: var(--accent); }
</style>
