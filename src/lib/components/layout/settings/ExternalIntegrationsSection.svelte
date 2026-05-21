<script lang="ts">
  /**
   * Settings → Project → External Integrations
   *
   * Per-repository overrides for external tools that have a global default
   * elsewhere (Settings → Tools). Today the only knob is "open this project
   * with [IDE]" — the choice is persisted to `.arbor/config.toml::ide_id`
   * and read back by `open_in_ide` whenever no explicit IDE is requested.
   *
   * Stays parallel to ProjectGitFlowSection: both are project-scoped
   * overrides for global Tools settings, surfaced under Project so the
   * user can flip them per-repo without leaving their context.
   */
  import { onMount } from 'svelte';
  import { ExternalLink, FolderGit2, RotateCcw, Info } from 'lucide-svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { getIdeConfig, getRepoIde, setRepoIde } from '$lib/ipc/worktree';
  import type { IdeConfig } from '$lib/types/git';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  // Pull the IDE catalogue + label resolver from the shared module —
  // never inline a parallel BUILTIN_IDES list (it drifts otherwise from
  // IdeSection / future per-project pickers).
  import { BUILTIN_IDES, findIdeLabel } from '$lib/constants/ide';

  /** Sentinel value for "use the global default" — the Select can't bind
   *  to `null` directly, so we marshal back to `null` on persist. */
  const NONE = '__none__';

  const tab = $derived(tabsStore.activeTab);

  let ideCfg     = $state<IdeConfig | null>(null);
  let selected   = $state<string>(NONE);
  let loading    = $state(true);
  let saving     = $state(false);
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  let saved      = $state(false);

  onMount(loadGlobal);

  // Re-load the per-repo override whenever the active tab changes — the
  // section is mounted once and switches context with the user.
  $effect(() => {
    if (tab) loadRepo(tab.id);
    else { selected = NONE; }
  });

  async function loadGlobal() {
    try {
      ideCfg = await getIdeConfig();
    } catch (e) {
      uiStore.showToast(`Failed to load IDE list: ${e}`, 'error');
    } finally {
      loading = false;
    }
  }

  async function loadRepo(tabId: string) {
    try {
      const ide = await getRepoIde(tabId);
      selected = ide ?? NONE;
    } catch {
      selected = NONE;
    }
  }

  async function persist(next: string) {
    if (!tab) return;
    saving = true;
    try {
      await setRepoIde(tab.id, next === NONE ? null : next);
      selected = next;
      saved = true;
      if (saveTimer) clearTimeout(saveTimer);
      saveTimer = setTimeout(() => { saved = false; }, 1800);
    } catch (e) {
      uiStore.showToast(`Save failed: ${e}`, 'error');
    } finally {
      saving = false;
    }
  }

  // Build the dropdown options live so newly-added custom IDEs appear
  // without re-mounting the section. The leading entry maps to NONE so
  // the user can clear the override back to the global default.
  const ideOptions = $derived.by(() => {
    const opts: { value: string; label: string }[] = [
      { value: NONE, label: '— Use global default —' },
    ];
    for (const b of BUILTIN_IDES) {
      opts.push({ value: b.id, label: b.name });
    }
    for (const c of ideCfg?.custom_ides ?? []) {
      opts.push({ value: c.id, label: `${c.name} (custom)` });
    }
    return opts;
  });

  // Resolved label for the global default — surfaced in the helper text
  // so the user understands what "use global default" means right now,
  // not in the abstract. Delegates to the shared resolver so the lookup
  // logic (built-in catalogue → custom IDEs → raw id fallback) stays in
  // one place.
  const defaultLabel = $derived(
    ideCfg ? findIdeLabel(ideCfg.default_ide, ideCfg.custom_ides ?? []) : '—'
  );
</script>

<SectionHeader
  title="External Integrations"
  description="Per-project overrides for the external tools configured globally under Settings → Tools."
/>

{#if loading}
  <div class="state-msg">Loading…</div>
{:else if !tab}
  <div class="empty-state">
    <FolderGit2 size={20} />
    <span>No repository open</span>
    <span class="empty-hint">Open a repository to configure project-bound integrations.</span>
  </div>
{:else}
  <div class="card">
    <div class="card-section-title">
      <ExternalLink size={12} />
      Open this project with…
      <span class="section-tab-name">{tab.name}</span>
    </div>

    <FormRow
      label="IDE"
      description="Pick an IDE to override the global default for this project. 'Open in IDE' actions on this repo will launch the chosen one. Manage the list of installed IDEs and detection under Settings → Tools → IDE Integration."
    >
      <div class="num-row">
        <Select
          value={selected}
          options={ideOptions}
          onchange={(v: string) => persist(v)}
        />
        {#if selected !== NONE}
          <button
            type="button"
            class="btn-ghost-sm"
            onclick={() => persist(NONE)}
            disabled={saving}
          >
            <RotateCcw size={11} /> Reset to global
          </button>
        {/if}
        {#if saving}
          <span class="status-text">Saving…</span>
        {:else if saved}
          <span class="status-text status-saved">Saved</span>
        {/if}
      </div>
    </FormRow>

    <div class="info-row">
      <Info size={12} />
      <span>
        {#if selected === NONE}
          Currently using the global default: <strong>{defaultLabel}</strong>.
        {:else}
          Stored in <code>.arbor/config.toml</code> in the repository root.
        {/if}
      </span>
    </div>
  </div>
{/if}

<style>
  .state-msg {
    padding: 12px 14px;
    color: var(--text-muted);
    font-size: 12px;
  }

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

  .num-row {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }

  .btn-ghost-sm {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 4px 9px;
    background: transparent;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    font-size: 11px;
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .btn-ghost-sm:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text-primary);
  }
  .btn-ghost-sm:disabled { opacity: 0.4; cursor: default; }

  .status-text {
    font-size: 11px;
    color: var(--text-muted);
  }
  .status-saved { color: var(--success); }

  .info-row {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    margin-top: 12px;
    padding: 9px 11px;
    background: color-mix(in srgb, var(--accent) 7%, transparent);
    color: var(--text-secondary);
    border-left: 2px solid var(--accent);
    border-radius: var(--radius-sm);
    font-size: 11.5px;
    line-height: 1.45;
  }
  .info-row :global(svg) { color: var(--accent); flex-shrink: 0; margin-top: 2px; }
  .info-row code {
    font-family: var(--font-code);
    font-size: 10.5px;
    padding: 1px 4px;
    background: rgba(255, 255, 255, 0.04);
    border-radius: 3px;
  }
</style>
