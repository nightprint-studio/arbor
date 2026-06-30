<script lang="ts">
  /**
   * Bottom detail footer for the selected product — rendered as the deepest
   * earth stratum: the footer itself is **solid rock** (an opaque warm-dark
   * gradient, so nothing shows through), topped by a thin **domed soil cap**
   * that echoes the grassy hill, a warm ember heat-line at the soil↔rock
   * boundary, rock texture and a molten core glow hugging the bottom. The
   * product identity + action row overlay it. Together with the hill it reads as
   * a cross-section of the earth.
   */
  import { hexA, RUN, type DecoratedTool } from './canopy';
  import CanopyGlyph from './CanopyGlyph.svelte';
  import CanopyVersionMenu from './CanopyVersionMenu.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';

  interface Props {
    tool: DecoratedTool;
    onaction: () => void;
    onstop: () => void;
    onpickVer: (v: string) => void;
  }
  let { tool, onaction, onstop, onpickVer }: Props = $props();

  const A = $derived(tool.accent);
  // Map the Canopy action kind → shared Button variant + colour.
  // primary (Launch) / run (Open) → soft tonal; update (Update) → solid accent.
  const actionVariant = $derived(tool.kind === 'update' ? 'primary' : 'tonal');
  const actionColor = $derived(tool.kind === 'run' ? RUN : A);
  const tileStyle = $derived(
    `width:40px;height:40px;border-radius:11px;display:flex;align-items:center;justify-content:center;flex:none;color:${A};background:${hexA(A, 0.13)};border:1px solid ${hexA(A, 0.24)};box-shadow:inset 0 0 18px ${hexA(A, 0.10)}`,
  );
</script>

<div class="card">
  <!-- decorative strata over the solid-rock background: domed soil cap + ember
       line + texture + molten core. preserveAspectRatio=none → stretches to fit. -->
  <svg class="rock" viewBox="0 0 400 130" preserveAspectRatio="none" aria-hidden="true">
    <defs>
      <radialGradient id="magmaPool" cx="50%" cy="100%" r="75%">
        <stop offset="0%" stop-color="rgba(245,124,46,0.55)" />
        <stop offset="42%" stop-color="rgba(222,84,32,0.20)" />
        <stop offset="100%" stop-color="rgba(222,84,32,0)" />
      </radialGradient>
    </defs>
    <!-- thin domed soil cap (echoes the hill, connects grass → rock) -->
    <path d="M0 0 L0 6 Q200 24 400 6 L400 0 Z" fill="#21412a" />
    <!-- ember heat-line at the soil↔rock boundary -->
    <path d="M0 6 Q200 24 400 6" fill="none" stroke="rgba(244,140,68,0.42)" stroke-width="2.2" />
    <path d="M0 6 Q200 24 400 6" fill="none" stroke="rgba(255,188,118,0.20)" stroke-width="0.8" />
    <!-- rock texture: darker pockets + a faint warm highlight -->
    <ellipse cx="90" cy="74" rx="30" ry="12" fill="rgba(0,0,0,0.16)" />
    <ellipse cx="318" cy="84" rx="32" ry="12" fill="rgba(0,0,0,0.14)" />
    <ellipse cx="150" cy="58" rx="16" ry="5" fill="rgba(255,180,120,0.05)" />
    <!-- molten core glow hugging the bottom edge (clipped by the footer) -->
    <ellipse cx="200" cy="148" rx="210" ry="36" fill="url(#magmaPool)" />
    <ellipse cx="200" cy="152" rx="96" ry="22" fill="rgba(255,150,70,0.28)" />
  </svg>

  <div class="card-content">
    <div class="head">
      <div class="tile" style={tileStyle}><CanopyGlyph id={tool.glyphId} size={22} /></div>
      <div class="ident">
        <div class="name-row">
          <span class="name">{tool.name}</span>
          <span class="bird">{tool.bird}</span>
        </div>
        <div class="role">{tool.role}</div>
      </div>
      <div class="status">
        <div class="status-row">
          <span class="dot" style="background:{tool.statusColor};box-shadow:0 0 7px {tool.statusColor}"></span>
          <span class="status-label">{tool.statusLabel}</span>
        </div>
        <div class="ver">{tool.versionLabel}</div>
      </div>
    </div>

    <div class="actions">
      <div class="act-main">
        <Button variant={actionVariant} color={actionColor} block onclick={onaction}>{tool.actionLabel}</Button>
      </div>
      {#if tool.isRunning}
        <Button variant="tonal" color="#f0908c" onclick={onstop}>Stop</Button>
      {/if}
      <CanopyVersionMenu versions={tool.verMenu} current={tool.versionLabel} onpick={onpickVer} />
    </div>
  </div>
</div>

<style>
  /* Solid rock footer — fully opaque so nothing dark shows through; the SVG only
     ADDS strata on top (no transparent fill boundaries). */
  .card {
    position: relative;
    flex: none;
    background: linear-gradient(180deg, #241b14 0%, #17100b 46%, #0c0807 100%);
  }
  /* The rock SVG clips its own magma overflow (viewBox) so the footer needs no
     `overflow:hidden` — which would otherwise eat the upward version dropdown. */
  .rock { position: absolute; inset: 0; width: 100%; height: 100%; display: block; overflow: hidden; }
  .card-content { position: relative; z-index: 1; padding: 22px 16px 16px; }

  .head { display: flex; align-items: center; gap: 12px; }
  .ident { flex: 1; min-width: 0; }
  .name-row { display: flex; align-items: center; gap: 8px; }
  .name { font-family: var(--canopy-display); font-weight: 600; font-size: 16px; color: #eef2f7; }
  .bird { font-size: 11px; color: #c2a98f; font-style: italic; }
  .role { font-size: 12px; color: #b39d88; margin-top: 2px; }
  .status { text-align: right; }
  .status-row { display: flex; align-items: center; gap: 6px; justify-content: flex-end; }
  .dot { width: 7px; height: 7px; border-radius: 50%; flex: none; display: inline-block; }
  .status-label { font-size: 11px; color: #c5b39f; }
  .ver { font-family: var(--canopy-mono); font-size: 11.5px; color: #b39d88; margin-top: 3px; }

  .actions { display: flex; gap: 8px; margin-top: 12px; }
  .act-main { flex: 1; display: flex; }
</style>
