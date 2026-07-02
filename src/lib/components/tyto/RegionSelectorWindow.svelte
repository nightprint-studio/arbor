<script lang="ts">
  /**
   * RegionSelectorWindow — the standalone OPAQUE frozen-frame region selector.
   *
   * Mounted in its own OS window (`tyto-region`, built by `window/region.rs`), sized to
   * cover one monitor (rect/free/smart) or the whole virtual desktop (window/display).
   * Shows the frozen backdrop full-bleed and offers five ways to pick, from the toolbar:
   *  • **Rectangle** — drag a box.
   *  • **Freehand** — trace a shape; its bounding box is captured.
   *  • **Smart** — hover an element (UI Automation rects captured before the overlay
   *    opened); scroll to widen/narrow through its container chain; click to capture.
   *  • **Window** — hover a window (blue outline), click to pick that whole window.
   *  • **Display** — hover a monitor (blue outline + name), click to pick that monitor.
   * Rectangle/freehand/smart confirm a CSS-pixel rect (→ physical region in Tyto);
   * window/display route the picked id back to Tyto with no rectangle.
   * Opaque, never transparent — a transparent WebView2 window traps input on Windows.
   */
  import { onMount } from 'svelte';
  import { convertFileSrc } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { Square, PenTool, MousePointer2, AppWindow, Monitor } from 'lucide-svelte';
  import {
    getRegionInit, regionSelectorConfirm, regionSelectorCancel, regionSelectorPick,
    type ElemRect, type WinRect, type MonRect,
  } from '$lib/ipc/tyto/region-window';
  import { signalWindowReady } from '$lib/ipc/window';
  import { themeStore } from '$lib/stores/theme.svelte';
  import { appearanceStore } from '$lib/stores/appearance.svelte';
  import { animStore } from '$lib/stores/animations.svelte';

  type SelMode = 'rect' | 'free' | 'smart' | 'window' | 'display';
  type Rect = { x: number; y: number; w: number; h: number };

  // Pushed by the shell when the (reused, hidden) overlay is re-opened for a new
  // selection — matches `REGION_REINIT_EVENT` in `src-tauri/src/window/region.rs`.
  const REGION_REINIT_EVENT = 'tyto://region-reinit';

  let mode = $state<SelMode>('rect');
  let points = $state<{ x: number; y: number }[]>([]);
  let elements = $state<ElemRect[]>([]);
  let windows = $state<WinRect[]>([]);
  let monitors = $state<MonRect[]>([]);

  let screenshotUrl = $state<string | null>(null);
  let root = $state<HTMLDivElement | null>(null);
  let dragging = $state(false);
  let start = { x: 0, y: 0 };
  let cur = $state({ x: 0, y: 0 });
  let hasRect = $state(false);
  // Smart mode: which container level (0 = smallest element under the cursor).
  let smartLevel = $state(0);

  // The hover-pick modes (smart / window / display) all work the same way: hit-test the
  // cursor against a list of rects, smallest-area-containing first, and highlight one.
  // `smart` walks the container chain with the scroll wheel; window/display click-pick.
  const isHover = $derived(mode === 'smart' || mode === 'window' || mode === 'display');

  // The window is built hidden; reveal it once the frozen frame has painted (via the
  // generic window_ready) so there's no white load flash. Excluded from the central
  // reveal in +page.svelte precisely so it can wait for the image's `load` — a generic
  // double-rAF could show the overlay before the PNG decodes. Idempotent + a shell-side
  // fallback so it can never stay hidden.
  let revealed = false;
  function reveal() {
    if (revealed) return;
    revealed = true;
    void signalWindowReady();
  }

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

  // The rects the active hover mode hit-tests against (elements / windows / monitors).
  // Windows & monitors carry an `id` (+ a `name` on monitors); elements don't — the
  // extra fields ride along harmlessly for the shared geometry.
  type HoverRect = Rect & { id?: string; name?: string };
  const hoverRects = $derived<HoverRect[]>(
    mode === 'smart' ? elements
    : mode === 'window' ? windows
    : mode === 'display' ? monitors
    : [],
  );

  // Rects under the cursor, smallest area first (≈ the ancestor chain / topmost window).
  // `smartLevel` walks it (scroll wheel) in smart mode; window/display use level 0.
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

  // In window/display mode the overlay spans the WHOLE virtual desktop, so a toolbar
  // centered on the overlay lands BETWEEN monitors (half off-screen). Anchor it to the
  // monitor under the cursor instead — the screen the user is actually working on —
  // falling back to the first. rect/free/smart cover a single monitor, so their overlay
  // is already that monitor and the CSS default (overlay-centered) is correct → null.
  const activeMonitor = $derived.by<MonRect | null>(() => {
    if ((mode !== 'window' && mode !== 'display') || !monitors.length) return null;
    const { x, y } = cur;
    return monitors.find((m) => x >= m.x && x < m.x + m.w && y >= m.y && y < m.y + m.h) ?? monitors[0];
  });
  // Inline toolbar placement: centered on the active monitor (window/display) or unset
  // (CSS centers it on the single-monitor overlay for rect/free/smart).
  const toolbarStyle = $derived(
    activeMonitor
      ? `top:${activeMonitor.y + 18}px; left:${activeMonitor.x + activeMonitor.w / 2}px; transform:translateX(-50%);`
      : '',
  );

  const MIN = 8;
  // The rect that will actually be captured, per mode.
  const selRect = $derived.by<Rect | null>(() => {
    if (isHover) return hoverRect;
    if (!hasRect) return null;
    return dragRect.w >= MIN && dragRect.h >= MIN ? dragRect : null;
  });
  const canConfirm = $derived(!!selRect && !dragging);

  function setMode(m: SelMode) {
    mode = m;
    hasRect = false;
    dragging = false;
    points = [];
    smartLevel = 0;
  }

  /** Load the current init (frozen frame + hover targets + starting mode) and reset the
   *  selection state. Run on mount AND on `tyto://region-reinit` — the overlay window is
   *  REUSED across selections (hidden, not closed, to avoid a laggy rebuild), so each new
   *  selection re-pulls its init here rather than remounting. */
  async function loadInit() {
    // Clear any leftover selection from the previous use.
    points = []; hasRect = false; dragging = false; smartLevel = 0;
    try {
      const init = await getRegionInit();
      if (!init) { void regionSelectorCancel(); return; }
      screenshotUrl = convertFileSrc(init.path);
      elements = init.elements ?? [];
      windows = init.windows ?? [];
      monitors = init.monitors ?? [];
      // Start in the mode the user picked on the mini toolbar. Hover modes fall back to
      // rect if their target list is empty (smart → no UI elements, window/display →
      // nothing enumerated, e.g. off Windows).
      const m = init.initial_mode;
      if (m === 'free') mode = 'free';
      else if (m === 'smart') mode = elements.length ? 'smart' : 'rect';
      else if (m === 'window') mode = windows.length ? 'window' : 'rect';
      else if (m === 'display') mode = monitors.length ? 'display' : 'rect';
      else mode = 'rect';
    } catch {
      void regionSelectorCancel();
    }
  }

  onMount(() => {
    // Standalone window: apply the app theme/appearance/animation config so the
    // overlay matches the main window (else it falls back to hardcoded defaults).
    void themeStore.init();
    void appearanceStore.loadConfig();
    void animStore.loadConfig();
    void loadInit();
    // Reused-window path: the shell hides (not closes) this overlay between selections
    // and pushes this event on the next open. Re-arm the reveal and blank the stale
    // frame, then reload — the new frame's `load` reveals the (still-hidden) window.
    let un: (() => void) | undefined;
    void listen(REGION_REINIT_EVENT, () => {
      revealed = false;
      screenshotUrl = null;
      void loadInit();
    }).then((f) => { un = f; });
    const t = setTimeout(reveal, 700);
    return () => { clearTimeout(t); un?.(); };
  });

  /** Window-local coords (the window origin is the monitor origin, so clientX/Y is
   *  already monitor-logical). */
  function local(e: MouseEvent): { x: number; y: number } {
    const b = root?.getBoundingClientRect();
    return { x: e.clientX - (b?.left ?? 0), y: e.clientY - (b?.top ?? 0) };
  }

  function onMouseDown(e: MouseEvent) {
    if (e.button !== 0 || isHover) return; // hover modes confirm on mouse-up click
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

  function cancel() { void regionSelectorCancel(); }
  function confirm() {
    // Window / display: route the hovered target's id back to Tyto — no rectangle.
    if (mode === 'window' || mode === 'display') {
      const id = hoverRect?.id;
      if (!id) return;
      void regionSelectorPick(mode === 'window' ? 'window' : 'display', id);
      return;
    }
    const r = selRect;
    if (!r) return;
    // Freehand: forward the traced polygon (window-local CSS px) so the screenshot is
    // masked to the shape. Other modes send just the bounding rect (no mask).
    const poly = mode === 'free' && points.length > 2
      ? points.map((p) => [Math.round(p.x), Math.round(p.y)])
      : null;
    void regionSelectorConfirm({ x: r.x, y: r.y, width: r.w, height: r.h, points: poly });
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') { e.preventDefault(); cancel(); }
    else if (e.key === 'Enter') { e.preventDefault(); confirm(); }
  }

  const hint = $derived(
    mode === 'free' ? 'to trace a shape'
    : mode === 'smart' ? 'over an element · scroll to resize · click'
    : mode === 'window' ? 'over a window · click to pick it'
    : mode === 'display' ? 'over a monitor · click to pick it'
    : 'to select a region',
  );
</script>

<svelte:window onkeydown={onKey} />

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="selector"
  class:smart={isHover}
  bind:this={root}
  role="application"
  aria-label="Select a capture region — Esc to cancel, Enter to confirm"
  onmousedown={onMouseDown}
  onmousemove={onMouseMove}
  onmouseup={onMouseUp}
  onwheel={onWheel}
  oncontextmenu={(e) => { e.preventDefault(); cancel(); }}
>
  {#if screenshotUrl}
    <img class="frozen" src={screenshotUrl} alt="" draggable="false" onload={reveal} />
  {/if}

  {#if !selRect && !(mode === 'free' && dragging)}
    <div class="veil"></div>
  {/if}

  {#if mode === 'free' && points.length > 1}
    <svg class="free-svg" aria-hidden="true">
      <polyline points={freePath} />
    </svg>
  {/if}

  {#if selRect}
    <div class="rect" class:is-hover={isHover} style={`left:${selRect.x}px; top:${selRect.y}px; width:${selRect.w}px; height:${selRect.h}px;`}>
      {#if mode === 'display' && hoverRect?.name}
        <span class="mon-name">{hoverRect.name}</span>
      {:else}
        <span class="dims">{selRect.w} × {selRect.h}</span>
      {/if}
    </div>
  {/if}

  <div class="toolbar" role="toolbar" tabindex="-1" aria-label="Region selection" style={toolbarStyle}
       onmousedown={(e) => e.stopPropagation()} onmouseup={(e) => e.stopPropagation()} onwheel={(e) => e.stopPropagation()}>
    <div class="tb-modes" role="group" aria-label="Selection shape">
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
        disabled={elements.length === 0}
        title={elements.length ? 'Smart (pick an element)' : 'Smart pick — no UI elements detected here'}
        aria-pressed={mode === 'smart'}
      >
        <MousePointer2 size={14} />
      </button>
      <!-- Window / display pickers — disabled (with a reason) when nothing was
           enumerated to hover (e.g. off Windows). -->
      <button
        type="button"
        class="tb-mode"
        class:on={mode === 'window'}
        onclick={() => setMode('window')}
        disabled={windows.length === 0}
        title={windows.length ? 'Window (pick a window)' : 'Window pick — no windows detected'}
        aria-pressed={mode === 'window'}
      >
        <AppWindow size={14} />
      </button>
      <button
        type="button"
        class="tb-mode"
        class:on={mode === 'display'}
        onclick={() => setMode('display')}
        disabled={monitors.length === 0}
        title={monitors.length ? 'Display (pick a monitor)' : 'Display pick — no monitors detected'}
        aria-pressed={mode === 'display'}
      >
        <Monitor size={14} />
      </button>
    </div>
    <span class="tb-hint"><strong>{isHover ? 'Hover' : 'Drag'}</strong> {hint}</span>
    <button type="button" class="tb-btn confirm" disabled={!canConfirm} onclick={confirm}>Capture <span class="tb-k">Enter</span></button>
    <button type="button" class="tb-btn cancel" onclick={cancel}>Cancel <span class="tb-k">Esc</span></button>
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
  /* Hover picks (smart / window / display) read distinctly — a bluish highlight. */
  .rect.is-hover {
    border-color: #4aa3ff;
    background: rgba(74, 163, 255, 0.10);
  }
  .rect.is-hover .dims,
  .rect.is-hover .mon-name { background: #4aa3ff; }
  .dims, .mon-name {
    position: absolute; top: -22px; left: 0;
    font-size: 11px; font-weight: 600; font-variant-numeric: tabular-nums;
    color: #fff; background: var(--accent, #f28b82);
    padding: 1px 6px; border-radius: 4px; white-space: nowrap;
  }
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

  .tb-hint { font-size: 12.5px; color: var(--text-secondary, #cbd5e1); }
  .tb-hint strong { color: var(--text-primary, #fff); }

  .tb-btn {
    display: inline-flex; align-items: center; gap: 7px;
    height: 28px; padding: 0 12px;
    border: 1px solid transparent; border-radius: 999px;
    font-size: 12.5px; font-weight: 600; cursor: pointer;
    transition: background var(--transition-fast, 0.12s), filter var(--transition-fast, 0.12s);
  }
  .tb-btn .tb-k {
    font-size: 10px; font-weight: 600;
    padding: 1px 5px; border-radius: 5px;
    background: rgba(255, 255, 255, 0.16); color: inherit;
  }
  .tb-btn.confirm { background: var(--accent, #f28b82); color: var(--text-on-accent, #fff); border-color: var(--accent, #f28b82); }
  .tb-btn.confirm:hover:not(:disabled) { filter: brightness(1.1); }
  .tb-btn.confirm:disabled { opacity: 0.4; cursor: default; }
  .tb-btn.cancel { background: var(--bg-input, #222); color: var(--text-primary, #fff); border-color: var(--border, #333); }
  .tb-btn.cancel:hover { background: var(--error, #e5484d); color: #fff; border-color: var(--error, #e5484d); }
</style>
