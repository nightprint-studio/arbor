<script lang="ts">
  /**
   * ExportOptionsModal — step 1 of the two-step WAV export flow ("Edit export").
   *
   * A `Pattern` has no intrinsic length, so the offline bounce renders the
   * arrangement's natural loop period (`arrangementStore.loopCycles`) repeated
   * N times. This dialog is the export's "run configuration": pick the output
   * format, the render-format details (sample rate · bit depth · reverb tail,
   * seeded from Settings → Render but overridable here for one export), and N
   * (Loops) — with a live duration · size estimate BEFORE the save picker, so the
   * user sees exactly what the file will be before committing to a path.
   *
   * Keyboard-first: the first field auto-focuses on open (Modal's focus trap),
   * Esc cancels, Ctrl/Cmd+Enter exports. When the arrangement hasn't been
   * evaluated yet (`loopCycles == 0`) Export is disabled with an inline hint —
   * we can't estimate a length without a known loop period.
   */
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import RenderFormatFields from './RenderFormatFields.svelte';
  import { Download } from 'lucide-svelte';

  import { projectActions } from '../stores/project-actions.svelte';
  import { arrangementStore } from '../viz/arrangement.svelte';
  import { transportStore } from '../stores/engine.svelte';
  import {
    estimateRender,
    estimateExportSize,
    fmtRenderDuration,
    fmtRenderSize,
  } from '../stores/render.svelte';

  // Can't render a meaningful window without a known loop period — gate Export.
  const canExport   = $derived(arrangementStore.loopCycles > 0);
  const totalCycles = $derived(arrangementStore.loopCycles * projectActions.exportLoops);

  const cpsLabel = $derived(Number(transportStore.cps.toPrecision(3)).toString());

  // The estimate (and the format detail) reflect the per-export overrides, so
  // they update live as the user tweaks sample rate / bit depth / tail here —
  // rather than echoing the global config.
  const sampleRate = $derived(projectActions.exportSampleRate);
  const bitDepth   = $derived(projectActions.exportBitDepth);
  const tailSecs   = $derived(projectActions.exportTail);
  const srLabel    = $derived(`${sampleRate / 1000} kHz`);

  const format = $derived(projectActions.exportFormat);
  const formatOptions = [
    { value: 'wav', label: 'WAV — lossless PCM' },
    { value: 'ogg', label: 'OGG Vorbis — compressed' },
  ];
  const estimate = $derived(estimateRender({
    cycles:     totalCycles,
    cps:        transportStore.cps,
    tailSecs,
    sampleRate,
    bitDepth,
  }));

  // Format-aware size (OGG is lossy → bitrate-based), shared with the footer.
  const sizeBytes = $derived(estimateExportSize(format, estimate.durationSecs, estimate.sizeBytes));

  const estimateLabel = $derived(
    estimate.durationSecs > 0
      ? `~${fmtRenderDuration(estimate.durationSecs)} · ~${fmtRenderSize(sizeBytes)}`
      : '—',
  );
  const formatDetail = $derived(
    format === 'ogg'
      ? `stereo Vorbis ~192 kbps @ ${srLabel}`
      : `stereo ${bitDepth} @ ${srLabel}`,
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
  width="600px"
  height="560px"
  ariaLabel="Export options"
>
  {#snippet header()}
    <ModalHeader title="Export audio" onClose={projectActions.cancelExportOptions} />
  {/snippet}

  {#snippet children()}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="body" onkeydown={onKeydown}>
      <div class="card">
        <FormRow label="Format">
          <Select
            value={format}
            options={formatOptions}
            onchange={(v) => projectActions.setExportFormat(v as 'wav' | 'ogg')}
          />
        </FormRow>

        <RenderFormatFields
          sampleRate={projectActions.exportSampleRate}
          bitDepth={projectActions.exportBitDepth}
          tail={projectActions.exportTail}
          onSampleRate={(v) => projectActions.setExportSampleRate(v)}
          onBitDepth={(v) => projectActions.setExportBitDepth(v)}
          onTail={(v) => projectActions.setExportTail(v)}
        />

        <FormRow label="Loops" description="Repeats the arrangement's natural loop period in the exported file.">
          <NumberStepper
            id="export-loops"
            value={projectActions.exportLoops}
            min={1}
            step={1}
            narrow
            ariaLabel="Number of times to repeat the loop"
            onchange={(v) => projectActions.setExportLoops(v)}
          />
        </FormRow>

        <FormRow label="Normalize" description="Match a target integrated loudness (LUFS, BS.1770) across the whole bounce; peak-limited so it never clips.">
          <div class="norm">
            <Toggle
              checked={projectActions.exportNormalizeOn}
              ariaLabel="Normalize loudness"
              onchange={(v) => projectActions.setExportNormalizeOn(v)}
            />
            {#if projectActions.exportNormalizeOn}
              <NumberStepper
                id="export-normalize"
                value={projectActions.exportNormalizeTarget}
                min={-40}
                max={0}
                step={1}
                narrow
                ariaLabel="Target loudness in LUFS"
                onchange={(v) => projectActions.setExportNormalizeTarget(v)}
              />
              <span class="norm-unit">LUFS</span>
            {/if}
          </div>
        </FormRow>
      </div>

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
    gap: 12px;
  }
  /* Bordered group around the form rows (FormRow draws its own row separators),
     matching the Settings → Render card so the two read identically. */
  .card {
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    overflow: hidden;
  }
  .summary {
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
  .norm {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .norm-unit {
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }
</style>
