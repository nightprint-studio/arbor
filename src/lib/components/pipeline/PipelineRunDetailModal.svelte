<script lang="ts">
  /**
   * Standalone modal showing a single local pipeline run — graph + per-step
   * output log. Mounted at the top level (AppShell) and opened by setting
   * `pipelinesStore.activeRunId`. Living outside PipelinesPanel means a
   * plugin can deep-link into a run without forcing the bottom panel open
   * (which would just be obscured by the caller's own modal anyway).
   *
   * The run object is re-derived from the live `runs` array on every tick,
   * so in-flight runs update status / output in real time without requiring
   * the caller to poll.
   */
  import {
    Square, RotateCw, Trash2, Copy, Check, ArrowDownToLine, ArrowLeft,
    CheckCircle, Circle, Ban, AlertCircle, Maximize2, Minimize2,
  } from 'lucide-svelte';
  import { pipelinesStore } from '$lib/stores/pipelines.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import type { StepRun, RunStatus } from '$lib/types/pipeline';
  import { cancelPipelineRun, resumePipelineRun, discardPipelineRun } from '$lib/ipc/pipeline';
  import PipelineRunGraph from './PipelineRunGraph.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import LogStream from '$lib/components/shared/ui/LogStream.svelte';
  import { renderStructuredLogLine, inferLogLevel } from '$lib/utils/log-highlight';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import { tooltip } from '$lib/actions/tooltip';


  // ── Reactive run lookup ────────────────────────────────────────────────────
  const run = $derived.by(() => {
    const id = pipelinesStore.activeRunId;
    if (!id) return null;
    return pipelinesStore.runs.find(r => r.id === id) ?? null;
  });

  // Selected step (for output log). Reset whenever the run changes.
  let selectedStep = $state<{ si: number; ti: number } | null>(null);
  let lastRunId    = $state<string | null>(null);

  // Expanded vs compact sizing. Default is compact: most pipeline graphs are
  // a handful of stages and the original 78vh × 1080px modal left so much
  // empty vertical space that the scrollbar didn't even reach the bottom.
  // The user can toggle expanded for big graphs; switching to a step's
  // output view auto-expands so the log gets its real estate.
  let userExpanded = $state(false);
  const expanded = $derived(userExpanded || !!selectedStep);
  const modalWidth  = $derived(expanded ? 'min(1080px, 96vw)' : 'min(820px, 96vw)');
  const modalHeight = $derived(expanded ? '80vh'              : '48vh');

  // 1Hz wall-clock so the header's elapsed counter ticks for in-flight runs
  // even between backend `arbor://pipeline-update` events.
  let tickNow = $state(Date.now());
  $effect(() => {
    const s = run?.status;
    if (s !== 'running' && s !== 'pending' && s !== 'paused') return;
    const id = setInterval(() => { tickNow = Date.now(); }, 1000);
    return () => clearInterval(id);
  });

  // Auto-select the first failed / running step when we open on a problem
  // run, so the user lands on the error immediately instead of hunting.
  $effect(() => {
    const r = run;
    if (!r) { selectedStep = null; lastRunId = null; return; }
    if (r.id === lastRunId) return;
    lastRunId = r.id;
    selectedStep = null;
    if (r.status === 'failed' || r.status === 'running' || r.status === 'paused' || r.status === 'cancelled') {
      for (let si = 0; si < r.stages.length; si++) {
        const st = r.stages[si];
        for (let ti = 0; ti < st.steps.length; ti++) {
          const stat = st.steps[ti].status;
          if (stat === 'failed' || stat === 'running' || stat === 'paused' || stat === 'cancelled') {
            selectedStep = { si, ti };
            return;
          }
        }
      }
    }
  });

  const stepRun: StepRun | null = $derived.by(() => {
    if (!run || !selectedStep) return null;
    return run.stages[selectedStep.si]?.steps[selectedStep.ti] ?? null;
  });

  // ── Output follow-mode (tail -f style) ────────────────────────────────────
  // LogStream owns the scroll viewport + auto-pin behavior; we just bind
  // `autoScroll` so the toolbar's Follow button stays in sync with manual
  // scroll-up pauses.
  let logStream: LogStream | undefined = $state();
  let autoScroll = $state(true);

  function toggleFollow() {
    if (autoScroll) autoScroll = false;
    else            logStream?.scrollToBottom();
  }

  // Per-line CSS class — `[stderr]`-prefixed lines get a red tint, matching
  // the convention used by JobOutputPanel so the user reads both surfaces
  // the same way.
  function lineClass(line: string): string | undefined {
    return line.startsWith('[stderr]') ? 'line-stderr' : undefined;
  }

  // Per-line HTML — synthesise a structured
  //   `<time> LEVEL [plugin] #<run> message`
  // prefix so the step view matches the global Plugin Logs panel visually.
  // The runtime captures all step lines in batch (no per-line timestamps),
  // so we anchor the time on the step's `started_at`; the level is inferred
  // per line via the same heuristic the backend uses when mirroring lines
  // into the global log buffer (kept in sync via `inferLogLevel`); the
  // run id is the live run's id (rendered as `#N` after stripping the
  // `pipe-run-` prefix, just like the modal header).
  function lineHtml(line: string): string {
    if (!run || !stepRun) return line;
    return renderStructuredLogLine({
      ts_ms:   stepRun.started_at || stepRun.finished_at || run.started_at || Date.now(),
      level:   inferLogLevel(line),
      plugin:  run.plugin,
      run_id:  run.id,
      message: line,
    });
  }

  let copied = $state(false);
  let copyTimer: ReturnType<typeof setTimeout> | null = null;

  async function copyOutput() {
    if (!stepRun || stepRun.output.length === 0) return;
    if (await copyToClipboard(stepRun.output.join('\n'), { errorToast: true })) {
      copied = true;
      if (copyTimer) clearTimeout(copyTimer);
      copyTimer = setTimeout(() => { copied = false; }, 1800);
    }
  }

  // ── Actions ───────────────────────────────────────────────────────────────
  async function doCancel() {
    if (!run) return;
    try {
      await cancelPipelineRun(run.id);
      // Acknowledge the click — the actual transition to Cancelled lands
      // asynchronously when the orchestrator's watcher tears down the
      // running process tree, but the user needs immediate feedback.
      uiStore.showToast('Cancel requested — terminating current step…', 'info');
    } catch (err) {
      uiStore.showToast(`Cancel failed: ${err}`, 'error');
    }
  }
  async function doResume() {
    if (!run) return;
    try { await resumePipelineRun(run.id); } catch (err) {
      uiStore.showToast(`Resume failed: ${err}`, 'error');
    }
  }
  async function doDiscard() {
    if (!run) return;
    try {
      await discardPipelineRun(run.id);
      pipelinesStore.setActiveRun(null);
    } catch (err) {
      uiStore.showToast(`Discard failed: ${err}`, 'error');
    }
  }

  function close() { pipelinesStore.setActiveRun(null); }

  function isTerminal(s: RunStatus) {
    return s === 'success' || s === 'failed' || s === 'cancelled';
  }

  // ── Formatting helpers ────────────────────────────────────────────────────
  function formatTs(ms: number | null | undefined): string {
    if (!ms) return '—';
    return new Date(ms).toLocaleString();
  }
  function elapsed(startedAt: number | null | undefined, finishedAt: number | null | undefined): string {
    if (!startedAt) return '—';
    const end = finishedAt && finishedAt > 0 ? finishedAt : tickNow;
    const ms  = Math.max(0, end - startedAt);
    if (ms < 1000) return `${ms}ms`;
    const s = Math.floor(ms / 1000);
    if (s < 60) return `${s}s`;
    const m = Math.floor(s / 60);
    return `${m}m ${String(s % 60).padStart(2, '0')}s`;
  }

  function statusIcon(s: RunStatus) {
    switch (s) {
      case 'success':   return CheckCircle;
      case 'failed':    return AlertCircle;
      // 'running' is rendered with <Spinner> instead of an icon — see header.
      case 'cancelled': return Ban;
      default:          return Circle;
    }
  }
  function statusLabel(s: RunStatus): string {
    return s.charAt(0).toUpperCase() + s.slice(1);
  }
</script>

{#if run}
  {@const HeaderStatusIcon = statusIcon(run.status)}
  <Modal onClose={close} width={modalWidth} height={modalHeight} padBody={false} ariaLabel="Pipeline run detail">
    {#snippet header()}
      <ModalHeader onClose={close}>
        <span class="prd-status prd-status-{run.status}">
          {#if run.status === 'running'}
            <Spinner size={12} color="currentColor" />
          {:else}
            <HeaderStatusIcon size={12} />
          {/if}
          {statusLabel(run.status)}
        </span>
        <span class="prd-title">{run.name}</span>
        <span class="prd-id">#{run.id.replace('pipe-run-', '')}</span>
        <span class="prd-meta">{formatTs(run.started_at)} · {elapsed(run.started_at, run.finished_at)}</span>
        {#snippet actions()}
          {#if run.status === 'running'}
            <button class="prd-btn" use:tooltip={'Cancel'} onclick={doCancel}>
              <Square size={12} />
            </button>
          {/if}
          {#if run.status === 'failed' || run.status === 'paused' || run.status === 'cancelled'}
            <button class="prd-btn" use:tooltip={'Resume'} onclick={doResume}>
              <RotateCw size={12} />
            </button>
          {/if}
          {#if isTerminal(run.status)}
            <button class="prd-btn prd-btn-danger" use:tooltip={'Discard'} onclick={doDiscard}>
              <Trash2 size={12} />
            </button>
          {/if}
          <!-- Toggle is hidden in step-output view: that mode auto-expands
               and the back arrow is the user's exit affordance. -->
          {#if !stepRun}
            <button class="prd-btn"
                    use:tooltip={userExpanded ? 'Shrink' : 'Expand'}
                    onclick={() => userExpanded = !userExpanded}>
              {#if userExpanded}
                <Minimize2 size={12} />
              {:else}
                <Maximize2 size={12} />
              {/if}
            </button>
          {/if}
        {/snippet}
      </ModalHeader>
    {/snippet}

    <!-- Body: either the run graph (when no step is selected) or a
         JobOutputPanel-style log view for the selected step (back arrow
         to return + LogStream with shared highlighter). Two modes, never
         both — clicking a step swaps the whole body so the log gets the
         full vertical room of the modal. -->
    <div class="prd-body">
      {#if stepRun}
        <div class="prd-step-view">
          <div class="prd-step-header">
            <button class="prd-btn prd-back-btn" use:tooltip={'Back to graph'}
                    onclick={() => selectedStep = null}>
              <ArrowLeft size={14} />
            </button>
            <span class="prd-step-sep"></span>
            <span class="prd-output-dot status-{stepRun.status}"></span>
            <span class="prd-output-name">{stepRun.name}</span>
            {#if stepRun.output.length > 0}
              <span class="prd-output-kind">
                {stepRun.output.length} {stepRun.output.length === 1 ? 'line' : 'lines'}
              </span>
            {/if}
            {#if stepRun.exit_code !== null}
              <span class="prd-output-exit" class:prd-output-exit-ok={stepRun.exit_code === 0}>
                exit {stepRun.exit_code}
              </span>
            {/if}
            <span class="prd-spacer"></span>
            <button class="prd-btn"
                    class:prd-btn-active={autoScroll}
                    use:tooltip={autoScroll ? 'Following — click to pause' : 'Follow output'}
                    onclick={toggleFollow}>
              <ArrowDownToLine size={12} />
              <span class="prd-btn-label">Follow</span>
            </button>
            <button class="prd-btn"
                    class:prd-btn-copied={copied}
                    use:tooltip={'Copy output'}
                    onclick={copyOutput}
                    disabled={stepRun.output.length === 0}>
              {#if copied}
                <Check size={12} />
                <span class="prd-btn-label">Copied</span>
              {:else}
                <Copy size={12} />
                <span class="prd-btn-label">Copy</span>
              {/if}
            </button>
          </div>
          <div class="prd-step-stream">
            <LogStream
              bind:this={logStream}
              bind:autoScroll
              lines={stepRun.output}
              {lineClass}
              {lineHtml}
              ansi={false}
              waiting={stepRun.status === 'running'}
              waitingMessage="Waiting for output…"
              emptyMessage="No output captured yet."
            />
          </div>
        </div>
      {:else}
        <div class="prd-graph">
          <PipelineRunGraph
            {run}
            {selectedStep}
            onSelectStep={(si, ti) => {
              selectedStep = (selectedStep?.si === si && selectedStep?.ti === ti)
                ? null : { si, ti };
            }}
          />
        </div>
      {/if}
    </div>
  </Modal>
{/if}

<style>
  .prd-title {
    font-size: 13px; font-weight: 600; color: var(--text-primary);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    max-width: 360px;
  }
  .prd-id     { font-family: var(--font-code); font-size: 11px; color: var(--text-muted); }
  .prd-meta   { font-size: 11px; color: var(--text-muted); }
  .prd-spacer { flex: 1; }

  .prd-status {
    display: inline-flex; align-items: center; gap: 4px;
    padding: 2px 8px; border-radius: 999px;
    font-size: 11px; font-weight: 600;
    border: 1px solid var(--border-subtle);
    background: var(--bg-hover); color: var(--text-secondary);
    flex-shrink: 0;
  }
  .prd-status-success { color: var(--success); border-color: color-mix(in srgb, var(--success) 33%, transparent); background: color-mix(in srgb, var(--success) 12%, transparent); }
  .prd-status-failed  { color: var(--error);   border-color: color-mix(in srgb, var(--error) 33%, transparent);   background: color-mix(in srgb, var(--error) 12%, transparent); }
  .prd-status-running { color: var(--accent); border-color: var(--accent); background: var(--accent-subtle); }
  .prd-status-cancelled { color: var(--text-muted); }

  .prd-btn {
    display: inline-flex; align-items: center; justify-content: center; gap: 4px;
    height: 22px; min-width: 22px; padding: 0 6px;
    background: transparent; border: 1px solid transparent; border-radius: var(--radius-sm);
    color: var(--text-secondary); cursor: pointer;
    font-family: var(--font-ui-sans); font-size: var(--font-size-xs);
    transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
  }
  .prd-btn:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); border-color: var(--border-subtle); }
  .prd-btn:disabled { opacity: 0.35; cursor: not-allowed; }
  .prd-btn-danger:hover:not(:disabled) { color: var(--error); border-color: var(--error); }
  .prd-btn-active { color: var(--accent); border-color: var(--accent); background: var(--accent-subtle); }
  .prd-btn-copied { color: var(--success); }
  .prd-btn-label { font-size: 11px; }

  /* Body fills the Modal's body card. */
  .prd-body {
    height: 100%;
    min-height: 0;
    display: flex; flex-direction: column;
    overflow: hidden;
    font-family: var(--font-ui-sans);
  }
  .prd-graph {
    flex: 1; min-height: 0;
    overflow: auto;
    background: var(--bg-base);
    padding: 14px;
  }

  /* ── Step log view (replaces the body when a step is selected) ──────── */
  /* Mirrors JobOutputPanel / PluginLogsPanel: a chrome row with the back
     arrow + status info + Follow/Copy actions, then a `<LogStream>` that
     fills the remaining vertical space and tail-follows new output. */
  .prd-step-view {
    flex: 1; min-height: 0;
    display: flex; flex-direction: column;
    background: var(--bg-base);
  }
  .prd-step-header {
    display: flex; align-items: center; gap: 8px;
    padding: 6px 10px;
    border-bottom: 1px solid var(--border-subtle);
    background: var(--bg-elevated);
    flex-shrink: 0;
  }
  .prd-back-btn { color: var(--text-secondary); }
  .prd-step-sep {
    display: inline-block; width: 1px; height: 14px;
    background: var(--border-subtle); flex-shrink: 0; margin: 0 2px;
  }
  .prd-output-dot { width: 8px; height: 8px; border-radius: 50%; background: var(--text-muted); flex-shrink: 0; }
  .prd-output-dot.status-success   { background: var(--success); }
  .prd-output-dot.status-failed    { background: var(--error); }
  .prd-output-dot.status-running   { background: var(--accent); }
  .prd-output-dot.status-cancelled { background: var(--text-muted); }
  .prd-output-name {
    font-size: 12px; font-weight: 600; color: var(--text-primary);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    max-width: 320px;
  }
  .prd-output-kind { font-size: 10px; color: var(--text-muted); text-transform: uppercase; letter-spacing: .5px; }
  .prd-output-exit {
    font-family: var(--font-code); font-size: 11px; font-weight: 600;
    color: var(--error);
    background: color-mix(in srgb, var(--error) 12%, transparent);
    border-radius: var(--radius-sm);
    padding: 1px 5px;
  }
  .prd-output-exit-ok {
    color: var(--success);
    background: color-mix(in srgb, var(--success) 12%, transparent);
  }
  .prd-step-stream {
    flex: 1; min-height: 0;
    display: flex; flex-direction: column;
    background: var(--bg-base);
    overflow: hidden;
  }

  /* stderr lines get the same red-tinted text JobOutputPanel uses, applied
     via the `lineClass` callback on the LogStream — the rule is :global
     because LogStream owns the .log-line element. */
  :global(.prd-step-stream .log-line.line-stderr) {
    color: var(--terminal-bright-red, #e06c6c);
  }
</style>
