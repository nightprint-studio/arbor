<script lang="ts">
  /**
   * SecurityFindingsBrowser — the body of the "security findings" window: severity tabs over a
   * virtualized list of findings, a footer with the count, and the per-finding detail modal.
   *
   * Presentational on purpose. Corvus feeds it a repository's findings from the provider
   * (`SecurityDetailModal`), Bennu feeds it the advisories OSV knows for a project's dependencies
   * (`BennuVulnerabilitiesModal`) — same window, same rows, same detail view, so a vulnerability
   * looks the same wherever it was found. Where the data comes from, how it is fetched and which
   * extra controls surround it stay with the caller (`toolbar`, `note`).
   */
  import { tick, type Snippet } from 'svelte';
  import { ShieldAlert } from 'lucide-svelte';
  import Tabs, { type TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import SecurityFindingRow from './SecurityFindingRow.svelte';
  import SecurityFindingDetailModal from './SecurityFindingDetailModal.svelte';
  import { SEVERITY_META } from './severity-meta';
  import {
    SEVERITY_ORDER,
    type Severity,
    type SecurityFinding,
    type SeverityCounts,
  } from '$lib/types/corvus/security';

  type TabKey = 'all' | Severity;

  interface Props {
    findings: SecurityFinding[];
    /** Tab counts. Defaults to counting `findings`; a caller whose list is a window onto a larger
     *  set passes the authoritative numbers. */
    counts?: SeverityCounts;
    /** A fetch is in flight: shows the progress bar, and the spinner while there is nothing yet. */
    loading?: boolean;
    /** Shown instead of the list when nothing loaded. */
    error?: string;
    /** The tab the window opens on. */
    initialTab?: TabKey;
    /** Drop the age/state columns — for findings with no lifecycle. */
    hideLifecycle?: boolean;
    /** What a row is called in the footer and the empty states. */
    noun?: string;
    /** Plural of `noun`; defaults to adding an "s". */
    plural?: string;
    /** Message for an empty list on the "All" tab. */
    emptyMessage?: string;
    /** Controls on the right of the tab strip (Corvus: the Active / Closed scope switch). */
    toolbar?: Snippet;
    /** Extra footer text (Corvus: the "capped at N" warning). */
    note?: Snippet;
  }

  let {
    findings,
    counts: countsOverride,
    loading = false,
    error = '',
    initialTab = 'all',
    hideLifecycle = false,
    noun = 'finding',
    plural = `${noun}s`,
    emptyMessage = 'No findings to show.',
    toolbar,
    note,
  }: Props = $props();

  // svelte-ignore state_referenced_locally
  let activeTab = $state<TabKey>(initialTab);
  /** True between a tab click and the next DOM tick, so the progress bar can paint while the row
   *  list is rebuilt — a large "All" tab otherwise feels frozen for the length of the diff. */
  let switching = $state(false);
  let selectedFinding = $state<SecurityFinding | null>(null);

  async function selectTab(id: TabKey) {
    if (id === activeTab) return;
    switching = true;
    await tick();
    activeTab = id;
    resetScroll();
    await tick();
    switching = false;
  }

  // ── Virtualization ──────────────────────────────────────────────────────
  // Each row is a fixed ROW_HEIGHT (enforced in SecurityFindingRow). Only the rows that intersect
  // the viewport plus a buffer are rendered — 300+ DOM nodes become ~20.
  const ROW_HEIGHT = 64;
  const OVERSCAN = 6;

  let listEl: HTMLElement | undefined = $state();
  let scrollTop = $state(0);
  let viewportHeight = $state(0);

  function resetScroll() {
    if (listEl) listEl.scrollTop = 0;
    scrollTop = 0;
  }

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

  // A different list (new scope, refresh) starts at the top.
  $effect(() => {
    void findings;
    resetScroll();
  });

  // ── Data ────────────────────────────────────────────────────────────────
  const counts = $derived.by<SeverityCounts>(() => {
    if (countsOverride) return countsOverride;
    const out: SeverityCounts = { critical: 0, high: 0, medium: 0, low: 0, info: 0, unknown: 0 };
    for (const f of findings) out[f.severity]++;
    return out;
  });
  const totalCount = $derived(
    counts.critical + counts.high + counts.medium + counts.low + counts.info + counts.unknown,
  );

  const SEV_RANK: Record<Severity, number> = {
    critical: 0, high: 1, medium: 2, low: 3, info: 4, unknown: 5,
  };

  /** The active tab's findings, worst first, then oldest first. */
  const visible = $derived.by<SecurityFinding[]>(() => {
    const arr = activeTab === 'all' ? [...findings] : findings.filter((f) => f.severity === activeTab);
    arr.sort((a, b) => SEV_RANK[a.severity] - SEV_RANK[b.severity] || b.age_days - a.age_days);
    return arr;
  });

  const startIdx = $derived(Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN));
  const endIdx = $derived(
    Math.min(visible.length, Math.ceil((scrollTop + viewportHeight) / ROW_HEIGHT) + OVERSCAN),
  );
  const offsetY = $derived(startIdx * ROW_HEIGHT);
  const totalHeight = $derived(visible.length * ROW_HEIGHT);
  const slice = $derived(visible.slice(startIdx, endIdx));

  const tabItems = $derived<TabItem[]>([
    { id: 'all', label: 'All', badge: totalCount > 0 ? totalCount : undefined, disabled: totalCount === 0 },
    ...SEVERITY_ORDER.map((sev) => ({
      id: sev,
      label: SEVERITY_META[sev].label,
      badge: counts[sev] > 0 ? counts[sev] : undefined,
      disabled: counts[sev] === 0,
    })),
  ]);

  const showProgress = $derived(loading || switching);
</script>

<div class="fb">
  <div class="tabs-row">
    <Tabs
      items={tabItems}
      value={activeTab}
      variant="underline"
      size="sm"
      onSelect={(id) => selectTab(id as TabKey)}
      ariaLabel="Severity filter"
    />
    {#if toolbar}{@render toolbar()}{/if}
  </div>

  <div class="list-region" bind:this={listEl}>
    {#if showProgress}
      <div class="progress-bar" aria-hidden="true"><div class="progress-bar-track"></div></div>
    {/if}

    {#if loading && findings.length === 0}
      <div class="state">
        <Spinner size={18} />
        <span class="state-hint">Loading {plural}…</span>
      </div>
    {:else if error && findings.length === 0}
      <div class="state">
        <ShieldAlert size={22} class="state-icon-warn" />
        <p class="state-title">Failed to load {plural}</p>
        <p class="state-hint err-msg">{error}</p>
      </div>
    {:else if visible.length === 0}
      <div class="state">
        <ShieldAlert size={22} class="state-icon-muted" />
        <p class="state-hint">
          {#if activeTab === 'all'}
            {emptyMessage}
          {:else}
            No <strong>{SEVERITY_META[activeTab].label}</strong> {plural}.
          {/if}
        </p>
      </div>
    {:else}
      <div class="virt-spacer" style:height="{totalHeight}px" role="list">
        <div class="virt-window" style:transform="translateY({offsetY}px)">
          {#each slice as finding (finding.id)}
            <SecurityFindingRow {finding} {hideLifecycle} onSelect={(f) => (selectedFinding = f)} />
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
      {visible.length === 1 ? noun : plural}
    </span>
    {#if note}{@render note()}{/if}
  </div>
</div>

{#if selectedFinding}
  <SecurityFindingDetailModal finding={selectedFinding} onClose={() => (selectedFinding = null)} />
{/if}

<style>
  .fb { display: flex; flex-direction: column; height: 100%; min-height: 0; background: var(--bg-base); }

  .tabs-row {
    flex-shrink: 0; padding: 0 12px; background: var(--bg-base);
    display: flex; align-items: center; justify-content: space-between; gap: 12px;
  }

  .list-region { flex: 1; min-height: 0; overflow: auto; position: relative; }

  /* Sticky so it stays visible when scrolled; runs for fetches and tab-switch alike. */
  .progress-bar {
    position: sticky; top: 0; left: 0; right: 0; height: 2px;
    background: var(--bg-elevated); overflow: hidden; z-index: 2; pointer-events: none;
  }
  .progress-bar-track {
    position: absolute; inset: 0;
    background: linear-gradient(90deg, transparent 0%, var(--accent) 50%, transparent 100%);
    animation: fb-progress-slide 1.1s ease-in-out infinite;
  }
  @keyframes fb-progress-slide {
    0%   { transform: translateX(-100%); }
    100% { transform: translateX(100%); }
  }

  .virt-spacer { position: relative; width: 100%; }
  .virt-window { position: absolute; top: 0; left: 0; right: 0; will-change: transform; }

  .footer-bar {
    flex-shrink: 0; display: flex; align-items: center; justify-content: space-between; gap: 12px;
    padding: 8px 14px; border-top: 1px solid var(--border-subtle); background: var(--bg-elevated);
    font-size: var(--font-size-xs); color: var(--text-muted); font-family: var(--font-ui-sans);
  }
  .count strong { color: var(--text-secondary); font-weight: 600; }

  .state {
    display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px;
    padding: 48px 24px; color: var(--text-muted); font-family: var(--font-ui-sans); text-align: center;
  }
  .state-title { margin: 0; font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }
  .state-hint { margin: 0; font-size: var(--font-size-sm); line-height: 1.5; max-width: 320px; }
  .err-msg {
    color: var(--error); font-family: var(--font-code); font-size: var(--font-size-xs); word-break: break-word;
  }
  :global(.state .state-icon-warn)  { color: var(--warning); }
  :global(.state .state-icon-muted) { color: var(--text-muted); opacity: 0.6; }
</style>
