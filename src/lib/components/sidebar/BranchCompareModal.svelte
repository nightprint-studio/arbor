<script lang="ts">
  import { GitBranch, ArrowRight, Loader } from 'lucide-svelte';
  import DiffViewer from '../diff/DiffViewer.svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { getBranchDiff } from '$lib/ipc/diff';
  import type { DiffFile } from '$lib/types/git';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  let {
    fromRef,
    toRef,
    onClose,
  }: { fromRef: string; toRef: string; onClose: () => void } = $props();

  const tab = $derived(tabsStore.activeTab);

  let files      = $state<DiffFile[]>([]);
  let selected   = $state<DiffFile | null>(null);
  let loading    = $state(true);
  let error      = $state<string | null>(null);

  function loadDiff(preservePath: string | null = null) {
    if (!tab) return;
    loading = true;
    error = null;
    getBranchDiff(tab.id, fromRef, toRef)
      .then(result => {
        files = result;
        selected = (preservePath && result.find(f => f.path === preservePath)) || result[0] || null;
      })
      .catch(e => { error = String(e); })
      .finally(() => { loading = false; });
  }

  $effect(() => {
    if (!tab) return;
    files = [];
    selected = null;
    loadDiff(null);
  });

  $effect(() => {
    function onReload() { loadDiff(selected?.path ?? null); }
    window.addEventListener('arbor:reload-diff', onReload);
    return () => window.removeEventListener('arbor:reload-diff', onReload);
  });

  const totalAdds = $derived(files.reduce((s, f) => s + f.stats.additions, 0));
  const totalDels = $derived(files.reduce((s, f) => s + f.stats.deletions, 0));

  const STATUS_ICON: Record<string, string> = {
    added: 'A', modified: 'M', deleted: 'D',
    renamed: 'R', copied: 'C', untracked: 'U', binary: 'B',
  };
  const STATUS_COLOR: Record<string, string> = {
    added:     'var(--success)',
    modified:  'var(--warning)',
    deleted:   'var(--error)',
    renamed:   'var(--color-file-renamed)',
    copied:    'var(--color-file-renamed)',
    untracked: 'var(--color-file-untracked)',
    binary:    'var(--text-muted)',
  };
</script>

<Modal {onClose} size="full" padBody={false} ariaLabel="Compare branches">
  {#snippet header()}
    <ModalHeader {onClose}>
      <div class="header-title">
        <GitBranch size={13} class="muted-icon" />
        <span class="ref-label">{fromRef}</span>
        <ArrowRight size={11} class="muted-icon" />
        <span class="ref-label accent">{toRef}</span>
      </div>

      {#snippet actions()}
        {#if !loading && files.length > 0}
          <div class="header-stats">
            <span class="stat-files">{files.length} file{files.length !== 1 ? 's' : ''}</span>
            {#if totalAdds > 0}<span class="stat-add">+{totalAdds}</span>{/if}
            {#if totalDels > 0}<span class="stat-del">−{totalDels}</span>{/if}
          </div>
        {/if}
      {/snippet}
    </ModalHeader>
  {/snippet}

  <div class="body">
    {#if loading}
      <div class="center-state">
        <div class="spinner"><Loader size={16} /></div>
        <span>Loading diff…</span>
      </div>
    {:else if error}
      <div class="center-state error">{error}</div>
    {:else if files.length === 0}
      <div class="center-state muted">These branches are identical — no differences found.</div>
    {:else}
      <div class="split-layout">
        <div class="file-list">
          <div class="file-list-header">
            <span class="file-list-title">Changed files</span>
            <span class="file-list-count">{files.length}</span>
          </div>
          <div class="file-list-body">
            {#each files as file (file.path)}
              <button
                class="file-item"
                class:selected={selected?.path === file.path}
                onclick={() => selected = file}
                use:tooltip={file.path}
              >
                <span
                  class="file-status"
                  style="color: {STATUS_COLOR[file.status] ?? 'var(--text-muted)'}"
                >{STATUS_ICON[file.status] ?? '?'}</span>

                <span class="file-path">{file.path}</span>

                {#if file.stats.additions > 0 || file.stats.deletions > 0}
                  <span class="file-stats">
                    {#if file.stats.additions > 0}
                      <span class="fstat-add">+{file.stats.additions}</span>
                    {/if}
                    {#if file.stats.deletions > 0}
                      <span class="fstat-del">−{file.stats.deletions}</span>
                    {/if}
                  </span>
                {/if}
              </button>
            {/each}
          </div>
        </div>

        <div class="diff-area">
          <DiffViewer
            file={selected}
            path={selected?.path}
            stageable={false}
            onEncodingChange={() => window.dispatchEvent(new CustomEvent('arbor:reload-diff'))}
          />
        </div>
      </div>
    {/if}
  </div>
</Modal>

<style>
  .header-title {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 1;
    min-width: 0;
    font-size: var(--font-size-sm);
  }

  :global(.muted-icon) { color: var(--text-muted); flex-shrink: 0; }

  .ref-label {
    font-family: var(--font-code);
    font-size: 12px;
    font-weight: 500;
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 220px;
  }
  .ref-label.accent { color: var(--accent); }

  .header-stats {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }
  .stat-files { font-size: var(--font-size-xs); color: var(--text-muted); }
  .stat-add   { font-size: var(--font-size-xs); font-weight: 600; color: var(--success); }
  .stat-del   { font-size: var(--font-size-xs); font-weight: 600; color: var(--error); }

  .body {
    flex: 1;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .center-state {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    flex: 1;
    color: var(--text-muted);
    font-size: var(--font-size-sm);
  }
  .center-state.error  { color: var(--error); }
  .center-state.muted  { color: var(--text-disabled); }

  .spinner {
    animation: spin 0.9s linear infinite;
    display: flex;
    color: var(--text-muted);
  }
  @keyframes spin { to { transform: rotate(360deg); } }

  .split-layout {
    display: flex;
    flex: 1;
    overflow: hidden;
    background: var(--bg-elevated);
    padding: 4px;
    gap: 4px;
  }

  .file-list {
    width: 240px;
    min-width: 160px;
    flex-shrink: 0;
    background: var(--bg-base);
    border-radius: var(--radius-lg);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .file-list-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }
  .file-list-title {
    flex: 1;
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
  }
  .file-list-count {
    font-size: var(--font-size-xs);
    color: var(--text-disabled);
    background: var(--bg-overlay);
    border-radius: 999px;
    padding: 0 5px;
    line-height: 16px;
  }

  .file-list-body {
    flex: 1;
    overflow-y: auto;
    padding: 6px 8px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .file-item {
    display: flex;
    align-items: center;
    gap: 7px;
    width: 100%;
    padding: 7px 10px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    cursor: pointer;
    text-align: left;
    color: var(--text-secondary);
    transition: background var(--transition-fast), border-color var(--transition-fast),
                box-shadow var(--transition-fast), color var(--transition-fast);
    overflow: hidden;
  }
  .file-item:hover {
    background: var(--bg-overlay);
    border-color: var(--border);
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.15);
    color: var(--text-primary);
  }
  .file-item.selected {
    background: var(--accent-subtle);
    border-color: color-mix(in srgb, var(--accent) 55%, transparent);
    color: var(--accent);
  }

  .file-status {
    font-family: var(--font-code);
    font-size: 10px;
    font-weight: 700;
    flex-shrink: 0;
    width: 11px;
    text-align: center;
  }

  .file-path {
    font-family: var(--font-code);
    font-size: 11px;
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .file-stats {
    display: flex;
    align-items: center;
    gap: 3px;
    flex-shrink: 0;
  }
  .fstat-add { font-size: 10px; font-weight: 600; color: var(--success); }
  .fstat-del { font-size: 10px; font-weight: 600; color: var(--error); }

  .diff-area {
    flex: 1;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    background: var(--bg-base);
    border-radius: var(--radius-lg);
  }
</style>
