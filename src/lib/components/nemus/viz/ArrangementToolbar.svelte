<script lang="ts">
  /**
   * Arrangement view toolbar — a thin IntelliJ/Logic-style strip of icon
   * toggles above the ruler that drive how the timeline is *drawn* (not its
   * contents). State lives in the viz-local `arrViewOptions` store; every toggle
   * is keyboard reachable (real <button>, aria-pressed) with a tooltip.
   *
   * Sticky-left so it stays in view while the (much wider) timeline scrolls.
   * Imports only nemus-local + the shared tooltip action.
   */
  import { AudioLines, Crosshair, Grid3x3, Tag } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { arrViewOptions as o } from './arr-view-options.svelte';

  const toggles = $derived([
    { key: 'waveform', icon: AudioLines, on: o.waveform, toggle: () => o.toggleWaveform(),
      tip: { content: 'Waveform', description: 'Draw audio regions as a waveform (sample / drum / signal lanes)' } },
    { key: 'follow', icon: Crosshair, on: o.follow, toggle: () => o.toggleFollow(),
      tip: { content: 'Follow playhead', description: 'Auto-scroll to keep the playhead in view while playing' } },
    { key: 'grid', icon: Grid3x3, on: o.grid, toggle: () => o.toggleGrid(),
      tip: { content: 'Grid', description: 'Show the bar grid lines' } },
    { key: 'labels', icon: Tag, on: o.labels, toggle: () => o.toggleLabels(),
      tip: { content: 'Labels', description: 'Show note / sound names on events when they fit' } },
  ] as const);
</script>

<div class="arr-tb" role="toolbar" tabindex="-1" aria-label="Arrangement view options">
  {#each toggles as t (t.key)}
    <button
      class="tb-btn"
      class:on={t.on}
      type="button"
      aria-pressed={t.on}
      aria-label={t.tip.content}
      use:tooltip={t.tip}
      onclick={t.toggle}
    >
      <t.icon size={14} />
    </button>
  {/each}
</div>

<style>
  .arr-tb {
    display: flex;
    align-items: center;
    gap: 2px;
    height: 100%;
    padding: 0 6px;
  }
  .tb-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 22px;
    flex-shrink: 0;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .tb-btn:hover { background: var(--bg-hover); color: var(--text-secondary); }
  .tb-btn.on {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    color: var(--accent);
  }
  .tb-btn.on:hover { background: color-mix(in srgb, var(--accent) 26%, transparent); }
  .tb-btn:focus-visible {
    outline: none;
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent) 55%, transparent);
  }
</style>
