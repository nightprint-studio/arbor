<script lang="ts">
  /**
   * Settings → Project → Git Flow
   *
   * Project-scoped override for the global Git Flow defaults configured
   * under Settings → Git → Git Flow. Mirrors the per-project IDE pattern
   * in ExternalIntegrationsSection: the global setting lives under its
   * top-level group; a thin Project-side entry lets the user flip the
   * override per-repo without leaving the project context.
   *
   * Lifted out of the original GitFlowSection (which carried both global
   * and project-override blocks) so each setting page does one thing —
   * the global page now stays focused on the defaults and points users
   * here for the per-repo knob.
   */
  import { GitMerge, FolderGit2, Info, RotateCcw } from 'lucide-svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import type { GitFlowConfig } from '$lib/types/git';
  import {
    getGitFlowGlobalConfig,
    getGitFlowConfig, setGitFlowRepoConfig, clearGitFlowRepoConfig,
    hasGitFlowRepoOverride,
  } from '$lib/ipc/gitflow';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  const tab = $derived(tabsStore.activeTab);

  // Global config is needed only as a seed when the user enables the
  // override (so the form starts populated with current defaults).
  let globalCfg = $state<GitFlowConfig | null>(null);

  // Per-repo override state. `hasOverride === false` means we render the
  // "Enable" affordance; once enabled the user edits a fully-formed
  // GitFlowConfig and saves with a single button.
  let repoCfg     = $state<GitFlowConfig | null>(null);
  let hasOverride = $state(false);
  let loading     = $state(false);
  let saving      = $state(false);
  let dirty       = $state(false);

  // Load global once on mount.
  $effect(() => { loadGlobal(); });

  // Reload the per-repo override every time the active tab changes —
  // mirrors how ExternalIntegrationsSection handles tab switching.
  $effect(() => {
    if (tab) loadRepo(tab.id);
    else { repoCfg = null; hasOverride = false; }
  });

  async function loadGlobal() {
    try {
      globalCfg = await getGitFlowGlobalConfig();
    } catch (e) {
      uiStore.showToast(String(e), 'error');
    }
  }

  async function loadRepo(tabId: string) {
    loading = true;
    try {
      hasOverride = await hasGitFlowRepoOverride(tabId);
      repoCfg     = hasOverride ? await getGitFlowConfig(tabId) : null;
      dirty       = false;
    } catch { /* ignore — show empty state */ } finally {
      loading = false;
    }
  }

  async function save() {
    if (!tab || !repoCfg) return;
    saving = true;
    try {
      await setGitFlowRepoConfig(tab.id, repoCfg);
      dirty = false;
      uiStore.showToast('Project Git Flow override saved', 'success');
    } catch (e) {
      uiStore.showToast(String(e), 'error');
    } finally {
      saving = false;
    }
  }

  async function enable() {
    if (!tab || !globalCfg) return;
    // Seed the override form with the current global values so the user
    // doesn't start from a blank slate.
    repoCfg     = structuredClone(globalCfg);
    hasOverride = true;
    dirty       = true;
  }

  async function clearOverride() {
    if (!tab) return;
    try {
      await clearGitFlowRepoConfig(tab.id);
      hasOverride = false;
      repoCfg     = null;
      dirty       = false;
      uiStore.showToast('Project override removed — using global settings', 'info');
    } catch (e) {
      uiStore.showToast(String(e), 'error');
    }
  }

  function markDirty() { dirty = true; }
</script>

<SectionHeader
  title="Git Flow"
  description="Override the global Git Flow defaults for this repository only. Configure the global defaults under Settings → Git → Git Flow."
/>

{#if !tab}
  <div class="empty-state">
    <FolderGit2 size={20} />
    <span>No repository open</span>
    <span class="empty-hint">Open a repository to configure project-specific Git Flow overrides.</span>
  </div>
{:else if loading}
  <div class="empty-state"><span class="loading-dots">Loading…</span></div>
{:else}
  <div class="card">
    <div class="card-section-title">
      <GitMerge size={12} /> Project override
      <span class="section-tab-name">{tab.name}</span>
    </div>

    {#if !hasOverride}
      <div class="card-row-note">
        <Info size={12} style="flex-shrink:0; opacity:0.6; margin-top:1px" />
        <span>
          Using global defaults. Enable a project override to customise Git Flow settings for
          <strong>{tab.name}</strong> only.
        </span>
      </div>
      <div class="card-row" style="border-bottom:none">
        <button class="btn-ghost-sm" onclick={enable}>
          Enable project override
        </button>
      </div>

    {:else if repoCfg}
      <FormRow label="Force PR/MR on feature finish" description="Override for this project">
        <Toggle bind:checked={repoCfg.finish.feature_use_pr} onchange={markDirty} />
      </FormRow>

      <FormRow label="Feature finish default → PR/MR" description="Override for this project">
        <Toggle bind:checked={repoCfg.finish.feature_pr_default} onchange={markDirty} />
      </FormRow>

      <FormRow label="Force PR/MR on release finish" description="Override for this project">
        <Toggle bind:checked={repoCfg.finish.release_use_pr} onchange={markDirty} />
      </FormRow>

      <FormRow label="Release finish default → PR/MR" description="Override for this project">
        <Toggle bind:checked={repoCfg.finish.release_pr_default} onchange={markDirty} />
      </FormRow>

      <FormRow label="Force PR/MR on hotfix finish" description="Override for this project">
        <Toggle bind:checked={repoCfg.finish.hotfix_use_pr} onchange={markDirty} />
      </FormRow>

      <FormRow label="Hotfix finish default → PR/MR" description="Override for this project">
        <Toggle bind:checked={repoCfg.finish.hotfix_pr_default} onchange={markDirty} />
      </FormRow>

      <FormRow label="Require ticket branch names" description="Override for this project">
        <Toggle bind:checked={repoCfg.require_ticket_branch} onchange={markDirty} />
      </FormRow>

      <div class="card-row-note" style="border-bottom:none">
        Stored in <code>.arbor/config.toml</code> in the repository root.
      </div>
    {/if}
  </div>

  {#if hasOverride && repoCfg}
    <div class="form-actions">
      <button class="btn-primary" onclick={save} disabled={saving || !dirty}>
        {saving ? 'Saving…' : 'Save Project Override'}
      </button>
      <button
        class="btn-ghost"
        onclick={clearOverride}
        use:tooltip={'Remove project override and use global settings'}
      >
        <RotateCcw size={12} /> Reset to global
      </button>
      {#if !dirty && !saving}
        <span class="saved-label">All changes saved</span>
      {/if}
    </div>
  {/if}
{/if}

<style>
  .section-tab-name {
    font-size: 9px;
    font-weight: 500;
    color: var(--accent);
    background: var(--accent-subtle);
    border: 1px solid rgba(77,120,204,0.28);
    border-radius: 999px;
    padding: 0 5px;
    line-height: 14px;
    text-transform: none;
    letter-spacing: 0;
    margin-left: 4px;
  }

  .card-row-note {
    display: flex;
    align-items: flex-start;
    gap: 7px;
  }

  .form-actions {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .btn-ghost {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 5px 10px;
    background: transparent;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans);
    font-size: 11px;
    color: var(--text-muted);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .btn-ghost:hover { background: var(--bg-hover); color: var(--text-primary); }
</style>
