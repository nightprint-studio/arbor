<script lang="ts">
  import { Trash2, RefreshCw, CheckSquare, Square, GitBranch, Globe, AlertTriangle, ChevronDown } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import type { BranchInfo } from '$lib/types/git';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { repoStore } from '$lib/stores/repo.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { graphStore } from '$lib/stores/graph.svelte';
  import {
    listMergedBranches, deleteBranches,
    listMergedRemoteBranches, deleteRemoteBranches,
  } from '$lib/ipc/branch';
  import { getGraph } from '$lib/ipc/graph';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Dropdown from '$lib/components/shared/ui/Dropdown.svelte';
  import type { DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';

  let {
    onClose,
    onRefresh,
    initialTab = 'local',
  }: { onClose: () => void; onRefresh: () => void; initialTab?: 'local' | 'remote' } = $props();

  type Tab = 'local' | 'remote';
  // svelte-ignore state_referenced_locally
  let activeTab = $state<Tab>(initialTab);

  const tab = $derived(tabsStore.activeTab);
  const localBranches = $derived(repoStore.localBranches);
  const headBranch = $derived(
    localBranches.find(b => b.is_head)?.name ??
    localBranches.find(b => b.name === 'main' || b.name === 'master')?.name ??
    localBranches[0]?.name ?? ''
  );

  let target   = $state('');
  let merged   = $state<BranchInfo[]>([]);
  let selected = $state(new Set<string>());
  let loading  = $state(false);
  let deleting = $state(false);
  let loaded   = $state(false);

  $effect(() => {
    if (headBranch && !target) target = headBranch;
  });

  function setTarget(name: string) {
    target = name;
    loaded = false;
    merged = [];
    selected = new Set();
  }

  const targetItems = $derived<DropdownItem[]>(
    localBranches.map(b => ({
      kind:    'item',
      id:      b.name,
      label:   b.is_head ? `${b.name} (HEAD)` : b.name,
      active:  target === b.name,
      onclick: () => setTarget(b.name),
    })),
  );

  const targetLabel = $derived(
    target
      ? (localBranches.find(b => b.name === target)?.is_head ? `${target} (HEAD)` : target)
      : '— select branch —',
  );

  // Auto-scan on open and on tab switch
  $effect(() => {
    if (!loaded && !loading && target) {
      activeTab; // track tab changes
      load();
    }
  });

  // Reset results when tab changes
  $effect(() => {
    activeTab; // track
    merged   = [];
    selected = new Set();
    loaded   = false;
  });

  async function load() {
    if (!tab || !target) return;
    loading = true;
    loaded  = false;
    try {
      const result = activeTab === 'local'
        ? await listMergedBranches(tab.id, target)
        : await listMergedRemoteBranches(tab.id, target);
      merged   = result;
      selected = new Set(result.map(b => b.name));
      loaded   = true;
    } catch (err) {
      uiStore.showToast(`${err}`, 'error');
    } finally {
      loading = false;
    }
  }

  function toggleAll() {
    selected = selected.size === merged.length
      ? new Set()
      : new Set(merged.map(b => b.name));
  }

  function toggle(name: string) {
    const next = new Set(selected);
    if (next.has(name)) next.delete(name); else next.add(name);
    selected = next;
  }

  async function handleDelete() {
    if (!tab || selected.size === 0) return;
    deleting = true;
    try {
      const names = [...selected];
      const failed = activeTab === 'local'
        ? await deleteBranches(tab.id, names)
        : await deleteRemoteBranches(tab.id, names);
      const deleted = names.length - failed.length;
      if (deleted > 0) {
        uiStore.showToast(`Deleted ${deleted} ${activeTab} branch${deleted !== 1 ? 'es' : ''}`, 'success');
        const gd = await getGraph(tab.id, 0, 500);
        graphStore.setGraph(gd);
        onRefresh();
      }
      if (failed.length > 0) {
        uiStore.showToast(`Could not delete: ${failed.join(', ')}`, 'warning');
      }
      await load();
    } catch (err) {
      uiStore.showToast(`${err}`, 'error');
    } finally {
      deleting = false;
    }
  }

  function oidShort(oid: string) { return oid.slice(0, 7); }

  /** For remote branches ("origin/feature-x") return [remote, shortName] */
  function splitRemoteName(name: string): [string, string] {
    const idx = name.indexOf('/');
    if (idx === -1) return ['', name];
    return [name.slice(0, idx), name.slice(idx + 1)];
  }
</script>

<Modal {onClose} width="580px" height="72vh" padBody={false} ariaLabel="Branch cleanup">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Trash2 size={14} class="header-icon" />
      <span class="modal-title">Branch Cleanup</span>
    </ModalHeader>
  {/snippet}

  <div class="bc-content">
    <!-- Tabs -->
    <div class="tab-bar">
      <Tabs
        items={[
          { id: 'local',  label: 'Local',  icon: GitBranch },
          { id: 'remote', label: 'Remote', icon: Globe },
        ]}
        value={activeTab}
        variant="underline"
        size="sm"
        onSelect={(id) => activeTab = id as Tab}
      />
    </div>

    <!-- Target selector -->
    <div class="target-row">
      <span class="target-label">Merged into</span>
      <div class="target-select-wrap">
        <Dropdown
          position="fixed"
          direction="down"
          matchTriggerWidth
          searchable={localBranches.length > 12}
          items={targetItems}
        >
          {#snippet trigger({ open, toggle })}
            <button
              class="target-select"
              onclick={toggle}
              type="button"
              aria-haspopup="listbox"
              aria-expanded={open}
            >
              <span class="target-select-label">{targetLabel}</span>
              <ChevronDown size={11} />
            </button>
          {/snippet}
        </Dropdown>
      </div>
      <button class="scan-btn" onclick={load} disabled={loading || !target}>
        {#if loading}
          <RefreshCw size={12} class="spin" />
          Scanning…
        {:else}
          <RefreshCw size={12} />
          Rescan
        {/if}
      </button>
    </div>

    <!-- Results -->
    <div class="results">
      {#if !loaded && !loading}
        <div class="empty-hint">
          {#if activeTab === 'local'}
            <GitBranch size={28} class="hint-icon" />
          {:else}
            <Globe size={28} class="hint-icon" />
          {/if}
          <p>Select a target branch to scan for merged {activeTab} branches.</p>
        </div>
      {:else if loading}
        <div class="empty-hint">
          <RefreshCw size={22} class="spin hint-icon" />
          <p>Scanning for merged {activeTab} branches…</p>
        </div>
      {:else if merged.length === 0}
        <div class="empty-hint success">
          <span class="success-check">✓</span>
          <p>No merged {activeTab} branches found. Your branch list is already clean.</p>
        </div>
      {:else}
        <!-- Select all toolbar -->
        <div class="list-toolbar">
          <button class="select-all-btn" onclick={toggleAll}>
            {#if selected.size === merged.length}
              <CheckSquare size={13} />
            {:else}
              <Square size={13} />
            {/if}
            {selected.size === merged.length ? 'Deselect all' : 'Select all'}
          </button>
          <span class="count-hint">{merged.length} merged branch{merged.length !== 1 ? 'es' : ''}</span>
        </div>

        <!-- Branch list -->
        <div class="branch-list">
          {#each merged as branch}
            {@const checked = selected.has(branch.name)}
            <button
              class="branch-row"
              class:checked
              onclick={() => toggle(branch.name)}
            >
              <span class="check-icon">
                {#if checked}
                  <CheckSquare size={13} class="check-on" />
                {:else}
                  <Square size={13} class="check-off" />
                {/if}
              </span>
              {#if activeTab === 'remote'}
                {@const [remote, shortName] = splitRemoteName(branch.name)}
                <Globe size={12} class="branch-icon-svg" />
                <span class="remote-prefix">{remote}/</span><span class="branch-name">{shortName}</span>
              {:else}
                <GitBranch size={12} class="branch-icon-svg" />
                <span class="branch-name">{branch.name}</span>
              {/if}
              <span class="branch-oid">{oidShort(branch.head_oid)}</span>
              <span class="branch-summary">{branch.head_summary}</span>
            </button>
          {/each}
        </div>
      {/if}
    </div>
  </div>

  {#if loaded && merged.length > 0}
    {#snippet footer()}
      <div class="warning-hint">
        <AlertTriangle size={12} />
        {#if activeTab === 'remote'}
          <span>This will delete branches from the <strong>remote server</strong>. Cannot be undone.</span>
        {:else}
          <span>Deletion is permanent and cannot be undone.</span>
        {/if}
      </div>
      <Button variant="secondary" onclick={onClose}>Cancel</Button>
      <Button
        variant="danger"
        onclick={handleDelete}
        disabled={selected.size === 0 || deleting}
        loading={deleting}
      >
        {#snippet iconStart()}
          <Trash2 size={12} />
        {/snippet}
        {deleting ? 'Deleting…' : `Delete ${selected.size > 0 ? `${selected.size} ` : ''}branch${selected.size !== 1 ? 'es' : ''}`}
      </Button>
    {/snippet}
  {/if}
</Modal>

<style>
  :global(.header-icon) { color: var(--error); }

  .bc-content {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  /* ── Tab bar ── */
  /* Strip rendered by shared <Tabs variant="underline" size="sm">. The
     wrapper just contributes the modal's side-padding + the bg colour. */
  .tab-bar {
    padding: 0 14px;
    background: var(--bg-base);
    flex-shrink: 0;
  }

  /* ── Target selector ── */
  .target-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
    background: var(--bg-base);
  }

  .target-label {
    font-size: 11px;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    white-space: nowrap;
  }

  .target-select-wrap { flex: 1; }
  .target-select-wrap :global(.dd-root) { width: 100%; }
  .target-select {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    width: 100%;
    box-sizing: border-box;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    font-size: 12px;
    padding: 4px 8px;
    outline: none;
    cursor: pointer;
    text-align: left;
    transition: border-color var(--transition-fast);
  }
  .target-select:hover,
  .target-select[aria-expanded='true'] { border-color: var(--accent); }
  .target-select-label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .scan-btn {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 5px 12px;
    background: var(--accent);
    color: var(--text-on-accent);
    border: none;
    border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans);
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
    transition: opacity var(--transition-fast);
    white-space: nowrap;
  }
  .scan-btn:disabled { opacity: 0.5; cursor: not-allowed; }
  .scan-btn:hover:not(:disabled) { opacity: 0.85; }

  /* ── Results area ── */
  .results {
    flex: 1;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    min-height: 120px;
  }

  .empty-hint {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 40px 24px;
    color: var(--text-muted);
    text-align: center;
  }
  .empty-hint p { font-size: 13px; line-height: 1.6; }
  .empty-hint.success p { color: var(--success); }

  :global(.hint-icon) { color: var(--text-disabled); opacity: 0.6; }

  .success-check {
    font-size: 28px;
    color: var(--success, #6a9956);
  }

  /* ── List toolbar ── */
  .list-toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 14px;
    border-bottom: 1px solid var(--border-subtle);
    background: var(--bg-base);
    flex-shrink: 0;
  }

  .select-all-btn {
    display: flex;
    align-items: center;
    gap: 5px;
    background: transparent;
    border: none;
    cursor: pointer;
    font-family: var(--font-ui-sans);
    font-size: 11px;
    color: var(--text-secondary);
    padding: 2px 4px;
    border-radius: var(--radius-sm);
    transition: color var(--transition-fast), background var(--transition-fast);
  }
  .select-all-btn:hover { color: var(--text-primary); background: var(--bg-hover); }

  .count-hint {
    margin-left: auto;
    font-size: 10px;
    color: var(--text-disabled);
  }

  /* ── Branch list ── */
  .branch-list {
    flex: 1;
    overflow-y: auto;
    padding: 4px 0;
    scrollbar-width: thin;
    scrollbar-color: var(--border) transparent;
  }

  .branch-row {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px 14px;
    background: transparent;
    border: none;
    cursor: pointer;
    text-align: left;
    transition: background var(--transition-fast);
    font-family: var(--font-ui-sans);
  }
  .branch-row:hover { background: rgba(255,255,255,0.04); }
  .branch-row.checked { background: rgba(77,120,204,0.08); }

  .check-icon { flex-shrink: 0; display: flex; color: var(--text-disabled); }
  :global(.check-on)  { color: var(--accent); }
  :global(.check-off) { color: var(--text-disabled); }
  :global(.branch-icon-svg) { color: var(--text-muted); flex-shrink: 0; }

  .remote-prefix {
    font-size: 11px;
    color: var(--text-disabled);
    font-family: var(--font-code);
    flex-shrink: 0;
  }

  .branch-name {
    font-size: 12px;
    font-weight: 500;
    color: var(--text-primary);
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex-shrink: 0;
    max-width: 200px;
  }

  .branch-oid {
    font-size: 10px;
    color: var(--text-disabled);
    font-family: var(--font-code);
    flex-shrink: 0;
  }

  .branch-summary {
    font-size: 11px;
    color: var(--text-muted);
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }

  /* ── Footer items ── */
  .warning-hint {
    display: flex;
    align-items: center;
    gap: 5px;
    color: var(--warning, #e2a335);
    font-size: 10px;
    flex: 1;
    margin-right: auto;
  }
  .warning-hint strong { color: var(--warning, #e2a335); font-weight: 600; }

  :global(.spin) { animation: spin 0.9s linear infinite; }
  @keyframes spin { from { transform: rotate(0deg); } to { transform: rotate(360deg); } }
</style>
