<script lang="ts">
  /**
   * Settings → Git → Git Flow
   *
   * Global Git Flow defaults. The per-project override block has moved to
   * Settings → Project → Git Flow (`ProjectGitFlowSection.svelte`) — keep
   * this page focused on the global knobs and link out for the per-repo
   * customisation.
   */
  import { GitMerge, ExternalLink } from 'lucide-svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import type { GitFlowConfig } from '$lib/types/git';
  import {
    getGitFlowGlobalConfig, setGitFlowGlobalConfig,
  } from '$lib/ipc/gitflow';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';

  // ── Global config ──────────────────────────────────────────────────────────
  let globalCfg     = $state<GitFlowConfig | null>(null);
  let globalLoading = $state(false);
  let globalDirty   = $state(false);
  let globalSaving  = $state(false);

  // ── Load global on mount ───────────────────────────────────────────────────
  $effect(() => {
    loadGlobal();
  });

  async function loadGlobal() {
    globalLoading = true;
    try {
      globalCfg = await getGitFlowGlobalConfig();
      globalDirty = false;
    } catch (e) {
      uiStore.showToast(String(e), 'error');
    } finally {
      globalLoading = false;
    }
  }

  async function saveGlobal() {
    if (!globalCfg) return;
    globalSaving = true;
    try {
      await setGitFlowGlobalConfig(globalCfg);
      globalDirty = false;
      uiStore.showToast('Global Git Flow settings saved', 'success');
    } catch (e) {
      uiStore.showToast(String(e), 'error');
    } finally {
      globalSaving = false;
    }
  }

  function markGlobalDirty() { globalDirty = true; }
</script>

<SectionHeader title="Git Flow" description="Global Git Flow automation defaults. Per-project overrides live under Settings → Project → Git Flow." />

<!-- ── Global defaults ───────────────────────────────────────────────────── -->
{#if globalLoading}
  <div class="empty-state"><span class="loading-dots">Loading…</span></div>
{:else if globalCfg}

  <div class="card">
    <div class="card-section-title"><GitMerge size={12} /> Global defaults</div>

    <FormRow label="Force PR/MR on feature finish" description="When on, finishing a feature always pushes the branch and opens a PR/MR — local merge is not offered">
      <Toggle bind:checked={globalCfg.finish.feature_use_pr} onchange={markGlobalDirty} />
    </FormRow>

    <FormRow label="Feature finish default → PR/MR" description="When 'Force' is off, set the primary button action. Off = merge locally; On = open PR/MR (the other option is always available via the split button)">
      <Toggle bind:checked={globalCfg.finish.feature_pr_default} onchange={markGlobalDirty} />
    </FormRow>

    <FormRow label="Force PR/MR on release finish" description="When on, finishing a release always opens a PR/MR to main">
      <Toggle bind:checked={globalCfg.finish.release_use_pr} onchange={markGlobalDirty} />
    </FormRow>

    <FormRow label="Release finish default → PR/MR" description="When 'Force' is off, sets the primary button default for release finish">
      <Toggle bind:checked={globalCfg.finish.release_pr_default} onchange={markGlobalDirty} />
    </FormRow>

    <FormRow label="Force PR/MR on hotfix finish" description="When on, finishing a hotfix always opens a PR/MR to main">
      <Toggle bind:checked={globalCfg.finish.hotfix_use_pr} onchange={markGlobalDirty} />
    </FormRow>

    <FormRow label="Hotfix finish default → PR/MR" description="When 'Force' is off, sets the primary button default for hotfix finish">
      <Toggle bind:checked={globalCfg.finish.hotfix_pr_default} onchange={markGlobalDirty} />
    </FormRow>

    <FormRow label="Require ticket branch names" description="Force feature/bugfix branch names to come from an issue tracker ticket (e.g. feature/ABO-123). Ticket picker is always available when a tracker is configured; this flag makes it mandatory.">
      <Toggle bind:checked={globalCfg.require_ticket_branch} onchange={markGlobalDirty} />
    </FormRow>

    <div class="card-row-note" style="border-bottom:none">
      These settings are stored in <code>~/.config/arbor/config.toml</code> under <code>[gitflow.finish]</code>.
    </div>
  </div>

  <div class="form-actions">
    <button class="btn-primary" onclick={saveGlobal} disabled={globalSaving || !globalDirty}>
      {globalSaving ? 'Saving…' : 'Save Global Settings'}
    </button>
    {#if !globalDirty && !globalSaving}
      <span class="saved-label">All changes saved</span>
    {/if}
  </div>

  <!-- Pointer to the per-project override page. Replaces the inline
       "Enable project override" card the page used to carry — the actual
       form lives under Settings → Project → Git Flow now. -->
  <div class="override-pointer">
    <ExternalLink size={12} />
    <span>
      Need to customise Git Flow for a single repository?
      Head to <strong>Settings → Project → Git Flow</strong>.
    </span>
  </div>
{/if}

<style>
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

  /* Pointer card sitting under the global form, redirecting users to
     Settings → Project → Git Flow for the per-repo override. */
  .override-pointer {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    margin-top: 14px;
    padding: 9px 11px;
    background: color-mix(in srgb, var(--accent) 6%, transparent);
    color: var(--text-secondary);
    border-left: 2px solid var(--accent);
    border-radius: var(--radius-sm);
    font-size: 11.5px;
    line-height: 1.45;
  }
  .override-pointer :global(svg) { color: var(--accent); flex-shrink: 0; margin-top: 2px; }
  .override-pointer strong { color: var(--text-primary); }
</style>
