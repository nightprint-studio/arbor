<script lang="ts">
  import { RefreshCw, BarChart2, Users, FileCode, FileDown } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import { statsStore } from '$lib/stores/stats.svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { exportRepoStats } from '$lib/ipc/stats';
  import { notificationsStore } from '$lib/stores/notifications.svelte';
  import CommitHeatmap    from './CommitHeatmap.svelte';
  import TimeDistribution from './TimeDistribution.svelte';
  import ContributorBar   from './ContributorBar.svelte';
  import FileTypeChart    from './FileTypeChart.svelte';
  import FilePickerModal  from '$lib/components/shared/FilePickerModal.svelte';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  let { onClose }: { onClose: () => void } = $props();

  type Tab = 'overview' | 'contributors' | 'files';
  let activeTab = $state<Tab>('overview');

  const stats   = $derived(statsStore.stats);
  const loading = $derived(statsStore.loading);
  const tabId   = $derived(tabsStore.activeTabId);

  let exportFormat     = $state<'json' | 'html' | null>(null);
  let exportMenuOpen   = $state(false);
  let exportWrapEl     = $state<HTMLElement | null>(null);
  let showExportPicker = $state(false);

  const exportExtensions = $derived(exportFormat === 'json' ? ['json'] : ['html', 'htm']);
  const exportFilename   = $derived(
    exportFormat === 'json'
      ? `${tabsStore.activeTab?.name ?? 'repo'}-stats.json`
      : `${tabsStore.activeTab?.name ?? 'repo'}-stats.html`
  );

  function startExport(fmt: 'json' | 'html') {
    exportMenuOpen = false;
    exportFormat = fmt;
    showExportPicker = true;
  }

  async function doExport(path: string) {
    if (!tabId || !exportFormat) return;
    showExportPicker = false;
    try {
      await exportRepoStats(tabId, path, exportFormat);
    } catch (err) {
      notificationsStore.add('Stats export failed', String(err), 'error');
    }
    exportFormat = null;
  }

  function onDocClick(e: MouseEvent) {
    if (exportMenuOpen && !exportWrapEl?.contains(e.target as Node)) {
      exportMenuOpen = false;
    }
  }

  function refresh() {
    if (tabId) statsStore.load(tabId, true);
  }

  function formatDate(ts: number): string {
    if (!ts) return '—';
    return new Date(ts * 1000).toLocaleDateString('default', {
      year: 'numeric', month: 'short', day: 'numeric',
    });
  }

  function formatAgeParts(firstTs: number, lastTs: number): { num: string; unit: string } {
    if (!firstTs || !lastTs) return { num: '—', unit: '' };
    const days = Math.round(Math.abs(lastTs - firstTs) / 86400);
    if (days < 30)  return { num: String(days),                  unit: 'd' };
    if (days < 365) return { num: String(Math.round(days / 30)), unit: 'mo' };
    return           { num: (days / 365).toFixed(1),             unit: 'y' };
  }
</script>

<svelte:document onclick={onDocClick} />

<Modal {onClose} width="860px" height="78vh" padBody={false} ariaLabel="Repository Statistics">
  {#snippet header()}
    <ModalHeader {onClose}>
      <BarChart2 size={14} />
      <span class="modal-title">Repository Statistics</span>
      {#if stats}
        <span class="hdr-repo">{tabsStore.activeTab?.name ?? ''}</span>
      {/if}
      {#snippet actions()}
        {#if stats}
          <div class="export-wrap" bind:this={exportWrapEl}>
            <button
              class="icon-btn"
              class:active={exportMenuOpen}
              onclick={() => exportMenuOpen = !exportMenuOpen}
              use:tooltip={'Export statistics'}
            >
              <FileDown size={14} />
            </button>
            {#if exportMenuOpen}
              <div class="export-menu" role="menu">
                <button role="menuitem" onclick={() => startExport('json')}>
                  <span class="export-ext">.json</span>
                  JSON data
                </button>
                <button role="menuitem" onclick={() => startExport('html')}>
                  <span class="export-ext">.html</span>
                  HTML report
                </button>
              </div>
            {/if}
          </div>
        {/if}
        <button
          class="icon-btn"
          onclick={refresh}
          disabled={loading}
          use:tooltip={'Recompute stats'}
        >
          <RefreshCw size={14} class={loading ? 'spin' : ''} />
        </button>
      {/snippet}
    </ModalHeader>
  {/snippet}

  <div class="stats-body">
    {#if loading && !stats}
      <div class="loading-full">
        <div class="spinner"></div>
        <span>Computing repository statistics…</span>
        <span class="loading-hint">This may take a moment on large repositories.</span>
      </div>

    {:else if statsStore.error && !stats}
      <div class="error-full">
        <p>Failed to compute statistics:</p>
        <pre>{statsStore.error}</pre>
        <button class="btn-primary" onclick={refresh}>Retry</button>
      </div>

    {:else if stats}
      <!-- Tab bar -->
      <div class="tab-bar">
        <Tabs
          items={[
            { id: 'overview',     label: 'Overview',     icon: BarChart2, iconSize: 13 },
            { id: 'contributors', label: 'Contributors', icon: Users,     iconSize: 13 },
            { id: 'files',        label: 'Files',        icon: FileCode,  iconSize: 13 },
          ]}
          value={activeTab}
          variant="underline"
          size="md"
          onSelect={(id) => activeTab = id as Tab}
        />
        {#if loading}
          <span class="tab-refreshing">
            <RefreshCw size={11} class="spin" /> Refreshing…
          </span>
        {/if}
      </div>

      <!-- Tab content -->
      <div class="tab-content">
        {#if activeTab === 'overview'}
          <!-- Summary cards -->
          <div class="cards">
            <div class="card">
              <div class="card-value">{stats.total_commits.toLocaleString()}</div>
              <div class="card-label">Total Commits</div>
            </div>
            <div class="card">
              <div class="card-value">{stats.total_contributors}</div>
              <div class="card-label">Contributors</div>
            </div>
            <div class="card">
              <div class="card-value">
                {formatAgeParts(stats.first_commit_time, stats.last_commit_time).num}<span class="card-unit">{formatAgeParts(stats.first_commit_time, stats.last_commit_time).unit}</span>
              </div>
              <div class="card-label">Repository Age</div>
            </div>
            <div class="card">
              <div class="card-value">{stats.active_days.toLocaleString()}</div>
              <div class="card-label">Active Days</div>
            </div>
            <div class="card">
              <div class="card-value">{stats.avg_commits_per_week.toFixed(1)}</div>
              <div class="card-label">Avg / Week</div>
            </div>
            <div class="card">
              <div class="card-value">{Math.round(stats.avg_commit_size)}<span class="card-unit">L</span></div>
              <div class="card-label">Avg Commit Size</div>
            </div>
            <div class="card">
              <div class="card-value">{stats.longest_streak}</div>
              <div class="card-label">Longest Streak</div>
            </div>
            <div class="card card-wide">
              <div class="card-value card-value-sm">{formatDate(stats.first_commit_time)}</div>
              <div class="card-label">First Commit</div>
            </div>
            <div class="card card-wide">
              <div class="card-value card-value-sm">{formatDate(stats.last_commit_time)}</div>
              <div class="card-label">Last Commit</div>
            </div>
            {#if stats.busiest_day}
              <div class="card card-wide">
                <div class="card-value card-value-sm">{stats.busiest_day[0]}</div>
                <div class="card-label">Busiest Day ({stats.busiest_day[1]} commits)</div>
              </div>
            {/if}
          </div>

          <!-- Commit heatmap -->
          <div class="section">
            <h3 class="section-title">Commit Activity — Last 12 Months</h3>
            <div class="heatmap-scroll">
              <CommitHeatmap days={stats.commits_by_day} />
            </div>
          </div>

          <!-- Hour + Weekday distributions -->
          <div class="section">
            <h3 class="section-title">Commit Timing</h3>
            <TimeDistribution byHour={stats.commits_by_hour} byWeekday={stats.commits_by_weekday} />
          </div>

        {:else if activeTab === 'contributors'}
          <div class="section">
            <h3 class="section-title">Top Contributors — By Commits</h3>
            <ContributorBar contributors={stats.top_contributors} />
          </div>

          {#if stats.top_changers.length > 0}
            <div class="section">
              <h3 class="section-title">
                Top Contributors — By Lines Changed
                <span class="section-note">(first 500 commits)</span>
              </h3>
              <div class="changers-list">
                {#each stats.top_changers as c, i}
                  {@const maxLines = stats.top_changers[0].total_changes}
                  {@const pct = (c.total_changes / maxLines) * 100}
                  <div class="changer-row">
                    <span class="changer-rank">#{i + 1}</span>
                    <div class="changer-info">
                      <div class="changer-meta">
                        <span class="changer-name">{c.name || c.email}</span>
                        <span class="changer-total">{c.total_changes.toLocaleString()} lines</span>
                      </div>
                      <div class="changer-pills">
                        <span class="pill pill-add">+{c.lines_added.toLocaleString()}</span>
                        <span class="pill pill-del">−{c.lines_deleted.toLocaleString()}</span>
                      </div>
                      <div class="bar-track changer-track">
                        <div class="bar-fill changer-bar-add" style="width:{pct * (c.lines_added / c.total_changes)}%;"></div>
                        <div class="bar-fill changer-bar-del" style="width:{pct * (c.lines_deleted / c.total_changes)}%;"></div>
                      </div>
                    </div>
                  </div>
                {/each}
              </div>
            </div>
          {/if}

        {:else if activeTab === 'files'}
          <FileTypeChart
            fileTypeBreakdown={stats.file_type_breakdown}
            mostChangedFiles={stats.most_changed_files}
          />
        {/if}
      </div>
    {/if}
  </div>
</Modal>

{#if showExportPicker && exportFormat}
  <FilePickerModal
    mode="save"
    extensions={exportExtensions}
    initialFilename={exportFilename}
    title="Export Statistics"
    onConfirm={doExport}
    onCancel={() => { showExportPicker = false; exportFormat = null; }}
  />
{/if}

<style>
  /* Body fills the .modal-body card (bg-base) provided by <Modal padBody={false}>.
     Tab bar sits flush on top of the scrollable tab-content. */
  .stats-body {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
  }

  .hdr-repo {
    font-weight: 400;
    color: var(--text-muted);
    font-size: 13px;
    border-left: 1px solid var(--border-subtle);
    padding-left: 8px;
    margin-left: 2px;
  }

  /* ── Export dropdown ──────────────────────────────────────────────────────── */
  .export-wrap { position: relative; display: flex; align-items: center; }

  .icon-btn.active { background: var(--bg-overlay); color: var(--accent); }

  .export-menu {
    position: absolute;
    top: calc(100% + 5px);
    right: 0;
    min-width: 160px;
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-radius: var(--radius-md, 6px);
    box-shadow: 0 6px 20px rgba(0,0,0,0.4);
    z-index: 200;
    overflow: hidden;
    animation: fadeInMenu 0.1s ease;
  }
  @keyframes fadeInMenu {
    from { opacity: 0; transform: translateY(-4px); }
    to   { opacity: 1; transform: translateY(0); }
  }
  .export-menu button {
    display: flex; align-items: center; gap: 10px;
    width: 100%; padding: 8px 12px;
    background: transparent; border: none;
    cursor: pointer; color: var(--text-secondary);
    font-family: var(--font-ui-sans); font-size: 13px;
    text-align: left;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .export-menu button:hover { background: var(--bg-hover); color: var(--text-primary); }
  .export-ext {
    font-family: var(--font-code); font-size: 10px;
    color: var(--accent); background: rgba(77,120,204,0.12);
    padding: 1px 5px; border-radius: var(--radius-sm);
    min-width: 36px; text-align: center;
  }
  .icon-btn {
    display: flex; align-items: center; justify-content: center;
    width: 22px; height: 22px;
    border: none; background: transparent;
    color: var(--text-muted);
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .icon-btn:hover   { background: var(--bg-hover); color: var(--text-primary); }
  .icon-btn:disabled { opacity: 0.4; cursor: not-allowed; }

  /* ── Tab bar ──────────────────────────────────────────────────────────────
     Strip rendered by shared <Tabs variant="underline">. The wrapper just
     contributes the modal's side-padding and hosts the trailing
     "Refreshing…" indicator next to the strip. */
  .tab-bar {
    display: flex;
    align-items: center;
    padding: 6px 14px 0;
    flex-shrink: 0;
  }
  .tab-bar :global(.tabs) { flex: 1; }

  .tab-refreshing {
    margin-left: auto;
    display: flex; align-items: center; gap: 4px;
    font-size: 11px; color: var(--text-muted); font-family: var(--font-ui-sans);
    padding-bottom: 8px;
  }

  /* ── Tab content — scrolls within the bg-base modal-body card. */
  .tab-content {
    flex: 1;
    overflow-y: auto;
    padding: 20px 24px;
    display: flex;
    flex-direction: column;
    gap: 24px;
  }

  /* ── Summary cards — slightly elevated so they stand out from --bg-base ── */
  .cards { display: flex; flex-wrap: wrap; gap: 10px; }
  .card {
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 11px 14px;
    min-width: 90px;
    flex: 1;
  }
  .card-wide { min-width: 140px; }
  .card-value {
    font-size: 20px; font-weight: 700;
    color: var(--text-primary); font-family: var(--font-ui-sans);
    line-height: 1.1;
    white-space: nowrap;
  }
  .card-value-sm { font-size: 14px; }
  .card-unit {
    font-size: 11px;
    font-weight: 400;
    color: var(--text-muted);
    margin-left: 2px;
  }
  .card-label {
    font-size: 10px; color: var(--text-muted); font-family: var(--font-ui-sans);
    margin-top: 4px; text-transform: uppercase; letter-spacing: 0.4px;
  }

  /* ── Section ──────────────────────────────────────────────────────────────── */
  .section { display: flex; flex-direction: column; gap: 10px; }
  .section-title {
    font-size: 12px; font-weight: 600;
    text-transform: uppercase; letter-spacing: 0.5px;
    color: var(--text-muted); font-family: var(--font-ui-sans); margin: 0;
  }
  .heatmap-scroll { padding-bottom: 4px; }

  /* ── Changers list ────────────────────────────────────────────────────────── */
  .changers-list { display: flex; flex-direction: column; gap: 10px; }

  .changer-row {
    display: flex;
    align-items: flex-start;
    gap: 10px;
  }
  .changer-rank {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-disabled);
    font-family: var(--font-ui-sans);
    width: 24px;
    flex-shrink: 0;
    padding-top: 2px;
  }
  .changer-info { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px; }
  .changer-meta {
    display: flex; align-items: baseline; justify-content: space-between; gap: 8px;
  }
  .changer-name {
    font-size: 13px; font-weight: 600;
    color: var(--text-primary); font-family: var(--font-ui-sans);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .changer-total {
    font-size: 11px; color: var(--text-muted); font-family: var(--font-ui-sans);
    flex-shrink: 0;
  }
  .changer-pills { display: flex; gap: 6px; }
  .pill {
    font-size: 11px; font-family: var(--font-ui-sans); font-weight: 600;
    padding: 1px 7px; border-radius: var(--radius-lg);
  }
  .pill-add { background: hsl(145,40%,16%); color: hsl(145,60%,52%); }
  .pill-del { background: hsl(0,35%,18%);   color: hsl(0,65%,60%); }

  .changer-track {
    height: 4px; display: flex; overflow: hidden;
    background: var(--bg-hover); border-radius: 2px;
  }
  .bar-fill { height: 100%; }
  .changer-bar-add { background: hsl(145,60%,42%); }
  .changer-bar-del { background: hsl(0,60%,50%); }

  .section-note {
    font-size: 10px; font-weight: 400;
    text-transform: none; letter-spacing: 0;
    color: var(--text-disabled); margin-left: 4px;
  }

  /* ── Loading / Error ──────────────────────────────────────────────────────── */
  .loading-full {
    flex: 1; display: flex; flex-direction: column;
    align-items: center; justify-content: center;
    gap: 12px; padding: 48px;
    color: var(--text-secondary); font-family: var(--font-ui-sans); font-size: 14px;
  }
  .loading-hint { font-size: 12px; color: var(--text-muted); }
  .spinner {
    width: 28px; height: 28px;
    border: 2px solid var(--border); border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.7s linear infinite;
  }
  .error-full {
    flex: 1; display: flex; flex-direction: column;
    align-items: center; justify-content: center;
    gap: 12px; padding: 48px;
    color: var(--text-secondary); font-family: var(--font-ui-sans);
  }
  .error-full pre {
    font-size: 12px; color: var(--diff-del-bg-strong, #e87474);
    background: var(--bg-base); border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm); padding: 10px 14px;
    max-width: 480px; overflow-x: auto;
  }
  .btn-primary {
    padding: 7px 18px; background: var(--accent); color: var(--text-on-accent);
    border: none; border-radius: var(--radius-sm);
    font-size: 13px; font-family: var(--font-ui-sans);
    cursor: pointer; transition: background var(--transition-fast);
  }
  .btn-primary:hover { background: var(--accent-hover); }

</style>
