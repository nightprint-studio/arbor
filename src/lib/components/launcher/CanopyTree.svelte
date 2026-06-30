<script lang="ts">
  /**
   * The circuit-tree scene: branch glow/trace underlay, a tapered filled trunk,
   * leaf foliage giving the canopy mass, the grassy hill, and one status-lit
   * node per product.
   *
   * Keyboard: nodes are a roving focus group — ←/→ (and ↑/↓) move between them,
   * focusing a node selects it (the detail footer follows), Enter/Space launches
   * it. The hover tooltip is an HTML overlay (real px) positioned by mapping the
   * node's viewBox coords to container px via the `xMidYMin meet` transform — the
   * SVG is scaled well below 1×, which would shrink in-SVG text to illegible.
   */
  import { buildScene, RUN, lerpColor, hexA, CANOPY_W, CANOPY_H, type Geometry, type DecoratedTool, type FilterKey } from './canopy';
  import CanopyGlyph from './CanopyGlyph.svelte';

  interface Props {
    geo: Geometry;
    tools: DecoratedTool[];
    sel: string;
    filter: FilterKey;
    hoverId: string | null;
    onselect: (id: string) => void;
    onactivate: (id: string) => void;
    onhover: (id: string | null) => void;
  }
  let { geo, tools, sel, filter, hoverId, onselect, onactivate, onhover }: Props = $props();

  const scene = $derived(buildScene(geo, tools, sel, filter));

  // Crop a little sky off the top so the canopy sits closer to the titlebar.
  const W = CANOPY_W, H = CANOPY_H, VIEW_TOP = 30;

  // Measured container size → map viewBox coords to CSS px for the HTML tooltip.
  let cw = $state(0);
  let ch = $state(0);
  const TTW = 226;

  const htmlTip = $derived.by(() => {
    if (!hoverId || !cw || !ch) return null;
    const node = scene.nodes.find(n => n.id === hoverId);
    const tool = tools.find(t => t.id === hoverId);
    if (!node || !tool) return null;
    const vw = W, vh = H - VIEW_TOP;
    const scale = Math.min(cw / vw, ch / vh);
    const offX = (cw - vw * scale) / 2;
    const nx = offX + node.x * scale;
    const ny = (node.y - VIEW_TOP) * scale;
    const nr = node.r * scale;
    let left = nx - TTW / 2;
    left = Math.max(6, Math.min(cw - TTW - 6, left));
    let top = ny - nr - 12 - 86;
    if (top < 6) top = ny + nr + 12;
    return { left, top, tool };
  });

  // Roving keyboard focus across the nodes.
  let nodeEls = $state<(SVGGElement | undefined)[]>([]);
  function nodeKey(e: KeyboardEvent, i: number, id: string) {
    if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); onactivate(id); return; }
    let next = -1;
    if (e.key === 'ArrowRight' || e.key === 'ArrowDown') next = i + 1;
    else if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') next = i - 1;
    if (next >= 0 && next < scene.nodes.length) { e.preventDefault(); nodeEls[next]?.focus(); }
  }
</script>

<div class="canopy-tree" bind:clientWidth={cw} bind:clientHeight={ch}>
  <svg viewBox="0 {VIEW_TOP} {W} {H - VIEW_TOP}" width="100%" height="100%" preserveAspectRatio="xMidYMin meet" style="display:block">
    <defs>
      <linearGradient id="arbHill" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0%" stop-color="#315738" />
        <stop offset="55%" stop-color="#1d3624" />
        <stop offset="100%" stop-color="#15281b" />
      </linearGradient>
      <linearGradient id="arbTrunk" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0%" stop-color="#a6df7c" />
        <stop offset="100%" stop-color="#3fb6a6" />
      </linearGradient>
    </defs>

    <!-- branch glow underlay -->
    <g>
      {#each scene.glow as g}
        <path d={g.d} fill="none" stroke={g.stroke} stroke-width={g.width} stroke-linecap="round" opacity={g.opacity} stroke-dasharray={g.dash} />
      {/each}
    </g>
    <!-- bright branch traces -->
    <g>
      {#each scene.trace as t}
        <path d={t.d} fill="none" stroke={t.stroke} stroke-width={t.width} stroke-linecap="round" opacity={t.opacity} stroke-dasharray={t.dash} />
      {/each}
    </g>
    <!-- tapered trunk (glow + fill) -->
    <path d={scene.trunk} fill="none" stroke={hexA('#8fce6a', 0.18)} stroke-width="13" stroke-linejoin="round" />
    <path d={scene.trunk} fill="url(#arbTrunk)" />
    <!-- fork via + branch twigs -->
    <g>
      {#each scene.dots as d}
        <circle cx={d.cx} cy={d.cy} r={d.r} fill={d.fill} stroke={d.stroke ?? 'none'} stroke-width={d.strokeWidth ?? 0} opacity={d.opacity} />
      {/each}
    </g>

    <!-- hill the tree is planted on -->
    <g>
      <path d={scene.hill.hillD} fill="url(#arbHill)" />
      <path d={scene.hill.crestD} fill="none" stroke={hexA('#8fce6a', 0.2)} stroke-width="6" />
      <path d={scene.hill.crestD} fill="none" stroke={hexA('#8fce6a', 0.45)} stroke-width="1.4" />
    </g>
    <text x={scene.plate.x} y={scene.plate.y} text-anchor="middle" font-size="11" font-weight="600"
          letter-spacing="5" fill={hexA('#d8e0cf', 0.55)} style="font-family:var(--canopy-display);text-transform:uppercase">ARBOR</text>

    <!-- foliage (behind the nodes) -->
    <g>
      {#each scene.foliage as l, i (i)}
        <path d={l.d} fill={l.fill} opacity={l.opacity} transform={l.transform} />
      {/each}
    </g>

    <!-- product nodes -->
    <g>
      {#each scene.nodes as n, i (n.id)}
        <g class="arb-node" style="opacity:{n.op}" role="button" tabindex={n.id === sel ? 0 : -1}
           bind:this={nodeEls[i]}
           data-node-id={n.id}
           aria-label={n.name}
           onclick={() => onselect(n.id)}
           onkeydown={(e) => nodeKey(e, i, n.id)}
           onmouseenter={() => onhover(n.id)}
           onmouseleave={() => onhover(null)}
           onfocus={() => { onhover(n.id); onselect(n.id); }}
           onblur={() => onhover(null)}>
          <g class="arb-pad" style="transform-box:fill-box;transform-origin:center">
            <circle cx={n.x} cy={n.y} r={n.r + 12} fill={hexA(n.accent, 0.10)} />
            <circle cx={n.x} cy={n.y} r={n.r + 5} fill={hexA(n.accent, 0.18)} />
            {#if n.isUpd}
              <circle cx={n.x} cy={n.y} r={n.r + 4} fill="none" stroke={n.accent} stroke-width="1.6"
                      style="transform-box:fill-box;transform-origin:center;animation:arbPulse 2.6s ease-out infinite" />
            {/if}
            {#if n.isRun}
              <circle cx={n.x} cy={n.y} r={n.r + 8} fill={hexA(RUN, 0.14)} />
              <circle cx={n.x} cy={n.y} r={n.r + 5} fill="none" stroke={RUN} stroke-width="2" stroke-dasharray="3 4"
                      style="transform-box:fill-box;transform-origin:center;animation:arbSpin 6s linear infinite" />
            {/if}
            <circle cx={n.x} cy={n.y} r={n.r} fill="rgba(8,13,19,.95)" stroke={n.accent}
                    stroke-width={n.sel ? 2.8 : 2.2} />
            <g transform="translate({n.x - (n.sel ? 25 : 21)} {n.y - (n.sel ? 25 : 21)})" style="color:{n.accent}">
              <CanopyGlyph id={n.glyphId} size={n.sel ? 50 : 42} />
            </g>
            {#if n.isUpd}
              <circle cx={n.x + n.r * 0.72} cy={n.y - n.r * 0.72} r="6.5" fill={n.accent} />
              <text x={n.x + n.r * 0.72} y={n.y - n.r * 0.72 + 3.6} text-anchor="middle" font-size="10" font-weight="700"
                    fill="#08101a" style="font-family:var(--canopy-display)">↑</text>
            {/if}
            {#if n.isRun}
              <!-- "running" LED badge (top-right): glow halo · dark ring · lit core -->
              <circle cx={n.x + n.r * 0.72} cy={n.y - n.r * 0.72} r="8.5" fill={hexA(RUN, 0.32)} />
              <circle cx={n.x + n.r * 0.72} cy={n.y - n.r * 0.72} r="6" fill="#0b1410" />
              <circle cx={n.x + n.r * 0.72} cy={n.y - n.r * 0.72} r="4" fill={RUN}
                      style="transform-box:fill-box;transform-origin:center;animation:arbTwinkle 2.4s ease-in-out infinite" />
            {/if}
          </g>
          {#if n.showLabel}
            <text x={n.x} y={n.y + n.r + (n.sel ? 18 : 16)} text-anchor="middle"
                  font-size={n.sel ? 13 : 11} font-weight={n.sel ? 600 : 500}
                  fill={n.sel ? '#ffffff' : lerpColor(n.accent, '#ffffff', 0.5)}
                  style="font-family:var(--canopy-display);paint-order:stroke;stroke:rgba(5,9,15,.8);stroke-width:3px;stroke-linejoin:round">{n.name}</text>
          {/if}
        </g>
      {/each}
    </g>
  </svg>

  <!-- HTML hover tooltip (real px — crisp regardless of the SVG's meet scale) -->
  {#if htmlTip}
    <div class="cv-tooltip" style="left:{htmlTip.left}px;top:{htmlTip.top}px">
      <span class="cv-bar" style="background:{htmlTip.tool.accent}"></span>
      <div class="cv-body">
        <div class="cv-name">{htmlTip.tool.name}<span class="cv-bird">{htmlTip.tool.bird}</span></div>
        <div class="cv-role">{htmlTip.tool.role}</div>
        <div class="cv-status" style="color:{htmlTip.tool.accent}">{htmlTip.tool.statusLabel} · {htmlTip.tool.versionLabel}</div>
      </div>
    </div>
  {/if}
</div>

<style>
  .canopy-tree { position: relative; width: 100%; height: 100%; }

  .arb-node { cursor: pointer; transition: opacity 0.35s ease; outline: none; }
  .arb-pad { transition: transform 0.16s ease; }
  .arb-node:hover .arb-pad,
  .arb-node:focus-visible .arb-pad { transform: scale(1.22); }
  .arb-node:hover { filter: brightness(1.12); }
  .arb-node:focus-visible { filter: brightness(1.25); }

  /* HTML tooltip — real px sizing, so it stays legible at any SVG scale. */
  .cv-tooltip {
    position: absolute; z-index: 5; width: 226px; pointer-events: none;
    display: flex; gap: 0; overflow: hidden;
    background: rgba(9, 13, 19, 0.97); border: 1px solid rgba(255, 255, 255, 0.14);
    border-radius: 12px; box-shadow: 0 16px 40px -14px rgba(0, 0, 0, 0.82);
  }
  .cv-bar { width: 4px; flex: none; }
  .cv-body { padding: 9px 13px; min-width: 0; }
  .cv-name { font-family: var(--canopy-display); font-weight: 600; font-size: 14px; color: #eef2f7; }
  .cv-bird { font-weight: 400; font-style: italic; font-size: 11px; color: #7d8696; margin-left: 5px; }
  .cv-role { font-family: var(--canopy-sans); font-size: 12px; color: #9aa3b2; margin-top: 2px; }
  .cv-status { font-family: var(--canopy-mono); font-size: 11.5px; margin-top: 4px; }
</style>
