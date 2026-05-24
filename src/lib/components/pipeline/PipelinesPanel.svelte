<script lang="ts">
  import {
    Play, Square, RefreshCw, ChevronRight, ChevronDown, Clock,
    CheckCircle, XCircle, Circle, Ban,
    ExternalLink, RotateCcw, GitBranch, AlertCircle, Workflow,
    Trash2, RotateCw, Filter, Link2,
  } from 'lucide-svelte';
  import { copyDeepLink } from '$lib/utils/deep-link-builder';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import BrandIcon from '$lib/components/shared/internal/BrandIcon.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';
  import Contribution from '$lib/components/shared/Contribution.svelte';
  import PluginIcon from '$lib/components/plugins/PluginIcon.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import ContextMenu, { type MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import PipelineRunGraph from './PipelineRunGraph.svelte';
  import CiPipelineDetailModal from './CiPipelineDetailModal.svelte';
  import CreateCiPipelineModal from './CreateCiPipelineModal.svelte';
  import { pipelinesStore } from '$lib/stores/pipelines.svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  const CI_POLL_INTERVAL_MS = 10_000;
  import {
    requestPipelineRun, cancelPipelineRun, retrigerCiRun,
    resumePipelineRun, discardPipelineRun,
  } from '$lib/ipc/pipeline';
  import type { PipelineDef, PipelineRun, CiRun } from '$lib/types/pipeline';

  // ── Tab switching ─────────────────────────────────────────────────────────
  let activeTab = $state<'local' | 'ci'>('local');

  const defs = $derived(pipelinesStore.defs);
  const runs = $derived(pipelinesStore.runs);

  // Augment the registered-def list with a synthetic entry for every run
  // whose pipeline_id is NOT currently registered (one-shot dynamic
  // pipelines, plugins unloaded after a run, persisted runs from a
  // previous session whose def has since changed, …). Without this a user
  // who kicked off a Tomcat deploy had no way to see the run in the panel
  // — the Run button is meaningless for a missing def, but viewing the
  // history is still useful.
  type DefWithFlag = PipelineDef & { __synthetic: boolean };
  const allDefs: DefWithFlag[] = $derived.by(() => {
    const out: DefWithFlag[] = defs.map(d => ({ ...d, __synthetic: false }));
    const known = new Set(defs.map(d => defKey(d)));
    const seen  = new Set<string>();
    // Most recent run for each unique (plugin, pipeline_id) seeds the
    // synthetic def so the chip label matches what the user expects.
    for (const run of runs) {
      const key = `${run.plugin}::${run.pipeline_id}`;
      if (known.has(key) || seen.has(key)) continue;
      seen.add(key);
      out.push({
        id:          run.pipeline_id,
        name:        run.name,
        plugin:      run.plugin,
        description: null,
        icon:        null,
        stages:      [],
        __synthetic: true,
      });
    }
    return out;
  });

  // ── Local pipeline filter ─────────────────────────────────────────────────
  // Multi-value filter applied to the run list. Empty set = show every run.
  // The filter sits in a `<Dropdown selectionMode="multiple">`; the run list
  // below it is `runs` masked through this set so different defs can coexist
  // in the same view.
  let defFilter = $state<Set<string>>(new Set());

  function defKey(d: PipelineDef) { return `${d.plugin}::${d.id}`; }

  /** Most recent first: the `runs` array is already store-sorted that way. */
  const filteredLocalRuns = $derived(
    defFilter.size === 0
      ? runs
      : runs.filter(r => defFilter.has(`${r.plugin}::${r.pipeline_id}`))
  );

  /** Look up the def matching a run so the card can show a tiny chip naming
   *  which pipeline it belongs to (the per-def selector strip is gone). */
  function defOfRun(run: PipelineRun): DefWithFlag | null {
    return allDefs.find(d => d.plugin === run.plugin && d.id === run.pipeline_id) ?? null;
  }

  function toggleDefFilter(key: string) {
    const next = new Set(defFilter);
    if (next.has(key)) next.delete(key); else next.add(key);
    defFilter = next;
  }
  function clearDefFilter() { defFilter = new Set(); }

  /** Items rendered inside the filter `<Dropdown>` — one per known def. */
  const filterDropdownItems = $derived<DropdownItem[]>(
    allDefs.map(d => {
      const k = defKey(d);
      return {
        kind: 'item',
        id: k,
        label: d.name,
        subtitle: d.plugin,
        active: defFilter.has(k),
        onclick: () => toggleDefFilter(k),
      } as DropdownItem;
    })
  );

  /** "Preferred" def used by the Run button — the most recently launched
   *  pipeline, so pressing Play replays the latest run. Falls back to the
   *  first filter match, then to defs[0]. To pick a different pipeline,
   *  the user right-clicks one of its runs in the list. */
  const preferredRunDef = $derived.by<PipelineDef | null>(() => {
    if (defs.length === 0) return null;
    if (defs.length === 1) return defs[0];
    const lastRun = runs[0];
    if (lastRun) {
      const lastDef = defs.find(d => d.plugin === lastRun.plugin && d.id === lastRun.pipeline_id);
      if (lastDef) return lastDef;
    }
    const filteredDef = defs.find(d => defFilter.has(defKey(d)));
    return filteredDef ?? defs[0];
  });

  async function triggerRun(def: PipelineDef | null | undefined) {
    if (!def || !def.plugin || !def.id) {
      uiStore.showToast('No pipeline selected to run.', 'warning');
      return;
    }
    try {
      const tabId = tabsStore.activeTabId ?? undefined;
      // Routed launch: backend delegates to the def's owning plugin via the
      // `on_pipeline_run_request` hook when the plugin has opted in (the
      // plugin then compiles its lazy stages + calls arbor.pipeline.run).
      // Plugins without a handler keep the legacy direct-run behaviour.
      await requestPipelineRun(def.plugin, def.id, tabId);
    } catch (err) {
      uiStore.showToast(`Failed to start pipeline: ${err}`, 'error');
    }
  }

  function runMain() { triggerRun(preferredRunDef); }

  // ── Run-card context menu ─────────────────────────────────────────────────
  // Right-click on a run card → menu with "Run pipeline" plus the same
  // status-aware actions the row's hover buttons expose. Lets the user
  // launch any pipeline they see — picking a different def than the
  // preferred one — without having to dig through the plugin's UI.
  type RunCtx = { x: number; y: number; run: PipelineRun };
  let runCtxMenu = $state<RunCtx | null>(null);

  function openRunContext(e: MouseEvent, run: PipelineRun) {
    e.preventDefault();
    e.stopPropagation();
    runCtxMenu = { x: e.clientX, y: e.clientY, run };
  }

  const runCtxItems = $derived.by<MenuItem[]>(() => {
    const m = runCtxMenu;
    if (!m) return [];
    const r          = m.run;
    const terminal   = isTerminalStatus(r.status);
    const resumable  = r.status === 'failed' || r.status === 'paused' || r.status === 'cancelled';
    const def        = defOfRun(r);
    const items: MenuItem[] = [
      {
        id: 'run',
        label: def ? `Run “${def.name}”` : 'Run pipeline',
        disabled: !def || def.__synthetic,
      },
      { id: 'open', label: 'Open detail' },
      { id: 'sep1', separator: true, label: '' },
    ];
    if (r.status === 'running') {
      items.push({ id: 'cancel', label: 'Cancel run' });
    }
    if (resumable) {
      items.push({ id: 'resume', label: 'Resume from failed step' });
    }
    if (terminal) {
      items.push({ id: 'discard', label: 'Discard run', danger: true });
    }
    return items;
  });

  function handleRunCtxSelect(id: string) {
    const m = runCtxMenu;
    if (!m) return;
    const r = m.run;
    switch (id) {
      case 'run': {
        const def = defOfRun(r);
        if (def && !def.__synthetic) triggerRun(def);
        break;
      }
      case 'open':    openLocalRun(r);    break;
      case 'cancel':  cancelRun(r.id);    break;
      case 'resume':  resumeRun(r.id);    break;
      case 'discard': discardRun(r.id);   break;
    }
  }

  // ── CI run context menu ───────────────────────────────────────────────
  // Mirror of `runCtxMenu` but for the GitHub Actions / GitLab CI list.
  // Currently only carries the deep-link copy entry.
  type CiCtx = { x: number; y: number; run: import('$lib/types/pipeline').CiRun };
  let ciCtxMenu = $state<CiCtx | null>(null);

  function openCiContext(e: MouseEvent, run: CiCtx['run']) {
    e.preventDefault();
    e.stopPropagation();
    ciCtxMenu = { x: e.clientX, y: e.clientY, run };
  }

  const ciCtxItems = $derived<MenuItem[]>(ciCtxMenu ? [
    { id: 'copy-deep-link', label: 'Copy arbor:// link', icon: Link2, iconColor: '#20b2aa' },
  ] : []);

  function handleCiCtxSelect(id: string) {
    const m = ciCtxMenu;
    ciCtxMenu = null;
    if (!m) return;
    const tabId = tabsStore.activeTabId;
    if (id === 'copy-deep-link' && tabId) {
      void copyDeepLink({ kind: 'pipeline_open', runId: String(m.run.id) }, tabId);
    }
  }

  // ── Bulk toolbar actions ──────────────────────────────────────────────────
  const hasRunningLocal   = $derived(runs.some(r => r.status === 'running'));
  const hasResumableLocal = $derived(runs.some(r => r.status === 'failed' || r.status === 'paused' || r.status === 'cancelled'));
  const hasTerminalLocal  = $derived(runs.some(r => isTerminalStatus(r.status)));

  async function stopAllRunning() {
    const ids = runs.filter(r => r.status === 'running').map(r => r.id);
    for (const id of ids) {
      try { await cancelPipelineRun(id); } catch { /* keep going */ }
    }
  }
  async function resumeLastFailed() {
    // `runs` is most-recent-first so the first match is the latest one.
    const target = runs.find(r => r.status === 'failed' || r.status === 'paused' || r.status === 'cancelled');
    if (target) await resumeRun(target.id);
  }
  async function clearTerminalRuns() {
    const ids = runs.filter(r => isTerminalStatus(r.status)).map(r => r.id);
    for (const id of ids) {
      try { await discardPipelineRun(id); } catch { /* keep going */ }
    }
  }

  async function cancelRun(runId: string) {
    try {
      await cancelPipelineRun(runId);
      uiStore.showToast('Cancel requested — terminating current step…', 'info');
    } catch (err) {
      uiStore.showToast(`Cancel failed: ${err}`, 'error');
    }
  }

  async function resumeRun(runId: string) {
    try {
      await resumePipelineRun(runId);
    } catch (err) {
      uiStore.showToast(`Resume failed: ${err}`, 'error');
    }
  }

  async function discardRun(runId: string) {
    try {
      await discardPipelineRun(runId);
    } catch (err) {
      uiStore.showToast(`Discard failed: ${err}`, 'error');
    }
  }

  function stepsCount(run: PipelineRun): number {
    let n = 0;
    for (const s of run.stages ?? []) n += s.steps?.length ?? 0;
    return n;
  }

  function isTerminalStatus(s: string): boolean {
    return s === 'success' || s === 'failed' || s === 'cancelled';
  }

  // Open the standard PipelineRunDetailModal (mounted in AppShell, watches
  // pipelinesStore.activeRunId). Delegating to the shared component keeps a
  // single source of truth for the detail UI — graph + per-step output log
  // wrapped in `<Modal>` chrome — instead of re-implementing a lookalike
  // floating panel here. The modal handles auto-selecting the first
  // failed/running step and tail-follow behavior on its own.
  function openLocalRun(run: PipelineRun) {
    pipelinesStore.setActiveRun(run.id);
  }

  // ── CI state ──────────────────────────────────────────────────────────────
  const ciProvider  = $derived(pipelinesStore.ciProvider);
  const ciRuns      = $derived(pipelinesStore.ciRuns);
  const ciLoading   = $derived(pipelinesStore.ciLoading);
  const ciError     = $derived(pipelinesStore.ciError);

  let retriggeringId      = $state<string | null>(null);
  let selectedCiRun       = $state<CiRun | null>(null);
  let showCreateModal     = $state(false);

  // Load CI info when switching to the CI tab or when the active tab changes.
  // Tab-change branch also clears any open modal / pending re-trigger from
  // the previous tab so a quick click on the detail button while the new
  // tab's CI runs are still loading can't open a stale CiRun whose `id`
  // belongs to a different provider/repo (would crash `fetch_ci_jobs`).
  let lastLoadedCiTabId = $state<string | null>(null);
  $effect(() => {
    if (activeTab !== 'ci') return;
    const tabId = tabsStore.activeTabId;
    if (tabId !== lastLoadedCiTabId) {
      selectedCiRun  = null;
      retriggeringId = null;
      lastLoadedCiTabId = tabId;
    }
    pipelinesStore.loadCi(tabId);
  });

  async function refreshCi() {
    const tabId = tabsStore.activeTabId;
    if (tabId) await pipelinesStore.refreshCiRuns(tabId);
  }

  // Auto-refresh the CI run list while the user is actively watching it.
  // Gated on three conditions to avoid burning the GitHub rate limit (5000/h)
  // or wasting bandwidth when the user can't see the panel:
  //   - CI tab is the active sub-tab in this panel
  //   - the app window is focused (uiStore.appFocused)
  //   - at least one visible run is in a non-terminal state
  // Stops automatically once everything has settled to success/failed/cancelled.
  const hasActiveCiRun = $derived(
    ciRuns.some(r => r.status === 'running' || r.status === 'pending'),
  );
  $effect(() => {
    if (activeTab !== 'ci') return;
    if (!uiStore.appFocused) return;
    if (!hasActiveCiRun) return;
    const tabId = tabsStore.activeTabId;
    if (!tabId) return;
    const id = setInterval(() => {
      pipelinesStore.refreshCiRuns(tabId).catch(() => { /* ignore */ });
    }, CI_POLL_INTERVAL_MS);
    return () => clearInterval(id);
  });

  // ── Shared helpers ────────────────────────────────────────────────────────
  function statusIcon(s: string) {
    switch (s) {
      case 'success':   return CheckCircle;
      case 'failed':    return XCircle;
      // 'running' is rendered with <Spinner> instead of an icon.
      case 'cancelled': return Ban;
      default:          return Circle;
    }
  }

  function statusClass(s: string): string {
    switch (s) {
      case 'success':   return 'status-success';
      case 'failed':    return 'status-failed';
      case 'running':   return 'status-running';
      case 'cancelled': return 'status-cancelled';
      default:          return 'status-pending';
    }
  }

  /** Status label that distinguishes a parked run (`queued = true`) from
   *  one that's in the brief moment between spawn and Running. CI runs
   *  never have a `queued` flag so callers can fall through to the plain
   *  `statusLabel(s)` overload below. */
  function localStatusLabel(run: PipelineRun): string {
    if (run.status === 'pending' && run.queued) return 'Queued';
    return statusLabel(run.status);
  }

  /** CSS class variant for the status pill — picks up the queued state so
   *  the pill can use a distinct accent (`.ci-status-queued`) rather than
   *  the bare grey "pending" look. */
  function localStatusClass(run: PipelineRun): string {
    if (run.status === 'pending' && run.queued) return 'ci-status-queued';
    return `ci-status-${run.status}`;
  }

  function formatTs(ms: number | null): string {
    if (!ms) return '—';
    return new Date(ms).toLocaleTimeString();
  }

  function elapsed(run: PipelineRun | null): string {
    if (!run?.started_at) return '';
    const end = run.finished_at ?? Date.now();
    const ms  = end - run.started_at;
    if (ms < 1000) return `${ms}ms`;
    if (ms < 60000) return `${(ms / 1000).toFixed(1)}s`;
    return `${Math.floor(ms / 60000)}m ${Math.floor((ms % 60000) / 1000)}s`;
  }

  function timeAgo(iso: string): string {
    const ms = Date.now() - new Date(iso).getTime();
    if (ms < 60_000)  return 'just now';
    if (ms < 3_600_000) return `${Math.floor(ms / 60_000)}m ago`;
    if (ms < 86_400_000) return `${Math.floor(ms / 3_600_000)}h ago`;
    return `${Math.floor(ms / 86_400_000)}d ago`;
  }

  function providerLabel(p: string): string {
    return p === 'github' ? 'GitHub Actions' : p === 'gitlab' ? 'GitLab CI' : p;
  }

  function statusLabel(s: string): string {
    switch (s) {
      case 'success':   return 'Passed';
      case 'failed':    return 'Failed';
      case 'running':   return 'Running';
      case 'cancelled': return 'Cancelled';
      default:          return 'Pending';
    }
  }

  function formatDuration(secs: number | null): string {
    if (!secs) return '';
    if (secs < 60) return `${Math.round(secs)}s`;
    const m = Math.floor(secs / 60);
    const s = Math.floor(secs % 60);
    return `${m}m ${s.toString().padStart(2, '0')}s`;
  }

  async function handleRetriggerForRun(run: CiRun) {
    const tabId = tabsStore.activeTabId;
    if (!tabId) return;
    retriggeringId = run.id;
    try {
      await retrigerCiRun(tabId, run.id);
      await pipelinesStore.refreshCiRuns(tabId);
      // Refresh the modal run reference too
      if (selectedCiRun?.id === run.id) {
        selectedCiRun = pipelinesStore.ciRuns.find(r => r.id === run.id) ?? selectedCiRun;
      }
    } catch (err) {
      uiStore.showToast(`Failed to re-trigger: ${err}`, 'error');
    } finally {
      retriggeringId = null;
    }
  }
</script>

<div class="pipe-root">
  <BottomPanelHeader title="Pipelines">
    {#snippet icon()}<Workflow size={14} />{/snippet}
    <!-- Sub-tabs live inline in the header so they don't claim a second
         strip of chrome below. The `pipe-tabs` class trims the underline
         tab strip's vertical padding to match the 34px header height. -->
    <div class="pipe-tabs">
      <Tabs
        items={[
          { id: 'local', label: 'Local Pipelines' },
          { id: 'ci',    label: 'CI / CD' },
        ]}
        value={activeTab}
        variant="underline"
        size="sm"
        onSelect={(id) => activeTab = id as 'local' | 'ci'}
      />
    </div>
    {#snippet actions()}
      {#if activeTab === 'local'}
        <button class="ps-btn" use:tooltip={'Refresh pipelines'} onclick={() => pipelinesStore.reload()}>
          <RefreshCw size={13} />
        </button>
      {:else if activeTab === 'ci' && ciProvider?.has_token}
        <button
          class="ps-btn ps-btn-accent"
          use:tooltip={'Create new pipeline run'}
          onclick={() => { showCreateModal = true; }}
        >
          <Play size={13} />
        </button>
        <button
          class="ps-btn"
          use:tooltip={'Refresh runs'}
          disabled={ciLoading}
          onclick={refreshCi}
        >
          {#if ciLoading}
            <Spinner size={13} color="currentColor" />
          {:else}
            <RefreshCw size={13} />
          {/if}
        </button>
      {/if}
    {/snippet}
  </BottomPanelHeader>

  <!-- ════════════════════════════════════════════════════════════════ -->
  <!-- LOCAL PIPELINES TAB                                              -->
  <!-- Two-column layout (IntelliJ Run-panel-style):                    -->
  <!--   • left vertical toolbar: Run / Stop all / Resume last / Clear  -->
  <!--   • right column: filter row + run list                          -->
  <!-- ════════════════════════════════════════════════════════════════ -->
  {#if activeTab === 'local'}
    <div class="local-layout">

      <!-- Left toolbar — global pipeline-level actions. The Run split-button
           main-click runs the only def (or the first filtered one) and the
           dropdown lists every registered def. Bulk actions (Stop / Resume
           last / Clear) operate over the whole `runs` list, not the
           filtered view, so they remain predictable regardless of filter. -->
      <div class="pipe-toolbar">
        <!-- Single Run button: replays the most-recently-launched pipeline
             (`preferredRunDef`). To run a different pipeline the user
             right-clicks one of its runs in the list — that opens a
             context menu with a Run entry which fires `triggerRun` for
             the run's owning def. Keeps the toolbar column narrow and
             removes the SplitButton dropdown plumbing entirely. -->
        <button
          class="pipe-tb-btn pipe-tb-run"
          onclick={runMain}
          disabled={!preferredRunDef}
          use:tooltip={preferredRunDef
            ? { content: `Run ${preferredRunDef.name}`, description: 'Right-click a run card to launch a different one' }
            : { content: 'No pipelines available', description: 'Start one from its plugin UI first' }}
          aria-label="Run last pipeline"
        >
          <Play size={13} />
        </button>

        <button
          class="pipe-tb-btn"
          use:tooltip={'Stop all running'}
          disabled={!hasRunningLocal}
          onclick={stopAllRunning}
        >
          <Square size={13} />
        </button>

        <button
          class="pipe-tb-btn"
          use:tooltip={'Resume last failed / paused / cancelled run'}
          disabled={!hasResumableLocal}
          onclick={resumeLastFailed}
        >
          <RotateCw size={13} />
        </button>

        <button
          class="pipe-tb-btn pipe-tb-danger"
          use:tooltip={{ content: 'Clear run history', description: 'Keeps running' }}
          disabled={!hasTerminalLocal}
          onclick={clearTerminalRuns}
        >
          <Trash2 size={13} />
        </button>

        <!-- Plugin extension point — plugins contribute extra toolbar
             buttons via `arbor:pipelines:toolbar`. Each contribution gets
             a payload of `{ icon, tooltip, label?, accent?, danger?,
             disabled?, divider_before? }` and a fire callback. -->
        <Contribution point="arbor:pipelines:toolbar">
          {#snippet item({ payload, fire })}
            {@const p = payload as { icon?: string; tooltip?: string; label?: string; accent?: boolean; success?: boolean; danger?: boolean; disabled?: boolean; divider_before?: boolean }}
            {#if p.divider_before}
              <span class="pipe-tb-sep"></span>
            {/if}
            <button
              type="button"
              class="pipe-tb-btn"
              class:pipe-tb-accent={p.accent}
              class:pipe-tb-success={p.success}
              class:pipe-tb-danger={p.danger}
              use:tooltip={p.tooltip ?? p.label ?? ''}
              disabled={!!p.disabled}
              onclick={() => fire()}
            >
              {#if p.icon}<PluginIcon name={p.icon} size={13} />{/if}
            </button>
          {/snippet}
        </Contribution>
      </div>

      <!-- Right column: filter row + run list -->
      <div class="pipe-content">
        {#if allDefs.length === 0 && runs.length === 0}
          <div class="ci-state-view">
            <Workflow size={28} class="ci-state-icon ci-state-muted" />
            <p class="ci-state-title">Nessuna pipeline registrata</p>
            <p class="ci-state-hint">
              Usa <code>arbor.pipeline.define()</code> da un plugin per registrare una pipeline.
            </p>
          </div>
        {:else}
          <!-- Filter row: multi-select dropdown of pipeline definitions.
               Empty selection = show every run. The "Clear" pill appears
               only when at least one filter is active. -->
          <div class="pipe-filter-row">
            <Dropdown
              items={filterDropdownItems}
              selectionMode="multiple"
              searchable
              searchPlaceholder="Search pipelines…"
              position="fixed"
              width="280px"
              emptyMessage="No pipelines"
            >
              {#snippet trigger({ toggle, open })}
                <button
                  type="button"
                  class="pipe-filter-btn"
                  class:open
                  onclick={toggle}
                  use:tooltip={'Filter run list by pipeline'}
                >
                  <Filter size={12} />
                  <span class="pipe-filter-label">
                    {defFilter.size === 0
                      ? 'All pipelines'
                      : `${defFilter.size} selected`}
                  </span>
                  <ChevronDown size={11} />
                </button>
              {/snippet}
            </Dropdown>
            {#if defFilter.size > 0}
              <button class="pipe-filter-clear" onclick={clearDefFilter}>
                Clear
              </button>
            {/if}
            <span class="pipe-filter-count">
              {filteredLocalRuns.length} run{filteredLocalRuns.length === 1 ? '' : 's'}
            </span>
          </div>

          {#if filteredLocalRuns.length === 0}
            <div class="ci-state-view">
              <Circle size={28} class="ci-state-icon ci-state-muted" />
              {#if runs.length === 0}
                <p class="ci-state-title">Nessuna run ancora eseguita</p>
                <p class="ci-state-hint">Clic su <strong>Run</strong> per lanciare la prima.</p>
              {:else}
                <p class="ci-state-title">No runs match the current filter</p>
                <p class="ci-state-hint">Remove the filter to see every run.</p>
              {/if}
            </div>
          {:else}
            <div class="ci-run-list">
              {#each filteredLocalRuns as run (run.id)}
                {@const steps = stepsCount(run)}
                {@const terminal = isTerminalStatus(run.status)}
                {@const resumable = (run.status === 'failed' || run.status === 'paused' || run.status === 'cancelled')}
                {@const StatusIcon = statusIcon(run.status)}
                {@const def = defOfRun(run)}
                <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
                <div
                  class="ci-run-card"
                  role="button"
                  tabindex="0"
                  onclick={() => openLocalRun(run)}
                  onkeydown={(e) => e.key === 'Enter' && openLocalRun(run)}
                  oncontextmenu={(e) => openRunContext(e, run)}
                >
                  <!-- Left: status badge + duration -->
                  <div class="ci-card-left">
                    <span class="ci-status-pill {localStatusClass(run)}">
                      {#if run.status === 'running'}
                        <Spinner size={11} color="currentColor" />
                      {:else if run.status === 'pending' && run.queued}
                        <Clock size={11} />
                      {:else}
                        <StatusIcon size={11} />
                      {/if}
                      {localStatusLabel(run)}
                    </span>
                    <span class="ci-card-dur">
                      <Clock size={9} />
                      {elapsed(run)}
                    </span>
                  </div>

                  <!-- Center: name, id, chips -->
                  <div class="ci-card-body">
                    <div class="ci-card-title">
                      <span class="ci-card-name">{run.name}</span>
                      <span class="ci-card-id">#{run.id.replace('pipe-run-', '')}</span>
                    </div>
                    <div class="ci-card-chips">
                      {#if def}
                        <Badge variant="tone" tone="neutral" size="md">
                          <span class="ci-chip-def-icon">{def.icon ?? '⚡'}</span>
                          {def.name}
                        </Badge>
                        {#if def.__synthetic}
                          <Badge variant="tone" tone="warning" size="sm" label="orphan" />
                        {/if}
                      {/if}
                      <span class="ci-chip">{steps} step</span>
                      <span class="ci-chip ci-chip-time">{formatTs(run.started_at)}</span>
                      {#if run.log_level && run.log_level !== 'info'}
                        <span class="ci-chip">log: {run.log_level}</span>
                      {/if}
                    </div>
                  </div>

                  <!-- Right: actions -->
                  <div class="ci-card-actions" role="toolbar" tabindex="-1" aria-label="Run actions" onclick={(e) => e.stopPropagation()}>
                    {#if run.status === 'running' || (run.status === 'pending' && run.queued)}
                      <button
                        class="ci-action-btn"
                        use:tooltip={run.queued ? 'Annulla run in coda' : 'Annulla run'}
                        onclick={() => cancelRun(run.id)}
                      >
                        <Square size={12} />
                      </button>
                    {/if}
                    {#if resumable}
                      <button class="ci-action-btn" use:tooltip={'Riprendi dallo step fallito'}
                              onclick={() => resumeRun(run.id)}>
                        <RotateCw size={12} />
                      </button>
                    {/if}
                    {#if terminal}
                      <button class="ci-action-btn ci-action-danger" use:tooltip={'Elimina run'}
                              onclick={() => discardRun(run.id)}>
                        <Trash2 size={12} />
                      </button>
                    {/if}
                    <button class="ci-action-btn" use:tooltip={'Apri dettaglio'}
                            onclick={() => openLocalRun(run)}>
                      <ChevronRight size={12} />
                    </button>
                  </div>
                </div>
              {/each}
            </div>
          {/if}
        {/if}
      </div>
    </div>

  <!-- ════════════════════════════════════════════════════════════════ -->
  <!-- CI / CD TAB                                                      -->
  <!-- ════════════════════════════════════════════════════════════════ -->
  {:else}
    <div class="ci-view">
      <!-- Provider info row -->
      {#if ciProvider || !ciLoading}
        <div class="ci-header">
          {#if ciProvider}
            <span class="ci-provider-badge ci-provider-{ciProvider.provider}">
              <!-- Provider glyph: GitHub follows the surrounding text colour
                   (themable), GitLab uses its absolute brand orange via the
                   `.ci-provider-gitlab` rule below. <BrandIcon> inherits
                   `color` from the parent span so the rule still applies. -->
              <BrandIcon brand={ciProvider.provider} size={12} />
              {providerLabel(ciProvider.provider)}
            </span>
            {#if ciProvider.owner && ciProvider.repo_name}
              <span class="ci-repo-path">{ciProvider.owner}/{ciProvider.repo_name}</span>
            {:else if ciProvider.project_path}
              <span class="ci-repo-path">{ciProvider.project_path}</span>
            {/if}
          {:else}
            <span class="ci-no-provider">No CI/CD remote detected</span>
          {/if}
        </div>
      {/if}

      <!-- Body -->
      {#if ciLoading && !ciProvider}
        <!-- Tab just switched: provider detection round-trip in flight. The
             spinner wins over the "No CI/CD remote" empty state so we don't
             flash a misleading message while the new tab's provider is being
             resolved. -->
        <div class="ci-state-view">
          <Spinner size="lg" label="Loading CI provider…" block />
        </div>

      {:else if !ciProvider}
        <div class="ci-state-view">
          <AlertCircle size={28} class="ci-state-icon ci-state-muted" />
          <p class="ci-state-title">No CI/CD remote detected</p>
          <p class="ci-state-hint">
            Open a repository with a GitHub or GitLab remote to see pipeline runs here.
          </p>
        </div>

      {:else if !ciProvider.has_token}
        <div class="ci-state-view">
          <AlertCircle size={28} class="ci-state-icon ci-state-warn" />
          <p class="ci-state-title">{providerLabel(ciProvider.provider)} detected</p>
          <p class="ci-state-hint">
            Connect your {providerLabel(ciProvider.provider)} account in
            <strong>Settings → Authentication</strong> to view and manage runs.
          </p>
        </div>

      {:else if ciLoading && ciRuns.length === 0}
        <div class="ci-state-view">
          <Spinner size="lg" label="Loading pipeline runs…" block />
        </div>

      {:else if ciError}
        <div class="ci-state-view">
          <AlertCircle size={28} class="ci-state-icon ci-state-warn" />
          <p class="ci-state-title">Failed to load runs</p>
          <p class="ci-state-hint ci-error-text">{ciError}</p>
          {#if ciError.includes('401') || ciError.toLowerCase().includes('unauthorized')}
            <p class="ci-state-hint ci-pat-hint">
              GitLab API requires a <strong>Personal Access Token</strong> (scope: <code>api</code>), not a password.
              Update your credential in <strong>Settings → Authentication</strong>.
            </p>
          {/if}
          <button class="run-btn" onclick={refreshCi}>
            <RefreshCw size={12} /> Retry
          </button>
        </div>

      {:else if ciRuns.length === 0}
        <div class="ci-state-view">
          <Circle size={28} class="ci-state-icon ci-state-muted" />
          <p class="ci-state-title">No runs found</p>
          <p class="ci-state-hint">Push a commit to trigger your first pipeline.</p>
        </div>

      {:else}
        <div class="ci-run-list">
          {#each ciRuns as run (run.id)}
            {@const StatusIcon = statusIcon(run.status)}
            <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
            <div
              class="ci-run-card"
              role="button"
              tabindex="0"
              onclick={() => { selectedCiRun = run; }}
              onkeydown={(e) => e.key === 'Enter' && (selectedCiRun = run)}
              oncontextmenu={(e) => openCiContext(e, run)}
            >
              <!-- Left: status badge + duration -->
              <div class="ci-card-left">
                <span class="ci-status-pill ci-status-{run.status}">
                  {#if run.status === 'running'}
                    <Spinner size={11} color="currentColor" />
                  {:else}
                    <StatusIcon size={11} />
                  {/if}
                  {statusLabel(run.status)}
                </span>
                {#if run.duration_secs}
                  <span class="ci-card-dur">
                    <Clock size={9} />
                    {formatDuration(run.duration_secs)}
                  </span>
                {/if}
              </div>

              <!-- Center: name, branch, sha, time -->
              <div class="ci-card-body">
                <div class="ci-card-title">
                  <span class="ci-card-name">{run.name}</span>
                  <span class="ci-card-id">#{run.id}</span>
                </div>
                <div class="ci-card-chips">
                  <span class="ci-chip ci-chip-branch">
                    <GitBranch size={9} />
                    {run.branch}
                  </span>
                  <span class="ci-chip ci-chip-sha">{run.commit_sha}</span>
                  <span class="ci-chip ci-chip-time">{timeAgo(run.created_at)}</span>
                </div>
              </div>

              <!-- Right: actions -->
              <div class="ci-card-actions" role="toolbar" tabindex="-1" aria-label="Run actions" onclick={(e) => e.stopPropagation()}>
                <button
                  class="ci-action-btn"
                  use:tooltip={'Re-trigger'}
                  disabled={retriggeringId === run.id || run.status === 'running'}
                  onclick={() => handleRetriggerForRun(run)}
                >
                  {#if retriggeringId === run.id}
                    <Spinner size={12} color="currentColor" />
                  {:else}
                    <RotateCcw size={12} />
                  {/if}
                </button>
                <button
                  class="ci-action-btn"
                  type="button"
                  use:tooltip={'Open in browser'}
                  onclick={() => openUrl(run.web_url).catch(() => {})}
                >
                  <ExternalLink size={12} />
                </button>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  {/if}
</div>

<!-- Local-pipeline run detail is rendered by the shared
     `<PipelineRunDetailModal />` (mounted unconditionally in AppShell).
     `openLocalRun` just sets `pipelinesStore.activeRunId` and the modal
     pops with the standard chrome — no inline pseudo-modal here. -->

<!-- Create CI Pipeline Modal -->
{#if showCreateModal && ciProvider}
  <CreateCiPipelineModal
    provider={ciProvider}
    tabId={tabsStore.activeTabId ?? ''}
    onClose={() => { showCreateModal = false; }}
    onCreated={async () => {
      showCreateModal = false;
      await refreshCi();
    }}
  />
{/if}

<!-- CI Pipeline Detail Modal -->
{#if selectedCiRun}
  {@const tabId = tabsStore.activeTabId ?? ''}
  <CiPipelineDetailModal
    run={selectedCiRun}
    {tabId}
    onClose={() => { selectedCiRun = null; }}
    onRetrigger={() => handleRetriggerForRun(selectedCiRun!)}
  />
{/if}

<!-- Run-card right-click menu — fired from `oncontextmenu` on each card.
     Items are derived from the target run's status (Run / Open / Cancel /
     Resume / Discard) so the menu mirrors the row's own hover buttons
     while adding an explicit Run entry that re-launches the pipeline
     through the same routed `request_pipeline_run` flow as the toolbar
     Play. Hidden when `runCtxMenu` is null. -->
{#if runCtxMenu}
  <ContextMenu
    x={runCtxMenu.x}
    y={runCtxMenu.y}
    items={runCtxItems}
    onSelect={handleRunCtxSelect}
    onClose={() => { runCtxMenu = null; }}
  />
{/if}

{#if ciCtxMenu}
  <ContextMenu
    x={ciCtxMenu.x}
    y={ciCtxMenu.y}
    items={ciCtxItems}
    onSelect={handleCiCtxSelect}
    onClose={() => { ciCtxMenu = null; }}
  />
{/if}

<style>
  /* Bottom-panel root: column flex hosting the BottomPanelHeader, the
     local/CI tab strip, and the active tab body. Mirrors the column
     layout used by the other standardized bottom panels. */
  .pipe-root {
    display: flex;
    flex-direction: column;
    height: 100%;
    width: 100%;
    overflow: hidden;
    background: var(--bg-base);
  }

  /* ── Sub-tabs (Local / CI) inline in BottomPanelHeader ──────────────
     The `<Tabs variant="underline">` strip is anchored to the bottom of
     the 34px header so its underline aligns with the header's
     `border-bottom`, giving a single continuous separator across the
     panel. Transparent — bg flows from `BottomPanelHeader` (--bg-base). */
  .pipe-tabs {
    align-self: stretch;
    display: flex;
    align-items: stretch;
    margin-left: 6px;
    min-width: 0;
  }
  .pipe-tabs :global(.tabs) { height: 100%; }
  .pipe-tabs :global(.tabs-tab) {
    height: 100%;
    padding: 0 12px;
    font-size: 11px;
  }

  /* ── Shared: tab content wrapper ────────────────────────────────── */
  .tab-content {
    display: flex;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }

  /* ── Status colours ─────────────────────────────────────────────── */
  .status-success  { color: var(--status-success, #4ade80); }
  .status-failed   { color: var(--status-error,   #f87171); }
  .status-running  { color: var(--accent); }
  .status-cancelled{ color: var(--text-muted); }
  .status-pending  { color: var(--text-disabled); }

  /* ══════════════════════════════════════════════════════════════════
     LOCAL PIPELINES — two-column IntelliJ Run-panel layout
  ══════════════════════════════════════════════════════════════════ */

  .local-layout {
    display: flex;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }

  /* ── Left vertical toolbar ─────────────────────────────────────────
     Mirrors the IntelliJ Run window toolbar: fixed-width column with
     stacked square buttons and a tiny separator line for visual rhythm.
     `--bg-base` so the toolbar reads as a continuation of the panel
     background; only the right border separates it from the run list. */
  .pipe-toolbar {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    width: 36px;
    flex-shrink: 0;
    padding: 6px 0;
    background: var(--bg-base);
    border-right: 1px solid var(--border-subtle);
    /* Visible so the Run dropdown can escape the 36px toolbar column. */
    overflow: visible;
  }

  /* Run button — same square footprint as the other toolbar buttons but
     accent-coloured (it's the primary action). The contrast keeps it
     legible against the `--bg-base` toolbar lane. */
  .pipe-tb-run {
    background: var(--accent);
    color: var(--text-on-accent);
  }
  .pipe-tb-run:hover:not(:disabled) {
    background: var(--accent-hover);
    color: var(--text-on-accent);
  }
  .pipe-tb-run:disabled {
    background: var(--bg-overlay);
    color: var(--text-disabled);
  }

  .pipe-tb-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .pipe-tb-btn:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text-primary);
  }
  .pipe-tb-btn:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }
  .pipe-tb-accent  { color: var(--accent); }
  .pipe-tb-success { color: var(--success); }
  .pipe-tb-danger  { color: var(--text-muted); }
  .pipe-tb-danger:hover:not(:disabled) {
    background: var(--error-subtle);
    color: var(--error);
  }
  .pipe-tb-sep {
    display: block;
    width: 16px;
    height: 1px;
    background: var(--border-subtle);
    margin: 4px 0;
  }

  /* ── Right column: filter row + run list ──────────────────────────── */
  .pipe-content {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  /* Filter row — single combobox + clear pill + run count summary.
     Sits above the run list with no extra background; the only chrome is
     the bottom hairline matching the rest of the panel. */
  .pipe-filter-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    background: var(--bg-base);
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }
  .pipe-filter-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 24px;
    padding: 0 8px;
    background: transparent;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    font-family: var(--font-ui-sans);
    font-size: 11px;
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
  }
  .pipe-filter-btn:hover,
  .pipe-filter-btn.open {
    background: var(--bg-hover);
    color: var(--text-primary);
    border-color: var(--border);
  }
  .pipe-filter-label {
    max-width: 160px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pipe-filter-clear {
    height: 22px;
    padding: 0 8px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    font-size: 11px;
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .pipe-filter-clear:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }
  .pipe-filter-count {
    margin-left: auto;
    font-family: var(--font-ui-sans);
    font-size: 10px;
    color: var(--text-muted);
  }

  /* ══════════════════════════════════════════════════════════════════
     CI / CD VIEW
  ══════════════════════════════════════════════════════════════════ */

  .ci-view {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }

  .ci-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 12px;
    height: 36px;
    border-bottom: 1px solid var(--border-subtle);
    background: var(--bg-base);
    flex-shrink: 0;
  }

  .ci-provider-badge {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 2px 8px;
    border-radius: var(--radius-lg);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.2px;
  }
  .ci-provider-github {
    background: rgba(255,255,255,0.08);
    color: var(--text-primary);
  }
  .ci-provider-gitlab {
    background: rgba(252, 109, 38, 0.12);
    color: var(--brand-gitlab);
  }


  .ci-repo-path {
    font-size: 12px;
    color: var(--text-secondary);
    font-family: var(--font-code);
  }

  .ci-no-provider {
    font-size: 12px;
    color: var(--text-muted);
    font-style: italic;
  }

  /* ── State views (empty / error / loading) ──────────────────────── */
  .ci-state-view {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    flex: 1;
    gap: 8px;
    padding: 32px 24px;
    text-align: center;
  }

  .ci-state-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
    margin: 0;
  }

  .ci-state-hint {
    font-size: 12px;
    color: var(--text-muted);
    line-height: 1.5;
    max-width: 320px;
    margin: 0;
  }

  :global(.ci-state-icon) { opacity: 0.5; }
  :global(.ci-state-warn) { opacity: 1; color: var(--status-warning, #fbbf24) !important; }
  :global(.ci-state-muted) { color: var(--text-disabled) !important; }

  /* Inline accent button used by empty-state / error-state CTAs (e.g.
     CI tab "Retry" after a failed token call). Kept narrow-scope here
     since the rest of the panel uses the toolbar / split cluster. */
  .run-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 4px 10px;
    border: 1px solid var(--accent);
    background: var(--accent-subtle);
    color: var(--accent);
    border-radius: var(--radius-md);
    cursor: pointer;
    font-size: 12px;
    font-family: var(--font-ui-sans);
    font-weight: 500;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .run-btn:hover { background: var(--accent); color: var(--text-on-accent); }

  .ci-error-text {
    font-family: var(--font-code);
    font-size: 11px;
    color: var(--status-error, #f87171);
    word-break: break-all;
  }
  .ci-pat-hint {
    background: color-mix(in srgb, var(--status-warning, #fbbf24) 8%, transparent);
    border: 1px solid color-mix(in srgb, var(--status-warning, #fbbf24) 25%, transparent);
    border-radius: var(--radius-sm);
    padding: 6px 10px;
    font-family: var(--font-ui-sans);
    color: var(--text-secondary) !important;
    font-size: 12px;
    line-height: 1.5;
    max-width: 220px;
    text-align: center;
  }
  .ci-pat-hint strong { color: var(--text-primary); }
  .ci-pat-hint code {
    font-family: var(--font-code);
    font-size: 10px;
    color: var(--accent);
  }

  /* ── CI run list ─────────────────────────────────────────────────── */
  .ci-run-list {
    flex: 1;
    overflow-y: auto;
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  /* ── CI run card ─────────────────────────────────────────────────── */
  .ci-run-card {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    border-radius: var(--radius-md);
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    cursor: pointer;
    transition: border-color var(--transition-fast), background var(--transition-fast);
    user-select: none;
  }
  .ci-run-card:hover {
    background: var(--bg-hover);
    border-color: var(--border);
  }

  /* Left column: status pill + duration */
  .ci-card-left {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 5px;
    flex-shrink: 0;
    min-width: 82px;
  }

  .ci-status-pill {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 8px;
    border-radius: 20px;
    font-size: 11px;
    font-weight: 600;
    white-space: nowrap;
  }
  .ci-status-success  { background: color-mix(in srgb, var(--success) 18%, transparent); color: var(--success); border: 1px solid color-mix(in srgb, var(--success) 35%, transparent); }
  .ci-status-failed   { background: color-mix(in srgb, var(--error) 18%, transparent);   color: var(--error);   border: 1px solid color-mix(in srgb, var(--error) 35%, transparent); }
  .ci-status-running  { background: var(--accent-subtle);   color: var(--accent); border: 1px solid var(--accent); }
  .ci-status-cancelled{ background: rgba(120,120,120,0.1);  color: var(--text-muted); border: 1px solid var(--border); }
  .ci-status-pending  { background: rgba(120,120,120,0.06); color: var(--text-disabled); border: 1px solid var(--border-subtle); }
  /* Distinct accent for runs parked behind the global concurrency cap.
     Warning-toned (amber-ish) so it reads as "waiting" rather than "idle"
     like the bare pending pill. */
  .ci-status-queued   { background: color-mix(in srgb, var(--warning) 14%, transparent); color: var(--warning); border: 1px solid color-mix(in srgb, var(--warning) 32%, transparent); }

  .ci-card-dur {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: 10px;
    font-family: var(--font-code);
    color: var(--text-muted);
    padding-left: 2px;
  }

  /* Center: name + chips */
  .ci-card-body {
    display: flex;
    flex-direction: column;
    gap: 5px;
    min-width: 0;
    flex: 1;
  }

  .ci-card-title {
    display: flex;
    align-items: baseline;
    gap: 6px;
    min-width: 0;
  }

  .ci-card-name {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .ci-card-id {
    font-size: 11px;
    font-family: var(--font-code);
    color: var(--text-muted);
    white-space: nowrap;
    flex-shrink: 0;
  }

  .ci-card-chips {
    display: flex;
    align-items: center;
    gap: 5px;
    flex-wrap: wrap;
  }

  .ci-chip {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: 10px;
    padding: 1px 6px;
    border-radius: var(--radius-sm);
    white-space: nowrap;
  }

  .ci-chip-branch {
    font-family: var(--font-code);
    background: var(--accent-subtle);
    color: var(--accent);
  }

  .ci-chip-sha {
    font-family: var(--font-code);
    background: var(--bg-overlay);
    color: var(--text-muted);
    border: 1px solid var(--border-subtle);
  }

  .ci-chip-time {
    background: transparent;
    color: var(--text-muted);
  }

  /* Per-run pipeline-definition chip — rendered as a shared <Badge
     variant="tone" tone="neutral">. The emoji-style icon is wrapped in
     this small inline-block so it doesn't fight the badge's font sizing. */
  .ci-chip-def-icon {
    font-size: 11px;
    line-height: 1;
    margin-right: 2px;
  }

  /* Right: action buttons */
  .ci-card-actions {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
  }

  .ci-action-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    border: none;
    background: transparent;
    color: var(--text-muted);
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: color var(--transition-fast), background var(--transition-fast);
    text-decoration: none;
    flex-shrink: 0;
    border: 1px solid transparent;
  }
  .ci-action-btn:hover { background: var(--bg-hover); color: var(--text-primary); border-color: var(--border-subtle); }
  .ci-action-btn:disabled { opacity: 0.35; cursor: default; }
  .ci-action-danger:hover {
    color: var(--status-error, #f87171) !important;
    border-color: var(--status-error, #f87171);
  }
  .ci-action-active {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    color: var(--accent);
    border-color: color-mix(in srgb, var(--accent) 35%, transparent);
  }

  /* Local-run detail modal markup moved to the shared
     `<PipelineRunDetailModal />`. The legacy `.lp-*` styles previously
     defined here have been removed along with the inline pseudo-modal. */

</style>
