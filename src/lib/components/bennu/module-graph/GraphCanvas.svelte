<script lang="ts">
  /**
   * The picture: one `<svg>` of boxes and curves, and nothing else.
   *
   * Split from the modal because the two have nothing to say to each other beyond a layout and a
   * selection — the modal owns the data, the filters and the keyboard, and this owns pixels. It also
   * keeps the `<style>` of a fairly involved SVG away from the shell's own.
   *
   * ## Panning is the browser's job
   *
   * The `<svg>` is sized in **pixels** at the current zoom with a fixed `viewBox`, inside a plain
   * scroll container. So the scrollbars, the wheel, shift-wheel, trackpad gestures, Home/End and
   * find-on-page all work because they are the host's, not a reimplementation — and drag-to-pan is
   * then three lines against `scrollLeft`/`scrollTop` rather than a transform matrix with its own
   * clamping bugs.
   *
   * ## Text does not ellipsize in SVG
   *
   * There is no `text-overflow` for `<text>`, and `<foreignObject>` inside a scaled SVG is a
   * rendering-bug generator on WebView. So labels are cut to the box's own width by character count —
   * the same estimate the layout used to size the box, so the two agree by construction.
   */
  import { tooltip } from '$lib/actions/tooltip';
  import { clipLabel, type GraphLayout, type PlacedEdge, type PlacedNode } from '../module-graph-layout';

  let {
    layout,
    /** Node index the reader has selected, or null. */
    selected = null,
    /** Node indices to draw as *related* to the selection (its transitive neighbourhood). */
    related = new Set<number>(),
    /** The nodes to keep at full strength while `dimOthers` is on — the search's answers, or the
     *  selection's neighbourhood. */
    highlight = new Set<number>(),
    /** Whether anything outside `highlight` should recede. One mechanism for both the search and
     *  focus mode: they ask the same question of the drawing, and two overlapping opacity systems
     *  would multiply into invisibility when both were on. */
    dimOthers = false,
    zoom = 1,
    onSelect,
    onOpen,
    onZoom,
  }: {
    layout: GraphLayout;
    selected?: number | null;
    related?: Set<number>;
    highlight?: Set<number>;
    dimOthers?: boolean;
    zoom?: number;
    onSelect: (index: number) => void;
    onOpen: (index: number) => void;
    /** Relative zoom request from Ctrl/⌘+wheel. The parent owns the level and its bounds. */
    onZoom?: (by: number) => void;
  } = $props();

  /**
   * Room around the drawing.
   *
   * Not only cosmetic: a cycle between two boxes in the **leftmost** column is drawn as a pair of arcs
   * bowing out into the gutter beside them, up to 36 units past the left edge of the layout. Less
   * padding than that and the one thing in the picture that must be seen would be clipped by the
   * viewBox.
   */
  const PAD = 48;

  const box = $derived({
    w: layout.width + PAD * 2,
    h: layout.height + PAD * 2,
  });

  let scroller = $state<HTMLDivElement | null>(null);

  /** What a box says on hover: the numbers the detail panel spells out, in one line each. */
  function summary(p: PlacedNode): string {
    const n = p.node;
    const bits = [
      n.kind || 'unknown kind',
      `${n.dependents} dependent${n.dependents === 1 ? '' : 's'}`,
      `${n.dependencies} internal`,
      `${n.external} third-party`,
    ];
    if (n.impact) bits.push(`${n.impact} rebuild on a change`);
    if (n.in_cycle) bits.push('in a cycle');
    return `${n.id} — ${bits.join(' · ')}`;
  }

  /**
   * The node the pointer is over.
   *
   * Hovering lights that module's own edges, which is the cheapest clarity there is in a picture with
   * ninety of them: "which of these lines is mine" is answered by moving the mouse instead of by
   * committing to a selection and then having to undo it.
   */
  let hovered = $state<number | null>(null);

  /** Whether an edge touches the selection, or whatever the pointer is resting on. */
  function onSelectionPath(e: PlacedEdge): boolean {
    const at = hovered ?? selected;
    return at !== null && (e.edge.from === at || e.edge.to === at);
  }

  /**
   * The edges in the order they should be painted.
   *
   * SVG has no `z-index` — paint order *is* document order — so a highlighted line drawn early
   * disappears under the fifty ordinary ones that cross it. The list is therefore sorted by how much
   * the line matters: ordinary, then cycles, then whatever the pointer or the selection is on. Keyed
   * in the template, so this is a reorder of existing elements rather than a rebuild.
   */
  const paintOrder = $derived.by(() => {
    const rank = (e: PlacedEdge) => (onSelectionPath(e) ? 2 : e.edge.in_cycle ? 1 : 0);
    return [...layout.edges].sort((a, b) => rank(a) - rank(b));
  });

  /** Receded: something is being highlighted and this is not part of it. */
  function dimmed(index: number): boolean {
    return dimOthers && !highlight.has(index);
  }

  /**
   * Ctrl/⌘+wheel zooms; a plain wheel scrolls.
   *
   * That split is the one every canvas in the app uses (the image preview included) and the one the
   * OS itself uses — taking the plain wheel for zoom would break the scrolling this deliberately left
   * to the host.
   */
  function onWheel(e: WheelEvent) {
    if (!(e.ctrlKey || e.metaKey) || !onZoom) return;
    e.preventDefault();
    onZoom(e.deltaY < 0 ? 0.1 : -0.1);
  }

  // ── Drag to pan ────────────────────────────────────────────────────────────────
  // Only from the background: a drag that starts on a box would fight the click that selects it.
  let panning = $state(false);
  let origin = { x: 0, y: 0, left: 0, top: 0 };

  function startPan(e: PointerEvent) {
    if (e.button !== 0 || !scroller) return;
    const target = e.target as Element;
    if (target.closest('[data-node]')) return;
    panning = true;
    origin = { x: e.clientX, y: e.clientY, left: scroller.scrollLeft, top: scroller.scrollTop };
    scroller.setPointerCapture(e.pointerId);
  }

  function movePan(e: PointerEvent) {
    if (!panning || !scroller) return;
    scroller.scrollLeft = origin.left - (e.clientX - origin.x);
    scroller.scrollTop = origin.top - (e.clientY - origin.y);
  }

  function endPan(e: PointerEvent) {
    if (!panning || !scroller) return;
    panning = false;
    if (scroller.hasPointerCapture(e.pointerId)) scroller.releasePointerCapture(e.pointerId);
  }

  /**
   * The zoom at which the whole drawing fits the viewport.
   *
   * Returned rather than applied: the parent owns the zoom level and its bounds, and a component that
   * reached in to set it would be a second owner of one number. Never above 1 — blowing a small graph
   * up to fill the window makes it look like a diagram of something else.
   */
  export function fitZoom(): number {
    if (!scroller || !box.w || !box.h) return 1;
    const fit = Math.min(scroller.clientWidth / box.w, scroller.clientHeight / box.h);
    return Math.min(1, Math.max(0.2, +fit.toFixed(2)));
  }

  /** Bring a node into view — the modal calls this when the list moves the selection. */
  export function reveal(index: number) {
    const p = layout.nodes.find((it) => it.index === index);
    if (!p || !scroller) return;
    const cx = (p.x + p.w / 2 + PAD) * zoom;
    const cy = (p.y + p.h / 2 + PAD) * zoom;
    scroller.scrollTo({
      left: cx - scroller.clientWidth / 2,
      top: cy - scroller.clientHeight / 2,
      behavior: 'smooth',
    });
  }
</script>

<div
  class="mgc"
  class:panning
  bind:this={scroller}
  onwheel={onWheel}
  onpointerdown={startPan}
  onpointermove={movePan}
  onpointerup={endPan}
  onpointercancel={endPan}
>
  <svg
    width={box.w * zoom}
    height={box.h * zoom}
    viewBox={`0 0 ${box.w} ${box.h}`}
    role="presentation"
  >
    <defs>
      <!-- One marker per edge colour: SVG markers do not inherit the path's stroke on WebView, so a
           single grey arrowhead would sit at the end of an accent-coloured curve. -->
      <marker id="mg-arrow" viewBox="0 0 8 8" refX="7" refY="4" markerWidth="7" markerHeight="7"
              orient="auto-start-reverse">
        <path d="M 0 1 L 7 4 L 0 7 z" fill="var(--text-disabled)" />
      </marker>
      <marker id="mg-arrow-sel" viewBox="0 0 8 8" refX="7" refY="4" markerWidth="7" markerHeight="7"
              orient="auto-start-reverse">
        <path d="M 0 1 L 7 4 L 0 7 z" fill="var(--accent)" />
      </marker>
      <marker id="mg-arrow-cycle" viewBox="0 0 8 8" refX="7" refY="4" markerWidth="7" markerHeight="7"
              orient="auto-start-reverse">
        <path d="M 0 1 L 7 4 L 0 7 z" fill="var(--error)" />
      </marker>
    </defs>

    <g transform={`translate(${PAD} ${PAD})`}>
      <!-- The axis, said once. Left-to-right means something here — dependents first, foundation last
           — and a reader who does not know that reads every arrow backwards. The tick carries the
           backend's own layer number, so it stays true when solo mode collapses empty columns. -->
      {#each layout.columns as col (col.layer)}
        <text class="mg-tick" x={col.x + col.width / 2} y={-14}>
          layer {col.layer}
        </text>
      {/each}

      <!-- Edges first, so a box is never drawn under a curve. -->
      {#each paintOrder as e (`${e.edge.from}-${e.edge.to}-${e.edge.scope}`)}
        {@const cycle = e.edge.in_cycle}
        {@const hot = onSelectionPath(e)}
        <path
          class="mg-edge"
          class:cycle
          class:hot
          class:soft={!e.edge.structural}
          class:faded={dimOthers && !(highlight.has(e.edge.from) && highlight.has(e.edge.to))}
          class:optional={e.edge.optional}
          d={e.path}
          marker-end={`url(#${cycle ? 'mg-arrow-cycle' : hot ? 'mg-arrow-sel' : 'mg-arrow'})`}
        />
      {/each}

      {#each layout.nodes as p (p.index)}
        {@const isSelected = p.index === selected}
        <g
          class="mg-node"
          class:selected={isSelected}
          class:hovered={p.index === hovered}
          class:related={related.has(p.index)}
          class:cycle={p.node.in_cycle}
          class:dimmed={dimmed(p.index)}
          data-node={p.index}
          transform={`translate(${p.x} ${p.y})`}
          role="button"
          tabindex="-1"
          aria-label={p.node.id}
          aria-pressed={isSelected}
          use:tooltip={summary(p)}
          onpointerenter={() => (hovered = p.index)}
          onpointerleave={() => { if (hovered === p.index) hovered = null; }}
          onclick={() => onSelect(p.index)}
          ondblclick={() => onOpen(p.index)}
          onkeydown={(e) => { if (e.key === 'Enter') onOpen(p.index); }}
        >
          <rect class="mg-box" width={p.w} height={p.h} rx="6" />
          <!-- The kind, as a colour bar rather than a word: there is no room for `proc-macro` in a
               96px box, and the legend in the footer names the colours once. -->
          <rect class={`mg-kind k-${(p.node.kind || 'unknown').replace('+', '-')}`} width="3" height={p.h} rx="1.5" />
          <text class="mg-label" x="10" y={p.h / 2 + 4}>{clipLabel(p.node.name || p.node.id, p.w)}</text>
          {#if p.node.impact > 0}
            <text class="mg-impact" x={p.w - 8} y={p.h / 2 + 4}>{p.node.impact}</text>
          {/if}
        </g>
      {/each}
    </g>
  </svg>
</div>

<style>
  .mgc {
    flex: 1; min-width: 0; min-height: 0;
    overflow: auto;
    background:
      radial-gradient(circle at 1px 1px, var(--border-subtle) 1px, transparent 0) 0 0 / 22px 22px;
    cursor: grab;
  }
  .mgc.panning { cursor: grabbing; }

  /* The layer ticks sit above the drawing, in the padding. */
  .mg-tick {
    fill: var(--text-disabled);
    font-family: var(--font-code);
    font-size: 9px;
    text-anchor: middle;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  /* ── Edges ── */
  /* Quiet by default and loud on demand. With ninety edges on screen the base state has to recede far
     enough that the boxes and the highlighted paths are what you see; hover and selection are what
     bring one line forward. Anything brighter than this and the picture is a net. */
  .mg-edge {
    fill: none;
    stroke: var(--border-strong);
    stroke-width: 1;
    opacity: 0.55;
    transition: opacity var(--transition-fast), stroke var(--transition-fast);
  }
  /* A dependency the ecosystem lets close a cycle is drawn solid; one it does not (a Cargo dev
     dependency) is dashed — it is real, and it is not what orders the build. */
  .mg-edge.soft { stroke-dasharray: 5 4; opacity: 0.38; }
  /* Optional: present only behind a feature. Dotted, distinct from the dev dash. */
  .mg-edge.optional { stroke-dasharray: 2 3; }
  .mg-edge.hot { stroke: var(--accent); stroke-width: 1.8; opacity: 1; }
  .mg-edge.cycle { stroke: var(--error); stroke-width: 1.8; opacity: 1; }
  .mg-edge.faded { opacity: 0.12; }

  /* ── Nodes ── */
  .mg-node { cursor: pointer; }
  .mg-box {
    fill: var(--bg-elevated);
    stroke: var(--border-default);
    stroke-width: 1;
  }
  .mg-node.hovered .mg-box { stroke: var(--text-primary); }
  .mg-node.related .mg-box { stroke: var(--accent); stroke-opacity: 0.55; }
  .mg-node.selected .mg-box {
    stroke: var(--accent);
    stroke-width: 2;
    fill: color-mix(in srgb, var(--accent) 12%, var(--bg-elevated));
  }
  /* A module in a cycle is the one thing here that is *wrong*, so it gets the danger colour even
     when something else is selected. */
  .mg-node.cycle .mg-box { stroke: var(--error); }
  .mg-node.dimmed { opacity: 0.22; }

  .mg-label {
    fill: var(--text-primary);
    font-family: var(--font-ui-sans);
    font-size: 11.5px;
    pointer-events: none;
  }
  .mg-node.selected .mg-label { fill: var(--text-primary); font-weight: 600; }
  /* How many modules rebuild when this one changes. Right-aligned, quiet: it is a number you look
     for, not one you read on every box. */
  .mg-impact {
    fill: var(--text-disabled);
    font-family: var(--font-code);
    font-size: 9.5px;
    text-anchor: end;
    pointer-events: none;
  }

  /* Kind bar. Library-ish things are cool colours, things that produce a program are warm — so the
     handful of boxes worth finding in a big picture (the binaries) stand out from the wall of libs. */
  .mg-kind { pointer-events: none; }
  .k-lib { fill: var(--info); }
  .k-bin { fill: var(--success); }
  .k-lib-bin { fill: var(--warning); }
  .k-proc-macro { fill: var(--accent); }
  .k-jar { fill: var(--info); }
  .k-war, .k-ear { fill: var(--success); }
  /* An aggregator pom builds nothing — it is scaffolding, and says so by being colourless. */
  .k-pom, .k-unknown { fill: var(--border-strong); }
</style>
