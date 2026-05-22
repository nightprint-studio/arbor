<script lang="ts">
  import { FolderGit2, ChevronDown, Check, Loader2, RefreshCw } from 'lucide-svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { getRepoConfig, setRepoConfig } from '$lib/ipc/config';
  import type { RepoConfig } from '$lib/ipc/config';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { linearGetAuthStatus, jiraGetAuthStatus, linearGetFilterOptions, jiraGetFilterOptions } from '$lib/ipc/issues';
  import { validateTicketRegex, setTicketLinkRepoConfig } from '$lib/ipc/ticket_links';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  // Unified entry used in the combo — works for both IssueProject and IssueTeam.
  type ProjectEntry = { id: string; name: string; color?: string | null };

  const tab = $derived(tabsStore.activeTab);

  let repoConfig        = $state<RepoConfig | null>(null);
  let repoConfigLoading = $state(false);
  let repoConfigSaving  = $state(false);
  let repoConfigError   = $state('');
  let repoConfigDirty   = $state(false);
  let regexError        = $state('');
  let regexValidating   = $state(false);

  // Local project state — completely independent from issuesStore (no sidebar side-effects).
  let projects        = $state<ProjectEntry[]>([]);
  let projectsLoading = $state(false);
  let projectsError   = $state('');
  let projectDropOpen  = $state(false);
  let projectSearchEl: HTMLInputElement | undefined = $state();
  $effect(() => { if (projectDropOpen) projectSearchEl?.focus(); });
  let projectAnchor    = $state<{ x: number; y: number; w: number } | null>(null);
  let projectSearch    = $state('');

  const filteredProjects = $derived(
    projectSearch.trim()
      ? projects.filter(p => p.name.toLowerCase().includes(projectSearch.toLowerCase()))
      : projects
  );

  $effect(() => {
    const t = tab;
    if (!t) { repoConfig = null; return; }
    repoConfigLoading = true;
    repoConfigError   = '';
    getRepoConfig(t.id)
      .then(cfg => {
        repoConfig = cfg;
        repoConfigDirty = false;
        if (cfg.issue_tracker === 'linear' || cfg.issue_tracker === 'jira') {
          void loadProjects(cfg.issue_tracker);
        } else {
          projects = [];
          projectsError = '';
        }
      })
      .catch(err => { repoConfigError = String(err); })
      .finally(() => { repoConfigLoading = false; });
  });

  async function loadProjects(tracker: string) {
    projectsLoading = true;
    projectsError   = '';
    projects        = [];
    try {
      // Step 1: init the backend client by calling auth status (same as the sidebar does).
      if (tracker === 'jira') {
        const auth = await jiraGetAuthStatus();
        if (!auth.authenticated) {
          projectsError = `Not connected to Jira (auth: ${JSON.stringify(auth)}). Go to Access → Issue Trackers to connect.`;
          return;
        }
      } else {
        const auth = await linearGetAuthStatus();
        if (!auth.authenticated) {
          projectsError = `Not connected to Linear. Go to Access → Issue Trackers to connect.`;
          return;
        }
      }
      // Step 2: fetch filter options (client now initialized).
      const opts = tracker === 'jira'
        ? await jiraGetFilterOptions()
        : await linearGetFilterOptions();
      // Jira maps its projects to `teams` (projects field is always empty for Jira).
      // Linear uses `projects` directly.
      projects = tracker === 'jira'
        ? (opts.teams ?? []).map(t => ({ id: t.id, name: t.name, color: null }))
        : (opts.projects ?? []);
      if (projects.length === 0) {
        projectsError = `Connected, but no projects found in ${tracker === 'jira' ? 'Jira' : 'Linear'}.`;
      }
    } catch (e) {
      projectsError = String(e);
    } finally {
      projectsLoading = false;
    }
  }

  async function refreshProjects() {
    if (!repoConfig?.issue_tracker) return;
    await loadProjects(repoConfig.issue_tracker);
  }

  async function save() {
    if (!tab || !repoConfig || regexError) return;
    repoConfigSaving = true;
    repoConfigError  = '';
    try {
      await setRepoConfig(tab.id, repoConfig);
      // Also call setTicketLinkRepoConfig so the backend clears the auto-parsed
      // cache — the new regex takes effect immediately without a restart.
      await setTicketLinkRepoConfig(tab.id, repoConfig.ticket_links ?? {});
      repoConfigDirty = false;
      uiStore.showToast('Issue Tracker settings saved', 'success');
    } catch (err) {
      repoConfigError = String(err);
    } finally {
      repoConfigSaving = false;
    }
  }

  function markDirty() { repoConfigDirty = true; }

  let _regexDebounce: ReturnType<typeof setTimeout> | null = null;

  function onPatternInput(value: string) {
    if (repoConfig) {
      repoConfig.ticket_links = value ? { custom_pattern: value } : undefined;
    }
    markDirty();
    regexError = '';
    if (_regexDebounce) clearTimeout(_regexDebounce);
    if (!value) return;
    regexValidating = true;
    _regexDebounce = setTimeout(async () => {
      try {
        regexError = await validateTicketRegex(value);
      } catch {
        regexError = '';
      } finally {
        regexValidating = false;
      }
    }, 350);
  }

  function onTrackerChange() {
    markDirty();
    if (repoConfig) repoConfig.issue_tracker_project_id = undefined;
    projects = [];
    projectsError = '';
    if (repoConfig?.issue_tracker === 'linear' || repoConfig?.issue_tracker === 'jira') {
      void loadProjects(repoConfig.issue_tracker);
    }
  }

  // ── Project dropdown ───────────────────────────────────────────────────────
  function openProjectDrop(el: HTMLElement) {
    const r = el.getBoundingClientRect();
    projectAnchor = { x: r.left, y: r.bottom + 4, w: r.width };
    projectDropOpen = true;
  }

  function selectProject(id: string | undefined) {
    if (!repoConfig) return;
    repoConfig.issue_tracker_project_id = id;
    markDirty();
    projectDropOpen = false;
    projectSearch   = '';
  }

  const selectedProject = $derived(
    repoConfig?.issue_tracker_project_id
      ? projects.find(p => p.id === repoConfig!.issue_tracker_project_id) ?? null
      : null
  );
</script>

{#if projectDropOpen}
  <button type="button" aria-label="Close menu" class="drop-backdrop" onclick={() => { projectDropOpen = false; projectSearch = ''; }}></button>
{/if}

<!-- Dropdown portal — rendered at root level, outside any card/overflow:hidden ancestor -->
{#if projectDropOpen && projectAnchor}
  <div
    class="proj-drop"
    style="left:{projectAnchor.x}px; top:{projectAnchor.y}px; min-width:{Math.max(projectAnchor.w, 280)}px;"
  >
    <div class="proj-search-wrap">
      <input
        class="proj-search"
        type="text"
        placeholder="Filter projects…"
        bind:value={projectSearch}
        bind:this={projectSearchEl}
      />
    </div>

    {#if !projectSearch}
      <button
        class="proj-item"
        class:proj-selected={!repoConfig?.issue_tracker_project_id}
        onclick={() => selectProject(undefined)}
      >
        <span class="proj-name">— All projects —</span>
        {#if !repoConfig?.issue_tracker_project_id}<span class="proj-check">✓</span>{/if}
      </button>
    {/if}

    {#if projectsLoading && projects.length === 0}
      <div class="proj-empty"><Loader2 size={11} class="spinning" />&nbsp;Loading…</div>
    {:else if projectsError && projects.length === 0}
      <div class="proj-empty">{projectsError}</div>
    {:else}
      {#each filteredProjects as p (p.id)}
        <button
          class="proj-item"
          class:proj-selected={repoConfig?.issue_tracker_project_id === p.id}
          onclick={() => selectProject(p.id)}
        >
          {#if p.color}
            <span class="proj-dot" style="background:{p.color}"></span>
          {/if}
          <span class="proj-name">{p.name}</span>
          {#if repoConfig?.issue_tracker_project_id === p.id}
            <span class="proj-check">✓</span>
          {/if}
        </button>
      {:else}
        <div class="proj-empty">No results</div>
      {/each}
    {/if}
  </div>
{/if}

<SectionHeader title="Issue Tracker" description="Per-project issue tracker and ticket link settings, stored in .arbor/config.toml." />

{#if !tab}
  <div class="empty-state">
    <FolderGit2 size={20} />
    <span>No repository open</span>
    <span class="empty-hint">Open a repository to configure issue tracker settings.</span>
  </div>

{:else if repoConfigLoading}
  <div class="empty-state">
    <span class="loading-dots">Loading…</span>
  </div>

{:else if repoConfig}

  <!-- Issue Tracker Provider -->
  <div class="card">
    <div class="card-section-title">Provider</div>
    <div class="card-row-note">
      The issue tracker to use in the Issues sidebar and Ticket Picker for this repository.
    </div>
    <FormRow label="Issue tracker" description="Active tracker for this project">
      <Select
        value={repoConfig.issue_tracker ?? ''}
        options={[
          { value: '', label: '— None —' },
          { value: 'linear', label: 'Linear' },
          { value: 'jira', label: 'Jira' },
        ]}
        onchange={(v) => {
          repoConfig!.issue_tracker = v || undefined;
          onTrackerChange();
        }}
      />
    </FormRow>
  </div>

  <!-- Project filter -->
  {#if repoConfig.issue_tracker === 'linear' || repoConfig.issue_tracker === 'jira'}
    <div class="card">
      <div class="card-section-title">
        Default Project Filter
        <button
          class="icon-btn refresh-btn"
          onclick={refreshProjects}
          disabled={projectsLoading}
          use:tooltip={`Reload projects from ${repoConfig.issue_tracker}`}
        >
          {#if projectsLoading}
            <Loader2 size={12} class="spinning" />
          {:else}
            <RefreshCw size={12} />
          {/if}
        </button>
      </div>
      <div class="card-row-note">
        When set, the sidebar and Ticket Picker will always pre-filter issues to this project.
        The user can still switch projects from the filter bar.
      </div>
      <FormRow label="Project" description="Default project filter for this repo">
        <button
          class="project-combo"
          onclick={(e) => openProjectDrop(e.currentTarget)}
          disabled={projectsLoading && projects.length === 0}
        >
          {#if projectsLoading && projects.length === 0}
            <Loader2 size={12} class="spinning" style="opacity:.5" />
            <span style="color:var(--text-muted)">Loading…</span>
          {:else if selectedProject}
            {#if selectedProject.color}
              <span class="proj-dot" style="background:{selectedProject.color}"></span>
            {/if}
            <span>{selectedProject.name}</span>
          {:else}
            <span style="color:var(--text-muted)">— All projects —</span>
          {/if}
          <ChevronDown size={12} style="margin-left:auto;opacity:.5;flex-shrink:0" />
        </button>
      </FormRow>

    </div>
  {/if}

  <!-- Ticket Links -->
  <div class="card">
    <div class="card-section-title">Ticket Links</div>
    <div class="card-row-note">
      Custom regex for extracting ticket IDs from commit messages and branch names.
      Leave blank to use the tracker default (<code>[A-Z][A-Z0-9]*-\d+</code> for Linear/Jira, <code>#\d+</code> for GitHub/GitLab).
      Must contain exactly one capture group, e.g. <code>\b(MYCO-\d+)\b</code>.
    </div>
    <FormRow label="Custom pattern" description="Overrides the default regex for this project">
      <div class="regex-control">
        <input
          class="text-input"
          class:input-error={!!regexError}
          type="text"
          placeholder="e.g. \b(MYCO-\d+)\b"
          value={repoConfig.ticket_links?.custom_pattern ?? ''}
          oninput={(e) => onPatternInput((e.target as HTMLInputElement).value.trim())}
        />
        {#if regexValidating}
          <span class="regex-hint"><Loader2 size={10} class="spinning" /> Validating…</span>
        {:else if regexError}
          <span class="regex-hint regex-hint-error">{regexError}</span>
        {:else if repoConfig.ticket_links?.custom_pattern}
          <span class="regex-hint regex-hint-ok">Valid pattern</span>
        {/if}
      </div>
    </FormRow>
  </div>

  {#if repoConfigError}
    <p class="form-error">{repoConfigError}</p>
  {/if}

  <div class="form-actions">
    <button
      class="btn-primary"
      onclick={save}
      disabled={repoConfigSaving || !repoConfigDirty || !!regexError}
    >
      {repoConfigSaving ? 'Saving…' : 'Save Changes'}
    </button>
    {#if !repoConfigDirty && !repoConfigSaving}
      <span class="saved-label">All changes saved</span>
    {/if}
  </div>

{:else if repoConfigError}
  <p class="form-error">{repoConfigError}</p>
{/if}

<style>
  /* ── Regex input ─────────────────────────────────────────────────────────── */
  .regex-control { flex-direction: column; align-items: stretch; gap: 4px; }
  .input-error { border-color: var(--danger, #e05252) !important; }
  .regex-hint {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 10px;
    font-family: var(--font-ui-sans);
    color: var(--text-muted);
  }
  .regex-hint-error { color: var(--danger, #e05252); }
  .regex-hint-ok    { color: var(--success, #4caf50); }

  /* ── Combo trigger button ─────────────────────────────────────────────────── */
  .project-combo {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 8px;
    min-width: 200px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    font-size: 12px;
    cursor: pointer;
    text-align: left;
    transition: border-color var(--transition-fast);
  }
  .project-combo:hover:not(:disabled) { border-color: var(--border-focus); }
  .project-combo:disabled { opacity: 0.5; cursor: not-allowed; }

  .proj-dot { width: 9px; height: 9px; border-radius: 50%; flex-shrink: 0; }

  /* ── Backdrop ─────────────────────────────────────────────────────────────── */
  .drop-backdrop { position: fixed; inset: 0; z-index: 1999; background: transparent; border: none; padding: 0; cursor: default; }

  /* ── Dropdown (matches sidebar .chip-drop) ────────────────────────────────── */
  .proj-drop {
    position: fixed;
    z-index: 2000;
    display: flex;
    flex-direction: column;
    max-height: 320px;
    overflow-y: auto;
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    padding: 4px;
    box-shadow: 0 8px 24px rgba(0,0,0,0.4);
  }

  /* sticky search — matches .chip-drop-search-wrap */
  .proj-search-wrap {
    padding: 4px 4px 2px;
    position: sticky;
    top: 0;
    background: var(--bg-overlay);
    z-index: 1;
  }
  .proj-search {
    width: 100%;
    box-sizing: border-box;
    padding: 4px 8px;
    font-size: 11px;
    font-family: var(--font-ui-sans);
    background: var(--bg-base);
    color: var(--text-primary);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    outline: none;
    transition: border-color var(--transition-fast);
  }
  .proj-search:focus { border-color: var(--accent); }

  /* items — matches .chip-drop-item */
  .proj-item {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 5px 8px;
    text-align: left;
    font-size: 11px;
    font-family: var(--font-ui-sans);
    color: var(--text-primary);
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: background var(--transition-fast);
  }
  .proj-item:hover { background: var(--bg-hover); }
  .proj-selected   { color: var(--accent); }

  .proj-name  { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .proj-check { margin-left: auto; font-size: 11px; flex-shrink: 0; }

  /* empty / error state — matches .chip-drop-empty */
  .proj-empty {
    display: flex;
    align-items: center;
    gap: 6px;
    justify-content: center;
    padding: 10px 8px;
    font-size: 11px;
    color: var(--text-muted);
    font-style: italic;
  }

  /* ── Refresh button in card-section-title ─────────────────────────────────── */
  .card-section-title { display: flex; align-items: center; gap: 6px; }
  .refresh-btn { margin-left: auto; width: 22px; height: 22px; }

  :global(.spinning) { animation: spin 1s linear infinite; }
</style>
