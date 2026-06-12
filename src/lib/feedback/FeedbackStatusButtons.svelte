<script lang="ts">
  /**
   * FeedbackStatusButtons — the right-cluster status badges shared by Arbor's
   * StatusBar and the Nemus footer: a jobs badge (running / finished count), an
   * optional minimized-dialogs (parked modals) badge, and a notifications bell.
   * Each click toggles the matching overlay rendered by <FeedbackHost>.
   *
   * Reads the per-window feedback stores, so it reflects whichever window it is
   * mounted in (main shows untagged + main-targeted items; nemus shows
   * nemus-targeted ones). Renders three bare buttons so it drops directly into
   * a flex status row; the `spin` keyframe comes from app.css (global).
   */
  import { Loader, Bell, Minimize2, ArrowDownToLine } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { jobsStore } from '$lib/feedback/stores/jobs.svelte';
  import { notificationsStore } from '$lib/feedback/stores/notifications.svelte';
  import { transfersStore } from '$lib/feedback/stores/transfers.svelte';
  import { parkedModalsStore } from '$lib/stores/parked-modals.svelte';

  let {
    /** Show the minimized-dialogs badge (main window only — nemus has no parked
     *  modals). */
    parked = false,
    /** Always show the transfers (downloads/exports) badge, even when idle. Nemus
     *  pins it on; elsewhere the badge only appears while a transfer is live. */
    transfers = false,
  }: { parked?: boolean; transfers?: boolean } = $props();

  const runningJobs = $derived(jobsStore.runningCount);
  const totalJobs   = $derived(jobsStore.runningCount + jobsStore.finishedCount);
  const parkedCount = $derived(parkedModalsStore.count);

  // Transfers (downloads / exports) — only surfaces when something is registered,
  // so the badge stays invisible in windows that don't use it.
  const activeTransfers = $derived(transfersStore.activeCount);
  const showTransfers   = $derived(transfers || transfersStore.hasAny);
</script>

{#if showTransfers}
  <!-- Transfers badge (downloads / exports with progress) -->
  <button
    class="transfer-badge"
    class:transfer-badge-active={activeTransfers > 0}
    use:tooltip={{
      content: activeTransfers > 0
        ? `${activeTransfers} transfer${activeTransfers > 1 ? 's' : ''} in progress`
        : 'Downloads & exports',
      description: 'Click to view',
    }}
    onclick={() => uiStore.toggleTransfersOverlay()}
  >
    {#if activeTransfers > 0}
      <span class="transfer-spinner"><ArrowDownToLine size={12} /></span>
      <span>{activeTransfers}</span>
    {:else}
      <ArrowDownToLine size={12} />
    {/if}
  </button>
{/if}

<!-- Jobs badge (IntelliJ-style) -->
<button
  class="job-badge"
  class:job-badge-running={runningJobs > 0}
  class:job-badge-idle={totalJobs === 0}
  use:tooltip={{
    content: runningJobs > 0
      ? `${runningJobs} job${runningJobs > 1 ? 's' : ''} running`
      : totalJobs > 0 ? 'All jobs finished' : 'No jobs',
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

{#if parked}
  <!-- Minimized dialogs (parked modals) -->
  <button
    class="parked-badge"
    class:parked-badge-has={parkedCount > 0}
    use:tooltip={{
      content: parkedCount > 0
        ? `${parkedCount} minimized dialog${parkedCount > 1 ? 's' : ''}`
        : 'No minimized dialogs',
      description: 'Click to view',
    }}
    onclick={() => uiStore.toggleParkedModalsOverlay()}
  >
    <Minimize2 size={12} />
    {#if parkedCount > 0}
      <span class="parked-count">{parkedCount > 99 ? '99+' : parkedCount}</span>
    {/if}
  </button>
{/if}

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

<style>
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
  .job-badge-running { color: var(--accent); }
  .job-badge-idle    { color: var(--text-muted); }

  /* ── Transfers badge ──────────────────────────────────────────────────────
     Same shape as the jobs badge; the down-arrow gently bobs while active. */
  .transfer-badge {
    display: flex;
    align-items: center;
    gap: 4px;
    height: 100%;
    padding: 0 10px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    font-weight: 500;
    cursor: pointer;
    white-space: nowrap;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .transfer-badge:hover { background: rgba(255,255,255,0.06); color: var(--text-primary); }
  .transfer-badge-active { color: var(--accent); }
  .transfer-spinner { display: flex; align-items: center; animation: transfer-bob 1.1s ease-in-out infinite; }
  @keyframes transfer-bob {
    0%, 100% { transform: translateY(-1px); }
    50%      { transform: translateY(1px); }
  }

  .job-spinner {
    display: flex;
    align-items: center;
    animation: spin 1.2s linear infinite;
  }
  .job-done-dot { font-size: 8px; color: var(--success); }

  /* ── Parked-modals badge ──────────────────────────────────────────────────
     Same shape as the bell so the right cluster reads as a unified group. */
  .parked-badge {
    display: flex;
    align-items: center;
    gap: 4px;
    height: 100%;
    padding: 0 10px;
    background: transparent;
    border: none;
    border-left: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    font-weight: 500;
    cursor: pointer;
    white-space: nowrap;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .parked-badge:hover { background: rgba(255,255,255,0.06); color: var(--text-primary); }
  .parked-badge-has   { color: var(--accent); }
  .parked-count {
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
</style>
