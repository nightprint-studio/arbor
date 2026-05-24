<script lang="ts" module>
  /**
   * Step descriptor consumed by `StepIndicator`. `icon` is a pre-resolved
   * Svelte component — the widget itself does NOT do icon-name resolution,
   * so consumers wanting plugin-style string lookup (PLUGIN_ICONS) must
   * map the string to the component before passing the step in. This keeps
   * StepIndicator app-agnostic / safe to live in `shared/ui/`.
   */
  export interface Step {
    id:    string;
    label: string;
    /** Optional Svelte icon component (Lucide / Iconify / custom). When
     *  set, replaces the numeric badge for pending and active states.
     *  Done steps always render the Check glyph regardless. */
    icon?: any;
  }

  export type StepStatus = 'pending' | 'active' | 'done';
</script>

<script lang="ts">
  /**
   * StepIndicator — wizard-style step navigation breadcrumb.
   *
   *   <StepIndicator steps={STEPS} current={step.id} />
   *
   * Distinct from `ProgressStepper` — that widget shows progress of an
   * ASYNC OPERATION (fetch / clone / sync) with a spinner on the active
   * row. This one is a NAVIGATION indicator for multi-step wizards: the
   * active step is "where the user is right now", not "what the system
   * is doing".
   *
   * Two visual variants:
   *   - `flat` (default) — plugin-wizard look: muted text + thin border
   *                        connector, active step in accent badge with
   *                        bold label, done steps tinted accent.
   *   - `pill`           — onboarding-tour look: active step lives in
   *                        an `accent-subtle` pill, no connector, done
   *                        steps have a solid accent dot with Check.
   *
   * `onStepClick` enables go-back navigation: when provided, every step
   * (active and done by default) renders as a button. Pass `null` to
   * disable click on specific cases by checking the index inside the
   * handler.
   */
  import { Check } from 'lucide-svelte';

  type Layout  = 'horizontal' | 'vertical';
  type Size    = 'sm' | 'md';
  type Variant = 'flat' | 'pill';

  interface Props {
    steps:   Step[];
    /** Id of the step the user is currently on. */
    current: string;
    layout?: Layout;
    size?:   Size;
    variant?: Variant;
    /** Show a 1px connector between adjacent steps. Defaults to true for
     *  `flat`, false for `pill` (the pill background already separates
     *  steps visually). */
    separator?: boolean;
    /** When true the labels collapse to badge-only at narrow viewports
     *  (< 768px). Onboarding turns this on because six labels wouldn't
     *  fit inside the modal header on small windows. */
    collapseLabels?: boolean;
    /** When set, every clickable step (done + active by default) renders
     *  as a <button>. The handler decides whether to honour the click —
     *  e.g. wizards typically only allow going back to done steps. */
    onStepClick?: (id: string, index: number) => void;
  }

  let {
    steps,
    current,
    layout    = 'horizontal',
    size      = 'md',
    variant   = 'flat',
    separator,
    collapseLabels = false,
    onStepClick,
  }: Props = $props();

  // Resolved separator value: variant-dependent default.
  const sepOn = $derived(separator ?? (variant === 'flat'));

  const currentIndex = $derived(steps.findIndex(s => s.id === current));

  function statusFor(i: number): StepStatus {
    if (i  < currentIndex) return 'done';
    if (i === currentIndex) return 'active';
    return 'pending';
  }

  function clickable(status: StepStatus): boolean {
    if (!onStepClick) return false;
    // By convention click-back is allowed for done & active steps,
    // never pending — pending would mean "skip forward" which most
    // wizards intentionally forbid.
    return status !== 'pending';
  }
</script>

<ol
  class="step-indicator l-{layout} sz-{size} v-{variant}"
  class:has-sep={sepOn}
  class:collapsible={collapseLabels}
  aria-label="Steps"
>
  {#each steps as step, i (step.id)}
    {@const status = statusFor(i)}
    {@const Icon = step.icon}
    {@const isClickable = clickable(status)}
    <li
      class="step st-{status}"
      aria-current={status === 'active' ? 'step' : undefined}
    >
      {#if isClickable}
        <button
          type="button"
          class="step-btn"
          onclick={() => onStepClick?.(step.id, i)}
          aria-label={step.label}
        >
          <span class="badge">
            {#if status === 'done'}
              <Check size={11} strokeWidth={3} />
            {:else if Icon}
              <Icon size={11} />
            {:else}
              {i + 1}
            {/if}
          </span>
          <span class="label">{step.label}</span>
        </button>
      {:else}
        <span class="badge">
          {#if status === 'done'}
            <Check size={11} strokeWidth={3} />
          {:else if Icon}
            <Icon size={11} />
          {:else}
            {i + 1}
          {/if}
        </span>
        <span class="label">{step.label}</span>
      {/if}

      {#if sepOn && i < steps.length - 1}
        <span class="sep" aria-hidden="true"></span>
      {/if}
    </li>
  {/each}
</ol>

<style>
  .step-indicator {
    display: flex;
    align-items: center;
    list-style: none;
    margin: 0;
    padding: 0;
    min-width: 0;
  }
  .step-indicator.l-horizontal {
    flex-direction: row;
    gap: 6px;
    overflow-x: auto;
  }
  .step-indicator.l-vertical {
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
  }

  /* ── Step row ─────────────────────────────────────────────────────────── */
  .step {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--text-muted);
    white-space: nowrap;
    flex-shrink: 0;
    transition: color 120ms ease, background 120ms ease;
  }
  .sz-sm .step { font-size: var(--font-size-xs); }
  .sz-md .step { font-size: var(--font-size-sm); }

  .step.st-active { color: var(--text-primary); font-weight: 600; }
  .step.st-done   { color: color-mix(in srgb, var(--accent) 80%, var(--text-secondary)); }

  /* The button wrapper carries no visual chrome — clickable steps inherit
     the same look as static ones, with a hover hint and focus ring. */
  .step-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: none;
    border: none;
    padding: 0;
    margin: 0;
    color: inherit;
    font: inherit;
    cursor: pointer;
    border-radius: var(--radius-sm);
  }
  .step-btn:hover .label { text-decoration: underline; }
  .step-btn:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  /* ── Badge (the circle / pill in front of the label) ──────────────────── */
  .badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 999px;
    background: var(--bg-base);
    border: 1px solid var(--border);
    font-size: 10px;
    font-weight: 600;
    color: inherit;
    flex-shrink: 0;
    transition: background 120ms ease, border-color 120ms ease, color 120ms ease;
  }
  .sz-sm .badge { width: 16px; height: 16px; font-size:  9px; }
  .sz-md .badge { width: 20px; height: 20px; font-size: 10px; }

  .st-active .badge {
    background: var(--accent);
    color: var(--text-on-accent);
    border-color: var(--accent);
  }
  .st-done .badge {
    background: color-mix(in srgb, var(--accent) 20%, transparent);
    border-color: color-mix(in srgb, var(--accent) 45%, transparent);
    color: var(--accent);
  }

  /* ── Separator (thin line between steps, horizontal layout) ───────────── */
  .step-indicator.l-horizontal .sep {
    width: 20px;
    height: 1px;
    background: var(--border);
    margin: 0 4px;
  }
  .step-indicator.l-vertical .sep {
    width: 1px;
    height: 14px;
    background: var(--border);
    margin: 0 0 0 9px;   /* aligns under the badge center for sz-md */
  }
  .step-indicator.sz-sm.l-vertical .sep { margin-left: 7px; }

  /* ── Pill variant ─────────────────────────────────────────────────────── */
  /* Active step gets a tinted background pill so it pops more strongly
     than the flat variant — used by Onboarding where the indicator
     doubles as a hero/header element. Done steps render the accent
     dot as a SOLID accent fill (with Check in bg-base) to read as
     "fully complete" rather than "tinted". */
  .v-pill .step {
    padding: 3px 8px 3px 5px;
    border-radius: var(--radius-pill, 999px);
  }
  .v-pill .step.st-active {
    background: var(--accent-subtle);
  }
  .v-pill .step.st-active .badge {
    background: var(--bg-base);
    color: var(--accent);
    border-color: var(--accent);
  }
  .v-pill .step.st-done .badge {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--bg-base);
  }
  /* The pill variant doesn't render connectors — the background pill on
     the active step replaces the visual separation between rows. */

  /* ── Responsive label collapse ────────────────────────────────────────── */
  /* When `collapseLabels` is on the labels disappear below 768px viewport
     width and only the badges remain — used by Onboarding's 6-step header
     in narrow modals. The threshold is intentionally fixed: when a future
     consumer needs a different breakpoint, they can wrap the indicator in
     their own media query, not parameterise this one (CSS media queries
     can't read dynamic CSS variables). */
  @media (max-width: 768px) {
    .step-indicator.collapsible .label { display: none; }
    .step-indicator.collapsible .v-pill .step { padding: 3px; }
  }
</style>
