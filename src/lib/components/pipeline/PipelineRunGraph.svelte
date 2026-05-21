<script lang="ts">
  import type { PipelineRun, StageRun, StepRun, RunStatus } from '$lib/types/pipeline';

  interface Props {
    run:            PipelineRun;
    onSelectStep?:  (stageIdx: number, stepIdx: number) => void;
    selectedStep?:  { si: number; ti: number } | null;
  }

  let { run, onSelectStep, selectedStep = null }: Props = $props();

  // Backend `arbor://pipeline-update` events only fire on step/stage
  // boundaries, so between transitions a running step's elapsed time would
  // freeze and pending stages would look static. A 1Hz ticker, active only
  // while the run is in-flight, keeps `elapsedMs()` re-rendering against a
  // moving `Date.now()` so the "in working" / "in attesa" UI feels live.
  let tickNow = $state(Date.now());
  $effect(() => {
    if (run.status !== 'running' && run.status !== 'pending' && run.status !== 'paused') return;
    const id = setInterval(() => { tickNow = Date.now(); }, 1000);
    return () => clearInterval(id);
  });

  // ── Layout constants ─────────────────────────────────────────────────────
  const STAGE_WIDTH  = 200;
  const STAGE_GAP    = 36;
  const NODE_HEIGHT  = 34;
  const NODE_GAP     = 6;
  const HEADER_H     = 32;
  const STAGE_PAD    = 10;
  const CONNECTOR_H  = 20;   // vertical space between stage columns

  function truncate(s: string, n: number): string {
    if (!s) return '';
    return s.length <= n ? s : s.slice(0, Math.max(0, n - 1)) + '…';
  }

  // ── Colour map ───────────────────────────────────────────────────────────
  function statusColor(s: RunStatus): string {
    switch (s) {
      case 'success':   return 'var(--status-success, #4ade80)';
      case 'failed':    return 'var(--status-error,   #f87171)';
      case 'running':   return 'var(--accent)';
      case 'cancelled': return 'var(--text-muted)';
      default:          return 'var(--border)';
    }
  }

  function statusFill(s: RunStatus): string {
    switch (s) {
      case 'success':   return 'rgba(74,222,128,0.12)';
      case 'failed':    return 'rgba(248,113,113,0.12)';
      case 'running':   return 'var(--accent-subtle)';
      case 'cancelled': return 'rgba(120,120,120,0.08)';
      default:          return 'rgba(120,120,120,0.04)';
    }
  }

  function statusIcon(s: RunStatus): string {
    switch (s) {
      case 'success':   return '✓';
      case 'failed':    return '✗';
      case 'running':   return '●';
      case 'cancelled': return '⊘';
      default:          return '○';
    }
  }

  // ── Derived geometry ─────────────────────────────────────────────────────
  interface StageLayout {
    x:      number;
    y:      number;
    width:  number;
    height: number;
    stage:  StageRun;
    si:     number;
    steps:  StepLayout[];
  }
  interface StepLayout {
    x: number; y: number; w: number; h: number;
    step: StepRun; si: number; ti: number;
  }

  const layout = $derived.by(() => {
    const stages: StageLayout[] = [];
    let cx = STAGE_PAD;

    for (let si = 0; si < run.stages.length; si++) {
      const stage = run.stages[si];
      const stepsH = stage.steps.length * (NODE_HEIGHT + NODE_GAP) - NODE_GAP;
      const colH   = HEADER_H + 10 + stepsH + STAGE_PAD;

      const stepLayouts: StepLayout[] = stage.steps.map((step, ti) => ({
        x: cx + STAGE_PAD,
        y: STAGE_PAD + HEADER_H + 10 + ti * (NODE_HEIGHT + NODE_GAP),
        w: STAGE_WIDTH - STAGE_PAD * 2,
        h: NODE_HEIGHT,
        step, si, ti,
      }));

      stages.push({
        x: cx, y: STAGE_PAD,
        width: STAGE_WIDTH, height: colH,
        stage, si, steps: stepLayouts,
      });
      cx += STAGE_WIDTH + STAGE_GAP;
    }

    const svgW = cx - STAGE_GAP + STAGE_PAD;
    const svgH = Math.max(...stages.map(s => s.y + s.height)) + STAGE_PAD;
    return { stages, svgW, svgH };
  });

  function elapsedMs(r: PipelineRun | StepRun): string {
    const start = r.started_at;
    const end   = r.finished_at ?? tickNow;
    if (!start) return '';
    const ms = end - start;
    if (ms < 1000) return `${ms}ms`;
    if (ms < 60000) return `${(ms/1000).toFixed(1)}s`;
    return `${Math.floor(ms/60000)}m ${Math.floor((ms%60000)/1000)}s`;
  }
</script>

<div class="graph-wrap">
  <svg
    width={layout.svgW}
    height={layout.svgH}
    viewBox="0 0 {layout.svgW} {layout.svgH}"
    class="pipeline-svg"
    role="img"
    aria-label="Pipeline run graph"
  >
    <!-- ── Connectors between stages ─────────────────────────────────── -->
    {#each layout.stages as col, i}
      {#if i < layout.stages.length - 1}
        {@const nextCol = layout.stages[i + 1]}
        {@const x1 = col.x + col.width}
        {@const x2 = nextCol.x}
        {@const y  = col.y + col.height / 2}
        <line
          x1={x1} y1={y} x2={x2} y2={y}
          stroke={statusColor(col.stage.status)}
          stroke-width="2"
          stroke-dasharray={col.stage.status === 'pending' ? '4 4' : 'none'}
          opacity="0.6"
        />
        <!-- Arrow head -->
        <polygon
          points="{x2},{y} {x2-8},{y-5} {x2-8},{y+5}"
          fill={statusColor(col.stage.status)}
          opacity="0.7"
        />
      {/if}
    {/each}

    <!-- ── Stage columns ─────────────────────────────────────────────── -->
    {#each layout.stages as col}
      <!-- Stage container — subdued border, clear background -->
      <rect
        x={col.x} y={col.y}
        width={col.width} height={col.height}
        rx="8" ry="8"
        fill="var(--bg-elevated)"
        stroke="var(--border-subtle)"
        stroke-width="1"
      />
      <!-- Left accent bar reflecting stage status -->
      <rect
        x={col.x} y={col.y}
        width="3" height={col.height}
        rx="8" ry="8"
        fill={statusColor(col.stage.status)}
        opacity="0.85"
      />

      <!-- Stage header background -->
      <rect
        x={col.x + 1} y={col.y + 1}
        width={col.width - 2} height={HEADER_H - 2}
        rx="7" ry="7"
        fill="var(--bg-overlay)"
        opacity="0.5"
      />

      <!-- Stage status dot -->
      <circle
        cx={col.x + 16}
        cy={col.y + HEADER_H / 2}
        r="5"
        fill={statusColor(col.stage.status)}
      />

      <!-- Stage header label (truncated) -->
      <text
        x={col.x + 26}
        y={col.y + HEADER_H / 2}
        dominant-baseline="central"
        class="stage-label"
        fill="var(--text-primary)"
      >{truncate(col.stage.name, 22)}</text>

      <!-- Step count badge -->
      <text
        x={col.x + col.width - 10}
        y={col.y + HEADER_H / 2}
        text-anchor="end"
        dominant-baseline="central"
        class="stage-meta"
        fill="var(--text-muted)"
      >{col.stage.steps.length} step</text>

      <!-- Divider under stage header -->
      <line
        x1={col.x + 1}
        y1={col.y + HEADER_H}
        x2={col.x + col.width - 1}
        y2={col.y + HEADER_H}
        stroke="var(--border-subtle)"
        stroke-width="1"
      />

      <!-- Step nodes -->
      {#each col.steps as sl}
        {@const isSelected = selectedStep?.si === sl.si && selectedStep?.ti === sl.ti}
        {@const isFailed   = sl.step.status === 'failed'}
        {@const isRunning  = sl.step.status === 'running'}
        <!-- svelte-ignore a11y_click_events_have_key_events a11y_interactive_supports_focus -->
        <g
          class="step-node"
          class:step-selected={isSelected}
          onclick={() => onSelectStep?.(sl.si, sl.ti)}
          role="button"
          tabindex="0"
          aria-label="{sl.step.name} — {sl.step.status}"
          onkeydown={(e) => e.key === 'Enter' && onSelectStep?.(sl.si, sl.ti)}
        >
          <rect
            x={sl.x} y={sl.y}
            width={sl.w} height={sl.h}
            rx="5" ry="5"
            fill={isSelected
              ? statusFill(sl.step.status)
              : (isFailed ? 'rgba(248,113,113,0.06)' : 'var(--bg-base)')}
            stroke={isSelected
              ? statusColor(sl.step.status)
              : (isFailed ? statusColor(sl.step.status) : 'var(--border-subtle)')}
            stroke-width={isSelected ? 2 : (isFailed ? 1.5 : 1)}
          />
          <!-- Status dot -->
          <circle
            cx={sl.x + 12}
            cy={sl.y + sl.h / 2}
            r="4"
            fill={statusColor(sl.step.status)}
          />
          <!-- Step name (truncated) -->
          <text
            x={sl.x + 22}
            y={sl.y + sl.h / 2}
            dominant-baseline="central"
            class="step-name"
            fill="var(--text-primary)"
          >{truncate(sl.step.name, 20)}</text>
          <!-- Elapsed time -->
          {#if sl.step.started_at}
            <text
              x={sl.x + sl.w - 6}
              y={sl.y + sl.h / 2}
              text-anchor="end"
              dominant-baseline="central"
              class="step-elapsed"
              fill="var(--text-muted)"
            >{elapsedMs(sl.step)}</text>
          {/if}
          <!-- Running glow -->
          {#if isRunning}
            <circle
              cx={sl.x + 12}
              cy={sl.y + sl.h / 2}
              r="8"
              fill="none"
              stroke="var(--accent)"
              stroke-width="1.5"
              stroke-dasharray="12 36"
              class="spin-ring"
            />
          {/if}
        </g>
      {/each}
    {/each}
  </svg>
</div>

<style>
  .graph-wrap {
    /* Fill the host so the horizontal scrollbar lands at the bottom of the
       containing panel. Without explicit sizing the wrapper would shrink to
       the SVG's natural height and the scrollbar would float mid-panel with
       a swath of empty space below it. */
    width: 100%;
    height: 100%;
    box-sizing: border-box;
    overflow: auto;
    padding: 8px;
    min-height: 0;
  }

  .pipeline-svg {
    display: block;
    font-family: var(--font-ui-sans);
  }

  .stage-label {
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.1px;
    pointer-events: none;
  }

  .stage-meta {
    font-size: 10.5px;
    font-family: var(--font-code);
    pointer-events: none;
    opacity: 0.85;
  }

  .step-name {
    font-size: 11.5px;
    pointer-events: none;
  }

  .step-elapsed {
    font-size: 10px;
    font-family: var(--font-code);
    pointer-events: none;
  }

  .step-node {
    cursor: pointer;
  }

  .step-node:hover rect {
    stroke: var(--accent);
    stroke-width: 1.5;
    filter: drop-shadow(0 0 0.5px var(--accent));
  }
  .step-node.step-selected rect {
    filter: drop-shadow(0 0 1px currentColor);
  }

  /* Unique keyframe name — a bare `spin` would collide with app.css's global
     `@keyframes spin` after Svelte's local-scope renaming, breaking the
     animation reference inside the `:global` rule. */
  @keyframes prg-spin {
    from { transform-origin: center; transform: rotate(0deg); }
    to   { transform-origin: center; transform: rotate(360deg); }
  }

  :global(.spin-ring) {
    animation: prg-spin 1.2s linear infinite;
    transform-box: fill-box;
    transform-origin: center;
  }
</style>
