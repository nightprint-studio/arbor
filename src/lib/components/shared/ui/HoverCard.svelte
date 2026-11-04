<script lang="ts">
  /**
   * HoverCard — a small floating panel with real content, pinned beside a trigger's rectangle.
   *
   * What a tooltip cannot be: its content is a snippet (code, a diff, a list), not a string. What a popover is
   * not: it belongs to a *thing on screen* rather than to a corner, takes no focus and no clicks
   * (`pointer-events: none`), and goes away the moment its owner stops showing it — which is what makes it safe
   * to open on hover without getting between the pointer and the row it describes.
   *
   * The owner decides when it is shown (a delay, a key); this only places it: below the anchor, above when
   * there is no room, clamped to the window, and moved to `<body>` so no ancestor can clip or offset it.
   */
  import type { Snippet } from 'svelte';
  import { portal } from '$lib/actions/portal';

  interface Props {
    /** The rectangle of the thing the card describes, in viewport coordinates. */
    anchor: DOMRect;
    width?: number;
    maxHeight?: number;
    ariaLabel?: string;
    children: Snippet;
  }

  let { anchor, width = 560, maxHeight = 340, ariaLabel = 'Preview', children }: Props = $props();

  const GAP = 6;
  const MARGIN = 8;

  let el = $state<HTMLDivElement | null>(null);
  let left = $state(0);
  let top = $state(0);

  // Placed once its content is measured, and again if the anchor moves.
  $effect(() => {
    void anchor;
    if (!el) return;
    const w = el.offsetWidth;
    const h = el.offsetHeight;
    const below = anchor.bottom + GAP;
    top = below + h <= window.innerHeight - MARGIN ? below : Math.max(MARGIN, anchor.top - GAP - h);
    left = Math.min(Math.max(MARGIN, anchor.left), Math.max(MARGIN, window.innerWidth - w - MARGIN));
  });
</script>

<div
  bind:this={el}
  use:portal
  class="hover-card"
  role="tooltip"
  aria-label={ariaLabel}
  style:left="{left}px"
  style:top="{top}px"
  style:width="{width}px"
  style:max-height="{maxHeight}px"
>
  {@render children()}
</div>

<style>
  .hover-card {
    position: fixed;
    z-index: var(--z-tooltip, 10000);
    max-width: calc(100vw - 16px);
    overflow: hidden;
    pointer-events: none;
    background: var(--bg-elevated);
    border: 1px solid var(--border-default);
    border-radius: var(--radius-md);
    box-shadow: 0 8px 28px rgba(0, 0, 0, 0.45);
  }
</style>
