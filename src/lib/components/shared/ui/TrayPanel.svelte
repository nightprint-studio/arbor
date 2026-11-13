<script lang="ts">
  /**
   * TrayPanel — a panel that rises from a bar at the bottom of a pane (a tray of buttons over a composer) and
   * floats over what is above it, the width of the bar.
   *
   * What `Popover` is not: that one hangs from the window's status bar, in a fixed corner. This one belongs to the
   * pane it is in — the caller wraps the bar in a `position: relative` box and puts the panel inside it — so it
   * opens right above the button that asked for it, and moves with the pane.
   *
   * It behaves like the rest of the overlays: it takes focus when it opens and gives it back when it goes, Esc
   * closes it (without reaching the shell's own Esc), and so does a press outside it — except inside `keep`, the
   * bar itself, so that pressing the open button again toggles it instead of closing and reopening it.
   */
  import type { Snippet } from 'svelte';

  let {
    label,
    onclose,
    keep,
    maxHeight = 360,
    children,
  }: {
    /** Accessible name of the panel. */
    label: string;
    /** Called for Esc and for a press outside the panel and outside `keep`. */
    onclose: () => void;
    /** The bar the panel rises from: presses there are the bar's to handle. */
    keep?: HTMLElement | null;
    maxHeight?: number;
    children: Snippet;
  } = $props();

  let panel = $state<HTMLDivElement | null>(null);

  $effect(() => {
    const opener = document.activeElement;
    panel?.focus({ preventScroll: true });
    return () => {
      if (opener instanceof HTMLElement && opener !== document.body && opener.isConnected) opener.focus({ preventScroll: true });
    };
  });

  $effect(() => {
    const outside = (e: PointerEvent) => {
      const target = e.target as Node | null;
      if (!target || panel?.contains(target) || keep?.contains(target)) return;
      onclose();
    };
    document.addEventListener('pointerdown', outside, true);
    return () => document.removeEventListener('pointerdown', outside, true);
  });

  function onkeydown(e: KeyboardEvent) {
    if (e.key !== 'Escape' || e.defaultPrevented) return;
    e.preventDefault();
    e.stopPropagation();
    onclose();
  }
</script>

<div bind:this={panel} class="tray-panel" role="dialog" aria-label={label} tabindex="-1" style:max-height="{maxHeight}px" {onkeydown}>
  {@render children()}
</div>

<style>
  .tray-panel {
    position: absolute; left: 0; right: 0; bottom: calc(100% + 6px); z-index: 20;
    display: flex; flex-direction: column; min-height: 0;
    overflow-y: auto;
    padding: 10px 12px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
    background: var(--bg-elevated);
    box-shadow: var(--shadow-lg, 0 8px 28px rgba(0, 0, 0, 0.35));
    outline: none;
  }
</style>
