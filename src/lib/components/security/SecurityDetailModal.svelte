<script lang="ts">
  /**
   * Detail modal opened from a counter card click.
   *
   *   • Header: title + risk score badge (if available) + "Open in provider"
   *   • Body:   tabbed list — `All | Critical | High | Medium | Low | Info | Unknown`.
   *             Tabs whose count is zero are rendered disabled (per the
   *             phase-4 spec) so the user gets a stable shape regardless
   *             of which severity they entered through.
   *
   * Findings are fetched via `securityStore.loadFindings(tabId)` which
   * honours the store's current filters (driven by the panel's filter bar).
   * The modal is a pure view over the same filtered list — narrowing in
   * the panel narrows the modal's tab counts and rows automatically.
   */
  import { tick } from 'svelte';
  import { ShieldAlert, ExternalLink, Loader2, AlertCircle } from 'lucide-svelte';
  import { openUrl } from '@tauri-apps/plugin-opener';

  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import Tabs, { type TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import SecurityFindingRow from './SecurityFindingRow.svelte';
  import SecurityFindingDetailModal from './SecurityFindingDetailModal.svelte';
  import { SEVERITY_META } from './severity-meta';

  import { securityStore } from '$lib/stores/security.svelte';
  import {
    SEVERITY_ORDER,
    type Severity,
    type SecurityFinding,
    type SecuritySummary,
    type SeverityCounts,
  } from '$lib/types/security';

  type TabKey = 'all' | Severity;

  interface Props {
    tabId:           string;
    summary:         SecuritySummary;
    initialSeverity: Severity;
    onClose:         () => void;
  }

  let { tabId, summary, initialSeverity, onClose }: Props = $props();

  /** Currently-open per-finding detail modal (null = none). */
  let selectedFinding = $state<SecurityFinding | null>(null);

  // svelte-ignore state_referenced_locally
  let activeTab = $state<TabKey>(initialSeverity);
  /** Transient "switching tabs" flag — true between the click and the next
   *  DOM tick so the progress bar can paint while Svelte rebuilds the row
   *  list. Without this, large severity tabs (`All` with 100+ findings)
   *  feel frozen for ~200ms on click because the heavy diff is synchronous. */
  let switching = $state(false);

  async function selectTab(id: TabKey) {
    if (id === activeTab) return;
    switching = true;
    // Yield once so the progress bar paints, THEN swap the active tab so
    // the row diff happens in the next frame.
    await tick();
    activeTab = id;
    // Reset scroll on tab change — each severity has its own list.
    if (listEl) listEl.scrollTop = 0;
    scrollTop = 0;
    await tick();
    switching = false;
  }

  // ── Virtualization ──────────────────────────────────────────────────────
  // Each row is a fixed `ROW_HEIGHT` (enforced in SecurityFindingRow.svelte).
  // We render only the rows that intersect the viewport plus a small buffer
  // — turns 300+ DOM nodes into ~20, which is what was making tab switches
  // feel sluggish even after the progress-bar tick yield.
  const ROW_HEIGHT = 64;
  const OVERSCAN   = 6;

  let listEl: HTMLElement | undefined = $state();
  let scrollTop      = $state(0);
  let viewportHeight = $state(0);

  $effect(() => {
    if (!listEl) return;
    const el = listEl;
    const onScroll = () => { scrollTop = el.scrollTop; };
    el.addEventListener('scroll', onScroll, { passive: true });
    const ro = new ResizeObserver(() => { viewportHeight = el.clientHeight; });
    ro.observe(el);
    viewportHeight = el.clientHeight;
    return () => {
      el.removeEventListener('scroll', onScroll);
      ro.disconnect();
    };
  });

  // ── Data ────────────────────────────────────────────────────────────────
  // Use the store's findings cache. The modal triggers a refresh once on
  // open if the cache is empty for this tab. Subsequent opens reuse the
  // existing list; the user can hit the panel's Refresh to invalidate.
  $effect(() => {
    const id = tabId;
    if (!id) return;
    if (securityStore.findings.length === 0
        && !securityStore.loading
        && !securityStore.findingsLoading) {
      securityStore.loadFindings(id);
    }
  });

  const loading       = $derived(securityStore.loading || securityStore.findingsLoading);
  /** Combined "show the progress bar" signal — covers fetch in flight and
   *  the brief tab-switch DOM thrash. */
  const showProgress  = $derived(loading || switching);
  const error   = $derived(securityStore.error);
  const all     = $derived(securityStore.findings);

  // Tab counts: scope-aware so they always describe the rows the modal is
  // *currently* showing. In the active scope this matches the panel grid
  // (filteredCounts is active-only by design); in the closed scope we
  // count whatever closed findings the backend returned.
  const scope  = $derived(securityStore.stateScope);
  const counts = $derived.by<SeverityCounts>(() => {
    if (scope === 'active') return securityStore.filteredCounts();
    const out: SeverityCounts = {
      critical: 0, high: 0, medium: 0, low: 0, info: 0, unknown: 0,
    };
    for (const f of all) {
      if (f.state === 'detected' || f.state === 'confirmed') continue;
      out[f.severity]++;
    }
    return out;
  });
  const totalCount = $derived(
    counts.critical + counts.high + counts.medium + counts.low + counts.info + counts.unknown,
  );

  async function setScope(next: 'active' | 'closed') {
    if (next === scope) return;
    securityStore.setStateScope(next);
    // Reset scroll + force the progress bar via the same `switching` flag
    // we already use for tab swaps.
    switching = true;
    if (listEl) listEl.scrollTop = 0;
    scrollTop = 0;
    // Refetch with the new state filter; backend round-trips are cheap.
    if (tabId) await securityStore.loadFindings(tabId);
    switching = false;
  }

  // Severity rank for the sort. Lower index ⇒ higher priority.
  const SEV_RANK: Record<Severity, number> = {
    critical: 0, high: 1, medium: 2, low: 3, info: 4, unknown: 5,
  };

  /** Findings filtered by the active tab + sorted severity desc → age desc. */
  const visible = $derived.by<SecurityFinding[]>(() => {
    const arr = activeTab === 'all'
      ? [...all]
      : all.filter(f => f.severity === activeTab);
    arr.sort((a, b) => {
      const r = SEV_RANK[a.severity] - SEV_RANK[b.severity];
      if (r !== 0) return r;
      return b.age_days - a.age_days;
    });
    return arr;
  });

  /** Virtualization window — recomputed on scroll/resize/list change. */
  const startIdx = $derived(
    Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN),
  );
  const endIdx = $derived(
    Math.min(
      visible.length,
      Math.ceil((scrollTop + viewportHeight) / ROW_HEIGHT) + OVERSCAN,
    ),
  );
  const offsetY     = $derived(startIdx * ROW_HEIGHT);
  const totalHeight = $derived(visible.length * ROW_HEIGHT);
  const slice       = $derived(visible.slice(startIdx, endIdx));

  const tabItems = $derived<TabItem[]>([
    {
      id:       'all',
      label:    'All',
      badge:    totalCount > 0 ? totalCount : undefined,
      disabled: totalCount === 0,
    },
    ...SEVERITY_ORDER.map(sev => ({
      id:       sev,
      label:    SEVERITY_META[sev].label,
      badge:    counts[sev] > 0 ? counts[sev] : undefined,
      disabled: counts[sev] === 0,
    })),
  ]);

  const providerLabel = $derived(
    summary.provider_kind === 'gitlab' ? 'GitLab'
      : summary.provider_kind === 'github' ? 'GitHub'
      : 'Provider',
  );

  function openExternal() {
    if (summary.web_url) openUrl(summary.web_url).catch(() => {});
  }
</script>

<Modal {onClose} width="min(94vw, 1080px)" height="82vh" padBody={false} ariaLabel="Security findings">
  {#snippet header()}
    <ModalHeader {onClose}>
      <ShieldAlert size={14} />
      <span class="modal-title">Security findings</span>

      {#if summary.risk_score}
        {@const rs = summary.risk_score}
        <span
          class="risk-pill"
          use:tooltip={`Risk score: ${rs.value.toFixed(1)} (${rs.label})`}
        >{rs.label}</span>
      {/if}

      {#snippet actions()}
        {#if summary.web_url}
          <button class="hdr-btn" onclick={openExternal} use:tooltip={`Open in ${providerLabel}`}>
            <ExternalLink size={13} />
            Open in {providerLabel}
          </button>
        {/if}
      {/snippet}
    </ModalHeader>
  {/snippet}

  <div class="modal-body">
    <div class="tabs-row">
      <Tabs
        items={tabItems}
        value={activeTab}
        variant="underline"
        size="sm"
        onSelect={(id) => selectTab(id as TabKey)}
        ariaLabel="Severity filter"
      />

      <!-- State scope segmented control. The dashboard panel always shows
           ACTIVE findings; this toggle lets the user temporarily flip the
           modal to "Closed" (Resolved / Dismissed) without affecting the
           panel grid. Persisted via the security store. -->
      <div
        class="scope-seg"
        role="radiogroup"
        aria-label="Finding state scope"
      >
        <button
          type="button"
          class="scope-btn"
          class:scope-active={scope === 'active'}
          aria-pressed={scope === 'active'}
          onclick={() => setScope('active')}
          use:tooltip={'Show open findings (Detected + Confirmed)'}
        >Active</button>
        <button
          type="button"
          class="scope-btn"
          class:scope-active={scope === 'closed'}
          aria-pressed={scope === 'closed'}
          onclick={() => setScope('closed')}
          use:tooltip={'Show managed findings (Resolved + Dismissed)'}
        >Closed</button>
      </div>
    </div>

    <div class="list-region" bind:this={listEl}>
      <!-- Indeterminate progress bar that paints during fetches AND during
           the tab-switch DOM thrash. Sits flush at the top of the scrollable
           area; doesn't push content down. -->
      {#if showProgress}
        <div class="progress-bar" aria-hidden="true">
          <div class="progress-bar-track"></div>
        </div>
      {/if}

      {#if loading && all.length === 0}
        <div class="state">
          <Loader2 size={20} class="spin" />
          <span class="state-hint">Loading findings…</span>
        </div>

      {:else if error && all.length === 0}
        <div class="state">
          <AlertCircle size={22} class="state-icon-warn" />
          <p class="state-title">Failed to load findings</p>
          <p class="state-hint err-msg">{error}</p>
        </div>

      {:else if visible.length === 0}
        <div class="state">
          <ShieldAlert size={22} class="state-icon-muted" />
          <p class="state-hint">
            {#if activeTab === 'all'}
              No findings to show.
            {:else}
              No <strong>{SEVERITY_META[activeTab].label}</strong> findings.
            {/if}
          </p>
        </div>

      {:else}
        <!-- Virtualized list. The outer spacer reserves the full scrollable
             height (rowCount × ROW_HEIGHT); the inner translated layer holds
             only the rows that intersect the viewport plus a buffer. -->
        <div class="virt-spacer" style:height="{totalHeight}px" role="list">
          <div class="virt-window" style:transform="translateY({offsetY}px)">
            {#each slice as finding (finding.id)}
              <SecurityFindingRow {finding} onSelect={(f) => (selectedFinding = f)} />
            {/each}
          </div>
        </div>
      {/if}
    </div>

    <div class="footer-bar">
      <span class="count">
        Showing <strong>{visible.length}</strong>
        {#if activeTab !== 'all'}
          of {counts[activeTab]} {SEVERITY_META[activeTab].label.toLowerCase()}
        {:else}
          of {totalCount}
        {/if}
        finding{visible.length === 1 ? '' : 's'}
      </span>
      {#if summary.truncated}
        <span class="trunc">Capped at {summary.findings_seen} — refine filters to narrow.</span>
      {/if}
    </div>
  </div>
</Modal>

{#if selectedFinding}
  <SecurityFindingDetailModal
    finding={selectedFinding}
    onClose={() => (selectedFinding = null)}
  />
{/if}

<style>
  .modal-body {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    background: var(--bg-base);
  }

  .tabs-row {
    flex-shrink: 0;
    padding: 0 12px;
    background: var(--bg-base);
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  /* Active / Closed segmented control */
  .scope-seg {
    display: inline-flex;
    align-items: stretch;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    overflow: hidden;
    flex-shrink: 0;
    margin: 6px 0;
  }
  .scope-btn {
    padding: 4px 10px;
    background: transparent;
    border: none;
    color: var(--text-secondary);
    font-size: 11px;
    font-family: var(--font-ui-sans);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .scope-btn + .scope-btn { border-left: 1px solid var(--border); }
  .scope-btn:hover { background: var(--bg-hover); color: var(--text-primary); }
  .scope-active {
    background: var(--accent-subtle);
    color: var(--accent);
    font-weight: 600;
  }

  .list-region {
    flex: 1;
    min-height: 0;
    overflow: auto;
    position: relative;
  }

  /* Indeterminate progress bar — sticky at the top of the scrollable list
     so it stays visible even when the user has scrolled. Animation runs
     for both fetch and tab-switch states. */
  .progress-bar {
    position: sticky;
    top: 0;
    left: 0;
    right: 0;
    height: 2px;
    background: var(--bg-elevated);
    overflow: hidden;
    z-index: 2;
    pointer-events: none;
  }
  .progress-bar-track {
    position: absolute;
    inset: 0;
    background: linear-gradient(
      90deg,
      transparent 0%,
      var(--accent) 50%,
      transparent 100%
    );
    animation: sec-progress-slide 1.1s ease-in-out infinite;
  }
  @keyframes sec-progress-slide {
    0%   { transform: translateX(-100%); }
    100% { transform: translateX(100%); }
  }

  /* ── Virtualized list ─────────────────────────────────────────────────── */
  .virt-spacer {
    position: relative;
    width: 100%;
  }
  .virt-window {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    will-change: transform;
  }

  .footer-bar {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 8px 14px;
    border-top: 1px solid var(--border-subtle);
    background: var(--bg-elevated);
    font-size: 11px;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
  }
  .count strong { color: var(--text-secondary); font-weight: 600; }
  .trunc { color: var(--warning); }

  /* ── States ──────────────────────────────────────────────────────────── */
  .state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 48px 24px;
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
    max-width: 320px;
  }
  .err-msg {
    color: var(--error);
    font-family: var(--font-code);
    font-size: 11px;
    word-break: break-word;
  }
  :global(.state .state-icon-warn)  { color: var(--warning); }
  :global(.state .state-icon-muted) { color: var(--text-muted); opacity: 0.6; }

  /* ── Header bits ─────────────────────────────────────────────────────── */
  .risk-pill {
    margin-left: 4px;
    padding: 2px 8px;
    border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--severity-high) 14%, transparent);
    color: var(--severity-high);
    border: 1px solid color-mix(in srgb, var(--severity-high) 32%, transparent);
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.4px;
    font-family: var(--font-ui-sans);
  }

  .hdr-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 4px 10px;
    background: transparent;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    font-size: 11px;
    font-family: var(--font-ui-sans);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .hdr-btn:hover { background: var(--bg-hover); color: var(--text-primary); }
</style>
