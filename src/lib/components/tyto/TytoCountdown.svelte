<script lang="ts">
  /**
   * TytoCountdown — the pre-recording 3-2-1, rendered IN the live Tyto window.
   *
   * Unlike a separate overlay window (which a WebView2 would recreate each time → a white
   * flash), this is a `position:fixed` surface inside the Tyto window while
   * `recorderStore.countingDown`. The window is already grown to cover the target monitor
   * with a frozen backdrop up; here we dim that backdrop and animate the digit the store
   * drives. Esc aborts (the store restores the full panel). Zero window creation, all OS.
   */
  import { recorderStore } from '$lib/stores/tyto/recorder.svelte';

  const value = $derived(recorderStore.countdownValue);
  const total = $derived(Math.max(1, recorderStore.countdownSecs));
  const progress = $derived((total - value) / total);

  // Ring geometry (matches the app's countdown visual language).
  const R = 118;
  const CIRC = 2 * Math.PI * R;
  const dashoffset = $derived(CIRC * (1 - progress));

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') { e.preventDefault(); recorderStore.cancelCountdown(); }
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="overlay">
  {#if recorderStore.selectFrozenUrl}
    <img class="frozen" src={recorderStore.selectFrozenUrl} alt="" draggable="false" />
  {/if}
  <div class="scrim"></div>

  <div class="center">
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

      {#if value > 0}
        {#key value}
          <div class="num">{value}</div>
        {/key}
      {/if}
    </div>

    <div class="label">
      <span class="rec-dot"></span>
      Recording starts…
    </div>
    <button type="button" class="cancel" onclick={() => recorderStore.cancelCountdown()}>Cancel · Esc</button>
  </div>
</div>

<style>
  .overlay {
    position: fixed; inset: 0;
    z-index: 2;
    overflow: hidden;
    user-select: none; -webkit-user-select: none;
    background: #000;
  }
  .frozen {
    position: absolute; inset: 0;
    width: 100%; height: 100%;
    object-fit: fill;
    pointer-events: none; -webkit-user-drag: none;
  }
  /* Dim the frozen desktop + a soft red vignette so the digit reads as "recording soon". */
  .scrim {
    position: absolute; inset: 0;
    background:
      radial-gradient(120% 120% at 50% 42%, color-mix(in srgb, var(--error) 22%, transparent) 0%, transparent 55%),
      rgba(0, 0, 0, 0.5);
  }

  .center {
    position: absolute; inset: 0;
    display: flex; flex-direction: column;
    align-items: center; justify-content: center;
    gap: 18px;
    color: var(--text-primary, #f3f5f9);
  }

  .ring-wrap {
    position: relative;
    width: 236px; height: 236px;
    display: flex; align-items: center; justify-content: center;
  }
  .ring { position: absolute; inset: 0; width: 100%; height: 100%; transform: rotate(-90deg); }
  .ring-track { fill: none; stroke: color-mix(in srgb, var(--text-primary) 14%, transparent); stroke-width: 6; }
  .ring-fill {
    fill: none;
    stroke: var(--error, #e5484d);
    stroke-width: 6; stroke-linecap: round;
    transition: stroke-dashoffset 0.95s linear;
    filter: drop-shadow(0 0 6px color-mix(in srgb, var(--error) 55%, transparent));
  }

  .num {
    font-size: 132px; line-height: 1; font-weight: 800;
    font-variant-numeric: tabular-nums;
    color: #fff;
    text-shadow: 0 6px 24px rgba(0, 0, 0, 0.5);
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
    color: var(--text-secondary, #cbd5e1);
  }
  .rec-dot {
    width: 9px; height: 9px; border-radius: 50%;
    background: var(--error, #e5484d);
    animation: blink 1.3s ease-in-out infinite;
  }
  @keyframes blink { 0%, 100% { opacity: 1; } 50% { opacity: 0.25; } }

  .cancel {
    border: 1px solid var(--border, #333);
    background: var(--bg-input, rgba(0, 0, 0, 0.35));
    color: var(--text-secondary, #cbd5e1);
    font-size: 11.5px; font-weight: 600;
    padding: 5px 12px; border-radius: 999px; cursor: pointer;
    transition: background var(--transition-fast, 0.12s), color var(--transition-fast, 0.12s);
  }
  .cancel:hover { background: var(--error, #e5484d); color: #fff; border-color: var(--error, #e5484d); }
</style>
