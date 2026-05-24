<script lang="ts">
  import { RefreshCw, BarChart2, Loader2, GitCommitHorizontal, Users, Calendar, Zap, TrendingUp, Flame, Clock } from 'lucide-svelte';
  import { statsStore } from '$lib/stores/stats.svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  let { onOpenFull }: { onOpenFull: () => void } = $props();

  const tabId   = $derived(tabsStore.activeTabId);
  const stats   = $derived(statsStore.stats);
  const loading = $derived(statsStore.loading);
  const error   = $derived(statsStore.error);

  $effect(() => {
    if (tabId) statsStore.load(tabId);
  });

  function refresh() {
    if (tabId) statsStore.load(tabId, true);
  }

  function formatAge(firstTs: number, lastTs: number): string {
    if (!firstTs || !lastTs) return '—';
    const days = Math.round(Math.abs(lastTs - firstTs) / 86400);
    if (days < 30) return `${days}d`;
    if (days < 365) return `${Math.round(days / 30)}mo`;
    return `${(days / 365).toFixed(1)}y`;
  }

  // Mini sparkline: last 12 weeks
  const SPARK_W = 176;
  const SPARK_H = 32;

  const sparkData = $derived((() => {
    if (!stats) return { bars: [], peak: 0 };
    const map = new Map<string, number>(stats.commits_by_day);
    const weeks: number[] = [];
    const now = new Date();
    for (let w = 11; w >= 0; w--) {
      let total = 0;
      for (let d = 0; d < 7; d++) {
        const dt  = new Date(now.getTime() - (w * 7 + d) * 86_400_000);
        const iso = dt.toISOString().slice(0, 10);
        total += map.get(iso) ?? 0;
      }
      weeks.push(total);
    }
    const peak = Math.max(0, ...weeks);
    const max  = Math.max(1, peak);
    const bw   = (SPARK_W - 11 * 2) / 12;
    const bars = weeks.map((v, i) => ({
      x: i * (bw + 2),
      h: Math.max(1, (v / max) * SPARK_H),
      y: SPARK_H - Math.max(1, (v / max) * SPARK_H),
      w: Math.max(1, bw),
    }));
    return { bars, peak };
  })());

  const sparkBars = $derived(sparkData.bars);
  const sparkPeak = $derived(sparkData.peak);
</script>

<PanelShell title="Statistics">
  {#snippet icon()}<BarChart2 size={14} />{/snippet}
  {#snippet actions()}
    <button class="ps-btn" onclick={refresh} disabled={loading} use:tooltip={'Recompute'}>
      <RefreshCw size={13} class={loading ? 'spin' : ''} />
    </button>
  {/snippet}

  {#if loading && !stats}
    <div class="center-state">
      <Loader2 size={18} class="spin" />
      <span>Computing…</span>
    </div>

  {:else if error && !stats}
    <div class="center-state">
      <p class="err-msg">{error}</p>
      <button class="text-btn" onclick={refresh}>Retry</button>
    </div>

  {:else if !stats}
    <div class="center-state">
      <BarChart2 size={26} style="opacity:0.3" />
      <p style="margin:0;font-size:12px;">No statistics yet</p>
      <button class="text-btn" onclick={refresh}>Compute Stats</button>
    </div>

  {:else}

    <!-- ── stat cards ───────────────────────────────────────────────────────── -->
    <div class="cards-grid">
      <div class="stat-card">
        <div class="card-icon-wrap card-icon-commits"><GitCommitHorizontal size={16} /></div>
        <div class="card-body">
          <div class="card-value">{stats.total_commits.toLocaleString()}</div>
          <div class="card-label">Commits</div>
        </div>
      </div>
      <div class="stat-card">
        <div class="card-icon-wrap card-icon-authors"><Users size={16} /></div>
        <div class="card-body">
          <div class="card-value">{stats.total_contributors}</div>
          <div class="card-label">Authors</div>
        </div>
      </div>
      <div class="stat-card">
        <div class="card-icon-wrap card-icon-age"><Calendar size={16} /></div>
        <div class="card-body">
          <div class="card-value">{formatAge(stats.first_commit_time, stats.last_commit_time)}</div>
          <div class="card-label">Age</div>
        </div>
      </div>
      <div class="stat-card">
        <div class="card-icon-wrap card-icon-active"><Zap size={16} /></div>
        <div class="card-body">
          <div class="card-value">{stats.active_days.toLocaleString()}</div>
          <div class="card-label">Active days</div>
        </div>
      </div>
    </div>

    <!-- ── Sparkline ─────────────────────────────────────────────────────── -->
    <div class="section-sep">
      Commits / week
      <span class="sep-sub">last 12 weeks</span>
    </div>
    <div class="spark-wrap">
      <div class="spark-scale">
        <span>{sparkPeak}</span>
        <span>0</span>
      </div>
      <svg width={SPARK_W} height={SPARK_H} role="img" aria-label="Commits per week, last 12 weeks">
        {#each sparkBars as bar}
          <rect x={bar.x} y={bar.y} width={bar.w} height={bar.h} rx={1} class="spark-bar" />
        {/each}
      </svg>
    </div>
    <div class="spark-timeline">
      <span>12w ago</span>
      <span>now</span>
    </div>

    <!-- ── Top contributor ────────────────────────────────────────────────── -->
    {#if stats.top_contributors.length > 0}
      {@const top = stats.top_contributors[0]}
      <div class="section-sep">Top contributor</div>
      <div class="contrib-wrap">
        <div class="contrib-row">
          <span class="contrib-name">{top.name || top.email}</span>
          <span class="contrib-pct">{top.percentage.toFixed(1)}%</span>
        </div>
        <div class="bar-track">
          <div class="bar-fill" style="width:{Math.min(100, top.percentage)}%;"></div>
        </div>
      </div>
    {/if}

    <!-- ── Highlights ───────────────────────────────────────────────────────── -->
    <div class="section-sep">Highlights</div>
    <div class="highlights">

      {#if stats.top_contributor_week}
        {@const w = stats.top_contributor_week}
        <div class="hl-row">
          <div class="hl-icon hl-week"><Flame size={13} /></div>
          <div class="hl-body">
            <span class="hl-label">This week</span>
            <span class="hl-name">{w.name || w.email}</span>
          </div>
          <span class="hl-value">{w.commit_count}c</span>
        </div>
      {/if}

      {#if stats.top_contributor_month}
        {@const m = stats.top_contributor_month}
        <div class="hl-row">
          <div class="hl-icon hl-month"><TrendingUp size={13} /></div>
          <div class="hl-body">
            <span class="hl-label">This month</span>
            <span class="hl-name">{m.name || m.email}</span>
          </div>
          <span class="hl-value">{m.commit_count}c</span>
        </div>
      {/if}

      {#if stats.top_changer}
        {@const ch = stats.top_changer}
        <div class="hl-row">
          <div class="hl-icon hl-changer"><Zap size={13} /></div>
          <div class="hl-body">
            <span class="hl-label">Most lines changed</span>
            <span class="hl-name">{ch.name || ch.email}</span>
          </div>
          <span class="hl-value">{ch.total_changes.toLocaleString()}L</span>
        </div>
      {/if}

      {#if stats.longest_streak > 1}
        <div class="hl-row">
          <div class="hl-icon hl-streak"><Clock size={13} /></div>
          <div class="hl-body">
            <span class="hl-label">Longest streak</span>
            <span class="hl-name">{stats.longest_streak} days</span>
          </div>
        </div>
      {/if}

    </div>

    {#if loading}
      <div class="refresh-note">
        <RefreshCw size={11} class="spin" /> Refreshing…
      </div>
    {/if}

  {/if}

  {#snippet footer()}
    {#if stats}
      <div class="footer">
        <button class="full-btn" onclick={onOpenFull}>
          <BarChart2 size={13} />
          Full Statistics
        </button>
      </div>
    {/if}
  {/snippet}
</PanelShell>

<style>
  /* ── Center states ──────────────────────────────────────────────────────────── */
  .center-state {
    flex: 1; display: flex; flex-direction: column;
    align-items: center; justify-content: center;
    gap: 10px; padding: 32px 16px;
    color: var(--text-muted); font-family: var(--font-ui-sans); font-size: 12px;
  }
  .err-msg { margin: 0; text-align: center; max-width: 180px; font-size: 11px; }
  .text-btn {
    padding: 4px 12px;
    border: 1px solid var(--border); border-radius: var(--radius-sm);
    background: transparent; color: var(--text-secondary);
    font-size: 11px; font-family: var(--font-ui-sans); cursor: pointer;
  }
  .text-btn:hover { background: var(--bg-hover); }

  /* ── stat cards ─────────────────────────────────────────────────────────────── */
  .cards-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
    padding: 12px;
  }

  .stat-card {
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 11px 12px;
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 10px;
  }

  .card-icon-wrap {
    width: 34px;
    height: 34px;
    border-radius: var(--radius-sm);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }
  .card-icon-commits { background: hsl(220,60%,28%); color: hsl(220,80%,70%); }
  .card-icon-authors { background: hsl(270,50%,28%); color: hsl(270,80%,72%); }
  .card-icon-age     { background: hsl(160,50%,22%); color: hsl(160,70%,58%); }
  .card-icon-active  { background: hsl( 40,60%,22%); color: hsl( 40,90%,62%); }

  .card-body {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .card-value {
    font-size: 18px;
    font-weight: 700;
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    line-height: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .card-label {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.4px;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
  }

  /* ── Section separator ──────────────────────────────────────────────────────── */
  .section-sep {
    display: flex;
    align-items: baseline;
    gap: 6px;
    padding: 6px 14px 3px 12px;
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
  }
  .sep-sub {
    font-size: 9px;
    font-weight: 400;
    text-transform: none;
    letter-spacing: 0;
    color: var(--text-disabled);
  }

  /* ── Sparkline ──────────────────────────────────────────────────────────────── */
  .spark-wrap {
    display: flex;
    align-items: flex-end;
    gap: 6px;
    padding: 0 12px 4px;
  }
  .spark-scale {
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    align-items: flex-end;
    height: 32px;
    flex-shrink: 0;
    font-size: 9px;
    font-family: var(--font-ui-sans);
    color: var(--text-disabled);
    line-height: 1;
  }
  .spark-timeline {
    display: flex;
    justify-content: space-between;
    padding: 2px 12px 8px;
    border-bottom: 1px solid var(--border-subtle);
    font-size: 9px;
    font-family: var(--font-ui-sans);
    color: var(--text-disabled);
  }
  .spark-bar { fill: var(--accent); fill-opacity: 0.65; }

  /* ── Top contributor ────────────────────────────────────────────────────────── */
  .contrib-wrap {
    padding: 4px 12px 10px;
    border-bottom: 1px solid var(--border-subtle);
    display: flex; flex-direction: column; gap: 5px;
  }
  .contrib-row {
    display: flex; align-items: baseline; justify-content: space-between;
  }
  .contrib-name {
    font-size: 13px; font-weight: 600;
    color: var(--text-primary); font-family: var(--font-ui-sans);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 160px;
  }
  .contrib-pct {
    font-size: 12px; font-weight: 600;
    color: var(--accent); font-family: var(--font-ui-sans); flex-shrink: 0;
  }
  .bar-track {
    height: 3px; background: var(--bg-hover); border-radius: 2px; overflow: hidden;
  }
  .bar-fill {
    height: 100%; background: var(--accent); border-radius: 2px;
    transition: width 0.4s cubic-bezier(0.16,1,0.3,1);
  }

  /* ── Highlights ─────────────────────────────────────────────────────────────── */
  .highlights {
    display: flex;
    flex-direction: column;
    padding: 2px 12px 10px;
    border-bottom: 1px solid var(--border-subtle);
    gap: 0;
  }
  .hl-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 0;
    border-bottom: 1px solid var(--border-subtle);
  }
  .hl-row:last-child { border-bottom: none; }

  .hl-icon {
    width: 26px; height: 26px;
    border-radius: var(--radius-sm);
    display: flex; align-items: center; justify-content: center;
    flex-shrink: 0;
  }
  .hl-week    { background: hsl(30,55%,22%);  color: hsl(30,90%,62%); }
  .hl-month   { background: hsl(220,55%,24%); color: hsl(220,80%,68%); }
  .hl-changer { background: hsl(280,45%,24%); color: hsl(280,75%,70%); }
  .hl-streak  { background: hsl(160,45%,20%); color: hsl(160,65%,55%); }

  .hl-body {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }
  .hl-label {
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.4px;
    color: var(--text-disabled);
    font-family: var(--font-ui-sans);
    line-height: 1;
  }
  .hl-name {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    line-height: 1.3;
  }
  .hl-value {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    flex-shrink: 0;
  }

  /* ── Full Statistics button ─────────────────────────────────────────────────── */
  .footer { padding: 10px 12px 4px; }

  .full-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    width: 100%;
    padding: 8px 0;
    border: none;
    border-radius: var(--radius-md);
    background: var(--accent);
    color: var(--text-on-accent);
    font-size: 12px;
    font-weight: 600;
    font-family: var(--font-ui-sans);
    cursor: pointer;
    transition: background var(--transition-fast), opacity var(--transition-fast);
    letter-spacing: 0.2px;
  }
  .full-btn:hover { background: var(--accent-hover, color-mix(in srgb, var(--accent) 85%, white)); }
  .full-btn:active { opacity: 0.85; }

  /* ── Refresh note ───────────────────────────────────────────────────────────── */
  .refresh-note {
    display: flex; align-items: center; justify-content: center;
    gap: 5px; font-size: 10px; color: var(--text-muted);
    font-family: var(--font-ui-sans); padding: 6px 0;
  }

</style>
