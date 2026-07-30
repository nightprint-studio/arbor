<script lang="ts">
  /**
   * CapturePanel — the capture surface: a colourful mode-aware "stage" hero that
   * previews the current target + settings, then the source picker and the
   * mode-specific options. The mode switcher lives in the titlebar (reduced tab
   * strip) and the primary action (Record / Stop / Screenshot) does too.
   */
  import { Monitor, AppWindow, Crop, Circle } from 'lucide-svelte';
  import { fly, scale, fade } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import SourcePicker from './SourcePicker.svelte';
  import CaptureOptions from './CaptureOptions.svelte';
  import { animStore } from '$lib/stores/animations.svelte';
  import { recorderStore, formatDuration, type TargetKind } from '$lib/stores/tyto/recorder.svelte';

  // Source kind switch, sitting right after the "Source" heading (Ctrl+Shift+S
  // cycles it from the keyboard). Single source of truth for the three targets.
  const kindOptions = [
    { value: 'monitor', label: 'Monitor', icon: Monitor,   description: 'Capture a whole display' },
    { value: 'window',  label: 'Window',  icon: AppWindow, description: 'Capture a single app window' },
    { value: 'region',  label: 'Region',  icon: Crop,      description: 'Drag a rectangular area' },
  ];

  const TargetIcon = $derived(
    recorderStore.targetKind === 'monitor' ? Monitor :
    recorderStore.targetKind === 'window'  ? AppWindow : Crop,
  );

  const audioLabel = $derived(
    recorderStore.systemAudio && recorderStore.micId ? 'System + mic audio' :
    recorderStore.systemAudio ? 'System audio' :
    recorderStore.micId ? 'Microphone only' : 'No audio',
  );

  const detailLine = $derived(
    recorderStore.mode === 'record'
      ? `${recorderStore.fps} fps · ${recorderStore.quality} · ${audioLabel}`
      : 'PNG · saved to file + clipboard',
  );
</script>

<div class="capture">
  <div class="scroll">
    <!-- Colourful stage: the focal preview of what's about to be captured.
         Slides in on open; the target glyph pops when the source kind changes. -->
    <div class="stage" data-mode={recorderStore.mode} in:fly={{ y: 10, duration: animStore.dPanel, easing: cubicOut }}>
      <div class="stage-icon">
        {#key recorderStore.targetKind}
          <span class="ti" in:scale={{ duration: animStore.dBase, start: 0.55, easing: cubicOut }}>
            <TargetIcon size={30} />
          </span>
        {/key}
      </div>
      <div class="stage-body">
        <div class="stage-kicker">{recorderStore.mode === 'record' ? 'Recording' : 'Screenshot'} · {recorderStore.targetKind}</div>
        {#key recorderStore.currentTargetLabel}
          <div class="stage-target" in:fade={{ duration: animStore.dFast }}>{recorderStore.currentTargetLabel}</div>
        {/key}
        <div class="stage-detail">{detailLine}</div>
      </div>
      <div class="stage-status">
        {#if recorderStore.recording}
          <span class="stage-rec"><Circle size={9} fill="currentColor" /> REC {formatDuration(recorderStore.elapsedMs)}</span>
        {:else if recorderStore.targetReady}
          <span class="stage-ready">Ready</span>
        {:else}
          <span class="stage-wait">Pick a region</span>
        {/if}
      </div>
    </div>

    <section>
      <div class="section-head">
        <h3 class="section-label">Source</h3>
        <div class="source-switch">
          <RadioGroup
            appearance="segment"
            value={recorderStore.targetKind}
            options={kindOptions}
            onchange={(v) => recorderStore.setTargetKind(v as TargetKind)}
          />
        </div>
      </div>
      <SourcePicker />
    </section>

    <section>
      <h3 class="section-label">{recorderStore.mode === 'record' ? 'Recording options' : 'Screenshot options'}</h3>
      <CaptureOptions />
    </section>
  </div>
</div>

<style>
  .capture { display: flex; flex-direction: column; height: 100%; width: 100%; min-width: 0; }

  .scroll {
    flex: 1;
    overflow: auto;
    padding: 18px 20px 20px;
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  /* ── Stage hero ─────────────────────────────────────────────────────────── */
  .stage {
    position: relative;
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 18px 20px;
    border-radius: var(--radius-lg);
    border: 1px solid var(--border-subtle);
    overflow: hidden;
    /* Never let the hero compress when the body overflows and scrolls. */
    flex-shrink: 0;
    transition: border-color var(--transition-base, 0.25s);
  }
  /* Mode-aware tint — colour carries the state (record = red, screenshot = accent). */
  .stage[data-mode='record'] {
    --stage-c: var(--error);
    background:
      radial-gradient(120% 140% at 100% 0%, color-mix(in srgb, var(--error) 26%, transparent), transparent 60%),
      linear-gradient(135deg, color-mix(in srgb, var(--error) 10%, var(--bg-elevated)), var(--bg-base));
    border-color: color-mix(in srgb, var(--error) 26%, var(--border-subtle));
  }
  .stage[data-mode='screenshot'] {
    --stage-c: var(--accent);
    background:
      radial-gradient(120% 140% at 100% 0%, color-mix(in srgb, var(--accent) 28%, transparent), transparent 60%),
      linear-gradient(135deg, color-mix(in srgb, var(--accent) 12%, var(--bg-elevated)), var(--bg-base));
    border-color: color-mix(in srgb, var(--accent) 28%, var(--border-subtle));
  }

  .stage-icon {
    position: relative;
    display: flex; align-items: center; justify-content: center;
    width: 58px; height: 58px; flex-shrink: 0;
    border-radius: var(--radius-lg);
    color: #fff;
    background: linear-gradient(140deg,
      color-mix(in srgb, var(--stage-c) 92%, #fff),
      color-mix(in srgb, var(--stage-c) 78%, #000));
    box-shadow: 0 6px 18px color-mix(in srgb, var(--stage-c) 45%, transparent), inset 0 0 0 1px rgba(255,255,255,0.12);
  }
  .stage-icon .ti { display: inline-flex; }

  .stage-body { flex: 1; min-width: 0; }
  .stage-kicker {
    font-size: var(--font-size-2xs); font-weight: 700; text-transform: uppercase; letter-spacing: 0.8px;
    color: color-mix(in srgb, var(--stage-c) 75%, var(--text-muted));
  }
  .stage-target {
    font-size: 17px; font-weight: 680; color: var(--text-primary);
    margin-top: 3px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .stage-detail { font-size: var(--font-size-xs); color: var(--text-secondary); margin-top: 3px; font-variant-numeric: tabular-nums; }

  .stage-status { flex-shrink: 0; align-self: flex-start; }
  .stage-rec {
    display: inline-flex; align-items: center; gap: 5px;
    font-size: var(--font-size-xs); font-weight: 700; color: #fff;
    background: var(--error); padding: 3px 9px; border-radius: 999px;
    font-variant-numeric: tabular-nums;
  }
  .stage-rec :global(svg) { animation: st-pulse 1.3s ease-in-out infinite; }
  @keyframes st-pulse { 0%,100% { opacity: 1; } 50% { opacity: 0.3; } }
  .stage-ready {
    font-size: var(--font-size-xs); font-weight: 600;
    color: var(--success);
    background: color-mix(in srgb, var(--success) 15%, transparent);
    padding: 3px 10px; border-radius: 999px;
  }
  .stage-wait {
    font-size: var(--font-size-xs); font-weight: 600;
    color: var(--warning);
    background: color-mix(in srgb, var(--warning) 15%, transparent);
    padding: 3px 10px; border-radius: 999px;
  }

  /* ── Sections ───────────────────────────────────────────────────────────── */
  section { display: flex; flex-direction: column; gap: 10px; flex-shrink: 0; }
  .section-label {
    margin: 0;
    font-size: var(--font-size-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.7px;
    color: var(--text-muted);
  }
  /* Heading row: the label on the left, the source switch on the right. Wraps
     gracefully if the panel gets narrow. */
  .section-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    flex-wrap: wrap;
  }
  .source-switch { -webkit-app-region: no-drag; }
</style>
