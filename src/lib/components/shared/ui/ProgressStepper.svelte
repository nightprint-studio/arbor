<script lang="ts">
  import { Check, X as XIcon, MinusCircle } from 'lucide-svelte';
  import Spinner from './Spinner.svelte';

  // ---------------------------------------------------------------------------
  // Props
  // ---------------------------------------------------------------------------

  export type StepStatus = 'pending' | 'active' | 'completed' | 'error' | 'skipped';

  export interface Step {
    /** Stable id used to match against `current`. */
    key:    string;
    /** Human-readable label rendered next to the step indicator. */
    label:  string;
    /** Optional sub-text shown muted under the label (e.g. the refs being
     *  fetched, or an error message for failed steps). */
    detail?: string | null;
    /** Override the position-derived status.  When set, takes precedence
     *  over `current` / `done` / `error` for THIS step.  Useful for bulk
     *  operations where individual steps complete with `ok` / `error` /
     *  `skipped` independently of the currently-active item. */
    status?: StepStatus;
  }

  type Layout = 'horizontal' | 'vertical';
  type Size   = 'sm' | 'md';

  interface Props {
    /** Ordered list of steps.  Order defines progression. */
    steps:   Step[];
    /** Key of the step that is currently in progress.  Steps before it are
     *  shown as completed (✓); steps after as pending (muted ring). */
    current: string | null;
    /** `horizontal` for in-line stepper bars (e.g. inside a button row);
     *  `vertical` for richer block layouts (default). */
    layout?: Layout;
    /** Size of the indicator + label text. */
    size?:   Size;
    /** Override detail text for the active step (takes precedence over
     *  `step.detail` when present).  Useful when the step is fixed but the
     *  sub-text varies live. */
    activeDetail?: string | null;
    /** When true the stepper enters a finished state — every step is rendered
     *  as completed and the spinner stops.  Steps with explicit `status`
     *  keep their override. */
    done?: boolean;
    /** When set, renders an error indicator on the active step + the message
     *  underneath.  Steps with explicit `status` are unaffected. */
    error?: string | null;
  }

  let {
    steps,
    current,
    layout = 'vertical',
    size   = 'md',
    activeDetail = null,
    done   = false,
    error  = null,
  }: Props = $props();

  // ---------------------------------------------------------------------------
  // Derived state
  // ---------------------------------------------------------------------------

  const currentIndex = $derived(
    current === null ? -1 : steps.findIndex(s => s.key === current),
  );

  function statusFor(i: number): StepStatus {
    // Per-step override wins for terminal states (completed/error/skipped) —
    // bulk ops use this to mark individual items independently of `current`.
    // 'active' override is intentionally ignored: position is the source of
    // truth for "what's running RIGHT NOW".  Without this guard, a stale
    // 'active' from an earlier phase event would stick forever, leaving its
    // step spinning even after `done = true`.
    const override = steps[i].status;
    if (override && override !== 'active' && override !== 'pending') return override;
    if (done)                        return 'completed';
    if (error && i === currentIndex) return 'error';
    if (i  < currentIndex)           return 'completed';
    if (i === currentIndex)          return 'active';
    return 'pending';
  }

  const indicatorPx = $derived(size === 'sm' ? 16 : 20);
  const spinnerSize = $derived<'xs' | 'sm'>(size === 'sm' ? 'xs' : 'sm');
</script>

<div class="stepper l-{layout} s-{size}" role="list">
  {#each steps as step, i (step.key)}
    {@const status = statusFor(i)}
    <div class="step st-{status}" role="listitem" aria-current={status === 'active' ? 'step' : undefined}>
      <span
        class="indicator"
        style="--indicator-size: {indicatorPx}px"
        aria-hidden="true"
      >
        {#if status === 'completed'}
          <Check size={Math.round(indicatorPx * 0.65)} strokeWidth={3} />
        {:else if status === 'active'}
          <Spinner size={spinnerSize} variant="spin" />
        {:else if status === 'error'}
          <XIcon size={Math.round(indicatorPx * 0.65)} strokeWidth={3} />
        {:else if status === 'skipped'}
          <MinusCircle size={Math.round(indicatorPx * 0.7)} strokeWidth={2} />
        {:else}
          <span class="dot"></span>
        {/if}
      </span>

      <span class="text">
        <span class="label">{step.label}</span>
        {#if status === 'active' && (activeDetail ?? step.detail)}
          <span class="detail">{activeDetail ?? step.detail}</span>
        {:else if status === 'error' && (step.detail ?? (i === currentIndex ? error : null))}
          <span class="detail err">{step.detail ?? error}</span>
        {:else if (status === 'completed' || status === 'skipped') && step.detail}
          <span class="detail">{step.detail}</span>
        {/if}
      </span>

      {#if layout === 'horizontal' && i < steps.length - 1}
        <span class="connector" aria-hidden="true"></span>
      {/if}
    </div>
  {/each}
</div>

<style>
  /* ─── Layout ─────────────────────────────────────────────────────────── */
  .stepper {
    display: flex;
    color: var(--text-primary);
  }
  .stepper.l-vertical   { flex-direction: column; gap: 8px; }
  .stepper.l-horizontal { flex-direction: row; align-items: center; gap: 0; }

  .step {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .stepper.l-horizontal .step { flex: 0 0 auto; }
  .stepper.l-horizontal .step.st-active { flex: 1 1 auto; min-width: 0; }

  /* ─── Indicator ──────────────────────────────────────────────────────── */
  .indicator {
    width:  var(--indicator-size);
    height: var(--indicator-size);
    border-radius: 50%;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: 0 0 auto;
    border: 1.5px solid var(--border);
    background: var(--bg-base);
    color: var(--text-muted);
    transition: background 120ms ease, border-color 120ms ease, color 120ms ease;
  }

  .step.st-completed .indicator {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--bg-base);
  }
  .step.st-active .indicator {
    border-color: var(--accent);
    color: var(--accent);
  }
  .step.st-error .indicator {
    background: var(--diff-del-bg-strong, #5a2a2a);
    border-color: var(--danger, #c94a4a);
    color: #fff;
  }
  .step.st-skipped .indicator {
    color: var(--text-muted);
    border-style: dashed;
    background: transparent;
  }
  .dot {
    width:  4px;
    height: 4px;
    border-radius: 50%;
    background: var(--text-muted);
    opacity: 0.5;
  }

  /* ─── Text ───────────────────────────────────────────────────────────── */
  .text {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }
  .label {
    font-size: var(--font-size-sm);
    color: var(--text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .step.st-active   .label { color: var(--text-primary); font-weight: 500; }
  .step.st-completed .label { color: var(--text-muted); }
  .step.st-error    .label { color: var(--text-primary); font-weight: 500; }
  .step.st-skipped  .label { color: var(--text-muted); font-style: italic; }

  .detail {
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .detail.err { color: var(--danger, #c94a4a); }

  /* ─── Connector (horizontal only) ────────────────────────────────────── */
  .connector {
    height: 1px;
    background: var(--border-subtle);
    flex: 1 1 12px;
    min-width: 12px;
    margin: 0 6px;
  }

  /* ─── Size: sm ───────────────────────────────────────────────────────── */
  .stepper.s-sm .label  { font-size: var(--font-size-xs); }
  .stepper.s-sm .detail { font-size: 10px; }
  .stepper.s-sm.l-vertical { gap: 6px; }
</style>
