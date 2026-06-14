<script lang="ts">
  /**
   * Nemus footer — a nemus-specific status strip wired to the live engine
   * telemetry: transport position · cps · active voices · DSP load · sample
   * rate · cursor row:col · render state. All figures come from the transport /
   * meters streams (no RAF, no mock); they idle naturally when stopped because
   * the engine stops emitting movement.
   */
  import { Activity, Cpu, AudioWaveform, AlertTriangle, Clock, Music } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { transportStore, metersStore, audioErrorStore } from '../stores/engine.svelte';
  import { mixerStore } from '../stores/mixer.svelte';
  import { keyStore } from '../stores/key.svelte';
  import { arrangementStore } from '../viz/arrangement.svelte';
  import { configStore } from '../stores/config.svelte';
  import { projectActions } from '../stores/project-actions.svelte';
  import {
    DEFAULT_RENDER_LOOPS,
    estimateRender,
    estimateExportSize,
    fmtRenderDuration,
    fmtRenderSize,
  } from '../stores/render.svelte';
  import type { Snippet } from 'svelte';

  // Right-cluster feedback badges (jobs · notifications) injected by the bridge
  // (NemusWindow) — see NemusShell. Optional so the footer renders standalone.
  let { footerExtra }: { footerExtra?: Snippet } = $props();

  const dspPct  = $derived(Math.round(metersStore.dspLoad * 100));
  // DSP load is the audio CPU budget; warn/crit thresholds tint the readout.
  const dspLevel = $derived(
    metersStore.dspLoad >= 0.85 ? 'crit' : metersStore.dspLoad >= 0.6 ? 'warn' : 'ok',
  );
  // The heaviest track (peak polyphony) — surfaced in the voices tooltip so a hot
  // voice budget points straight at the track to thin out.
  const heaviest = $derived(mixerStore.heaviestTrack);
  const voicesTip = $derived(
    heaviest
      ? `${metersStore.voices} voices sounding now · heaviest track: ${heaviest.name} (up to ${heaviest.polyphony})`
      : `${metersStore.voices} voices sounding now`,
  );
  const dspTip = $derived(
    dspLevel === 'ok' ? 'Audio DSP load (CPU budget)' : 'Audio DSP load running hot — thin voices / FX',
  );
  // Detected key + out-of-scale tally (the editor underlines the offending notes).
  const keyTip = $derived.by(() => {
    const a = keyStore.analysis;
    if (!a.spec) return '';
    const cov = Math.round(a.coverage * 100);
    const off = a.offScale.length;
    return off
      ? `Detected key: ${a.label} · ${cov}% in scale · ${off} note${off === 1 ? '' : 's'} out of scale (underlined in the editor)`
      : `Detected key: ${a.label} · ${cov}% in scale`;
  });
  // Live audio-output sample rate (the badge) — distinct from the *render* sample
  // rate used by the estimate below.
  const srLabel = $derived(`${transportStore.sampleRate / 1000} kHz`);

  // Effective tempo: live transport while playing, else the evaluated
  // arrangement's cps (refreshed by every eval / file switch via the central
  // query), falling back to the configured default. So cps + the estimate track
  // the active file even when stopped — not just during playback.
  const renderCps = $derived(arrangementStore.cps ?? configStore.defaultCps);
  const liveCps   = $derived(transportStore.playing ? transportStore.cps : renderCps);
  const cpsLabel  = $derived(Number(liveCps.toPrecision(3)).toString());

  // ── Render estimate (duration · size) ───────────────────────────────────────
  // Mirrors a default export: the arrangement's natural loop period (`loopCycles`)
  // repeated the default number of times, at the evaluated tempo, with the
  // configured render format (sample rate · bit depth · tail) and the *chosen*
  // container — so the size reflects the picked WAV/OGG. The math is shared with
  // the Export dialog (`estimateRender` / `estimateExportSize`) so they agree.
  const totalCycles = $derived(arrangementStore.loopCycles * DEFAULT_RENDER_LOOPS);
  const tailSecs    = $derived(configStore.render.tail_max_secs || 4.0);
  const renderSr    = $derived(configStore.render.sample_rate);
  const bitDepth    = $derived(configStore.render.bit_depth);
  const format      = $derived(projectActions.exportFormat);

  const estimate = $derived(estimateRender({
    cycles:     totalCycles,
    cps:        renderCps,
    tailSecs,
    sampleRate: renderSr,
    bitDepth,
  }));
  const sizeBytes = $derived(estimateExportSize(format, estimate.durationSecs, estimate.sizeBytes));

  const estimateLabel = $derived(
    estimate.durationSecs > 0
      ? `~${fmtRenderDuration(estimate.durationSecs)} · ~${fmtRenderSize(sizeBytes)}`
      : '—',
  );
  const renderSrLabel = $derived(`${renderSr / 1000} kHz`);
  const formatDetail  = $derived(
    format === 'ogg' ? `OGG Vorbis ~192 kbps @ ${renderSrLabel}` : `WAV ${bitDepth} @ ${renderSrLabel}`,
  );
  const estimateTip = $derived(
    estimate.durationSecs > 0
      ? `Render estimate: ${totalCycles.toFixed(1)} cycles @ ${Number(renderCps.toPrecision(3))} cps + ${tailSecs}s tail · stereo ${formatDetail}`
      : 'Render estimate (evaluate an arrangement to see it)',
  );
</script>

<div class="gf">
  <span class="gf-item gf-pos">
    <Activity size={12} />
    <span class:live={transportStore.playing}>{transportStore.position}</span>
  </span>
  <span class="gf-item">cps {cpsLabel}</span>
  {#if keyStore.analysis.spec}
    <span class="gf-item" class:warn={keyStore.hasOffScale} use:tooltip={keyTip}>
      <Music size={12} /> {keyStore.analysis.label}
    </span>
  {/if}
  <span class="gf-sep"></span>
  <span class="gf-item" use:tooltip={voicesTip}><AudioWaveform size={12} /> {metersStore.voices} voices</span>
  <span class="gf-item" class:warn={dspLevel === 'warn'} class:crit={dspLevel === 'crit'} use:tooltip={dspTip}>
    <Cpu size={12} /> {dspPct}% DSP
  </span>
  {#if metersStore.anyClipped}
    <button type="button" class="gf-item gf-clip"
            use:tooltip={`Clipping — ${metersStore.clipCount} source${metersStore.clipCount === 1 ? '' : 's'} hit 0 dBFS. Click to reset.`}
            onclick={() => metersStore.resetClips()}>
      <AlertTriangle size={12} /> CLIP
    </button>
  {/if}
  <span class="gf-sep"></span>
  {#if audioErrorStore.message}
    <span class="gf-item gf-error" use:tooltip={audioErrorStore.message}>
      <AlertTriangle size={12} /> audio error
    </span>
  {:else}
    <span class="gf-item">{srLabel}</span>
  {/if}
  <span class="gf-sep"></span>
  <span class="gf-item gf-estimate" use:tooltip={estimateTip}>
    <Clock size={12} /> {estimateLabel}
  </span>

  <span class="gf-spacer"></span>

  {#if footerExtra}
    {@render footerExtra()}
  {/if}
</div>

<style>
  .gf {
    display: flex; align-items: center; gap: 12px;
    height: 24px; flex-shrink: 0;
    padding: 0 12px;
    background: var(--bg-elevated);
    border-top: 1px solid var(--border-subtle);
    font-size: 11px; color: var(--text-muted);
    user-select: none;
  }
  .gf-item { display: flex; align-items: center; gap: 4px; white-space: nowrap; }
  .gf-item :global(svg) { color: var(--text-disabled); }
  .gf-pos { font-variant-numeric: tabular-nums; }
  .gf-estimate { font-variant-numeric: tabular-nums; }
  .gf-pos .live { color: var(--success); font-weight: 600; }
  /* DSP/CPU budget tint — warn (amber) / crit (red), icon follows the text. */
  .gf-item.warn { color: var(--warning); }
  .gf-item.warn :global(svg) { color: var(--warning); }
  .gf-item.crit { color: var(--error); font-weight: 600; }
  .gf-item.crit :global(svg) { color: var(--error); }
  .gf-error { color: var(--error); }
  .gf-error :global(svg) { color: var(--error); }
  /* Latched clip badge — a reset button styled like a status item. */
  .gf-clip {
    border: none; background: transparent; cursor: pointer;
    font: inherit; padding: 0 4px; height: 16px; border-radius: var(--radius-sm);
    color: var(--error); font-weight: 700; letter-spacing: 0.4px;
  }
  .gf-clip :global(svg) { color: var(--error); }
  .gf-clip:hover { background: color-mix(in srgb, var(--error) 16%, transparent); }
  .gf-clip:focus-visible { outline: none; box-shadow: 0 0 0 2px var(--accent); }
  .gf-spacer { flex: 1; }
  .gf-sep { width: 1px; height: 12px; background: var(--border-subtle); }
</style>
