<script lang="ts">
  /**
   * Singleton tooltip host. Mount once at the root (AppShell) — every
   * `use:tooltip` action publishes through `tooltipState` and this component
   * renders the result. Handles smart placement (auto-flip on viewport
   * collision), arrow alignment, and dismissal on scroll/wheel/Escape.
   */
  import { cubicOut } from 'svelte/easing';
  import type { TransitionConfig } from 'svelte/transition';
  import { tooltipState, type TooltipPlacement } from '$lib/stores/tooltip.svelte';
  import { renderMarkdown } from '$lib/utils/markdown';

  type Side = 'top' | 'bottom' | 'left' | 'right';

  const VIEWPORT_MARGIN = 6;
  // Slide distance for the entry/exit animation (matches the visual budget of overlay slides elsewhere).
  const SLIDE_PX = 4;

  let tipEl: HTMLDivElement | null = $state(null);
  let scrollEl: HTMLDivElement | null = $state(null);
  let coords = $state({ x: 0, y: 0, side: 'top' as Side, arrowOffset: 0, ready: false });
  /** Set when the inner content's scrollHeight exceeds the max-height — only
   *  then do we apply the fade mask, so short tooltips don't get a chunk of
   *  their last line clipped by an unconditional gradient. */
  let overflowing = $state(false);

  /**
   * Custom Svelte transition. Reads `--anim-dur-overlay` and applies a
   * directional slide + opacity + scale, easing aligned with
   * `--anim-easing-spring` (approximated by `cubicOut`, identical perceived
   * feel at 140ms).
   */
  function tooltipFly(_node: HTMLElement, { side }: { side: Side }): TransitionConfig {
    const root = getComputedStyle(document.documentElement);
    const durRaw = root.getPropertyValue('--anim-dur-overlay').trim();
    const duration = parseInt(durRaw.replace('ms', ''), 10) || 140;

    let dx = 0;
    let dy = 0;
    if (side === 'top') dy = SLIDE_PX;
    else if (side === 'bottom') dy = -SLIDE_PX;
    else if (side === 'left') dx = SLIDE_PX;
    else if (side === 'right') dx = -SLIDE_PX;

    return {
      duration,
      easing: cubicOut,
      css: (t, u) => {
        const tx = dx * u;
        const ty = dy * u;
        const scale = 0.96 + 0.04 * t;
        return `opacity: ${t}; transform: translate(${tx}px, ${ty}px) scale(${scale});`;
      },
    };
  }

  function resolveSide(
    placement: TooltipPlacement,
    triggerRect: DOMRect,
    tipW: number,
    tipH: number,
    offset: number,
  ): Side {
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    const fits = {
      top: triggerRect.top - tipH - offset >= VIEWPORT_MARGIN,
      bottom: triggerRect.bottom + tipH + offset <= vh - VIEWPORT_MARGIN,
      left: triggerRect.left - tipW - offset >= VIEWPORT_MARGIN,
      right: triggerRect.right + tipW + offset <= vw - VIEWPORT_MARGIN,
    };

    if (placement === 'auto') {
      if (fits.top) return 'top';
      if (fits.bottom) return 'bottom';
      if (fits.right) return 'right';
      if (fits.left) return 'left';
      return 'top';
    }

    // Explicit placement: honour it if it fits, otherwise flip to opposite, else fall back to any side that fits.
    const opposite: Record<Side, Side> = { top: 'bottom', bottom: 'top', left: 'right', right: 'left' };
    if (fits[placement]) return placement;
    const opp = opposite[placement];
    if (fits[opp]) return opp;
    return (['top', 'bottom', 'right', 'left'] as Side[]).find((s) => fits[s]) ?? placement;
  }

  function computePosition(triggerRect: DOMRect, tipW: number, tipH: number, side: Side, offset: number) {
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    let x = 0;
    let y = 0;
    let arrowOffset = 0; // px from the start edge of the tooltip on the perpendicular axis

    if (side === 'top' || side === 'bottom') {
      const triggerCx = triggerRect.left + triggerRect.width / 2;
      x = triggerCx - tipW / 2;
      const xClamped = Math.max(VIEWPORT_MARGIN, Math.min(x, vw - tipW - VIEWPORT_MARGIN));
      arrowOffset = Math.max(10, Math.min(tipW - 10, triggerCx - xClamped));
      x = xClamped;
      y = side === 'top' ? triggerRect.top - tipH - offset : triggerRect.bottom + offset;
    } else {
      const triggerCy = triggerRect.top + triggerRect.height / 2;
      y = triggerCy - tipH / 2;
      const yClamped = Math.max(VIEWPORT_MARGIN, Math.min(y, vh - tipH - VIEWPORT_MARGIN));
      arrowOffset = Math.max(10, Math.min(tipH - 10, triggerCy - yClamped));
      y = yClamped;
      x = side === 'left' ? triggerRect.left - tipW - offset : triggerRect.right + offset;
    }

    return { x, y, arrowOffset };
  }

  function reposition() {
    const active = tooltipState.active;
    if (!active || !tipEl) return;
    if (!active.trigger.isConnected) {
      tooltipState.hide();
      return;
    }
    const tr = active.trigger.getBoundingClientRect();
    const tw = tipEl.offsetWidth;
    const th = tipEl.offsetHeight;
    const side = resolveSide(active.opts.placement, tr, tw, th, active.opts.offset);
    const { x, y, arrowOffset } = computePosition(tr, tw, th, side, active.opts.offset);
    coords = { x, y, side, arrowOffset, ready: true };
    // Decide whether the fade-out gradient is warranted: only when the
    // content actually needs to be clipped. +1 absorbs sub-pixel rounding
    // so a perfectly-fitting tooltip doesn't accidentally fade its bottom.
    overflowing = !!scrollEl && scrollEl.scrollHeight > scrollEl.clientHeight + 1;
  }

  // Re-measure & reposition whenever the active tooltip changes.
  // IMPORTANT: must not READ `coords` inside this effect — that would create a
  // reactive dep on a value we also write, causing an infinite loop. We always
  // assign a fresh object; `reposition()` (called from rAF) fills in the real
  // measurements on the next frame.
  $effect(() => {
    const active = tooltipState.active;
    if (!active) {
      coords = { x: 0, y: 0, side: 'top', arrowOffset: 0, ready: false };
      return;
    }
    coords = { x: 0, y: 0, side: 'top', arrowOffset: 0, ready: false };
    requestAnimationFrame(reposition);
  });

  // Global dismiss / reposition listeners — only attached while a tooltip is active.
  $effect(() => {
    if (!tooltipState.active) return;

    const onScroll = () => tooltipState.hide();
    const onWheel = () => tooltipState.hide();
    const onResize = () => reposition();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') tooltipState.hide();
    };

    window.addEventListener('scroll', onScroll, true);
    window.addEventListener('wheel', onWheel, { capture: true, passive: true });
    window.addEventListener('resize', onResize);
    window.addEventListener('keydown', onKey, true);

    return () => {
      window.removeEventListener('scroll', onScroll, true);
      window.removeEventListener('wheel', onWheel, true);
      window.removeEventListener('resize', onResize);
      window.removeEventListener('keydown', onKey, true);
    };
  });
</script>

{#if tooltipState.active}
  {@const a = tooltipState.active}
  <div
    bind:this={tipEl}
    class="arbor-tooltip side-{coords.side} {a.opts.className ?? ''}"
    class:measuring={!coords.ready}
    role="tooltip"
    in:tooltipFly={{ side: coords.side }}
    out:tooltipFly={{ side: coords.side }}
    style="left: {coords.x}px; top: {coords.y}px; max-width: {a.opts.maxWidth}px;
           --tip-max-h: {a.opts.maxHeight}px;
           --arrow-offset: {coords.arrowOffset}px;"
  >
    <div class="scroll-clip" class:overflowing bind:this={scrollEl}>
      {#if a.opts.markdown}
        <div class="content md-body">{@html renderMarkdown(a.opts.content)}</div>
      {:else}
        <div class="content">{a.opts.content}</div>
      {/if}
      {#if a.opts.description}
        <div class="description">{a.opts.description}</div>
      {/if}
    </div>
    {#if a.opts.shortcut && a.opts.shortcut.length}
      <div class="shortcut">
        {#each a.opts.shortcut as key, i (i)}
          <kbd>{key}</kbd>
        {/each}
      </div>
    {/if}
    <span class="arrow" aria-hidden="true"></span>
  </div>
{/if}

<style>
  .arbor-tooltip {
    position: fixed;
    z-index: var(--z-tooltip);
    pointer-events: none;
    background: var(--bg-overlay);
    color: var(--text-primary);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    padding: 6px 10px;
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    line-height: 1.4;
    box-shadow: var(--shadow-md);
    white-space: normal;
    word-wrap: break-word;
    /* Animations are owned by the Svelte `tooltipFly` transition (opacity + transform).
       We only suppress the very first paint here, before measurement, to avoid a flash at (0,0). */
  }

  .arbor-tooltip.measuring {
    visibility: hidden;
  }

  /* Bounds the visible vertical area; long content fades out at the bottom
     instead of stretching the tooltip into a page-tall column. The fade
     mask only applies when content is actually clipped — otherwise short
     tooltips would lose ~18px of their last line to an invisible gradient. */
  .scroll-clip {
    max-height: var(--tip-max-h, 280px);
    overflow: hidden;
  }
  .scroll-clip.overflowing {
    mask-image: linear-gradient(
      to bottom,
      #000 0,
      #000 calc(100% - 18px),
      transparent 100%
    );
    -webkit-mask-image: linear-gradient(
      to bottom,
      #000 0,
      #000 calc(100% - 18px),
      transparent 100%
    );
  }

  .content {
    color: var(--text-primary);
    white-space: pre-wrap;
    word-break: break-word;
    overflow-wrap: anywhere;
  }

  /* Markdown variant — compact spacing tuned for tooltips. */
  .content.md-body :global(.md-p) { margin: 0 0 4px; }
  .content.md-body :global(.md-p:last-child) { margin-bottom: 0; }
  .content.md-body :global(.md-h1),
  .content.md-body :global(.md-h2),
  .content.md-body :global(.md-h3) {
    margin: 6px 0 3px;
    font-size: var(--font-size-sm);
    font-weight: 600;
    color: var(--text-primary);
  }
  .content.md-body :global(.md-h1:first-child),
  .content.md-body :global(.md-h2:first-child),
  .content.md-body :global(.md-h3:first-child) { margin-top: 0; }
  .content.md-body :global(.md-inline-code) {
    font-family: var(--font-code);
    font-size: 0.9em;
    padding: 0 3px;
    border-radius: 3px;
    background: var(--bg-elevated);
    color: var(--text-primary);
  }
  .content.md-body :global(.md-pre) {
    margin: 4px 0;
    padding: 4px 6px;
    border-radius: 3px;
    background: var(--bg-elevated);
    overflow: hidden;
  }
  .content.md-body :global(.md-code) {
    font-family: var(--font-code);
    font-size: 0.85em;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .content.md-body :global(.md-ul),
  .content.md-body :global(.md-ol) {
    margin: 2px 0 4px;
    padding-left: 18px;
  }
  .content.md-body :global(.md-bq) {
    margin: 4px 0;
    padding-left: 6px;
    border-left: 2px solid var(--border);
    color: var(--text-muted);
  }
  .content.md-body :global(.md-link) { color: var(--accent); }
  .content.md-body :global(.md-spacer) { height: 4px; }
  .content.md-body :global(.md-hr) {
    margin: 6px 0;
    border: none;
    border-top: 1px solid var(--border-subtle);
  }

  .description {
    margin-top: 3px;
    color: var(--text-muted);
    font-size: var(--font-size-xs);
  }

  .shortcut {
    margin-top: 6px;
    display: inline-flex;
    gap: 3px;
    align-items: center;
  }

  .shortcut kbd {
    font-family: var(--font-mono, var(--font-ui-sans));
    font-size: 10px;
    padding: 1px 5px;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: 3px;
    color: var(--text-muted);
    line-height: 1.4;
    box-shadow: 0 1px 0 var(--border-subtle);
  }

  /* Arrow */
  .arrow {
    position: absolute;
    width: 8px;
    height: 8px;
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    transform: rotate(45deg);
  }

  .side-top .arrow {
    bottom: -5px;
    left: var(--arrow-offset);
    margin-left: -4px;
    border-top: none;
    border-left: none;
  }

  .side-bottom .arrow {
    top: -5px;
    left: var(--arrow-offset);
    margin-left: -4px;
    border-bottom: none;
    border-right: none;
  }

  .side-left .arrow {
    right: -5px;
    top: var(--arrow-offset);
    margin-top: -4px;
    border-bottom: none;
    border-left: none;
  }

  .side-right .arrow {
    left: -5px;
    top: var(--arrow-offset);
    margin-top: -4px;
    border-top: none;
    border-right: none;
  }
</style>
