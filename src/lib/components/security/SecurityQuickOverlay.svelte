<script lang="ts">
  /**
   * Security quick-overlay — Phase 2.5.
   *
   * Floating popup anchored above the StatusBar (same pattern as
   * JobsOverlay / NotificationsOverlay). Gives the user a one-click
   * read of the active repo's security posture without forcing them
   * to open the sidebar panel:
   *
   *   - compact risk-score pill (full SVG gauge lands in Phase 3 once
   *     the shared `<GaugeChart>` widget exists)
   *   - 6 severity rows (Critical..Unknown) with count + median age
   *   - footer: Open dashboard (sidebar) / Open in provider (web)
   */
  import { ShieldAlert, ExternalLink, LayoutDashboard, RefreshCw, Loader2 } from 'lucide-svelte';
  import { openUrl } from '@tauri-apps/plugin-opener';

  import { uiStore }       from '$lib/stores/ui.svelte';
  import { tabsStore }     from '$lib/stores/tabs.svelte';
  import { securityStore } from '$lib/stores/security.svelte';
  import { SEVERITY_ORDER, totalCount } from '$lib/types/security';
  import { SEVERITY_META, formatMedianAge } from './severity-meta';
  import RiskScoreGauge from './RiskScoreGauge.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  const tabId   = $derived(tabsStore.activeTabId);
  const summary = $derived(securityStore.summary);
  const loading = $derived(securityStore.loading);
  const error   = $derived(securityStore.error);

  const isEmpty = $derived(summary != null && totalCount(summary.counts) === 0);

  const providerLabel = $derived.by(() => {
    if (summary?.provider_kind === 'gitlab') return 'GitLab';
    if (summary?.provider_kind === 'github') return 'GitHub';
    return 'Provider';
  });

  // Auto-load on first open if the store snapshot is for a different tab,
  // or hasn't been loaded yet. Refresh button gives manual override.
  $effect(() => {
    const id = tabId;
    if (!id) return;
    if (securityStore.snapshotTabId !== id) {
      securityStore.loadSummary(id);
    }
  });

  function close() { uiStore.setSecurityOverlayOpen(false); }

  function openDashboard() {
    uiStore.setActiveSidebarSection('security');
    close();
  }

  function openExternal() {
    if (summary?.web_url) openUrl(summary.web_url).catch(() => {});
  }

  function refresh() {
    if (tabId && !loading) securityStore.loadSummary(tabId);
  }

</script>

<button type="button" aria-label="Close overlay" class="overlay-backdrop" onclick={close}></button>

<div class="overlay-panel security-overlay" role="dialog" aria-label="Security summary">
  <div class="overlay-header">
    <span class="overlay-title">
      <ShieldAlert size={13} />
      Security
    </span>
    <div class="header-actions">
      <button class="hdr-btn" onclick={refresh} disabled={loading || !tabId} use:tooltip={'Refresh'}>
        <RefreshCw size={12} class={loading ? 'spin' : ''} />
      </button>
      <button class="mac-close-btn" onclick={close} use:tooltip={'Close'} aria-label="Close"></button>
    </div>
  </div>

  {#if loading && !summary}
    <div class="overlay-state">
      <Loader2 size={18} class="spin" />
      <span>Loading…</span>
    </div>

  {:else if error && !summary}
    <div class="overlay-state err">
      <span class="err-msg" use:tooltip={error}>{error}</span>
      <button class="text-btn" onclick={refresh}>Retry</button>
    </div>

  {:else if !summary}
    <div class="overlay-state">
      <span>No data yet.</span>
    </div>

  {:else}
    {#if summary.risk_score}
      <div class="risk-gauge-wrap">
        <span class="risk-caption">Risk score</span>
        <RiskScoreGauge score={summary.risk_score} size="sm" />
      </div>
    {/if}

    {#if isEmpty}
      <div class="overlay-state ok">
        <ShieldAlert size={16} />
        <span>No findings — repo is clean.</span>
      </div>
    {:else}
      <ul class="sev-list">
        {#each SEVERITY_ORDER as sev (sev)}
          {@const meta  = SEVERITY_META[sev]}
          {@const count = summary.counts[sev]}
          {@const age   = summary.median_age_days[sev]}
          {@const empty = count === 0}
          <li
            class="sev-row"
            class:empty
            style:--sev-color={meta.color}
            style:--sev-bg={meta.bgColor}
          >
            <span class="sev-dot"></span>
            <span class="sev-label">{meta.label}</span>
            <span class="sev-count">{count}</span>
            <span class="sev-age">{empty ? '—' : formatMedianAge(age)}</span>
          </li>
        {/each}
      </ul>
    {/if}

    {#if summary.truncated}
      <div class="trunc-note">
        Showing {summary.findings_seen} findings (capped).
      </div>
    {/if}
  {/if}

  <div class="overlay-footer">
    <button class="ft-btn" onclick={openDashboard} use:tooltip={'Open Security panel'}>
      <LayoutDashboard size={12} />
      <span>Dashboard</span>
    </button>
    {#if summary?.web_url}
      <button class="ft-btn" onclick={openExternal} use:tooltip={`Open in ${providerLabel}`}>
        <ExternalLink size={12} />
        <span>Open in {providerLabel}</span>
      </button>
    {/if}
  </div>
</div>

<style>
  .security-overlay {
    /* Anchor to the LEFT of the footer instead of inheriting the default
       `right: 12px` from `.overlay-panel` — the security trigger now sits
       on the left side of the StatusBar, so the overlay should slide up
       from there to stay visually connected to its origin. */
    right: auto;
    left: 12px;

    width: 320px;
    max-height: 460px;
    background: var(--bg-base);
    border-color: var(--border);
    box-shadow: 0 8px 32px rgba(0,0,0,0.7);
  }

  .overlay-title {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--text-primary);
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .hdr-btn {
    display: flex;
    align-items: center;
    height: 22px;
    padding: 0 6px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .hdr-btn:hover:not(:disabled) { background: var(--bg-elevated); color: var(--text-primary); }
  .hdr-btn:disabled { opacity: 0.45; cursor: not-allowed; }

  /* Compact risk-score gauge — sits above the per-severity list. Uses
     the shared `<RiskScoreGauge size="sm" />` so the visual matches the
     full panel exactly. */
  .risk-gauge-wrap {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    margin: 8px 10px 4px;
    padding: 8px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
  }
  .risk-caption {
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
  }

  /* Severity list */
  .sev-list {
    list-style: none;
    margin: 4px 0 4px;
    padding: 0 6px;
    display: flex;
    flex-direction: column;
    gap: 1px;
    overflow-y: auto;
    flex: 1 1 auto;
    min-height: 0;
  }

  .sev-row {
    display: grid;
    grid-template-columns: 10px 1fr auto auto;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans);
    font-size: 12px;
    color: var(--text-primary);
    transition: background var(--transition-fast);
  }
  .sev-row:hover:not(.empty) { background: var(--sev-bg); }
  .sev-row.empty { color: var(--text-disabled); }

  .sev-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--sev-color);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--sev-color) 22%, transparent);
  }
  .sev-row.empty .sev-dot { opacity: 0.35; box-shadow: none; }

  .sev-label {
    font-weight: 500;
    color: var(--sev-color);
  }
  .sev-row.empty .sev-label { color: var(--text-disabled); }

  .sev-count {
    font-size: 13px;
    font-weight: 700;
    color: var(--text-primary);
    font-variant-numeric: tabular-nums;
    min-width: 24px;
    text-align: right;
  }
  .sev-row.empty .sev-count { color: var(--text-disabled); }

  .sev-age {
    font-size: 11px;
    color: var(--text-muted);
    min-width: 56px;
    text-align: right;
  }

  /* States */
  .overlay-state {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 24px 16px;
    color: var(--text-muted);
    font-size: 12px;
    font-family: var(--font-ui-sans);
  }
  .overlay-state.err   { color: var(--error); flex-direction: column; }
  .overlay-state.ok    { color: var(--success); }

  .err-msg {
    font-family: var(--font-code);
    font-size: 11px;
    word-break: break-word;
    max-width: 280px;
    text-align: center;
  }

  .text-btn {
    padding: 3px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-secondary);
    font-size: 11px;
    font-family: var(--font-ui-sans);
    cursor: pointer;
  }
  .text-btn:hover { background: var(--bg-hover); }

  .trunc-note {
    margin: 0 10px 6px;
    padding: 5px 8px;
    font-size: 10px;
    color: var(--warning);
    background: var(--warning-subtle);
    border: 1px solid color-mix(in srgb, var(--warning) 30%, transparent);
    border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans);
    text-align: center;
  }

  /* Footer */
  .overlay-footer {
    display: flex;
    gap: 4px;
    padding: 6px;
    border-top: 1px solid var(--border);
    flex-shrink: 0;
  }
  .ft-btn {
    flex: 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    padding: 6px 8px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    font-family: var(--font-ui-sans);
    font-size: 11px;
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
  }
  .ft-btn:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
    border-color: var(--border);
  }
</style>
