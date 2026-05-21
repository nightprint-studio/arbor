<script lang="ts">
  import { AlertTriangle, Check, X } from 'lucide-svelte';
  import type { StatusEntry } from '$lib/types/git';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { stageFile } from '$lib/ipc/stage';
  import { tooltip } from '$lib/actions/tooltip';

  let { conflicts, onResolved }: { conflicts: StatusEntry[]; onResolved: () => void } = $props();

  const tab = $derived(tabsStore.activeTab);

  let resolving = $state<Set<string>>(new Set());

  async function markResolved(path: string) {
    if (!tab || resolving.has(path)) return;
    resolving = new Set([...resolving, path]);
    try {
      await stageFile(tab.id, path);
      uiStore.showToast(`Marked ${path} as resolved`, 'success');
      onResolved();
    } catch (err) {
      uiStore.showToast(`${err}`, 'error');
    } finally {
      resolving.delete(path);
      resolving = new Set(resolving);
    }
  }
</script>

<div class="conflict-resolver">
  <div class="header">
    <AlertTriangle size={14} class="warn-icon" />
    <span class="title">Merge Conflicts</span>
    <span class="count">{conflicts.length} file{conflicts.length !== 1 ? 's' : ''}</span>
  </div>

  <p class="hint">
    Resolve conflicts in your editor, then mark each file as resolved.
  </p>

  <div class="conflict-list">
    {#each conflicts as entry (entry.path)}
      <div class="conflict-item">
        <div class="file-path" use:tooltip={entry.path}>
          <X size={11} class="conflict-icon" />
          <span class="truncate">{entry.path}</span>
        </div>
        <button
          class="resolve-btn"
          disabled={resolving.has(entry.path)}
          onclick={() => markResolved(entry.path)}
          use:tooltip={'Mark as resolved (stage file)'}
        >
          <Check size={12} />
          <span>Mark Resolved</span>
        </button>
      </div>
    {/each}
  </div>

  <div class="footer-hint">
    <span>Tip: Use your external editor or mergetool to resolve conflicts, then click "Mark Resolved".</span>
  </div>
</div>

<style>
  .conflict-resolver {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px;
    background: var(--error-subtle);
    border: 1px solid var(--error);
    border-radius: var(--radius-md);
    margin: 8px;
  }

  .header {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  :global(.warn-icon) { color: var(--error); flex-shrink: 0; }

  .title {
    font-size: var(--font-size-sm);
    font-weight: 600;
    color: var(--error);
    flex: 1;
  }

  .count {
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    background: var(--bg-overlay);
    padding: 1px 6px;
    border-radius: 999px;
  }

  .hint {
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
    line-height: 1.5;
  }

  .conflict-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: 200px;
    overflow-y: auto;
  }

  .conflict-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    background: var(--bg-elevated);
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
  }

  .file-path {
    display: flex;
    align-items: center;
    gap: 5px;
    flex: 1;
    overflow: hidden;
    font-size: var(--font-size-sm);
    color: var(--text-primary);
  }

  :global(.conflict-icon) { color: var(--error); flex-shrink: 0; }

  .resolve-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 3px 8px;
    background: transparent;
    border: 1px solid var(--success);
    color: var(--success);
    border-radius: var(--radius-sm);
    font-size: var(--font-size-xs);
    font-family: var(--font-ui-sans);
    cursor: pointer;
    white-space: nowrap;
    flex-shrink: 0;
    transition: background var(--transition-fast);
  }

  .resolve-btn:hover:not(:disabled) { background: var(--success-subtle); }
  .resolve-btn:disabled { opacity: 0.5; cursor: not-allowed; }

  .footer-hint {
    font-size: var(--font-size-xs);
    color: var(--text-disabled);
    line-height: 1.4;
    border-top: 1px solid var(--border-subtle);
    padding-top: 8px;
  }
</style>
