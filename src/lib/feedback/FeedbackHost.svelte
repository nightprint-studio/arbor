<script lang="ts">
  /**
   * FeedbackHost — the single mountable surface for Arbor's user-feedback
   * systems (toasts, notifications, operations, jobs). Any window mounts ONE
   * with its own `id`; the host wires the backend listeners, filters incoming
   * items by routing target, and renders the bottom-right feed + the bell
   * archive. The main window passes `main` so it also adopts untagged items.
   *
   * Backend job / notification / operation events broadcast to EVERY window;
   * the host filters them at ingest via `makeAccepts(id, main)`, so each
   * window's stores only ever hold its own items and the downstream widgets
   * (JobsOverlay, StatusBar, OperationsOverlay, the bell) need no routing
   * logic of their own.
   *
   * `children` renders at the top of the bottom-right column — the main window
   * uses it to inject its linked-worktree sync summary; other windows omit it.
   */
  import { onMount, type Snippet } from 'svelte';
  import { fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { animStore } from '$lib/stores/animations.svelte';
  import { setupTauriListeners } from '$lib/utils/tauri-listeners';
  import { makeAccepts } from '$lib/feedback/routing';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { notificationsStore, type NotificationAction } from '$lib/feedback/stores/notifications.svelte';
  import { jobsStore } from '$lib/feedback/stores/jobs.svelte';
  import { setupOperationBridge } from '$lib/feedback/bridge/operations-bridge';
  import ToastItem from '$lib/components/shared/Toast.svelte';
  import NotificationItem from '$lib/components/shared/NotificationItem.svelte';
  import NotificationsOverlay from '$lib/components/shared/NotificationsOverlay.svelte';
  import OperationsOverlay from '$lib/components/shared/OperationsOverlay.svelte';
  import JobsOverlay from '$lib/components/jobs/JobsOverlay.svelte';

  let { id, main = false, children }: { id: string; main?: boolean; children?: Snippet } = $props();

  const accepts = makeAccepts(id, main);

  // Unified bottom-right feed: toasts + freshly-added notifications, oldest
  // first (new ones append just above the operations zone). Only transient
  // notifications appear here; the full archive lives in the bell overlay.
  type FeedItem =
    | { kind: 'toast';        key: string; ts: number; value: typeof toastStore.toasts[number] }
    | { kind: 'notification'; key: string; ts: number; value: typeof notificationsStore.notifications[number] };
  const feedItems = $derived.by<FeedItem[]>(() => {
    const out: FeedItem[] = [];
    for (const t of toastStore.toasts)
      out.push({ kind: 'toast',        key: `t:${t.id}`, ts: t.addedAt,  value: t });
    for (const n of notificationsStore.transient)
      out.push({ kind: 'notification', key: `n:${n.id}`, ts: n.timestamp, value: n });
    out.sort((a, b) => a.ts - b.ts);
    return out;
  });

  onMount(() => {
    // Plugin notification + legacy-toast events (broadcast → routed here).
    const unlistenNotify = setupTauriListeners([
      {
        event: 'plugin:toast',
        handler: (e: { payload: { plugin: string; message: string; level: string; target?: string | null } }) => {
          if (!accepts(e.payload.target)) return;
          const { plugin, message, level } = e.payload;
          toastStore.show(`[${plugin}] ${message}`, (level as any) ?? 'info');
        },
      },
      {
        // In-app notification center (arbor.notify). A persisted notification
        // already surfaces transiently in the feed via `transient`, so we only
        // fall back to a toast when the caller opted OUT of persistence —
        // that's the one mode where a transient pop is the only channel.
        event: 'plugin:notification',
        handler: (e: { payload: { plugin: string; title: string; message: string; level: string; toast?: boolean; persist?: boolean; action?: NotificationAction; target?: string | null } }) => {
          const p = e.payload;
          if (!accepts(p.target)) return;
          const showToast   = p.toast   !== false;
          const persistBell = p.persist !== false;
          if (persistBell) {
            notificationsStore.add(p.title, p.message, (p.level as any) ?? 'info', p.plugin, p.action);
          } else if (showToast) {
            const msg = p.message ? `${p.title} — ${p.message}` : p.title;
            toastStore.show(msg, (p.level as any) ?? 'info', 5000);
          }
        },
      },
    ]);

    // Jobs: registry mirror + initial list, both routed by `accepts`.
    const unlistenJobs = jobsStore.setupListeners(accepts);
    jobsStore.load(accepts);

    // Operations overlay bridge (pull / workspace bulk / linked-WT / plugin ops).
    const unlistenOps = setupOperationBridge(accepts);

    return () => {
      unlistenNotify();
      unlistenJobs();
      unlistenOps();
    };
  });
</script>

<!-- Bottom-right unified feedback column. A single fixed-positioned column
     avoids the cross-overlay overlap we used to fight by chasing z-index:
       1. host-provided header (e.g. linked-worktree sync summary)
       2. toasts + notifications, interleaved chronologically (oldest first)
       3. operation cards — anchored at the bottom, above the status bar -->
<div class="bottom-right-stack" aria-live="polite" aria-atomic="false">
  {@render children?.()}
  {#each feedItems as item (item.key)}
    {#if item.kind === 'toast'}
      <ToastItem toast={item.value} />
    {:else}
      <NotificationItem notif={item.value} alwaysShowDismiss />
    {/if}
  {/each}
  <OperationsOverlay />
</div>

<!-- Notifications archive overlay (toggleable bell panel — full history;
     new notifications also live transiently in the stack above). -->
{#if uiStore.notificationsOverlayOpen}
  <div transition:fly={{ y: 10, duration: animStore.dBase, easing: cubicOut }}>
    <NotificationsOverlay />
  </div>
{/if}

<!-- Jobs overlay (floating above the status bar / footer). -->
{#if uiStore.jobsOverlayOpen}
  <div transition:fly={{ y: 10, duration: animStore.dBase, easing: cubicOut }}>
    <JobsOverlay />
  </div>
{/if}

<style>
  .bottom-right-stack {
    position: fixed;
    bottom: 36px;
    right: 16px;
    z-index: 800;
    display: flex;
    flex-direction: column; /* header on top, toasts below; new toasts append at the bottom */
    align-items: flex-end;
    gap: 8px;
    pointer-events: none;
    max-width: calc(100vw - 32px);
  }
  .bottom-right-stack > :global(*) { pointer-events: auto; }
</style>
