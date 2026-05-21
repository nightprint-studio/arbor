<script lang="ts">
  import { onMount } from 'svelte';
  import { FolderX, Trash2, Loader } from 'lucide-svelte';
  import {
    getMissingProjectsConfig, setMissingProjectsConfig, cleanupMissingRecentRepos,
    type MissingProjectsConfig,
  } from '$lib/ipc/missing';
  import { uiStore } from '$lib/stores/ui.svelte';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';

  let cfg     = $state<MissingProjectsConfig | null>(null);
  let loading = $state(true);
  let saving  = $state(false);
  let cleaning = $state(false);
  let lastCleanup = $state<string[] | null>(null);

  onMount(async () => {
    try { cfg = await getMissingProjectsConfig(); }
    catch (e) { uiStore.showToast(`Failed to load: ${e}`, 'error'); }
    finally { loading = false; }
  });

  async function persist(next: MissingProjectsConfig) {
    saving = true;
    try {
      await setMissingProjectsConfig(next);
      cfg = next;
    } catch (e) {
      uiStore.showToast(`Save failed: ${e}`, 'error');
    } finally {
      saving = false;
    }
  }

  async function runCleanup() {
    cleaning = true;
    lastCleanup = null;
    try {
      const removed = await cleanupMissingRecentRepos();
      lastCleanup = removed;
      if (removed.length === 0) {
        uiStore.showToast('No missing recent projects found', 'info');
      } else {
        uiStore.showToast(`Removed ${removed.length} missing recent project${removed.length === 1 ? '' : 's'}`, 'success');
      }
    } catch (e) {
      uiStore.showToast(`Cleanup failed: ${e}`, 'error');
    } finally {
      cleaning = false;
    }
  }
</script>

<SectionHeader
  title="Missing Projects"
  description="What Arbor does when a registered project's folder is no longer available on disk (deleted, moved, drive offline)."
/>

{#if loading}
  <div class="empty-state">
    <Loader size={14} class="spin" />
    <span>Loading…</span>
  </div>
{:else if cfg}
  <div class="card">
    <FormRow
      label="Auto-prune missing recents"
      description="Silently drop entries from the Recent list when their folder is missing.  When off (default), missing recents are shown with a warning badge so you can locate or remove them yourself."
    >
      <Toggle
        checked={cfg.auto_prune_recents}
        onchange={(v) => cfg && persist({ ...cfg, auto_prune_recents: v })}
        disabled={saving}
      />
    </FormRow>

    <FormRow
      label="Confirm before removing"
      description="Ask for a second click before deregistering a project from the tombstone screen.  The folder on disk is never touched either way."
    >
      <Toggle
        checked={cfg.confirm_before_remove}
        onchange={(v) => cfg && persist({ ...cfg, confirm_before_remove: v })}
        disabled={saving}
      />
    </FormRow>

    <FormRow
      label="Re-validate on focus"
      description="When the window regains focus, re-classify every tombstoned tab.  Useful if you remounted a drive or reconnected to a VPN — promotes the tab back to a normal repo without a manual Retry."
    >
      <Toggle
        checked={cfg.revalidate_on_focus}
        onchange={(v) => cfg && persist({ ...cfg, revalidate_on_focus: v })}
        disabled={saving}
      />
    </FormRow>
  </div>

  <div class="card">
    <div class="card-section-title">Manual cleanup</div>
    <div class="card-row-note">
      Scan the recent-projects list right now and remove every entry whose folder is missing or unreachable.
      The repository registry and any open tabs are untouched.
    </div>
    <div class="cleanup-actions">
      <button class="btn-secondary" onclick={runCleanup} disabled={cleaning}>
        {#if cleaning}
          <Loader size={13} class="spin" />
          Scanning…
        {:else}
          <Trash2 size={13} />
          Clean up missing recents
        {/if}
      </button>
    </div>

    {#if lastCleanup && lastCleanup.length > 0}
      <div class="result-list">
        <div class="result-header">
          <FolderX size={13} />
          <span>Removed {lastCleanup.length} entr{lastCleanup.length === 1 ? 'y' : 'ies'}:</span>
        </div>
        <ul>
          {#each lastCleanup as p}<li><code>{p}</code></li>{/each}
        </ul>
      </div>
    {/if}
  </div>
{/if}

<style>
  .cleanup-actions {
    display: flex;
    gap: 8px;
    padding: 8px 0 0;
  }
  .btn-secondary {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    cursor: pointer;
    transition: background var(--transition-fast), border-color var(--transition-fast);
  }
  .btn-secondary:hover:not(:disabled) {
    background: var(--bg-hover);
    border-color: var(--accent);
  }
  .btn-secondary:disabled { opacity: 0.55; cursor: not-allowed; }

  .result-list {
    margin-top: 12px;
    padding: 10px 12px;
    background: rgba(204, 167, 58, 0.06);
    border-left: 3px solid rgba(204, 167, 58, 0.5);
    border-radius: 0 4px 4px 0;
  }
  .result-header {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--warning);
    font-size: var(--font-size-xs);
    font-weight: 600;
    margin-bottom: 6px;
  }
  .result-list ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .result-list li {
    font-family: var(--font-code);
    font-size: 11px;
    color: var(--text-secondary);
    word-break: break-all;
  }

  :global(.spin) { animation: spin 1s linear infinite; }
  @keyframes spin { from {transform: rotate(0deg);} to {transform: rotate(360deg);} }
</style>
