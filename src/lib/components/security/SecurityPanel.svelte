<script lang="ts">
  /**
   * Security dashboard sidebar panel.
   *
   * Renders the headline summary returned by `fetch_security_summary`:
   *   - search + severity + report-type filter bar (drives the rest)
   *   - 6 severity counter cards (Critical / High / Medium / Low / Info / Unknown)
   *   - risk-score gauge + vulnerabilities-over-time chart
   *   - click-through detail modal with severity tabs
   *   - graceful states for loading / error / no token / no findings
   *
   * GitHub provider is Phase 6.
   */
  import { ShieldAlert, RefreshCw, Loader2, AlertCircle, ExternalLink, FileDown, FileCode2, FileText } from 'lucide-svelte';
  import { openUrl } from '@tauri-apps/plugin-opener';

  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import Dropdown from '$lib/components/shared/ui/Dropdown.svelte';
  import FilePickerModal from '$lib/components/shared/FilePickerModal.svelte';
  import SeverityCounterGrid from './SeverityCounterGrid.svelte';
  import SecurityNoTokenState from './SecurityNoTokenState.svelte';
  import SecurityEmptyState from './SecurityEmptyState.svelte';
  import SecurityFilterBar from './SecurityFilterBar.svelte';
  import RiskScoreGauge from './RiskScoreGauge.svelte';
  import VulnTimeSeriesChart from './VulnTimeSeriesChart.svelte';
  import SecurityDetailModal from './SecurityDetailModal.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { securityStore } from '$lib/stores/security.svelte';
  import { notificationsStore } from '$lib/stores/notifications.svelte';
  import { exportSecurityReport, captureSecurityExportTheme } from '$lib/ipc/security';
  import { totalCount, type Severity } from '$lib/types/security';

  const tabId        = $derived(tabsStore.activeTabId);
  const summary      = $derived(securityStore.summary);
  const loading      = $derived(securityStore.loading);
  const error        = $derived(securityStore.error);
  const isEmpty      = $derived(summary != null && totalCount(summary.counts) === 0);
  /** Tri-state: 'unknown' while the AppShell probe is in flight, 'supported'
   *  when the provider answered yes, 'unsupported' for everything else
   *  (no remote, missing token, feature off, GitLab Free without Ultimate). */
  const supportState = $derived(securityStore.providerSupportState(tabId));

  /** Counter grid + chart read from the filtered findings list, not the
   *  raw `summary.counts`. Falls back to the summary's counts before the
   *  findings list has loaded so the cards aren't blank during the
   *  initial fetch. */
  const counts = $derived(securityStore.filteredCounts());
  /** Severity filter — empty means "all" (no narrowing). When non-empty,
   *  the time-series chart should only draw the selected lines. */
  const severityFilter = $derived(securityStore.filters.severities);

  // Provider label for the no-token state. Provider kind comes from a
  // successful summary fetch; before that we fall back to a neutral label.
  const providerLabel = $derived.by(() => {
    if (summary?.provider_kind === 'gitlab') return 'GitLab';
    if (summary?.provider_kind === 'github') return 'GitHub';
    return 'Provider';
  });

  // Detect "no token" / "unauthorized" as a graceful state instead of an
  // error so the user gets the same affordance as the CI panel.
  const isAuthError = $derived.by(() => {
    if (!error) return false;
    const m = error.toLowerCase();
    return m.includes('401') || m.includes('403')
        || m.includes('unauthorized') || m.includes('no token')
        || m.includes('missing token');
  });

  // Load on first render and reload when the active tab switches while
  // the panel is open. Svelte 5 `$effect` fires on mount, so we don't
  // need a separate onMount path. Findings are pulled right after the
  // summary lands so the filter bar (Phase 5) can drive counters + chart
  // even before the user opens the detail modal.
  //
  // The probe is awaited first — `probeSupport` is idempotent (cached per
  // tab) so it's a no-op when AppShell has already populated the result.
  // Skipping the summary fetch on `unsupported` avoids the "Failed to load"
  // error toast that the backend would otherwise surface for repos without
  // a security dashboard.
  $effect(() => {
    const id = tabId;
    if (!id) return;
    (async () => {
      const ok = await securityStore.probeSupport(id);
      if (!ok) return;
      await securityStore.loadSummary(id);
      if (securityStore.summary && totalCount(securityStore.summary.counts) > 0) {
        await securityStore.loadFindings(id);
      }
    })();
  });

  async function refresh() {
    if (!tabId || loading) return;
    // Drop the cached probe so an explicit refresh re-checks the provider
    // (useful after the user signs in / changes token).
    securityStore.invalidateSupport(tabId);
    const ok = await securityStore.probeSupport(tabId);
    if (!ok) return;
    await securityStore.loadSummary(tabId);
    if (securityStore.summary && totalCount(securityStore.summary.counts) > 0) {
      await securityStore.loadFindings(tabId);
    }
  }

  function openExternal() {
    if (summary?.web_url) openUrl(summary.web_url).catch(() => {});
  }

  // Detail modal — opened from a counter card. The summary it needs is the
  // one already rendered above, so we keep a reference to it as the modal's
  // initial severity tab.
  let detailSeverity = $state<Severity | null>(null);

  function onCardClick(severity: Severity) {
    // Empty severities are a no-op (the card already disables itself
    // visually) — opening a modal with a forced-disabled tab and zero
    // findings would just be a dead end. We check against the *filtered*
    // counts so a severity hidden by the active filter doesn't open an
    // empty modal tab.
    if (!summary || totalCount(summary.counts) === 0) return;
    if (counts[severity] === 0) return;
    detailSeverity = severity;
  }

  function closeDetail() {
    detailSeverity = null;
  }

  // ── Export ──────────────────────────────────────────────────────────────
  // Header dropdown → in-app `FilePickerModal` (mode='save') → backend
  // `export_security_report` command. The success/failure toast is emitted
  // by the backend on `arbor://job-done`, so we only surface IPC errors here.
  let exportFormat = $state<'html' | 'csv' | null>(null);
  let showPicker   = $state(false);

  const exportExtensions = $derived(exportFormat === 'csv' ? ['csv'] : ['html', 'htm']);
  const exportFilename   = $derived.by(() => {
    const base = tabsStore.activeTab?.name ?? 'repo';
    return exportFormat === 'csv' ? `${base}-security.csv` : `${base}-security.html`;
  });

  function startExport(fmt: 'html' | 'csv') {
    exportFormat = fmt;
    showPicker = true;
  }

  async function doExport(path: string) {
    if (!tabId || !exportFormat) return;
    showPicker = false;
    const fmt = exportFormat;
    exportFormat = null;
    // Snapshot the current theme tokens only for the HTML report — the CSV
    // export ignores them backend-side.
    const theme = fmt === 'html' ? captureSecurityExportTheme() : undefined;
    try {
      await exportSecurityReport(tabId, path, fmt, theme);
    } catch (err) {
      notificationsStore.add('Security export failed', String(err), 'error');
    }
  }

  function cancelExport() {
    showPicker = false;
    exportFormat = null;
  }
</script>

<PanelShell title="Security">
  {#snippet icon()}<ShieldAlert size={14} />{/snippet}
  {#snippet actions()}
    {#if summary && !isEmpty}
      <Dropdown
        position="fixed"
        direction="down"
        width="180px"
        items={[
          { kind: 'item', id: 'html', label: 'HTML report',  icon: FileCode2, onclick: () => startExport('html') },
          { kind: 'item', id: 'csv',  label: 'CSV findings', icon: FileText,  onclick: () => startExport('csv')  },
        ]}
      >
        {#snippet trigger({ toggle, open })}
          <button class="ps-btn" class:active={open} onclick={toggle} use:tooltip={'Export report'}>
            <FileDown size={13} />
          </button>
        {/snippet}
      </Dropdown>
    {/if}
    {#if summary?.web_url}
      <button class="ps-btn" onclick={openExternal} use:tooltip={`Open in ${providerLabel}`}>
        <ExternalLink size={13} />
      </button>
    {/if}
    <button class="ps-btn" onclick={refresh} disabled={loading || !tabId} use:tooltip={'Refresh'}>
      <RefreshCw size={13} class={loading ? 'spin' : ''} />
    </button>
  {/snippet}

  {#if !tabId}
    <div class="center-state">
      <ShieldAlert size={26} class="state-icon-muted" />
      <p class="state-hint">Open a repository to view its security posture.</p>
    </div>

  {:else if supportState === 'unknown'}
    <div class="center-state">
      <Loader2 size={20} class="spin" />
      <p class="state-hint">Checking provider…</p>
    </div>

  {:else if supportState === 'unsupported'}
    <div class="center-state">
      <ShieldAlert size={26} class="state-icon-muted" />
      <p class="state-title">Vulnerability dashboard not available</p>
      <p class="state-hint">
        The active repository's remote provider doesn't expose a security
        dashboard for this account — typically because there's no GitHub /
        GitLab remote, no token is stored, or the project's plan/settings
        don't include security scanning.
      </p>
      <button class="text-btn" onclick={refresh}>Re-check</button>
    </div>

  {:else if loading && !summary}
    <div class="center-state">
      <Loader2 size={20} class="spin" />
      <p class="state-hint">Loading security summary…</p>
    </div>

  {:else if error && isAuthError}
    <SecurityNoTokenState providerLabel={providerLabel} />

  {:else if error && !summary}
    <div class="center-state">
      <AlertCircle size={26} class="state-icon-warn" />
      <p class="state-title">Failed to load</p>
      <p class="state-hint err-msg">{error}</p>
      <button class="text-btn" onclick={refresh}>Retry</button>
    </div>

  {:else if !summary}
    <div class="center-state">
      <ShieldAlert size={26} class="state-icon-muted" />
      <p class="state-hint">No data yet — open a repo to load findings.</p>
    </div>

  {:else if isEmpty}
    <SecurityEmptyState />

  {:else}
    {#if tabId}
      <SecurityFilterBar tabId={tabId} />
    {/if}

    <SeverityCounterGrid
      counts={counts}
      medians={summary.median_age_days}
      onSelect={onCardClick}
    />

    {#if summary.risk_score || summary.time_series}
      <div class="dashboard-container">
        <div class="dashboard-row">
          {#if summary.risk_score}
            <section class="dash-cell gauge-cell">
              <h4 class="cell-title">Risk score</h4>
              <RiskScoreGauge score={summary.risk_score} size="md" />
            </section>
          {/if}
          {#if summary.time_series}
            <section class="dash-cell chart-cell">
              <VulnTimeSeriesChart
                timeSeries={summary.time_series}
                height={220}
                severityFilter={severityFilter}
              />
            </section>
          {/if}
        </div>
      </div>
    {/if}

    {#if summary.truncated}
      <div class="trunc-note">
        Showing the first {summary.findings_seen} findings — refine filters to narrow.
      </div>
    {/if}

    {#if loading}
      <div class="refresh-note">
        <RefreshCw size={11} class="spin" /> Refreshing…
      </div>
    {/if}
  {/if}
</PanelShell>

{#if summary && detailSeverity && tabId}
  <SecurityDetailModal
    tabId={tabId}
    summary={summary}
    initialSeverity={detailSeverity}
    onClose={closeDetail}
  />
{/if}

{#if showPicker && exportFormat}
  <FilePickerModal
    mode="save"
    title={exportFormat === 'csv' ? 'Export Security CSV' : 'Export Security Report'}
    extensions={exportExtensions}
    initialFilename={exportFilename}
    onConfirm={doExport}
    onCancel={cancelExport}
  />
{/if}

<style>
  .center-state {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 32px 24px;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    text-align: center;
  }
  .state-title {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
  }
  .state-hint {
    margin: 0;
    font-size: 12px;
    line-height: 1.5;
    max-width: 240px;
  }
  .err-msg {
    color: var(--error);
    font-family: var(--font-code);
    font-size: 11px;
    word-break: break-word;
  }
  :global(.center-state .state-icon-warn)  { color: var(--warning); }
  :global(.center-state .state-icon-muted) { color: var(--text-muted); opacity: 0.6; }

  .text-btn {
    padding: 4px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-secondary);
    font-size: 11px;
    font-family: var(--font-ui-sans);
    cursor: pointer;
  }
  .text-btn:hover { background: var(--bg-hover); }

  /* `container-type: inline-size` opts the wrapper into container queries
     so `.dashboard-row` collapses based on the *panel* width rather than
     the viewport — the sidebar is narrow even on wide windows. */
  .dashboard-container {
    container-type: inline-size;
  }
  .dashboard-row {
    display: grid;
    grid-template-columns: minmax(180px, auto) 1fr;
    gap: 16px;
    padding: 4px 12px 12px;
    align-items: start;
  }
  .dashboard-row :global(> .gauge-cell) {
    align-self: center;
  }
  .dash-cell {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  .gauge-cell {
    align-items: center;
    text-align: center;
    padding: 8px 4px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    background: var(--bg-elevated);
  }
  .cell-title {
    margin: 0;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-secondary);
    font-family: var(--font-ui-sans);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  /* Stack gauge + chart when the panel itself gets narrow. The threshold
     is roughly "panel width below which the chart's x-axis labels start
     to overlap if the gauge is also taking ~180px on the left". */
  @container (max-width: 480px) {
    .dashboard-row {
      grid-template-columns: 1fr;
    }
  }

  .trunc-note {
    padding: 8px 14px;
    margin: 0 12px 8px;
    font-size: 11px;
    color: var(--warning);
    background: var(--warning-subtle);
    border: 1px solid color-mix(in srgb, var(--warning) 35%, transparent);
    border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans);
  }

  .refresh-note {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    font-size: 10px;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    padding: 6px 0;
  }

  /* Match the active state of the export trigger to the panel-shell button
     vocabulary (the Dropdown widget exposes `open` for that). */
  :global(.ps-btn.active) { background: var(--bg-overlay); color: var(--accent); }
</style>
