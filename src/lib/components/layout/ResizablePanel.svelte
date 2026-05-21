<script lang="ts">
  let {
    direction = 'horizontal',
    initialSize = 240,
    minSize = 80,
    maxSize = 800,
    onResize,
    reverse = false,
    children,
  }: {
    direction?: 'horizontal' | 'vertical';
    initialSize?: number;
    minSize?: number;
    maxSize?: number;
    onResize?: (size: number) => void;
    reverse?: boolean;
    children: any;
  } = $props();

  // svelte-ignore state_referenced_locally
  let size = $state(initialSize);
  let dragging = $state(false);
  let panelEl = $state<HTMLElement | null>(null);

  const isH = $derived(direction === 'horizontal');

  function onMouseDown(e: MouseEvent) {
    e.preventDefault();
    dragging = true;
    const startPos  = isH ? e.clientX : e.clientY;
    const startSize = size;
    let   pending   = size;

    function onMove(ev: MouseEvent) {
      const delta = (isH ? ev.clientX : ev.clientY) - startPos;
      pending = Math.max(minSize, Math.min(maxSize, startSize + (reverse ? -delta : delta)));
      // Mutate DOM directly — zero Svelte overhead during drag.
      if (panelEl) panelEl.style[isH ? 'width' : 'height'] = `${pending}px`;
    }

    function onUp() {
      dragging = false;
      size = pending;       // commit to reactive state once
      onResize?.(pending);  // persist only at drag end
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
    }

    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
  }

  function onKeyDown(e: KeyboardEvent) {
    const fwd  = isH ? e.key === 'ArrowRight' : e.key === 'ArrowDown';
    const back = isH ? e.key === 'ArrowLeft'  : e.key === 'ArrowUp';
    if (!fwd && !back) return;
    e.preventDefault();
    const step = e.shiftKey ? 32 : 8;
    const delta = (fwd ? step : -step) * (reverse ? -1 : 1);
    size = Math.max(minSize, Math.min(maxSize, size + delta));
    onResize?.(size);
  }
</script>

<div
  bind:this={panelEl}
  class="panel"
  class:panel-h={isH}
  class:panel-v={!isH}
  style={isH ? `width: ${size}px` : `height: ${size}px`}
>
  {#if !reverse}
    <div class="panel-content">
      {@render children()}
    </div>
    <div
      class="resize-handle {isH ? 'resize-handle-h' : 'resize-handle-v'}"
      class:active={dragging}
      onmousedown={onMouseDown}
      onkeydown={onKeyDown}
      role="slider"
      aria-orientation={isH ? 'vertical' : 'horizontal'}
      aria-label="Resize panel"
      aria-valuenow={size}
      aria-valuemin={minSize}
      aria-valuemax={maxSize}
      tabindex="0"
    ></div>
  {:else}
    <div
      class="resize-handle {isH ? 'resize-handle-h' : 'resize-handle-v'}"
      class:active={dragging}
      onmousedown={onMouseDown}
      onkeydown={onKeyDown}
      role="slider"
      aria-orientation={isH ? 'vertical' : 'horizontal'}
      aria-label="Resize panel"
      aria-valuenow={size}
      aria-valuemin={minSize}
      aria-valuemax={maxSize}
      tabindex="0"
    ></div>
    <div class="panel-content">
      {@render children()}
    </div>
  {/if}
</div>

<style>
  .panel {
    display: flex;
    flex-shrink: 0;
    position: relative;
  }

  /* ── Handle separator lines ─────────────────────────────────────────────
     The handle itself is 3px (comfortable hit area) but transparent.
     A 1px pseudo-element draws the always-visible divider line in the
     centre of the handle, turning accent on hover/drag — so the handle
     IS the visible separator between adjacent panels, IntelliJ-style. */
  .resize-handle-h {
    position: relative;
  }
  .resize-handle-h::after {
    content: '';
    position: absolute;
    top: 0;
    bottom: 0;
    left: 50%;
    width: 1px;
    transform: translateX(-0.5px);
    background: var(--border-subtle);
    transition: background var(--transition-base);
    pointer-events: none;
  }
  .resize-handle-h:hover::after,
  .resize-handle-h.active::after {
    background: var(--accent);
  }

  .resize-handle-v {
    position: relative;
  }
  .resize-handle-v::after {
    content: '';
    position: absolute;
    left: 0;
    right: 0;
    top: 50%;
    height: 1px;
    transform: translateY(-0.5px);
    background: var(--border-subtle);
    transition: background var(--transition-base);
    pointer-events: none;
  }
  .resize-handle-v:hover::after,
  .resize-handle-v.active::after {
    background: var(--accent);
  }

  .panel-h {
    flex-direction: row;
    height: 100%;
  }

  .panel-v {
    flex-direction: column;
    width: 100%;
  }

  .panel-content {
    flex: 1;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }
</style>
