<script lang="ts">
  /**
   * TytoSelector — the IN-WINDOW fullscreen Snip-style capture selector.
   *
   * Unlike the old standalone `tyto-region` OS window, this is a `position:fixed`
   * surface rendered INSIDE the Tyto window while `recorderStore.selecting`. The shell
   * has already grown the window to cover ONE monitor (`setTytoSelection`) and painted
   * a frozen backdrop of it; here we let the user pick directly on that surface.
   *
   * Five methods, chosen from the top-center toolbar:
   *  • **Rectangle** — drag a box.
   *  • **Freehand** — trace a shape; its bounding box (+ polygon mask) is captured.
   *  • **Smart** — hover a UI-Automation element; scroll to widen/narrow through its
   *    container chain; click to capture.
   *  • **Window** — hover a window (blue outline), click to pick that whole window.
   *  • **Display** — capture the WHOLE current monitor (no hover — one display at a time).
   * Rectangle/freehand/smart resolve a monitor-local CSS rect (→ physical region);
   * window routes the picked id; display commits the current monitor id.
   *
   * All geometry (frozen backdrop, `selectElements`, `selectWindows`, drawn rects and
   * freehand points) lives in the SAME monitor-local CSS-px space. State is read/written
   * straight on `recorderStore` — no window_ready / reveal / getRegionInit plumbing.
   */
  import { Square, PenTool, MousePointer2, AppWindow, Monitor, Video, Camera, X, Maximize2 } from 'lucide-svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { recorderStore, type SelectMethod } from '$lib/stores/tyto/recorder.svelte';

  type Rect = { x: number; y: number; w: number; h: number };
  type HoverRect = Rect & { id?: string; name?: string };

  // Active pick method — mirrors the store (setMode writes both).
  let mode = $state<SelectMethod>(recorderStore.selectMethod);

  let root = $state<HTMLDivElement | null>(null);
  let points = $state<{ x: number; y: number }[]>([]);
  let dragging = $state(false);
  let start = { x: 0, y: 0 };
  let cur = $state({ x: 0, y: 0 });
  let hasRect = $state(false);
  // Smart mode: which container level (0 = smallest element under the cursor).
  let smartLevel = $state(0);

  // The hover-pick modes (smart / window) hit-test the cursor against a list of rects
  // and highlight one. Display is a whole-monitor pick (no hover rects).
  const isHover = $derived(mode === 'smart' || mode === 'window');

  // Rectangle / freehand selection box (freehand → bounding box of the traced path).
  const dragRect = $derived.by<Rect>(() => {
    if (mode === 'free' && points.length > 1) {
      const xs = points.map((p) => p.x);
      const ys = points.map((p) => p.y);
      const x = Math.min(...xs);
      const y = Math.min(...ys);
      return { x: Math.round(x), y: Math.round(y), w: Math.round(Math.max(...xs) - x), h: Math.round(Math.max(...ys) - y) };
    }
    return {
      x: Math.round(Math.min(start.x, cur.x)),
      y: Math.round(Math.min(start.y, cur.y)),
      w: Math.round(Math.abs(cur.x - start.x)),
      h: Math.round(Math.abs(cur.y - start.y)),
    };
  });

  const freePath = $derived(points.map((p) => `${p.x},${p.y}`).join(' '));

  // The rects the active hover mode hit-tests against (smart elements / windows).
  const hoverRects = $derived<HoverRect[]>(
    mode === 'smart' ? recorderStore.selectElements
    : mode === 'window' ? recorderStore.selectWindows
    : [],
  );

  // Rects under the cursor, smallest area first (≈ the ancestor chain / topmost window).
  // `smartLevel` walks it (scroll wheel) in smart mode; window uses level 0.
  const hoverContaining = $derived.by<HoverRect[]>(() => {
    if (!isHover) return [];
    const { x, y } = cur;
    return hoverRects
      .filter((r) => x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h)
      .sort((a, b) => a.w * a.h - b.w * b.h);
  });
  const hoverRect = $derived(
    hoverContaining.length ? hoverContaining[Math.min(smartLevel, hoverContaining.length - 1)] : null,
  );

  const MIN = 8;
  // The rect that will actually be captured, per mode (null in display mode — the whole
  // monitor is the target, drawn frame-wide via a full-surface highlight instead).
  const selRect = $derived.by<Rect | null>(() => {
    if (mode === 'display') return null;
    if (isHover) return hoverRect;
    if (!hasRect) return null;
    return dragRect.w >= MIN && dragRect.h >= MIN ? dragRect : null;
  });

  // A confirmable target exists: a rect for rect/free/smart, a hovered window for window,
  // always in display (the current monitor).
  const canConfirm = $derived(
    mode === 'display' ? true : mode === 'window' ? !!hoverRect : (!!selRect && !dragging),
  );

  const captureVerb = $derived(recorderStore.mode === 'record' ? 'Record' : 'Capture');

  function setMode(m: SelectMethod) {
    mode = m;
    recorderStore.setSelectMethod(m);
    hasRect = false;
    dragging = false;
    points = [];
    smartLevel = 0;
  }

  /** Surface-local coords (the surface fills the frozen monitor, so clientX/Y offset by
   *  the surface origin is monitor-logical). */
  function local(e: MouseEvent): { x: number; y: number } {
    const b = root?.getBoundingClientRect();
    return { x: e.clientX - (b?.left ?? 0), y: e.clientY - (b?.top ?? 0) };
  }

  function onMouseDown(e: MouseEvent) {
    if (e.button !== 0 || isHover || mode === 'display') return; // hover/display click-confirm
    const p = local(e);
    if (mode === 'free') {
      points = [p];
      dragging = true; hasRect = false;
    } else {
      start = p; cur = p;
      dragging = true; hasRect = true;
    }
  }
  function onMouseMove(e: MouseEvent) {
    const p = local(e);
    if (isHover) { cur = p; smartLevel = 0; return; } // hover hit-test; reset level
    if (!dragging) return;
    if (mode === 'free') points = [...points, p];
    else cur = p;
  }
  function onMouseUp() {
    if (isHover) { if (hoverRect) confirm(); return; }
    if (!dragging) return;
    dragging = false;
    if (mode === 'free') hasRect = points.length > 2 && dragRect.w >= MIN && dragRect.h >= MIN;
    else if (dragRect.w < MIN || dragRect.h < MIN) hasRect = false;
  }
  function onWheel(e: WheelEvent) {
    if (mode !== 'smart') return; // only smart walks the container chain
    e.preventDefault();
    const n = hoverContaining.length;
    if (n === 0) return;
    smartLevel = Math.max(0, Math.min(smartLevel + (e.deltaY > 0 ? 1 : -1), n - 1));
  }

  // The selector IS Tyto's primary (compact) presentation, so dismissing it (Esc /
  // right-click / Close) closes the whole Tyto window — like Windows' Snip. Going to the
  // full control panel ("expand") is the explicit secondary action.
  function closeTyto() { void getCurrentWindow().close(); }
  function expand() { void recorderStore.exitSelection(); }

  function confirm() {
    if (mode === 'display') {
      void recorderStore.commitMonitor(recorderStore.selectMonitorId);
      return;
    }
    if (mode === 'window') {
      const id = hoverRect?.id;
      if (!id) return;
      void recorderStore.commitWindow(id);
      return;
    }
    const r = selRect;
    if (!r) return;
    // Freehand: forward the traced polygon (monitor-local CSS px) so the screenshot is
    // masked to the shape. Other modes send just the bounding rect (no mask).
    const poly = mode === 'free' && points.length > 2
      ? points.map((p) => [Math.round(p.x), Math.round(p.y)])
      : null;
    void recorderStore.commitRegion({ x: r.x, y: r.y, width: r.w, height: r.h }, poly);
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') { e.preventDefault(); closeTyto(); }
    else if (e.key === 'Enter') { e.preventDefault(); if (canConfirm) confirm(); }
  }

  const hint = $derived(
    mode === 'free' ? 'to trace a shape'
    : mode === 'smart' ? 'over an element · scroll to resize · click'
    : mode === 'window' ? 'over a window · click to pick it'
    : mode === 'display' ? 'the whole monitor — switch display or Capture'
    : 'to select a region',
  );
  const hintLead = $derived(mode === 'display' ? 'Captures' : isHover ? 'Hover' : 'Drag');

  const canSmart = $derived(recorderStore.selectElements.length > 0);
  const canWindow = $derived(recorderStore.selectWindows.length > 0);
  const canSwitchMonitor = $derived(recorderStore.monitors.length > 1);
</script>

<svelte:window onkeydown={onKey} />

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="selector"
  class:smart={isHover}
  class:display={mode === 'display'}
  bind:this={root}
  role="application"
  aria-label="Select a capture region — Esc to exit, Enter to capture"
  onmousedown={onMouseDown}
  onmousemove={onMouseMove}
  onmouseup={onMouseUp}
  onwheel={onWheel}
  oncontextmenu={(e) => { e.preventDefault(); closeTyto(); }}
>
  {#if recorderStore.selectFrozenUrl}
    <img class="frozen" src={recorderStore.selectFrozenUrl} alt="" draggable="false" />
  {/if}

  {#if !selRect && !(mode === 'free' && dragging) && mode !== 'display'}
    <div class="veil"></div>
  {/if}

  {#if mode === 'free' && points.length > 1}
    <svg class="free-svg" aria-hidden="true">
      <polyline points={freePath} />
    </svg>
  {/if}

  {#if mode === 'display'}
    <!-- Whole-monitor pick: outline the entire frozen surface + its name. -->
    <div class="rect is-hover full">
      <span class="mon-name">{recorderStore.selectMonitorName}</span>
    </div>
  {:else if selRect}
    <div class="rect" class:is-hover={isHover} style={`left:${selRect.x}px; top:${selRect.y}px; width:${selRect.w}px; height:${selRect.h}px;`}>
      <span class="dims">{selRect.w} × {selRect.h}</span>
    </div>
  {/if}

  <div class="toolbar" role="toolbar" tabindex="-1" aria-label="Capture selection"
       onmousedown={(e) => e.stopPropagation()} onmouseup={(e) => e.stopPropagation()} onwheel={(e) => e.stopPropagation()}>
    <div class="tb-modes" role="group" aria-label="Selection method">
      <button type="button" class="tb-mode" class:on={mode === 'rect'} onclick={() => setMode('rect')} title="Rectangle" aria-pressed={mode === 'rect'}>
        <Square size={14} />
      </button>
      <button type="button" class="tb-mode" class:on={mode === 'free'} onclick={() => setMode('free')} title="Freehand" aria-pressed={mode === 'free'}>
        <PenTool size={14} />
      </button>
      <!-- Always shown so the feature is discoverable; disabled (with a reason) when
           the foreground window exposed no UI-Automation elements to snap to. -->
      <button
        type="button"
        class="tb-mode"
        class:on={mode === 'smart'}
        onclick={() => setMode('smart')}
        disabled={!canSmart}
        title={canSmart ? 'Smart (pick an element)' : 'Smart pick — no UI elements detected here'}
        aria-pressed={mode === 'smart'}
      >
        <MousePointer2 size={14} />
      </button>
      <button
        type="button"
        class="tb-mode"
        class:on={mode === 'window'}
        onclick={() => setMode('window')}
        disabled={!canWindow}
        title={canWindow ? 'Window (pick a window)' : 'Window pick — no windows detected'}
        aria-pressed={mode === 'window'}
      >
        <AppWindow size={14} />
      </button>
      <button type="button" class="tb-mode" class:on={mode === 'display'} onclick={() => setMode('display')} title="Display (whole monitor)" aria-pressed={mode === 'display'}>
        <Monitor size={14} />
      </button>
    </div>

    <span class="tb-hint"><strong>{hintLead}</strong> {hint}</span>

    <!-- Record / Screenshot 2-way toggle so Capture knows what to do. -->
    <div class="tb-cap" role="group" aria-label="Capture mode">
      <button type="button" class="tb-cap-btn" class:on={recorderStore.mode === 'record'} onclick={() => recorderStore.setMode('record')} title="Record video" aria-pressed={recorderStore.mode === 'record'}>
        <Video size={13} /> Record
      </button>
      <button type="button" class="tb-cap-btn" class:on={recorderStore.mode === 'screenshot'} onclick={() => recorderStore.setMode('screenshot')} title="Screenshot" aria-pressed={recorderStore.mode === 'screenshot'}>
        <Camera size={13} /> Shot
      </button>
    </div>

    <!-- Monitor switch — cycles the frozen backdrop across displays. -->
    <button
      type="button"
      class="tb-btn monitor"
      onclick={() => void recorderStore.switchSelectionMonitor()}
      disabled={!canSwitchMonitor}
      title={canSwitchMonitor ? 'Switch monitor' : 'Only one monitor'}
    >
      <Monitor size={13} /> {recorderStore.selectMonitorName}
    </button>

    <button type="button" class="tb-btn confirm" disabled={!canConfirm} onclick={confirm}>{captureVerb} <span class="tb-k">Enter</span></button>
    <button type="button" class="tb-btn expand" onclick={expand} title="Full control panel" aria-label="Full control panel">
      <Maximize2 size={13} />
    </button>
    <button type="button" class="tb-btn cancel" onclick={closeTyto} title="Close Tyto">
      <X size={13} /> <span class="tb-k">Esc</span>
    </button>
  </div>
</div>

<style>
  .selector {
    position: fixed;
    inset: 0;
    z-index: 1;
    cursor: crosshair;
    user-select: none;
    overflow: hidden;
    background: #000;
  }
  .selector.smart { cursor: default; }
  .selector.display { cursor: default; }
  .frozen {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: fill;
    pointer-events: none;
    -webkit-user-drag: none;
  }
  .veil { position: absolute; inset: 0; background: rgba(0, 0, 0, 0.35); }

  .free-svg { position: absolute; inset: 0; width: 100%; height: 100%; pointer-events: none; z-index: 1; }
  .free-svg polyline {
    fill: color-mix(in srgb, var(--accent, #f28b82) 14%, transparent);
    stroke: var(--accent, #f28b82);
    stroke-width: 2; stroke-linejoin: round; stroke-linecap: round;
  }

  .rect {
    position: absolute;
    border: 1.5px solid var(--accent, #f28b82);
    background: transparent;
    box-shadow: 0 0 0 100vmax rgba(0, 0, 0, 0.35);
    pointer-events: none;
  }
  /* Display mode: outline the whole surface (inset so the border reads). */
  .rect.full { inset: 0; box-shadow: none; }
  /* Hover / whole picks (smart / window / display) read distinctly — a bluish highlight. */
  .rect.is-hover {
    border-color: #4aa3ff;
    background: rgba(74, 163, 255, 0.10);
  }
  .rect.is-hover .dims,
  .rect.is-hover .mon-name { background: #4aa3ff; }
  .dims, .mon-name {
    position: absolute; top: -22px; left: 0;
    font-size: var(--font-size-xs); font-weight: 600; font-variant-numeric: tabular-nums;
    color: #fff; background: var(--accent, #f28b82);
    padding: 1px 6px; border-radius: 4px; white-space: nowrap;
  }
  /* In display mode the outline hugs the surface edges, so its label can't sit above
     the top border (off-screen) — nudge it inside. */
  .rect.full .mon-name { top: 14px; left: 14px; }
  .mon-name { font-variant-numeric: normal; max-width: 60vw; overflow: hidden; text-overflow: ellipsis; }

  .toolbar {
    position: absolute;
    top: 18px; left: 50%;
    transform: translateX(-50%);
    z-index: 2;
    display: flex; align-items: center; gap: 10px;
    padding: 8px 10px 8px 16px;
    background: var(--bg-elevated, #1a1f2e);
    /* Match the compact mini-bar: soft bluish accent border + subtle glow, over a
       stronger drop shadow since this floats above the frozen desktop. */
    border: 1px solid color-mix(in srgb, var(--accent) 55%, var(--border));
    border-radius: 999px;
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.05),
                0 0 0 1px color-mix(in srgb, var(--accent) 14%, transparent),
                0 8px 28px rgba(0, 0, 0, 0.5);
    cursor: default;
  }
  .tb-modes { display: flex; align-items: center; gap: 3px; padding-right: 4px; border-right: 1px solid var(--border, #333); }
  .tb-mode {
    display: inline-flex; align-items: center; justify-content: center;
    width: 30px; height: 28px;
    border: none; border-radius: var(--radius-sm, 7px); cursor: pointer;
    background: transparent; color: var(--text-secondary, #cbd5e1);
    transition: background var(--transition-fast, 0.12s), color var(--transition-fast, 0.12s);
  }
  .tb-mode:hover:not(:disabled) { background: var(--bg-hover, #262b38); color: var(--text-primary, #fff); }
  .tb-mode.on { background: var(--accent, #f28b82); color: var(--text-on-accent, #fff); }
  .tb-mode:disabled { opacity: 0.4; cursor: default; }

  .tb-hint { font-size: var(--font-size-sm); color: var(--text-secondary, #cbd5e1); }
  .tb-hint strong { color: var(--text-primary, #fff); }

  /* Record / Screenshot segmented toggle. */
  .tb-cap {
    display: inline-flex; align-items: center; gap: 2px;
    padding: 2px; border-radius: 999px;
    background: var(--bg-input, #222); border: 1px solid var(--border, #333);
  }
  .tb-cap-btn {
    display: inline-flex; align-items: center; gap: 5px;
    height: 24px; padding: 0 10px;
    border: none; border-radius: 999px; cursor: pointer;
    font-size: var(--font-size-sm); font-weight: 600;
    background: transparent; color: var(--text-secondary, #cbd5e1);
    transition: background var(--transition-fast, 0.12s), color var(--transition-fast, 0.12s);
  }
  .tb-cap-btn:hover:not(.on) { color: var(--text-primary, #fff); }
  .tb-cap-btn.on { background: var(--accent, #f28b82); color: var(--text-on-accent, #fff); }

  .tb-btn {
    display: inline-flex; align-items: center; gap: 7px;
    height: 28px; padding: 0 12px;
    border: 1px solid transparent; border-radius: 999px;
    font-size: var(--font-size-sm); font-weight: 600; cursor: pointer;
    transition: background var(--transition-fast, 0.12s), filter var(--transition-fast, 0.12s);
  }
  .tb-btn .tb-k {
    font-size: var(--font-size-2xs); font-weight: 600;
    padding: 1px 5px; border-radius: 5px;
    background: rgba(255, 255, 255, 0.16); color: inherit;
  }
  .tb-btn.monitor {
    background: var(--bg-input, #222); color: var(--text-primary, #fff); border-color: var(--border, #333);
    max-width: 220px;
  }
  .tb-btn.monitor:hover:not(:disabled) { background: var(--bg-hover, #262b38); }
  .tb-btn.monitor:disabled { opacity: 0.4; cursor: default; }
  .tb-btn.expand { background: var(--bg-input, #222); color: var(--text-secondary, #cbd5e1); border-color: var(--border, #333); padding: 0 9px; }
  .tb-btn.expand:hover { background: var(--bg-hover, #262b38); color: var(--text-primary, #fff); }
  .tb-btn.confirm { background: var(--accent, #f28b82); color: var(--text-on-accent, #fff); border-color: var(--accent, #f28b82); }
  .tb-btn.confirm:hover:not(:disabled) { filter: brightness(1.1); }
  .tb-btn.confirm:disabled { opacity: 0.4; cursor: default; }
  .tb-btn.cancel { background: var(--bg-input, #222); color: var(--text-primary, #fff); border-color: var(--border, #333); }
  .tb-btn.cancel:hover { background: var(--error, #e5484d); color: #fff; border-color: var(--error, #e5484d); }
</style>
