<!--
  BootSplash — full-screen overlay shown while the host loads plugins
  AND while the frontend restores the active workspace's open tabs.

  Subscribes to:
    · `arbor://boot-progress` — { phase, name, current, total, message }
    · `arbor://boot-done`     — host finished loading plugins.
    · `tabsStore.isInitializing` / `tabsStore.initProgress` — frontend
      tab-restore phase (driven by `workspaces.svelte.ts`).

  The splash stays visible until BOTH the host has fired `boot-done` AND
  the frontend has finished opening repositories.  This way the user
  goes straight from splash → ready UI, instead of splash → spinner →
  ready (the old `init-overlay` in AppShell).

  Fade-in / fade-out are driven by Svelte's `transition:fade` so the
  exit animation actually plays before the node is unmounted, and honours
  the user's animation-speed setting via `animStore.dSlow` (CSS vars
  `--anim-dur-*` cover the inner keyframes).

  There's also a safety timeout (15s) that hides the overlay even if
  either phase never reports completion — better to land in a half-loaded
  UI than to permanently strand the user behind a spinner.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { fade } from 'svelte/transition';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { invoke } from '@tauri-apps/api/core';
  import ArborLogo from './internal/ArborLogo.svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { animStore } from '$lib/stores/animations.svelte';

  interface BootProgress {
    phase:    string;
    name?:    string;
    current?: number;
    total?:   number;
    message?: string;
  }

  let visible      = $state(true);
  /** Set as soon as we start the dismiss so the splash stops eating
   *  pointer events while Svelte plays the fade-out. */
  let dismissing   = $state(false);
  let hostProgress = $state<BootProgress>({
    phase:   'booting',
    message: 'Starting Arbor…',
    current: 0,
    total:   0,
  });
  /** True once `arbor://boot-done` (or the polled snapshot) has fired. */
  let hostDone     = $state(false);

  // Fallback hide — never strand the user behind the splash if either
  // phase forgets to report completion. 15s covers worst-case plugin load
  // plus a workspace with many repos.
  let fallbackTimer: number | null = null;

  function tryDismiss() {
    // Both gates must close before we fade out the overlay.
    if (!hostDone || tabsStore.isInitializing) return;
    if (dismissing) return;
    dismissing = true;
    // `transition:fade` on the root takes care of the actual fade-out —
    // we just flip `visible` and Svelte holds the node in the DOM until
    // the outro completes, then unmounts it.
    visible = false;
    if (fallbackTimer !== null) {
      window.clearTimeout(fallbackTimer);
      fallbackTimer = null;
    }
  }

  // Re-check the dismiss gate whenever the tabs-init flag flips.
  $effect(() => {
    // Read both reactive signals so this $effect re-runs on either change.
    void tabsStore.isInitializing;
    tryDismiss();
  });

  onMount(() => {
    const unlistens: UnlistenFn[] = [];
    let stopped = false;

    listen<BootProgress>('arbor://boot-progress', (ev) => {
      if (stopped) return;
      const p = ev.payload;
      if (p) hostProgress = { ...hostProgress, ...p };
    }).then((u) => { if (!stopped) unlistens.push(u); else u(); });

    listen<unknown>('arbor://boot-done', () => {
      hostDone = true;
      tryDismiss();
    }).then((u) => { if (stopped) u(); else unlistens.push(u); });

    // Recovery for the dev-mode race: in `tauri dev` the WebView mounts
    // *after* the boot thread has often already fired `arbor://boot-done`,
    // so our listener above never sees it and we'd sit through the 15s
    // fallback. Poll the backend's mirrored state instead — if done, mark
    // the host gate closed; otherwise seed the last-known progress payload
    // so the splash doesn't show a stale "Starting Arbor…" while loading
    // is well underway.
    invoke<{ done: boolean; progress?: BootProgress | null }>('get_boot_state')
      .then((snap) => {
        if (stopped) return;
        if (snap?.progress) hostProgress = { ...hostProgress, ...snap.progress };
        if (snap?.done) { hostDone = true; tryDismiss(); }
      })
      .catch(() => { /* command unavailable on first dev rebuild — ignore */ });

    fallbackTimer = window.setTimeout(() => {
      if (!dismissing) {
        console.warn('[BootSplash] boot timeout reached — hiding splash');
        // Force-close both gates so tryDismiss() proceeds.
        hostDone = true;
        if (tabsStore.isInitializing) tabsStore.endInit();
        tryDismiss();
      }
    }, 15_000);

    return () => {
      stopped = true;
      for (const u of unlistens) u();
      if (fallbackTimer !== null) window.clearTimeout(fallbackTimer);
    };
  });

  // While the host is still loading plugins, the host progress drives the
  // bar.  Once `boot-done` has fired we hand over to the frontend
  // tab-restore progress (if any).  This gives the user a single, coherent
  // "preparing your workspace" view instead of two consecutive spinners.
  const progress = $derived.by<BootProgress>(() => {
    const tp = tabsStore.initProgress;
    if (hostDone && (tp || tabsStore.isInitializing)) {
      return {
        phase:   'tabs',
        current: tp?.current ?? 0,
        total:   tp?.total   ?? 0,
        message: tp?.message ?? 'Loading repositories…',
      };
    }
    return hostProgress;
  });

  const pct = $derived.by<number | null>(() => {
    const c = progress.current ?? 0;
    const t = progress.total   ?? 0;
    if (t <= 0) return null;
    return Math.min(100, Math.max(0, (c / t) * 100));
  });

  const displayMessage = $derived(progress.message ?? 'Loading…');
</script>

{#if visible}
  <div
    class="boot-splash"
    class:dismissing
    role="status"
    aria-live="polite"
    aria-busy={!dismissing}
    transition:fade={{ duration: animStore.dSlow }}
  >
    <!-- Decorative animated background mesh -->
    <div class="boot-mesh" aria-hidden="true">
      <div class="boot-orb boot-orb-1"></div>
      <div class="boot-orb boot-orb-2"></div>
      <div class="boot-orb boot-orb-3"></div>
    </div>

    <div class="boot-stage">
      <div class="boot-mark">
        <div class="boot-halo" aria-hidden="true"></div>
        <div class="boot-logo">
          <ArborLogo size={64} />
        </div>
      </div>

      <div class="boot-wordmark">Arbor</div>
      <div class="boot-tagline">Git workspace</div>

      <div class="boot-progress-area">
        {#if progress.total && progress.total > 0}
          <div class="boot-track" aria-hidden="true">
            <div
              class="boot-bar"
              style="width: {pct ?? 0}%"
            >
              <div class="boot-bar-shimmer"></div>
            </div>
          </div>
          <div class="boot-meta">
            <span class="boot-message" title={displayMessage}>{displayMessage}</span>
            <span class="boot-count">{progress.current ?? 0}<span class="boot-count-sep">/</span>{progress.total}</span>
          </div>
        {:else}
          <div class="boot-track boot-track-indeterminate" aria-hidden="true">
            <div class="boot-bar-indeterminate"></div>
          </div>
          <div class="boot-meta">
            <span class="boot-message" title={displayMessage}>{displayMessage}</span>
          </div>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  /* ── Overlay container ──────────────────────────────────────────── */
  .boot-splash {
    position: fixed;
    inset: 0;
    z-index: 99999;
    display: flex;
    align-items: center;
    justify-content: center;
    background:
      radial-gradient(
        ellipse 80% 60% at 50% 35%,
        color-mix(in srgb, var(--bg-elevated) 95%, transparent) 0%,
        var(--bg-base) 65%,
        color-mix(in srgb, var(--bg-base) 92%, #000 8%) 100%
      );
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    user-select: none;
    -webkit-user-select: none;
    overflow: hidden;
    /* fade-in / fade-out handled by Svelte transition:fade */
  }
  .boot-splash.dismissing {
    pointer-events: none;
  }

  /* ── Decorative animated mesh ───────────────────────────────────── */
  .boot-mesh {
    position: absolute;
    inset: 0;
    pointer-events: none;
    overflow: hidden;
  }
  .boot-orb {
    position: absolute;
    border-radius: 50%;
    filter: blur(80px);
    opacity: 0.55;
    will-change: transform;
  }
  .boot-orb-1 {
    width:  520px;
    height: 520px;
    left:   -120px;
    top:    -160px;
    background: radial-gradient(circle at 30% 30%,
      color-mix(in srgb, var(--accent) 70%, transparent) 0%,
      transparent 70%);
    animation: boot-orb-drift-1 14s ease-in-out infinite;
  }
  .boot-orb-2 {
    width:  460px;
    height: 460px;
    right:  -140px;
    bottom: -120px;
    background: radial-gradient(circle at 70% 70%,
      color-mix(in srgb, var(--accent) 50%, var(--info) 50%) 0%,
      transparent 70%);
    opacity: 0.35;
    animation: boot-orb-drift-2 18s ease-in-out infinite;
  }
  .boot-orb-3 {
    width:  340px;
    height: 340px;
    left:   55%;
    top:    65%;
    background: radial-gradient(circle at 50% 50%,
      color-mix(in srgb, var(--accent) 35%, transparent) 0%,
      transparent 70%);
    opacity: 0.4;
    animation: boot-orb-drift-3 22s ease-in-out infinite;
  }

  @keyframes boot-orb-drift-1 {
    0%, 100% { transform: translate(0, 0) scale(1); }
    50%      { transform: translate(40px, 60px) scale(1.08); }
  }
  @keyframes boot-orb-drift-2 {
    0%, 100% { transform: translate(0, 0) scale(1); }
    50%      { transform: translate(-50px, -30px) scale(1.06); }
  }
  @keyframes boot-orb-drift-3 {
    0%, 100% { transform: translate(0, 0) scale(1); }
    50%      { transform: translate(30px, -50px) scale(1.1); }
  }

  /* ── Main stage column ──────────────────────────────────────────── */
  .boot-stage {
    position: relative;
    z-index: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0;
    padding: 0 var(--space-8);
    min-width: 360px;
    max-width: 520px;
    animation: boot-stage-rise var(--anim-dur-slow, 240ms) cubic-bezier(0.16, 1, 0.3, 1) both;
  }

  /* ── Logo + pulsing halo ────────────────────────────────────────── */
  .boot-mark {
    position: relative;
    width:  96px;
    height: 96px;
    display: flex;
    align-items: center;
    justify-content: center;
    margin-bottom: var(--space-6);
  }
  .boot-halo {
    position: absolute;
    inset: -18px;
    border-radius: 50%;
    background: radial-gradient(circle,
      color-mix(in srgb, var(--accent) 28%, transparent) 0%,
      transparent 70%);
    animation: boot-halo-pulse 2.4s ease-in-out infinite;
  }
  .boot-logo {
    position: relative;
    width:  64px;
    height: 64px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--accent);
    filter: drop-shadow(0 4px 16px color-mix(in srgb, var(--accent) 40%, transparent));
  }

  @keyframes boot-halo-pulse {
    0%, 100% { transform: scale(0.92); opacity: 0.55; }
    50%      { transform: scale(1.08); opacity: 0.95; }
  }

  /* ── Wordmark + tagline ─────────────────────────────────────────── */
  .boot-wordmark {
    font-size: 28px;
    font-weight: 300;
    letter-spacing: 4px;
    text-transform: uppercase;
    color: var(--text-primary);
    background: linear-gradient(180deg,
      var(--text-primary) 0%,
      color-mix(in srgb, var(--text-primary) 70%, transparent) 100%);
    -webkit-background-clip: text;
            background-clip: text;
    -webkit-text-fill-color: transparent;
    line-height: 1;
    margin-bottom: var(--space-3);
  }
  .boot-tagline {
    font-size: 10px;
    font-weight: 500;
    letter-spacing: 2.5px;
    text-transform: uppercase;
    color: var(--text-muted);
    margin-bottom: var(--space-8);
  }

  /* ── Progress area ──────────────────────────────────────────────── */
  .boot-progress-area {
    width: 100%;
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: var(--space-4);
  }
  .boot-track {
    position: relative;
    width: 100%;
    height: 3px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--bg-overlay) 70%, transparent);
    overflow: hidden;
  }
  .boot-bar {
    position: relative;
    height: 100%;
    border-radius: inherit;
    background: linear-gradient(90deg,
      color-mix(in srgb, var(--accent) 65%, transparent) 0%,
      var(--accent) 50%,
      color-mix(in srgb, var(--accent) 80%, var(--accent-hover) 20%) 100%);
    box-shadow: 0 0 12px color-mix(in srgb, var(--accent) 55%, transparent);
    transition: width var(--anim-dur-slow, 240ms) cubic-bezier(0.22, 1, 0.36, 1);
    overflow: hidden;
  }
  .boot-bar-shimmer {
    position: absolute;
    inset: 0;
    background: linear-gradient(90deg,
      transparent 0%,
      color-mix(in srgb, #fff 35%, transparent) 50%,
      transparent 100%);
    animation: boot-shimmer 1.6s ease-in-out infinite;
  }
  @keyframes boot-shimmer {
    0%   { transform: translateX(-100%); }
    100% { transform: translateX(100%); }
  }

  /* Indeterminate variant (when total is unknown). */
  .boot-track-indeterminate {
    background: color-mix(in srgb, var(--bg-overlay) 70%, transparent);
  }
  .boot-bar-indeterminate {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 40%;
    border-radius: inherit;
    background: linear-gradient(90deg,
      transparent 0%,
      var(--accent) 50%,
      transparent 100%);
    animation: boot-indeterminate 1.6s cubic-bezier(0.4, 0, 0.6, 1) infinite;
  }
  @keyframes boot-indeterminate {
    0%   { left: -40%; }
    100% { left: 100%; }
  }

  /* ── Meta row (message + counter) ──────────────────────────────── */
  .boot-meta {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--space-5);
    min-height: 14px;
  }
  .boot-message {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-code);
    font-size: 11px;
    color: var(--text-secondary);
    letter-spacing: 0.2px;
  }
  .boot-count {
    font-family: var(--font-code);
    font-size: 10px;
    font-weight: 600;
    color: var(--text-muted);
    letter-spacing: 0.6px;
    font-variant-numeric: tabular-nums;
    flex-shrink: 0;
  }
  .boot-count-sep {
    margin: 0 2px;
    color: var(--text-disabled);
  }

  /* ── Enter animation (exit is handled by transition:fade) ──────── */
  @keyframes boot-stage-rise {
    from {
      opacity: 0;
      transform: translateY(8px) scale(0.985);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }

  /* ── Reduced-motion friendliness ───────────────────────────────── */
  @media (prefers-reduced-motion: reduce) {
    .boot-splash,
    .boot-splash.dismissing,
    .boot-stage,
    .boot-orb,
    .boot-halo,
    .boot-bar-shimmer,
    .boot-bar-indeterminate {
      animation: none !important;
      transition: none !important;
    }
  }
</style>
