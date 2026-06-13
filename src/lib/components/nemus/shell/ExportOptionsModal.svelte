<script lang="ts">
  /**
   * ExportOptionsModal — step 1 of the two-step WAV export flow.
   *
   * A `Pattern` has no intrinsic length, so the offline bounce renders the
   * arrangement's natural loop period (`arrangementStore.loopCycles`) repeated
   * N times. This dialog lets the user pick N (Loops) and shows the resulting
   * cycle count plus a live duration · size estimate BEFORE the save picker, so
   * they see exactly what the WAV will be before committing to a path.
   *
   * Keyboard-first: the Loops field auto-focuses on open (Modal's focus trap),
   * Esc cancels, Ctrl/Cmd+Enter exports. When the arrangement hasn't been
   * evaluated yet (`loopCycles == 0`) Export is disabled with an inline hint —
   * we can't estimate a length without a known loop period.
   */
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import { Download } from 'lucide-svelte';

  import { projectActions } from '../stores/project-actions.svelte';
  import { arrangementStore } from '../viz/arrangement.svelte';
  import { transportStore } from '../stores/engine.svelte';
  import { configStore } from '../stores/config.svelte';
  import {
    estimateRender,
    fmtRenderDuration,
    fmtRenderSize,
  } from '../stores/render.svelte';

  // Can't render a meaningful window without a known loop period — gate Export.
  const canExport   = $derived(arrangementStore.loopCycles > 0);
  const totalCycles = $derived(arrangementStore.loopCycles * projectActions.exportLoops);

  const cpsLabel = $derived(Number(transportStore.cps.toPrecision(3)).toString());
  const srLabel  = $derived(`${transportStore.sampleRate / 1000} kHz`);
  const tailSecs = $derived(configStore.render.tail_max_secs || 4.0);

  const format = $derived(projectActions.exportFormat);
  const formatOptions = [
    { value: 'wav', label: 'WAV — lossless PCM' },
    { value: 'ogg', label: 'OGG Vorbis — compressed' },
  ];
  /** Rough VBR bitrate (~q0.6) used for the OGG size estimate. */
  const OGG_BITRATE = 192_000;

  const estimate = $derived(estimateRender({
    cycles:     totalCycles,
    cps:        transportStore.cps,
    tailSecs,
    sampleRate: transportStore.sampleRate,
    bitDepth:   configStore.render.bit_depth,
  }));

  // OGG is lossy: estimate size from a nominal bitrate, not the PCM bit depth.
  const sizeBytes = $derived(
    format === 'ogg' ? Math.round((estimate.durationSecs * OGG_BITRATE) / 8) : estimate.sizeBytes,
  );

  const estimateLabel = $derived(
    estimate.durationSecs > 0
      ? `~${fmtRenderDuration(estimate.durationSecs)} · ~${fmtRenderSize(sizeBytes)}`
      : '—',
  );
  const formatDetail = $derived(
    format === 'ogg'
      ? `stereo Vorbis ~192 kbps @ ${srLabel}`
      : `stereo ${configStore.render.bit_depth ?? 'int24'} @ ${srLabel}`,
  );

  function submit() {
    if (!canExport) return;
    projectActions.confirmExportOptions();
  }

  // Ctrl/Cmd+Enter submits from anywhere in the dialog (the Loops field is a
  // number input, so plain Enter would otherwise just commit the value).
  function onKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      submit();
    }
  }
</script>

<Modal
  onClose={projectActions.cancelExportOptions}
  width="560px"
  height="380px"
  ariaLabel="Export options"
>
  {#snippet header()}
    <ModalHeader title="Export audio" onClose={projectActions.cancelExportOptions} />
  {/snippet}

  {#snippet children()}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="body" onkeydown={onKeydown}>
      <div class="row">
        <label class="row-label" for="export-format">Format</label>
        <div class="row-control">
          <Select
            id="export-format"
            value={format}
            options={formatOptions}
            onchange={(v) => projectActions.setExportFormat(v as 'wav' | 'ogg')}
          />
        </div>
      </div>

      <div class="row">
        <label class="row-label" for="export-loops">Loops</label>
        <div class="row-control">
          <NumberStepper
            id="export-loops"
            value={projectActions.exportLoops}
            min={1}
            step={1}
            narrow={false}
            ariaLabel="Number of times to repeat the loop"
            onchange={(v) => projectActions.setExportLoops(v)}
          />
        </div>
      </div>
      <p class="hint">
        Repeats the arrangement's natural loop period in the exported file.
      </p>

      <div class="summary">
        <div class="summary-row">
          <span class="summary-key">Cycles</span>
          <span class="summary-val">
            {#if canExport}
              {totalCycles.toFixed(1)}
              <span class="summary-detail">
                ({arrangementStore.loopCycles.toFixed(1)} × {projectActions.exportLoops})
              </span>
            {:else}
              —
            {/if}
          </span>
        </div>
        <div class="summary-row">
          <span class="summary-key">Estimate</span>
          <span class="summary-val">
            {estimateLabel}
            {#if canExport}
              <span class="summary-detail">
                {formatDetail} · {cpsLabel} cps + {tailSecs}s tail
              </span>
            {/if}
          </span>
        </div>
      </div>

      {#if !canExport}
        <p class="warn">Evaluate the arrangement first to set a render length.</p>
      {/if}
    </div>
  {/snippet}

  {#snippet footer()}
    <ModalFooter>
      <Button variant="ghost" onclick={projectActions.cancelExportOptions}>Cancel</Button>
      <Button
        variant="primary"
        disabled={!canExport}
        onclick={submit}
        tooltip={{ content: 'Choose a file and export', shortcut: 'Ctrl+Enter' }}
      >
        {#snippet iconStart()}<Download size={14} />{/snippet}
        Export…
      </Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .body {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .row-label {
    width: 64px;
    flex-shrink: 0;
    font-size: var(--font-size-sm);
    color: var(--text-secondary);
  }
  .row-control {
    width: 140px;
  }
  .hint {
    margin: 0;
    font-size: var(--font-size-xs);
    color: var(--text-muted);
  }
  .summary {
    margin-top: 4px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 12px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
  }
  .summary-row {
    display: flex;
    align-items: baseline;
    gap: 10px;
  }
  .summary-key {
    width: 60px;
    flex-shrink: 0;
    font-size: var(--font-size-xs);
    text-transform: uppercase;
    letter-spacing: 0.4px;
    color: var(--text-muted);
  }
  .summary-val {
    font-size: var(--font-size-sm);
    color: var(--text-primary);
    font-variant-numeric: tabular-nums;
  }
  .summary-detail {
    color: var(--text-muted);
    font-variant-numeric: normal;
  }
  .warn {
    margin: 0;
    font-size: var(--font-size-xs);
    color: var(--warning, var(--text-secondary));
  }
</style>
