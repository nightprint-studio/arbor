<script lang="ts" module>
  /** A node of the graph. Generic: `value` is any additive weight (samples, bytes, ms), a node's
   *  `value` is the weight of itself AND its children, and `meta` is carried to the callbacks
   *  untouched — the widget never looks inside it. */
  export interface FlameNodeData {
    name: string;
    value: number;
    children?: FlameNodeData[];
    /** Weight spent in the node itself (shown in the tooltip). Defaults to `value - Σ children`. */
    self?: number;
    /** Opaque to the widget; handed back through the callbacks and the colour function. */
    meta?: Record<string, unknown>;
  }
</script>

<script lang="ts">
  /**
   * Flame graph on a `<canvas>` — thousands of boxes cost one paint, not thousands of elements.
   *
   * Drawn top-down (root on top, callees below): the same orientation as an IntelliJ call tree
   * and the one that scrolls naturally inside a panel. The width of a box is its share of the
   * zoomed-in root; boxes under one pixel are skipped rather than drawn as noise.
   *
   * Keyboard (the graph is one focus stop): ↑ parent, ↓ heaviest child, ← → siblings, Enter
   * zoom into the selection, Backspace / Esc zoom out, Home reset, Space activate (the host
   * decides what that means — typically "go to source").
   *
   * Colours are read from the theme tokens at paint time, so a theme switch needs one repaint
   * (the widget watches the document root for that) and no per-theme code here.
   */
  import { onMount, untrack, type Snippet } from 'svelte';
  import Button from './Button.svelte';

  interface Props {
    root: FlameNodeData;
    /** Highlight boxes whose name contains this text (case-insensitive); the rest dim. */
    search?: string;
    /** Selected box (bindable). */
    selected?: FlameNodeData | null;
    /** A box was clicked / Space pressed on. */
    onActivate?: (node: FlameNodeData) => void;
    /** Fill colour of a box; `undefined` falls back to a stable colour derived from `group(node)`. */
    color?: (node: FlameNodeData) => string | undefined;
    /** Key used to derive the default colour (same key → same colour). Default: the name's prefix up to the last dot. */
    group?: (node: FlameNodeData) => string;
    /** Formats a value for the tooltip. */
    format?: (value: number) => string;
    /** Custom tooltip body. */
    tip?: Snippet<[FlameNodeData, number]>;
    rowHeight?: number;
    /** Accessible name of the graph. */
    ariaLabel?: string;
    class?: string;
  }

  let {
    root,
    search = '',
    selected = $bindable(null),
    onActivate,
    color,
    group,
    format = (v: number) => String(Math.round(v)),
    tip,
    rowHeight = 18,
    ariaLabel = 'Flame graph',
    class: klass = '',
  }: Props = $props();

  interface Box { node: FlameNodeData; x: number; y: number; w: number }

  let host = $state<HTMLDivElement | null>(null);
  let canvas = $state<HTMLCanvasElement | null>(null);
  let width = $state(0);
  let zoom = $state<FlameNodeData | null>(null);
  let hover = $state<{ node: FlameNodeData; x: number; y: number } | null>(null);
  let themeTick = $state(0);

  /** Rebuilt on every paint: the boxes actually drawn, for hit-testing. */
  let boxes: Box[] = [];
  const parents = new Map<FlameNodeData, FlameNodeData>();

  const view = $derived(zoom ?? root);

  function indexParents(n: FlameNodeData) {
    for (const c of n.children ?? []) { parents.set(c, n); indexParents(c); }
  }
  $effect(() => {
    const r = root;
    untrack(() => {
      parents.clear();
      indexParents(r);
      zoom = null;
      selected = null;
      hover = null;
    });
  });

  function depthOf(n: FlameNodeData, cap = 400): number {
    let d = 0;
    let level = n.children ?? [];
    while (level.length && d < cap) {
      d++;
      level = level.flatMap((c) => c.children ?? []);
      if (level.length > 50_000) break;
    }
    return d + 1;
  }
  const height = $derived(Math.max(rowHeight, depthOf(view) * rowHeight));

  const groupOf = (n: FlameNodeData) => (group ? group(n) : n.name.slice(0, Math.max(0, n.name.lastIndexOf('.'))) || n.name);

  function hue(s: string): number {
    let h = 7;
    for (let i = 0; i < s.length; i++) h = (h * 31 + s.charCodeAt(i)) >>> 0;
    return h % 360;
  }

  function matches(n: FlameNodeData, q: string) { return n.name.toLowerCase().includes(q); }

  function paint() {
    if (!canvas || width <= 0) return;
    void themeTick;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.floor(width * dpr);
    canvas.height = Math.floor(height * dpr);
    canvas.style.width = `${width}px`;
    canvas.style.height = `${height}px`;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    const css = getComputedStyle(canvas);
    const text = css.getPropertyValue('--text-primary').trim() || '#ddd';
    const edge = css.getPropertyValue('--bg-base').trim() || '#111';
    const accent = css.getPropertyValue('--accent').trim() || '#4c8dff';
    const dark = matchMedia('(prefers-color-scheme: dark)').matches || document.documentElement.dataset.theme === 'dark';
    ctx.clearRect(0, 0, width, height);
    ctx.font = `11px ${css.getPropertyValue('--font-mono').trim() || 'monospace'}`;
    ctx.textBaseline = 'middle';
    boxes = [];
    const q = search.trim().toLowerCase();
    const total = view.value || 1;
    const sel = selected;

    const draw = (n: FlameNodeData, x: number, w: number, depth: number) => {
      if (w < 0.8) return;
      const y = depth * rowHeight;
      boxes.push({ node: n, x, y, w });
      const base = color?.(n);
      const fill = base ?? `hsl(${hue(groupOf(n))} ${dark ? 42 : 55}% ${dark ? 38 : 66}%)`;
      const dim = q !== '' && !matches(n, q);
      ctx.globalAlpha = dim ? 0.25 : 1;
      ctx.fillStyle = fill;
      ctx.fillRect(x, y, Math.max(0, w - 1), rowHeight - 1);
      if (q !== '' && !dim) {
        ctx.strokeStyle = accent;
        ctx.lineWidth = 1.5;
        ctx.strokeRect(x + 0.75, y + 0.75, Math.max(0, w - 2.5), rowHeight - 2.5);
      }
      if (n === sel) {
        ctx.globalAlpha = 1;
        ctx.strokeStyle = text;
        ctx.lineWidth = 1.5;
        ctx.strokeRect(x + 0.75, y + 0.75, Math.max(0, w - 2.5), rowHeight - 2.5);
      }
      if (w > 34) {
        ctx.globalAlpha = dim ? 0.35 : 1;
        ctx.fillStyle = dark ? '#fff' : '#111';
        ctx.save();
        ctx.beginPath();
        ctx.rect(x + 2, y, Math.max(0, w - 4), rowHeight);
        ctx.clip();
        ctx.fillText(n.name, x + 4, y + rowHeight / 2);
        ctx.restore();
      }
      ctx.globalAlpha = 1;
      let cx = x;
      for (const c of n.children ?? []) {
        const cw = (c.value / (n.value || 1)) * w;
        draw(c, cx, cw, depth + 1);
        cx += cw;
      }
    };
    void edge;
    draw(view, 0, width, 0);
    void total;
  }

  $effect(() => {
    // Everything the paint depends on, read here so the effect re-runs.
    void [view, height, width, search, selected, themeTick, rowHeight];
    paint();
  });

  onMount(() => {
    const ro = new ResizeObserver((e) => { width = Math.floor(e[0].contentRect.width); });
    if (host) ro.observe(host);
    const mo = new MutationObserver(() => { themeTick++; });
    mo.observe(document.documentElement, { attributes: true, attributeFilter: ['class', 'data-theme', 'style'] });
    return () => { ro.disconnect(); mo.disconnect(); };
  });

  function hit(e: MouseEvent): Box | null {
    if (!canvas) return null;
    const r = canvas.getBoundingClientRect();
    const x = e.clientX - r.left;
    const y = e.clientY - r.top;
    for (let i = boxes.length - 1; i >= 0; i--) {
      const b = boxes[i];
      if (y >= b.y && y < b.y + rowHeight && x >= b.x && x < b.x + b.w) return b;
    }
    return null;
  }

  function onMove(e: MouseEvent) {
    const b = hit(e);
    if (!b) { hover = null; return; }
    const h = host!;
    const r = h.getBoundingClientRect();
    // The tip is absolutely positioned inside the scrolling host, so it lives in CONTENT
    // coordinates: the pointer's offset in the viewport is not enough once the host has scrolled.
    // Flipped above the pointer near the bottom edge so it never sits below the visible area.
    const px = e.clientX - r.left;
    const py = e.clientY - r.top;
    const flip = py > r.height - 76;
    hover = {
      node: b.node,
      x: h.scrollLeft + Math.min(px + 12, Math.max(0, r.width - 280)),
      y: h.scrollTop + (flip ? py - 64 : py + 14),
    };
  }

  function select(n: FlameNodeData | null) {
    selected = n;
    if (n && host && canvas) {
      const b = boxes.find((x) => x.node === n);
      if (b) {
        const top = b.y, bottom = b.y + rowHeight;
        if (top < host.scrollTop) host.scrollTop = top;
        else if (bottom > host.scrollTop + host.clientHeight) host.scrollTop = bottom - host.clientHeight;
      }
    }
  }

  function zoomInto(n: FlameNodeData) { zoom = n === root ? null : n; }

  function onClick(e: MouseEvent) {
    host?.focus();
    const b = hit(e);
    select(b?.node ?? null);
    if (b) onActivate?.(b.node);
  }

  function onDblClick(e: MouseEvent) {
    const b = hit(e);
    if (b) zoomInto(b.node);
  }

  function siblings(n: FlameNodeData): FlameNodeData[] { const p = parents.get(n); return p?.children ?? []; }

  function onKey(e: KeyboardEvent) {
    const cur = selected ?? view;
    let next: FlameNodeData | undefined;
    switch (e.key) {
      case 'ArrowUp': next = parents.get(cur); if (cur === view) next = undefined; break;
      case 'ArrowDown': next = [...(cur.children ?? [])].sort((a, b) => b.value - a.value)[0]; break;
      case 'ArrowLeft': case 'ArrowRight': {
        const s = siblings(cur);
        const i = s.indexOf(cur);
        if (i >= 0) next = s[i + (e.key === 'ArrowRight' ? 1 : -1)];
        break;
      }
      case 'Enter': zoomInto(cur); e.preventDefault(); return;
      case ' ': onActivate?.(cur); e.preventDefault(); return;
      case 'Backspace': case 'Escape': {
        if (zoom) { const p = parents.get(zoom); zoom = p && p !== root ? p : null; e.preventDefault(); }
        return;
      }
      case 'Home': zoom = null; select(root); e.preventDefault(); return;
      default: return;
    }
    e.preventDefault();
    if (next) select(next);
  }

  const hoverPct = $derived(hover ? (hover.node.value / (root.value || 1)) * 100 : 0);
  const hoverSelf = $derived(
    hover ? (hover.node.self ?? hover.node.value - (hover.node.children ?? []).reduce((s, c) => s + c.value, 0)) : 0,
  );
</script>

<!-- A single focus stop with its own key handling: the canvas has no inner elements to tab through. -->
<div
  bind:this={host}
  class="flame {klass}"
  role="application"
  tabindex="0"
  aria-label={ariaLabel}
  onkeydown={onKey}
  onscroll={() => (hover = null)}
>
  <canvas
    bind:this={canvas}
    onmousemove={onMove}
    onmouseleave={() => (hover = null)}
    onclick={onClick}
    ondblclick={onDblClick}
  ></canvas>
  {#if zoom}
    <div class="flame-reset"><Button variant="outline" size="xs" onclick={() => (zoom = null)}>Reset zoom</Button></div>
  {/if}
  {#if hover}
    <div class="flame-tip" style="left:{hover.x}px; top:{hover.y}px">
      {#if tip}
        {@render tip(hover.node, hoverPct)}
      {:else}
        <div class="flame-tip-name">{hover.node.name}</div>
        <div class="flame-tip-row">{format(hover.node.value)} · {hoverPct.toFixed(1)}%</div>
        <div class="flame-tip-row muted">self {format(hoverSelf)}</div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .flame {
    position: relative;
    overflow: auto;
    flex: 1 1 auto;
    min-height: 0;
    outline: none;
    background: var(--bg-base);
  }
  .flame:focus-visible { box-shadow: inset 0 0 0 1px var(--accent); }
  canvas { display: block; cursor: pointer; }
  .flame-tip {
    position: absolute;
    z-index: 5;
    max-width: 280px;
    padding: 6px 8px;
    border-radius: var(--radius-md);
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle, var(--border));
    box-shadow: var(--shadow-md, 0 4px 14px rgba(0, 0, 0, 0.35));
    font-size: 11px;
    pointer-events: none;
    color: var(--text-primary);
  }
  .flame-tip-name { font-family: var(--font-mono); word-break: break-all; margin-bottom: 2px; }
  .flame-tip-row.muted { color: var(--text-secondary); }
  .flame-reset { position: sticky; float: right; top: 4px; right: 6px; z-index: 4; }
</style>
