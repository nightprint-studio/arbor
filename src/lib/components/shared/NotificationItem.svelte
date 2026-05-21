<script lang="ts">
  import { fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { X, Info, CheckCircle, AlertTriangle, XCircle, ChevronRight } from 'lucide-svelte';
  import { notificationsStore, dispatchNotificationAction } from '$lib/stores/notifications.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { animStore } from '$lib/stores/animations.svelte';
  import type { AppNotification } from '$lib/stores/notifications.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  interface Props {
    notif: AppNotification;
    /** When true the close-button stays visible at all times (used in the
     *  unified bottom-right stack where there's no list-row hover state). */
    alwaysShowDismiss?: boolean;
  }

  let { notif, alwaysShowDismiss = false }: Props = $props();

  async function runAction() {
    if (!notif.action) return;
    await dispatchNotificationAction(notif.action);
    notificationsStore.dismiss(notif.id);
    uiStore.setNotificationsOverlayOpen(false);
  }

  function timeAgo(ts: number): string {
    const secs = Math.floor((Date.now() - ts) / 1000);
    if (secs < 60)   return `${secs}s ago`;
    if (secs < 3600) return `${Math.floor(secs / 60)}m ago`;
    return `${Math.floor(secs / 3600)}h ago`;
  }

  /** Out transition: slide horizontally off-screen AND collapse the row's
   *  height + paddings so the items below close the gap smoothly. The
   *  default `fly` keeps the layout slot reserved until the animation ends,
   *  which made the overlay panel "expand upward" once the row was finally
   *  removed from the DOM — the height collapse here fixes that. */
  function flyAndCollapse(node: HTMLElement, { duration }: { duration: number }) {
    const style = getComputedStyle(node);
    const h     = node.offsetHeight;
    const pt    = parseFloat(style.paddingTop)    || 0;
    const pb    = parseFloat(style.paddingBottom) || 0;
    const mt    = parseFloat(style.marginTop)     || 0;
    const mb    = parseFloat(style.marginBottom)  || 0;
    const bt    = parseFloat(style.borderTopWidth)    || 0;
    const bb    = parseFloat(style.borderBottomWidth) || 0;
    return {
      duration,
      easing: cubicOut,
      css: (t: number, u: number) => `
        transform: translateX(${360 * u}px);
        opacity: ${t};
        max-height: ${t * h}px;
        padding-top: ${t * pt}px;
        padding-bottom: ${t * pb}px;
        margin-top: ${t * mt}px;
        margin-bottom: ${t * mb}px;
        border-top-width: ${t * bt}px;
        border-bottom-width: ${t * bb}px;
        overflow: hidden;
      `,
    };
  }
</script>

<div
  class="notif-row kind-{notif.level}"
  class:always-x={alwaysShowDismiss}
  in:fly|global={{ x: 360, duration: animStore.dPanel, easing: cubicOut }}
  out:flyAndCollapse|global={{ duration: animStore.dPanel }}
>
  <span class="notif-stripe" aria-hidden="true"></span>
  <div class="notif-icon">
    {#if notif.level === 'success'}      <CheckCircle    size={14} />
    {:else if notif.level === 'warning'} <AlertTriangle  size={14} />
    {:else if notif.level === 'error'}   <XCircle        size={14} />
    {:else}                              <Info           size={14} />
    {/if}
  </div>

  <div class="notif-body">
    <div class="notif-title">{notif.title}</div>
    <div class="notif-message">{notif.message}</div>
    <div class="notif-meta">
      <span class="notif-time">{timeAgo(notif.timestamp)}</span>
      {#if notif.plugin}
        <span class="notif-dot" aria-hidden="true"></span>
        <span class="notif-plugin">{notif.plugin}</span>
      {/if}
      {#if notif.action}
        <button class="notif-action" onclick={runAction}>
          {notif.action.label} <ChevronRight size={10} />
        </button>
      {/if}
    </div>
  </div>

  <button
    class="dismiss-btn"
    onclick={() => notificationsStore.dismiss(notif.id)}
    use:tooltip={'Dismiss'}
    aria-label="Dismiss"
  >
    <X size={11} />
  </button>
</div>

<style>
  /* Notification row card — used both in the unified bottom-right stack
     and inside the bell-archive panel.  Same visual language as toasts:
     dark-glass card, 3px coloured stripe on the left, soft shadow. */
  .notif-row {
    position: relative;
    display: flex;
    align-items: flex-start;
    /* Inside any column-flex stack (NotificationsOverlay list, AppShell
       toast feed) rows must keep their natural height — a list with too
       many entries to fit must scroll, not squish each row to a few
       pixels. Without this the title/icon get clipped under `overflow:
       hidden` below. */
    flex-shrink: 0;
    gap: 9px;
    padding: 9px 10px 9px 14px;
    border-radius: var(--radius-md);
    /* See Modal.svelte for the rationale on why `backdrop-filter: blur()`
       is removed across the codebase. Bumped from 95% to fully opaque so
       the diffusion the blur was providing isn't missed. */
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    box-shadow:
      0 1px 0 0 rgba(255, 255, 255, 0.04) inset,
      0 8px 24px rgba(0, 0, 0, 0.32),
      0 1px 3px rgba(0, 0, 0, 0.2);
    overflow: hidden;
    min-width: 240px;
    max-width: 480px;
    transition: border-color var(--transition-fast), background var(--transition-fast);
  }
  .notif-row:hover {
    border-color: var(--border);
    background: var(--bg-elevated);
  }
  .notif-row:hover .dismiss-btn { opacity: 1; }
  .notif-row.always-x .dismiss-btn { opacity: 1; }

  .notif-stripe {
    position: absolute;
    inset: 0 auto 0 0;
    width: 3px;
    border-radius: 2px;
  }
  .kind-info    .notif-stripe { background: var(--accent); }
  .kind-success .notif-stripe { background: var(--success); }
  .kind-warning .notif-stripe { background: var(--warning); }
  .kind-error   .notif-stripe { background: var(--error); }

  .notif-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    width: 22px;
    height: 22px;
    border-radius: var(--radius-md);
    background: var(--bg-overlay);
    margin-top: 1px;
  }
  .kind-info    .notif-icon { color: var(--accent); }
  .kind-success .notif-icon { color: var(--success); }
  .kind-warning .notif-icon { color: var(--warning); }
  .kind-error   .notif-icon { color: var(--error); }

  .notif-body { flex: 1; min-width: 0; }
  .notif-title {
    font-size: 12.5px; font-weight: 600;
    color: var(--text-primary); line-height: 1.3;
    word-break: break-word;
  }
  .notif-message {
    font-size: 11.5px; color: var(--text-secondary);
    margin-top: 3px; line-height: 1.4;
    word-break: break-word;
  }
  .notif-meta { display: flex; gap: 6px; margin-top: 6px; align-items: center; flex-wrap: wrap; }
  .notif-time   { font-size: 10px; color: var(--text-disabled); }
  .notif-dot    { width: 2px; height: 2px; border-radius: 50%; background: var(--text-disabled); }
  .notif-plugin { font-size: 10px; color: var(--text-muted); font-family: var(--font-code); }

  .notif-action {
    margin-left: auto;
    display: inline-flex; align-items: center; gap: 3px;
    padding: 2px 8px;
    border-radius: 999px;
    background: var(--accent-subtle);
    border: 1px solid color-mix(in srgb, var(--accent) 45%, transparent);
    color: var(--accent);
    font-size: 10.5px; font-weight: 500;
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .notif-action:hover { background: var(--accent); color: var(--text-on-accent); }

  .dismiss-btn {
    flex-shrink: 0; opacity: 0;
    width: 20px; height: 20px;
    border-radius: 5px;
    border: none; background: transparent;
    color: var(--text-disabled);
    cursor: pointer;
    display: inline-flex; align-items: center; justify-content: center;
    transition: opacity var(--transition-fast), background var(--transition-fast), color var(--transition-fast);
  }
  .dismiss-btn:hover { background: var(--bg-overlay); color: var(--text-primary); }
</style>
