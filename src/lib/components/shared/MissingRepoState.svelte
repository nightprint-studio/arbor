<script lang="ts">
  import { FolderSearch, FolderX, RefreshCw, Trash2, Plug, AlertTriangle } from 'lucide-svelte';
  import { tabsStore, type RepoTab } from '$lib/stores/tabs.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { workspacesStore } from '$lib/stores/workspaces.svelte';
  import { openRepo } from '$lib/ipc/graph';
  import {
    relocateRepo, validateRepoPath, removeRecentRepo,
    type MissingProjectsConfig,
  } from '$lib/ipc/missing';
  import FilePickerModal from './FilePickerModal.svelte';
  import { closeRepo } from '$lib/ipc/graph';

  let { tab, config }: { tab: RepoTab; config: MissingProjectsConfig } = $props();

  let pickerOpen     = $state(false);
  let busyAction     = $state<null | 'locate' | 'retry' | 'remove'>(null);
  let confirmRemove  = $state(false);
  let recentValidationError = $state('');

  const reasonLabel = $derived.by(() => {
    switch (tab.tombstone?.reason) {
      case 'unreachable': return 'Drive or share unavailable';
      case 'not_a_repo':  return 'Folder is no longer a git repository';
      case 'missing':
      default:            return 'Folder no longer exists';
    }
  });
  const ReasonIcon = $derived(tab.tombstone?.reason === 'unreachable' ? AlertTriangle : FolderX);

  async function handleLocate(newPath: string) {
    pickerOpen = false;
    busyAction = 'locate';
    recentValidationError = '';
    try {
      const v = await validateRepoPath(newPath);
      if (v.status !== 'ok') {
        recentValidationError = v.message || 'The selected folder is not a valid git repository.';
        busyAction = null;
        return;
      }
      const result = await relocateRepo(tab.id, newPath);
      // Reopen the repo at its new path under the same tab id.
      const info = await openRepo(result.new_path, tab.id);
      tabsStore.updateTab(tab.id, {
        info,
        path:          info.path,
        name:          info.name,
        currentBranch: info.current_branch ?? null,
        tombstone:     null,
      });
      void workspacesStore.reloadRegistry().catch(() => {});
      uiStore.showToast(`Relocated to ${info.path}`, 'success');
    } catch (err) {
      recentValidationError = `${err}`;
    } finally {
      busyAction = null;
    }
  }

  async function handleRetry() {
    busyAction = 'retry';
    recentValidationError = '';
    try {
      const v = await validateRepoPath(tab.path);
      if (v.status === 'ok') {
        const info = await openRepo(tab.path, tab.id);
        tabsStore.updateTab(tab.id, {
          info,
          name:          info.name,
          currentBranch: info.current_branch ?? null,
          tombstone:     null,
        });
        uiStore.showToast(`Reopened ${info.name}`, 'success');
      } else {
        // Refresh the message — the reason might have changed
        // (e.g. drive remounted but .git deleted).
        tabsStore.setTombstone(tab.id, {
          reason: v.status, message: v.message, checkedAt: Date.now(),
        });
        recentValidationError = v.message;
      }
    } catch (err) {
      recentValidationError = `${err}`;
    } finally {
      busyAction = null;
    }
  }

  async function handleRemove() {
    if (config.confirm_before_remove && !confirmRemove) {
      confirmRemove = true;
      return;
    }
    busyAction = 'remove';
    try {
      // Drop the tab + deregister from the workspace registry. Path on disk
      // is never touched; if it comes back the user can re-open it normally.
      try { await closeRepo(tab.id); } catch { /* tab not actually open */ }
      await workspacesStore.deregisterRepo(tab.id);
      try { await removeRecentRepo(tab.path); } catch { /* fire-and-forget */ }
      tabsStore.removeTab(tab.id);
      uiStore.showToast('Project removed', 'success');
    } catch (err) {
      recentValidationError = `${err}`;
      busyAction = null;
    }
  }
</script>

<div class="missing-state">
  <div class="card">
    <div class="header">
      <ReasonIcon size={26} class="header-icon" />
      <div class="header-text">
        <h2>{tab.name}</h2>
        <p class="reason">{reasonLabel}</p>
      </div>
    </div>

    <div class="path-row">
      <span class="path-label">Path</span>
      <code class="path">{tab.path}</code>
    </div>

    {#if tab.tombstone?.message}
      <p class="message">{tab.tombstone.message}</p>
    {/if}

    {#if recentValidationError}
      <div class="error-banner">
        <AlertTriangle size={14} />
        <span>{recentValidationError}</span>
      </div>
    {/if}

    <div class="actions">
      <button
        class="action-btn primary"
        onclick={() => { confirmRemove = false; pickerOpen = true; }}
        disabled={busyAction !== null}
      >
        <FolderSearch size={14} />
        <span>Locate folder…</span>
      </button>

      <button
        class="action-btn"
        onclick={handleRetry}
        disabled={busyAction !== null}
      >
        <RefreshCw size={14} class={busyAction === 'retry' ? 'spin' : ''} />
        <span>{busyAction === 'retry' ? 'Checking…' : 'Retry'}</span>
      </button>

      <button
        class="action-btn danger"
        onclick={handleRemove}
        disabled={busyAction !== null}
      >
        <Trash2 size={14} />
        <span>
          {#if confirmRemove}
            Click again to confirm
          {:else}
            Remove from Arbor
          {/if}
        </span>
      </button>
    </div>

    <div class="hint">
      <Plug size={12} />
      <span>
        Removing the project unregisters it from Arbor and clears its
        per-repo settings. The folder on disk is never touched.
      </span>
    </div>
  </div>
</div>

{#if pickerOpen}
  <FilePickerModal
    mode="folder"
    title="Locate '{tab.name}'"
    initialPath={tab.path}
    onConfirm={handleLocate}
    onCancel={() => pickerOpen = false}
  />
{/if}

<style>
  .missing-state {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 32px 24px;
    background: var(--bg-base);
    overflow-y: auto;
  }

  .card {
    width: 100%;
    max-width: 540px;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: 24px 24px 20px;
    display: flex;
    flex-direction: column;
    gap: 16px;
    box-shadow: 0 6px 20px rgba(0, 0, 0, 0.25);
  }

  .header {
    display: flex;
    align-items: flex-start;
    gap: 14px;
  }
  :global(.missing-state .header-icon) {
    color: var(--warning);
    flex-shrink: 0;
    margin-top: 2px;
  }
  .header-text { flex: 1; min-width: 0; }
  .header-text h2 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
    color: var(--text-primary);
    word-break: break-word;
  }
  .reason {
    margin: 4px 0 0;
    font-size: var(--font-size-sm);
    color: var(--text-secondary);
  }

  .path-row {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .path-label {
    font-size: 9px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.6px;
    color: var(--text-muted);
  }
  .path {
    font-family: var(--font-code);
    font-size: 11.5px;
    color: var(--text-secondary);
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    word-break: break-all;
  }

  .message {
    margin: 0;
    color: var(--text-muted);
    font-size: var(--font-size-sm);
    line-height: 1.5;
  }

  .error-banner {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    background: var(--error-subtle);
    border: 1px solid color-mix(in srgb, var(--error) 40%, transparent);
    color: var(--error);
    border-radius: 5px;
    padding: 8px 10px;
    font-size: var(--font-size-xs);
    line-height: 1.45;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .action-btn {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 7px 12px;
    background: var(--bg-base);
    border: 1px solid var(--border);
    border-radius: 5px;
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    cursor: pointer;
    transition: background var(--transition-fast), border-color var(--transition-fast), color var(--transition-fast);
  }
  .action-btn:hover:not(:disabled) {
    background: var(--bg-hover);
    border-color: var(--accent);
  }
  .action-btn:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }
  .action-btn.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--text-on-accent);
  }
  .action-btn.primary:hover:not(:disabled) {
    background: var(--accent-hover, var(--accent));
    filter: brightness(1.08);
  }
  .action-btn.danger {
    color: var(--error);
    border-color: color-mix(in srgb, var(--error) 50%, transparent);
  }
  .action-btn.danger:hover:not(:disabled) {
    background: var(--error-subtle);
    border-color: var(--error);
  }

  .hint {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    color: var(--text-muted);
    font-size: var(--font-size-xs);
    line-height: 1.55;
  }

  :global(.missing-state .spin) {
    animation: spin 1s linear infinite;
  }
  @keyframes spin {
    from { transform: rotate(0deg); }
    to   { transform: rotate(360deg); }
  }
</style>
