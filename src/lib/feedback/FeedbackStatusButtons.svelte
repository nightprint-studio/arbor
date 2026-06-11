<script lang="ts">
  /**
   * FeedbackStatusButtons — the right-cluster status badges shared by Arbor's
   * StatusBar and the Grove footer: a jobs badge (running / finished count), an
   * optional minimized-dialogs (parked modals) badge, and a notifications bell.
   * Each click toggles the matching overlay rendered by <FeedbackHost>.
   *
   * Reads the per-window feedback stores, so it reflects whichever window it is
   * mounted in (main shows untagged + main-targeted items; grove shows
   * grove-targeted ones). Renders three bare buttons so it drops directly into
   * a flex status row; the `spin` keyframe comes from app.css (global).
   */
  import { Loader, Bell, Minimize2 } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { jobsStore } from '$lib/feedback/stores/jobs.svelte';
  import { notificationsStore } from '$lib/feedback/stores/notifications.svelte';
  import { parkedModalsStore } from '$lib/stores/parked-modals.svelte';

  /** Show the minimized-dialogs badge (main window only — grove has no parked
   *  modals). */
  let { parked = false }: { parked?: boolean } = $props();

  const runningJobs = $derived(jobsStore.runningCount);
  const totalJobs   = $derived(jobsStore.runningCount + jobsStore.finishedCount);
  const parkedCount = $derived(parkedModalsStore.count);
</script>

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
