<script lang="ts">
  /**
   * CountdownOverlay — the pre-recording 3-2-1 (window `tyto-countdown`).
   *
   * Self-driven: it pulls the second count from the shell on mount, animates a big
   * centered number once per second, and calls `countdown_finished` when the digits
   * reach zero (the shell records completion + closes this window; the Tyto store,
   * polling, then starts the recording). Opaque + content-protected by the window.
   */
  import { onMount } from 'svelte';
  import { getCountdownInit, countdownFinished } from '$lib/ipc/tyto/countdown-window';
  import { themeStore } from '$lib/stores/theme.svelte';
  import { appearanceStore } from '$lib/stores/appearance.svelte';
  import { animStore } from '$lib/stores/animations.svelte';

  let total = $state(3);
  let current = $state(0); // 0 = not started yet (nothing shown)
  const progress = $derived(total > 0 ? (total - current) / total : 0);

  // Ring geometry.
  const R = 118;
  const CIRC = 2 * Math.PI * R;
  const dashoffset = $derived(CIRC * (1 - progress));

  onMount(() => {
    // Standalone window: apply the app theme/appearance so the overlay matches.
    void themeStore.init();
    void appearanceStore.loadConfig();
    void animStore.loadConfig();
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;

    const tick = () => {
      if (cancelled) return;
      if (current <= 1) {
        // The final digit's second has elapsed — hand back to the shell + store.
        void countdownFinished();
        return;
      }
      current -= 1;
      timer = setTimeout(tick, 1000);
    };

    void (async () => {
      const n = await getCountdownInit().catch(() => null);
      if (cancelled) return;
      total = n && n > 0 ? n : 3;
      current = total;
      timer = setTimeout(tick, 1000);
    })();

    return () => { cancelled = true; if (timer) clearTimeout(timer); };
  });
</script>

<div class="overlay">
  <div class="ring-wrap">
    <svg class="ring" viewBox="0 0 260 260" aria-hidden="true">
      <circle class="ring-track" cx="130" cy="130" r={R} />
      <circle
        class="ring-fill"
        cx="130" cy="130" r={R}
        stroke-dasharray={CIRC}
        stroke-dashoffset={dashoffset}
      />
    </svg>

    {#if current > 0}
      {#key current}
        <div class="num">{current}</div>
      {/key}
    {/if}
  </div>

  <div class="label">
    <span class="rec-dot"></span>
    Recording starts…
  </div>
</div>

<style>
  :global(html), :global(body) { margin: 0; height: 100%; overflow: hidden; background: var(--bg-elevated); }

  .overlay {
    position: fixed; inset: 0;
    display: flex; flex-direction: column;
    align-items: center; justify-content: center;
    gap: 18px;
    background:
      radial-gradient(120% 120% at 50% 40%, color-mix(in srgb, var(--error) 16%, var(--bg-elevated)) 0%, var(--bg-elevated) 62%);
    color: var(--text-primary);
    user-select: none; -webkit-user-select: none;
    -webkit-app-region: drag;
  }

  .ring-wrap {
    position: relative;
    width: 236px; height: 236px;
    display: flex; align-items: center; justify-content: center;
  }
  .ring { position: absolute; inset: 0; width: 100%; height: 100%; transform: rotate(-90deg); }
  .ring-track { fill: none; stroke: color-mix(in srgb, var(--text-primary) 12%, transparent); stroke-width: 6; }
  .ring-fill {
    fill: none;
    stroke: var(--error);
    stroke-width: 6; stroke-linecap: round;
    transition: stroke-dashoffset 0.95s linear;
    filter: drop-shadow(0 0 6px color-mix(in srgb, var(--error) 55%, transparent));
  }

  .num {
    font-size: 132px; line-height: 1; font-weight: 800;
    font-variant-numeric: tabular-nums;
    color: var(--text-primary);
    text-shadow: 0 6px 24px rgba(0, 0, 0, 0.4);
    animation: pop 0.42s cubic-bezier(0.2, 1.3, 0.35, 1) both;
  }
  @keyframes pop {
    0%   { transform: scale(0.55); opacity: 0; }
    45%  { transform: scale(1.06); opacity: 1; }
    100% { transform: scale(1); opacity: 1; }
  }

  .label {
    display: inline-flex; align-items: center; gap: 8px;
    font-size: 13px; font-weight: 600; letter-spacing: 0.3px;
    color: var(--text-secondary);
  }
  .rec-dot {
    width: 9px; height: 9px; border-radius: 50%;
    background: var(--error);
    animation: blink 1.3s ease-in-out infinite;
  }
  @keyframes blink { 0%, 100% { opacity: 1; } 50% { opacity: 0.25; } }
</style>
