<script lang="ts">
  import { fly, fade } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { Layers, X, AlertCircle, CheckCircle2, ChevronRight } from 'lucide-svelte';
  import { animStore } from '$lib/stores/animations.svelte';
  import { linkedWorktreesStore } from '$lib/stores/linkedWorktrees.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { notificationsStore } from '$lib/stores/notifications.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  // Mirror every sync result into the persistent notifications overlay so
  // the user has a history with click-to-open-manager.  The transient toast
  // below auto-dismisses (longer than a regular toast: ~15 s baseline, or
  // 25 s when there are conflicts/errors so the user has time to react).
  let lastNotifiedSummaryId: string | null = null;
  let dismissTimer: ReturnType<typeof setTimeout> | null = null;

  function clearTimer() {
    if (dismissTimer) { clearTimeout(dismissTimer); dismissTimer = null; }
  }

  $effect(() => {
    const s = linkedWorktreesStore.latestSummary;
    if (!s) { clearTimer(); return; }

    // De-dupe in case the effect fires twice on the same value.
    const signature = `${s.link_id}::${s.target_branch}::${s.results.length}::${s.initiator_repo_id}`;
    if (signature !== lastNotifiedSummaryId) {
      lastNotifiedSummaryId = signature;

      // Initiator was successfully checked out before the orchestrator ran,
      // so it counts as updated.  s.results contains the OTHER members only.
      const updated   = s.results.filter(r => r.status.kind === 'updated').length + 1;
      const total     = s.results.length + 1;
      const conflicts = s.results.filter(r => r.status.kind === 'conflict').length;
      const errors    = s.results.filter(r => r.status.kind === 'error').length;
      const level: 'info' | 'success' | 'warning' | 'error' =
        errors > 0 ? 'error' :
        conflicts > 0 ? 'warning' :
        updated > 0 ? 'success' : 'info';
      notificationsStore.add(
        `Sync · ${s.link_name}`,
        `Checked out ${s.target_branch} · ${updated}/${total} member${total === 1 ? '' : 's'} updated`
          + (conflicts > 0 ? ` · ${conflicts} conflict${conflicts === 1 ? '' : 's'}` : '')
          + (errors > 0    ? ` · ${errors} error${errors === 1 ? '' : 's'}` : ''),
        level,
        undefined,
        { kind: 'open-link-manager', label: 'View link', link_id: s.link_id },
      );
    }

    // Schedule auto-dismiss.  Pause via hover handlers below.
    clearTimer();
    const issues   = s.results.some(r => r.status.kind === 'conflict' || r.status.kind === 'error');
    const duration = issues ? 25_000 : 15_000;
    dismissTimer = setTimeout(() => linkedWorktreesStore.dismissSummary(), duration);
    return clearTimer;
  });

  const summary = $derived(linkedWorktreesStore.latestSummary);
  // +1 because the initiator was already checked out before sync ran and is
  // not included in `summary.results` (which only covers the other members).
  const updatedCount = $derived(
    summary ? summary.results.filter(r => r.status.kind === 'updated').length + 1 : 0,
  );
  const totalMembers = $derived(summary ? summary.results.length + 1 : 0);
  const skippedCount = $derived(
    summary ? summary.results.filter(r => r.status.kind === 'skipped_missing' || r.status.kind === 'skipped').length : 0,
  );
  const conflictCount = $derived(
    summary ? summary.results.filter(r => r.status.kind === 'conflict').length : 0,
  );
  const errorCount = $derived(
    summary ? summary.results.filter(r => r.status.kind === 'error').length : 0,
  );
  const kind = $derived<'success' | 'warning' | 'error'>(
    errorCount > 0    ? 'error'   :
    conflictCount > 0 ? 'warning' :
    'success',
  );

  function openManager() {
    if (!summary) return;
    uiStore.openLinkManager(summary.link_id);
    linkedWorktreesStore.dismissSummary();
  }
  function dismiss() { linkedWorktreesStore.dismissSummary(); }

  // Pause the auto-dismiss while the user hovers — same UX as standard
  // toasts.  Re-arm the same total-time on leave.
  function onEnter() { clearTimer(); }
  function onLeave() {
    if (!summary) return;
    const issues   = summary.results.some(r => r.status.kind === 'conflict' || r.status.kind === 'error');
    dismissTimer = setTimeout(() => linkedWorktreesStore.dismissSummary(), issues ? 8_000 : 4_000);
  }
</script>

{#if summary}
  <!-- svelte-ignore a11y_mouse_events_have_key_events -->
  <div
    class="summary kind-{kind}"
    role="status"
    aria-live="polite"
    onmouseenter={onEnter}
    onmouseleave={onLeave}
    in:fly={{ y: 12, duration: animStore.dPanel, easing: cubicOut }}
    out:fade={{ duration: animStore.dBase }}
  >
    <span class="stripe" aria-hidden="true"></span>
    <span class="icon"><Layers size={14}/></span>
    <div class="content">
      <div class="title">
        <strong>{summary.link_name}</strong>
        <span class="dim">·</span>
        <span class="branch">{summary.target_branch}</span>
      </div>
      <div class="stats">
        <span class="stat ok">
          <CheckCircle2 size={11}/> {updatedCount}/{totalMembers} updated
        </span>
        {#if skippedCount > 0}
          <span class="stat muted">{skippedCount} skipped</span>
        {/if}
        {#if conflictCount > 0}
          <span class="stat warn"><AlertCircle size={11}/> {conflictCount} conflict{conflictCount === 1 ? '' : 's'}</span>
        {/if}
        {#if errorCount > 0}
          <span class="stat err"><AlertCircle size={11}/> {errorCount} error{errorCount === 1 ? '' : 's'}</span>
        {/if}
      </div>
    </div>
    <button class="details-btn" onclick={openManager} use:tooltip={'Open link details'}>
      Details <ChevronRight size={11} />
    </button>
    <button class="dismiss-btn" onclick={dismiss} aria-label="Dismiss">
      <X size={11}/>
    </button>
  </div>
{/if}

<style>
  /* Same visual language as the new Toast component: dark glass card +
     coloured stripe.  Sits inside .bottom-right-stack so it never overlaps
     other toasts. */
  .summary {
    position: relative;
    display: flex; align-items: center; gap: 10px;
    padding: 10px 12px 10px 16px;
    border-radius: var(--radius-lg);
    /* `backdrop-filter: blur()` removed — see Modal.svelte. Bumped to fully
       opaque for the same reason. */
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    box-shadow:
      0 1px 0 0 rgba(255, 255, 255, 0.04) inset,
      0 8px 24px rgba(0, 0, 0, 0.32),
      0 1px 3px rgba(0, 0, 0, 0.2);
    color: var(--text-primary);
    min-width: 320px;
    max-width: 480px;
    overflow: hidden;
  }

  .stripe {
    position: absolute;
    inset: 0 auto 0 0;
    width: 3px;
    border-radius: 2px;
  }
  .kind-success .stripe { background: var(--success); }
  .kind-warning .stripe { background: var(--warning); }
  .kind-error   .stripe { background: var(--error); }

  .icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    width: 22px; height: 22px;
    border-radius: var(--radius-md);
    background: var(--bg-overlay);
    color: var(--accent);
  }
  .kind-success .icon { color: var(--success); }
  .kind-warning .icon { color: var(--warning); }
  .kind-error   .icon { color: var(--error); }

  .content { flex: 1; min-width: 0; }

  .title {
    font-size: 12.5px;
    font-weight: 500;
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .title strong { font-weight: 600; }
  .title .dim { color: var(--text-disabled); margin: 0 2px; }
  .title .branch {
    font-family: var(--font-code);
    font-size: 11.5px;
    color: var(--accent);
  }

  .stats {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
    margin-top: 4px;
    font-size: 10.5px;
  }
  .stat {
    display: inline-flex; align-items: center; gap: 3px;
    padding: 1px 6px;
    border-radius: 999px;
    background: var(--bg-overlay);
    color: var(--text-secondary);
    line-height: 14px;
  }
  .stat.ok    { background: color-mix(in srgb, var(--success) 16%, transparent); color: var(--success); }
  .stat.warn  { background: color-mix(in srgb, var(--warning) 16%, transparent); color: var(--warning); }
  .stat.err   { background: color-mix(in srgb, var(--error) 16%, transparent); color: var(--error); }
  .stat.muted { color: var(--text-muted); }

  .details-btn {
    display: inline-flex; align-items: center; gap: 3px;
    padding: 4px 9px;
    border-radius: var(--radius-md);
    background: var(--accent-subtle);
    border: 1px solid color-mix(in srgb, var(--accent) 50%, transparent);
    color: var(--accent);
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
    flex-shrink: 0;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .details-btn:hover { background: var(--accent); color: var(--text-on-accent); }

  .dismiss-btn {
    display: inline-flex; align-items: center; justify-content: center;
    width: 20px; height: 20px;
    border-radius: 5px;
    background: transparent; border: none;
    color: var(--text-disabled); cursor: pointer;
    flex-shrink: 0;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .dismiss-btn:hover { background: var(--bg-overlay); color: var(--text-primary); }
</style>
