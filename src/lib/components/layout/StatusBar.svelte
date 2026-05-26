<script lang="ts">
  import { ArrowUp, ArrowDown, GitBranch, Tag, AlertCircle, GitMerge, Loader, Bell, Clock, Undo2, ShieldAlert } from 'lucide-svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { repoStore } from '$lib/stores/repo.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { jobsStore } from '$lib/stores/jobs.svelte';
  import { notificationsStore } from '$lib/stores/notifications.svelte';
  import { securityStore } from '$lib/stores/security.svelte';
  import { cacheStore } from '$lib/stores/cache.svelte';
  import type { RepoStatus } from '$lib/types/git';
  import Contribution from '$lib/components/shared/Contribution.svelte';
  import PluginIcon   from '$lib/components/plugins/PluginIcon.svelte';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import { tooltip } from '$lib/actions/tooltip';

  const activeTab = $derived(tabsStore.activeTab);
  const status    = $derived(repoStore.status);

  function getChangeCounts(s: RepoStatus) {
    const paths = new Map<string, 'modified' | 'added' | 'deleted'>();
    for (const f of s.untracked) paths.set(f.path, 'added');
    for (const f of s.staged) {
      if (f.index_status === 'added')   paths.set(f.path, 'added');
      else if (f.index_status === 'deleted') paths.set(f.path, 'deleted');
      else paths.set(f.path, 'modified');
    }
    for (const f of s.unstaged) {
      if (f.workdir_status === 'deleted') paths.set(f.path, 'deleted');
      else if (!paths.has(f.path))        paths.set(f.path, 'modified');
    }
    let modified = 0, added = 0, deleted = 0;
    for (const v of paths.values()) {
      if (v === 'modified') modified++;
      else if (v === 'added') added++;
      else deleted++;
    }
    return { modified, added, deleted };
  }

  const changeCounts = $derived(status ? getChangeCounts(status) : null);
  const totalChanges = $derived(changeCounts ? changeCounts.modified + changeCounts.added + changeCounts.deleted : 0);

  // ── Last-refresh badge ────────────────────────────────────────────────────
  // Re-computed every 15 s so the displayed age stays fresh without rerenders.
  // Paused while the app is in the background (power saving mode).
  let _tick = $state(0);
  $effect(() => {
    const id = setInterval(() => {
      // Skip tick updates while the window is unfocused — no point rerendering
      // age labels when the user isn't looking at the app.
      if (uiStore.appFocused) _tick++;
    }, 15_000);
    return () => clearInterval(id);
  });

  function formatLastRefresh(ts: number | null): string | null {
    if (!ts) return null;
    _tick; // track tick so this recomputes on interval
    const secs = Math.floor((Date.now() - ts) / 1000);
    if (secs < 10)  return 'just now';
    if (secs < 60)  return `${secs}s ago`;
    const mins = Math.floor(secs / 60);
    if (mins < 60)  return `${mins}m ago`;
    const hrs  = Math.floor(mins / 60);
    return `${hrs}h ago`;
  }

  const lastRefreshLabel = $derived(formatLastRefresh(cacheStore.activeTabLastRefreshed));

  // ── Security quick-overlay gating ─────────────────────────────────────────
  // The provider probe lives in AppShell and caches per tab; here we just
  // read the cached answer.  When the active tab's provider doesn't support
  // a security dashboard the chip stays hidden.
  const activeTabId       = $derived(tabsStore.activeTabId);
  const securitySupported = $derived(securityStore.providerSupportsSecurity(activeTabId));
  // Only honour the store snapshot if it belongs to the active tab — avoids
  // showing pills from a previously-loaded repo while a freshly switched-to
  // tab's summary is still in-flight.
  const securitySummary   = $derived(
    securityStore.snapshotTabId === activeTabId ? securityStore.summary : null
  );
  // Status-bar chip shows just an icon + a total-count badge — the overlay
  // exposes the per-severity breakdown. We keep `criticalHigh` around to
  // tint the chip when the repo has a high-severity backlog (otherwise it
  // stays neutral so a clean repo doesn't shout for attention).
  const securityTotal = $derived(
    securitySummary
      ? Object.values(securitySummary.counts).reduce((a, b) => a + b, 0)
      : 0
  );
  const securityHasCriticalOrHigh = $derived(
    !!securitySummary && (securitySummary.counts.critical + securitySummary.counts.high) > 0
  );

  const runningJobs  = $derived(jobsStore.runningCount);
  // Visible job total: hidden jobs are excluded from the badge unless the
  // user has flipped the "Show hidden" toggle on the Jobs panels.
  const totalJobs    = $derived(jobsStore.runningCount + jobsStore.finishedCount);

  async function copyRefName(text: string, kind: 'branch' | 'tag') {
    if (await copyToClipboard(text, { errorToast: 'Copy failed' })) {
      uiStore.showToast(`Copied ${kind} "${text}"`, 'info', 1800);
    }
  }
</script>

<div class="statusbar">
  {#if activeTab}

    <!-- Branch chip (accent colored) — click copies branch name -->
    {@const branchName = status?.current_branch ?? activeTab.currentBranch ?? 'detached'}
    <button
      class="status-chip chip-branch"
      onclick={() => copyRefName(branchName, 'branch')}
      use:tooltip={'Click to copy branch name'}
    >
      <GitBranch size={13} />
      <span class="branch-name">{branchName}</span>
    </button>

    <!-- Security quick-overlay trigger (sits right after the branch chip,
         gated by the provider support probe). Compact: shield icon with a
         corner badge for the total finding count. -->
    {#if securitySupported}
      <button
        class="security-badge"
        class:security-badge-has={securityTotal > 0}
        class:security-badge-alert={securityHasCriticalOrHigh}
        onclick={() => uiStore.toggleSecurityOverlay()}
        use:tooltip={
          securitySummary
            ? {
                content: `Security: ${securityTotal} finding${securityTotal === 1 ? '' : 's'}`,
                description:
                  `C:${securitySummary.counts.critical}` +
                  ` H:${securitySummary.counts.high}` +
                  ` M:${securitySummary.counts.medium}` +
                  ` L:${securitySummary.counts.low}`,
              }
            : 'Security summary — click to view'
        }
      >
        <span class="security-icon-wrap">
          <ShieldAlert size={14} />
          {#if securityTotal > 0}
            <span class="security-count">{securityTotal > 99 ? '99+' : securityTotal}</span>
          {/if}
        </span>
      </button>
    {/if}

    <!-- Nearest tag chip — click copies tag name -->
    {#if repoStore.nearestTag}
      <button
        class="status-chip chip-tag"
        onclick={() => copyRefName(repoStore.nearestTag!, 'tag')}
        use:tooltip={'Click to copy tag name'}
      >
        <Tag size={12} />
        <span>{repoStore.nearestTag}</span>
      </button>
    {/if}

    <!-- Ahead / Behind -->
    {#if status && (status.ahead > 0 || status.behind > 0)}
      <div class="status-item">
        {#if status.ahead > 0}
          <span class="chip-ahead"><ArrowUp size={12} />{status.ahead}</span>
        {/if}
        {#if status.behind > 0}
          <span class="chip-behind"><ArrowDown size={12} />{status.behind}</span>
        {/if}
      </div>
    {/if}

    <!-- Conflicts -->
    {#if status?.conflicted?.length || 0 > 0}
      <div class="status-item item-error">
        <AlertCircle size={13} />
        <span>{status?.conflicted.length} conflicts</span>
      </div>
    {/if}

    <!-- Changes breakdown -->
    {#if changeCounts && totalChanges > 0}
      <div class="status-item item-changes">
        <span class="change-dot">●</span>
        {#if changeCounts.modified > 0}
          <span class="change-pill chip-modified" use:tooltip={`${changeCounts.modified} modified`}>{changeCounts.modified}M</span>
        {/if}
        {#if changeCounts.added > 0}
          <span class="change-pill chip-added" use:tooltip={`${changeCounts.added} added`}>{changeCounts.added}A</span>
        {/if}
        {#if changeCounts.deleted > 0}
          <span class="change-pill chip-deleted" use:tooltip={`${changeCounts.deleted} deleted`}>{changeCounts.deleted}D</span>
        {/if}
      </div>
    {/if}

    <!-- Rebase/Merge state badge -->
    {#if status?.is_rebasing}
      <div class="state-badge badge-rebase">REBASING</div>
    {:else if status?.is_merging}
      <div class="state-badge badge-merge"><GitMerge size={12} /> MERGING</div>
    {:else if status?.is_cherry_picking}
      <div class="state-badge badge-cherry">CHERRY-PICK</div>
    {:else if status?.is_reverting}
      <div class="state-badge badge-revert"><Undo2 size={12} /> REVERTING</div>
    {/if}

    <!-- Last-refresh badge -->
    {#if lastRefreshLabel}
      <div class="last-refresh" use:tooltip={`Data last refreshed ${lastRefreshLabel}`}>
        <Clock size={11} />
        <span>{lastRefreshLabel}</span>
      </div>
    {/if}

    <!-- Plugin-contributed items (left segment) -->
    <Contribution point="arbor:status-bar:left">
      {#snippet item({ payload, fire })}
        {@const p = payload as { label?: string; icon?: string; action?: string; tooltip?: string; color?: string }}
        {#if p.action}
          <button
            type="button"
            class="plugin-status-item plugin-status-clickable"
            class:plugin-color-info={p.color === 'info'}
            class:plugin-color-success={p.color === 'success'}
            class:plugin-color-warning={p.color === 'warning'}
            class:plugin-color-error={p.color === 'error'}
            class:plugin-color-muted={p.color === 'muted'}
            class:plugin-color-accent={p.color === 'accent'}
            use:tooltip={p.tooltip ?? p.label ?? ''}
            onclick={() => fire()}
          >
            {#if p.icon}<PluginIcon name={p.icon} size={12} />{/if}
            {#if p.label}<span>{p.label}</span>{/if}
          </button>
        {:else}
          <span
            class="plugin-status-item"
            class:plugin-color-info={p.color === 'info'}
            class:plugin-color-success={p.color === 'success'}
            class:plugin-color-warning={p.color === 'warning'}
            class:plugin-color-error={p.color === 'error'}
            class:plugin-color-muted={p.color === 'muted'}
            class:plugin-color-accent={p.color === 'accent'}
            use:tooltip={p.tooltip ?? p.label ?? ''}
          >
            {#if p.icon}<PluginIcon name={p.icon} size={12} />{/if}
            {#if p.label}<span>{p.label}</span>{/if}
          </span>
        {/if}
      {/snippet}
    </Contribution>

    <!-- Repo path chip (truncated, clickable to copy) — anchored at the
         right edge of the left segment, just before the spacer, so it
         doesn't compete with the branch/status chips for leftmost attention
         but still lives on the "info" half of the bar. -->
    <button
      class="repo-path"
      use:tooltip={{ content: 'Copy path', description: activeTab.path }}
      onclick={() => copyToClipboard(activeTab.path, { successToast: 'Path copied' })}
    >{activeTab.path}</button>

    <div class="spacer"></div>
  {:else}
    <span class="no-repo">No repository open</span>
    <div class="spacer"></div>
  {/if}

  <!-- Plugin-contributed items (right segment, always visible regardless of
       active repo so background indicators stay reachable). -->
  <Contribution point="arbor:status-bar:right">
    {#snippet item({ payload, fire })}
      {@const p = payload as { label?: string; icon?: string; action?: string; tooltip?: string; color?: string }}
      {#if p.action}
        <button
          type="button"
          class="plugin-status-item plugin-status-clickable"
          class:plugin-color-info={p.color === 'info'}
          class:plugin-color-success={p.color === 'success'}
          class:plugin-color-warning={p.color === 'warning'}
          class:plugin-color-error={p.color === 'error'}
          class:plugin-color-muted={p.color === 'muted'}
          class:plugin-color-accent={p.color === 'accent'}
          use:tooltip={p.tooltip ?? p.label ?? ''}
          onclick={() => fire()}
        >
          {#if p.icon}<PluginIcon name={p.icon} size={12} />{/if}
          {#if p.label}<span>{p.label}</span>{/if}
        </button>
      {:else}
        <span
          class="plugin-status-item"
          class:plugin-color-info={p.color === 'info'}
          class:plugin-color-success={p.color === 'success'}
          class:plugin-color-warning={p.color === 'warning'}
          class:plugin-color-error={p.color === 'error'}
          class:plugin-color-muted={p.color === 'muted'}
          class:plugin-color-accent={p.color === 'accent'}
          use:tooltip={p.tooltip ?? p.label ?? ''}
        >
          {#if p.icon}<PluginIcon name={p.icon} size={12} />{/if}
          {#if p.label}<span>{p.label}</span>{/if}
        </span>
      {/if}
    {/snippet}
  </Contribution>

  <!-- Jobs badge (IntelliJ-style) -->
  <div class="status-sep"></div>
  <button
    class="job-badge"
    class:job-badge-running={runningJobs > 0}
    class:job-badge-idle={totalJobs === 0}
    use:tooltip={{
      content: runningJobs > 0
        ? `${runningJobs} job${runningJobs > 1 ? 's' : ''} running`
        : totalJobs > 0
          ? 'All jobs finished'
          : 'No jobs',
      description: 'Click to view',
    }}
    onclick={() => uiStore.toggleJobsOverlay()}
  >
    {#if runningJobs > 0}
      <span class="job-spinner"><Loader size={12} /></span>
      <span>{runningJobs}</span>
    {:else if totalJobs > 0}
      <span class="job-done-dot">●</span>
      <span>{totalJobs}</span>
    {:else}
      <Loader size={12} />
    {/if}
  </button>

  <!-- Notifications bell -->
  <button
    class="notif-badge"
    class:notif-badge-has={notificationsStore.count > 0}
    use:tooltip={{
      content: notificationsStore.count > 0
        ? `${notificationsStore.count} notification${notificationsStore.count > 1 ? 's' : ''}`
        : 'No notifications',
      description: 'Click to view',
    }}
    onclick={() => uiStore.toggleNotificationsOverlay()}
  >
    <Bell size={13} />
    {#if notificationsStore.count > 0}
      <span class="notif-count">{notificationsStore.count > 99 ? '99+' : notificationsStore.count}</span>
    {/if}
  </button>
</div>

<style>
  .statusbar {
    display: flex;
    align-items: center;
    height: 26px;
    background: var(--bg-elevated);
    /* border-top: 1px solid var(--border-subtle); */
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    padding: 0 6px;
    gap: 4px;
    flex-shrink: 0;
    overflow: hidden;
  }

  /* Branch chip — primary accent */
  .status-chip {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 0 7px;
    height: 20px;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    white-space: nowrap;
    transition: background var(--transition-fast);
  }

  .chip-branch {
    background: color-mix(in srgb, var(--graph-lane-0) 25%, transparent);
    color: var(--graph-lane-0);
  }
  .chip-branch:hover { background: color-mix(in srgb, var(--graph-lane-0) 38%, transparent); }

  .chip-tag {
    background: color-mix(in srgb, var(--color-tag) 18%, transparent);
    color: var(--color-tag);
    border: none;
    font-size: var(--font-size-sm);
  }
  .chip-tag:hover { background: color-mix(in srgb, var(--color-tag) 30%, transparent); }

  .branch-name {
    max-width: 180px;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* Generic status item */
  .status-item {
    display: flex;
    align-items: center;
    gap: 3px;
    padding: 0 5px;
    height: 20px;
    border-radius: var(--radius-sm);
    white-space: nowrap;
    color: var(--text-muted);
  }

  .chip-ahead {
    display: flex; align-items: center; gap: 1px;
    color: var(--success);
    font-size: var(--font-size-sm);
  }
  .chip-behind {
    display: flex; align-items: center; gap: 1px;
    color: var(--warning);
    font-size: var(--font-size-sm);
  }

  .item-error { color: var(--error); background: var(--error-subtle); }
  .item-changes { gap: 4px; }
  .change-dot { font-size: 9px; color: var(--color-stash); }

  .change-pill {
    font-size: 11px;
    font-weight: 600;
    padding: 0 4px;
    border-radius: var(--radius-sm);
    line-height: 18px;
    letter-spacing: 0.2px;
  }
  .chip-modified { color: var(--warning); background: var(--warning-subtle); }
  .chip-added    { color: var(--success); background: var(--success-subtle); }
  .chip-deleted  { color: var(--error);   background: var(--error-subtle); }

  /* State badges */
  .state-badge {
    display: flex;
    align-items: center;
    gap: 3px;
    padding: 0 7px;
    height: 20px;
    border-radius: var(--radius-sm);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.4px;
    white-space: nowrap;
  }
  .badge-rebase  { background: color-mix(in srgb, var(--warning) 24%, transparent);   color: var(--warning); }
  .badge-merge   { background: color-mix(in srgb, var(--color-tag) 24%, transparent); color: var(--color-tag); }
  .badge-cherry  { background: color-mix(in srgb, var(--success) 18%, transparent);   color: var(--success); }
  .badge-revert  { background: color-mix(in srgb, var(--error) 22%, transparent);    color: var(--error); }

  /* ── Last-refresh badge ────────────────��────────────────────────────── */
  .last-refresh {
    display: flex;
    align-items: center;
    gap: 3px;
    font-size: 11px;
    color: var(--text-disabled);
    padding: 0 5px;
    white-space: nowrap;
    user-select: none;
  }

  .spacer { flex: 1; }

  .no-repo { color: var(--text-disabled); font-size: var(--font-size-sm); }

  .status-sep {
    width: 1px;
    height: 16px;
    background: var(--border);
    flex-shrink: 0;
    margin: 0 4px;
  }

  .repo-path {
    font-size: 11px;
    color: var(--text-muted);
    max-width: 340px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: rtl;        /* show tail of path when truncated */
    background: transparent;
    border: none;
    cursor: pointer;
    padding: 0 4px;
    border-radius: var(--radius-sm);
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .repo-path:hover { background: rgba(255,255,255,0.06); color: var(--text-muted); }

  /* ── Jobs badge ─────────────────────────────────────────────────────────── */
  .job-badge {
    display: flex;
    align-items: center;
    gap: 4px;
    height: 100%;
    padding: 0 10px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    font-weight: 500;
    cursor: pointer;
    white-space: nowrap;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .job-badge:hover { background: rgba(255,255,255,0.06); color: var(--text-primary); }

  .job-badge-running {
    color: var(--accent);
  }

  .job-badge-idle {
    color: var(--text-muted);
  }

  .job-spinner {
    display: flex;
    align-items: center;
    animation: spin 1.2s linear infinite;
  }

  .job-done-dot {
    font-size: 8px;
    color: var(--success);
  }


  /* ── Notifications badge ────────────────────────────────────────────────── */
  .notif-badge {
    display: flex;
    align-items: center;
    gap: 3px;
    height: 100%;
    padding: 0 10px;
    background: transparent;
    border: none;
    border-left: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    font-weight: 500;
    cursor: pointer;
    white-space: nowrap;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .notif-badge:hover { background: rgba(255,255,255,0.06); color: var(--text-primary); }

  .notif-badge-has { color: var(--accent); }

  .notif-count {
    font-size: 11px;
    font-weight: 700;
    background: var(--accent);
    color: var(--text-on-accent);
    border-radius: var(--radius-md);
    padding: 0 4px;
    min-width: 16px;
    text-align: center;
    line-height: 16px;
  }

  /* ── Plugin-contributed status bar items ──────────────────────────────────
     Shared shape for both left and right segments. */
  .plugin-status-item {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 0 6px;
    height: 100%;
    font-size: 11px;
    color: var(--text-secondary);
    user-select: none;
  }
  .plugin-status-clickable {
    background: transparent;
    border: none;
    cursor: pointer;
    color: var(--text-secondary);
  }
  .plugin-status-clickable:hover { background: var(--bg-hover); color: var(--text-primary); }
  .plugin-color-info    { color: var(--accent); }
  .plugin-color-success { color: var(--diff-add-strong, #4ade80); }
  .plugin-color-warning { color: #f59e0b; }
  .plugin-color-error   { color: var(--diff-del-strong, #f87171); }
  .plugin-color-muted   { color: var(--text-muted); }
  .plugin-color-accent  { color: var(--accent); }

  /* ── Security badge ─────────────────────────────────────────────────────
     IntelliJ-style icon-only chip with a tiny corner count badge. Mirrors
     the dimensions of the notifications bell so they line up. */
  .security-badge {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    padding: 0 10px;
    background: transparent;
    border: none;
    border-right: 1px solid var(--border);
    color: var(--text-secondary);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .security-badge:hover { background: rgba(255,255,255,0.06); color: var(--text-primary); }
  .security-badge-has    { color: var(--text-primary); }
  .security-badge-alert  { color: var(--severity-high); }

  /* Wrapper that anchors the badge directly to the icon (so the badge
     overlays the icon itself, not the chip's outer padding box). The
     fixed 14×14 size matches the icon exactly — without it, `inline-flex`
     could stretch the wrap and push the "50% center" past the icon. */
  .security-icon-wrap {
    position: relative;
    display: block;
    width: 14px;
    height: 14px;
    flex: 0 0 auto;
    line-height: 0;
  }
  .security-icon-wrap > :global(svg) {
    display: block;
    width: 14px;
    height: 14px;
  }
  .security-count {
    position: absolute;
    /* Centered horizontally above the icon — sits directly on top of it,
       not displaced to the right of the chip. */
    top: -9px;
    left: 50%;
    transform: translateX(-50%);
    min-width: 14px;
    height: 14px;
    padding: 0 3px;
    border-radius: 7px;
    background: var(--severity-critical);
    color: #fff;
    font-size: 9px;
    font-weight: 700;
    line-height: 14px;
    text-align: center;
    font-variant-numeric: tabular-nums;
    pointer-events: none;
    box-shadow: 0 0 0 1.5px var(--bg-base);
  }
  .security-badge:not(.security-badge-alert) .security-count {
    background: var(--accent);
  }
</style>
