<script lang="ts">
  /**
   * Numeric input with custom inline ▲ / ▼ stepper buttons.
   *
   * The browser-native spinner arrows are suppressed globally in
   * `src/app.css` because they bleed outside the design tokens, ignore
   * the dark theme, and on narrow widths they crowd the value column.
   * This widget replaces them with a small two-button column on the
   * right of the input, painted with the same border / hover / focus
   * tokens as `<Input>`.
   *
   * Why a separate widget instead of a `stepper` prop on `<Input>`:
   * the markup is genuinely different (input + flex-column of two
   * buttons inside a single border) and keeping `<Input>` focused on
   * the textual input case keeps both components readable.
   *
   * Convention: anywhere a numeric value needs to be typed AND
   * incremented/decremented (cap, retention days, threshold, etc.)
   * use this widget, not a raw `<input type="number">` next to a
   * separate "Reset" button.  The keyboard ↑ / ↓ keys still increment
   * natively because the underlying element is `<input type="number">`.
   */
  import type { Snippet } from 'svelte';
  import { ChevronUp, ChevronDown } from 'lucide-svelte';

  type Size = 'sm' | 'md' | 'lg';

  interface Props {
    value:        number | null | undefined;
    min?:         number;
    max?:         number;
    step?:        number;
    disabled?:    boolean;
    /** Read-only mode — text stays selectable, the stepper buttons are
     *  inert. Distinct from `disabled` so plugin forms can render an
     *  uneditable-but-focusable value (matches the `<input readonly>`
     *  semantic). */
    readonly?:    boolean;
    /** Constrain to ~80 px (mirror of `<Input narrow>`). Default true —
     *  most settings rows want a tight value column.  Pass `false` for
     *  full-width contexts. */
    narrow?:      boolean;
    /** Optional aria-label forwarded to the inner input. */
    ariaLabel?:   string;
    /** Optional id for the inner input — useful when an outside `<label
     *  for="…">` needs to point at it (plugin forms do this). */
    id?:          string;
    /** Optional placeholder forwarded to the inner input. Renders only
     *  when the value is null/undefined/empty. */
    placeholder?: string;
    /** Padding / font-size tier — mirrors `<Input size>` so a plugin form
     *  can render number fields at the same height as adjacent text inputs.
     *  Default `md`. */
    size?:        Size;
    /** Leading icon snippet (rendered inside the input on the left). Mirrors
     *  `<Input iconStart>` so the two widgets compose at the same baseline. */
    iconStart?:   Snippet;
    /** Trailing icon snippet (rendered between the input and the stepper
     *  column). */
    iconEnd?:     Snippet;
    /** Leading text affix (e.g. "$"). Distinct from `iconStart`: muted text,
     *  not a Snippet. */
    prefix?:      string;
    /** Trailing text affix (e.g. "kg", "%"). Sits between the input and the
     *  stepper column. */
    suffix?:      string;
    /** Per-keystroke change.  Most callers want `onchange` instead so
     *  the orchestrator isn't churned on intermediate values. */
    oninput?:     (v: number) => void;
    /** Final commit — fires on blur, on Enter, and after every stepper
     *  click.  This is the right hook for "persist to backend / config". */
    onchange?:    (v: number) => void;
    onblur?:      (e: FocusEvent) => void;
    onkeydown?:   (e: KeyboardEvent) => void;
  }

  let {
    value = $bindable(),
    min, max, step = 1,
    disabled = false,
    readonly = false,
    narrow = true,
    ariaLabel,
    id,
    placeholder,
    size = 'md',
    iconStart, iconEnd,
    prefix, suffix,
    oninput, onchange, onblur, onkeydown,
  }: Props = $props();

  /** Pin a candidate value to the [min, max] range when those bounds
   *  are defined.  Returned untouched otherwise — this keeps the widget
   *  usable for unbounded counters (e.g. retention days = 0..∞). */
  function clampVal(n: number): number {
    let v = n;
    if (typeof min === 'number') v = Math.max(min, v);
    if (typeof max === 'number') v = Math.min(max, v);
    return v;
  }

  /** Stepper click handler — always commits via `onchange` because the
   *  user explicitly asked for a discrete increment.  Bypasses the
   *  blur-to-commit pattern that the typed input uses. Bails early on
   *  `disabled` and `readonly` since both should leave the value pinned. */
  function nudge(delta: number) {
    if (disabled || readonly) return;
    const current = Number(value);
    const base    = Number.isFinite(current) ? current : (min ?? 0);
    const next    = clampVal(base + delta);
    if (next === current) return;
    value = next;
    onchange?.(next);
  }

  // Disable each arrow at the corresponding bound — visual cue + no
  // wasted IPC roundtrip when the user spams a fully-clamped button.
  const atMin = $derived(
    typeof min === 'number' && Number(value) <= min
  );
  const atMax = $derived(
    typeof max === 'number' && Number(value) >= max
  );
</script>

<span
  class="num-stepper sz-{size}"
  class:narrow
  class:disabled
  class:readonly
  class:has-icon-start={!!iconStart || !!prefix}
  class:has-icon-end={!!iconEnd || !!suffix}
>
  {#if iconStart}
    <span class="num-icon-start">{@render iconStart()}</span>
  {/if}
  {#if prefix}
    <span class="num-affix num-affix-start">{prefix}</span>
  {/if}
  <input
    class="num-value"
    type="number"
    bind:value
    {min}
    {max}
    {step}
    {disabled}
    {readonly}
    {id}
    {placeholder}
    aria-label={ariaLabel}
    oninput={(e) => oninput?.(Number((e.target as HTMLInputElement).value))}
    onchange={(e) => onchange?.(Number((e.target as HTMLInputElement).value))}
    {onblur}
    {onkeydown}
  />
  {#if suffix}
    <span class="num-affix num-affix-end">{suffix}</span>
  {/if}
  {#if iconEnd}
    <span class="num-icon-end">{@render iconEnd()}</span>
  {/if}
  <span class="num-ctrls" aria-hidden="true">
    <!-- tabindex=-1 keeps Tab focus on the input, not the steppers —
         keyboard users get ↑/↓ via the native number-input behavior. -->
    <button
      type="button"
      class="num-btn"
      onclick={() => nudge(step)}
      disabled={disabled || readonly || atMax}
      tabindex="-1"
      aria-label="Increase value"
    >
      <ChevronUp size={10} />
    </button>
    <button
      type="button"
      class="num-btn"
      onclick={() => nudge(-step)}
      disabled={disabled || readonly || atMin}
      tabindex="-1"
      aria-label="Decrease value"
    >
      <ChevronDown size={10} />
    </button>
  </span>
</span>

<style>
  /* Default layout: full-width flex so the stepper fills its column
     (plugin form fields, kv_list cells, anywhere it sits inside a
     grid track or a flex item with stretchable width). The `narrow`
     modifier overrides this with `inline-flex` + a fixed 80 px column
     for settings rows that want a tight value chip beside their label. */
  .num-stepper {
    display: flex;
    align-items: stretch;
    height: 26px;
    width: 100%;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    transition: border-color var(--transition-fast);
    overflow: hidden; /* keep the inner button column inside rounded corners */
  }
  /* Sizes mirror <Input> — height + value font-size only. The stepper
     column width stays constant so the chevrons keep the same hit-target
     across tiers. */
  .num-stepper.sz-sm { height: 22px; }
  .num-stepper.sz-md { height: 26px; }
  .num-stepper.sz-lg { height: 32px; }
  .sz-sm .num-value { font-size: var(--font-size-xs); padding: 0 6px 0 7px; }
  .sz-md .num-value { font-size: var(--font-size-sm); padding: 0 6px 0 8px; }
  .sz-lg .num-value { font-size: var(--font-size-md); padding: 0 8px 0 12px; }
  .num-stepper.narrow {
    display: inline-flex;
    width: 80px;
  }
  .num-stepper:focus-within { border-color: var(--border-focus); }
  .num-stepper.disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  /* Readonly: input still focusable / selectable, but the wrapper
     reads as "view-only" with a muted background so it visually
     differs from an editable field. The stepper buttons are also
     disabled in markup. */
  .num-stepper.readonly {
    background: var(--bg-elevated);
  }

  .num-value {
    flex: 1;
    min-width: 0;
    background: transparent;
    border: none;
    outline: none;
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
  }
  /* Trim the typed-text left padding when an icon / prefix is in front, so
     the digits don't drift right of the leading affix. Mirror logic on the
     right when a suffix / iconEnd sits between the digits and the stepper
     column. */
  .num-stepper.has-icon-start .num-value { padding-left: 4px; }
  .num-stepper.has-icon-end .num-value { padding-right: 4px; }

  .num-icon-start, .num-icon-end {
    display: inline-flex;
    align-items: center;
    color: var(--text-muted);
    flex-shrink: 0;
    padding: 0 4px;
  }
  .num-icon-start { padding-left: 7px; }
  .num-icon-end { padding-right: 4px; }
  .num-affix {
    display: inline-flex;
    align-items: center;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    flex-shrink: 0;
    user-select: none;
    pointer-events: none;
  }
  .num-affix-start { padding-left: 7px; }
  .num-affix-end { padding-right: 4px; }
  .sz-sm .num-affix { font-size: var(--font-size-xs); }
  .sz-md .num-affix { font-size: var(--font-size-sm); }
  .sz-lg .num-affix { font-size: var(--font-size-md); }
  /* Native spinners are already suppressed in app.css. The selector
     here is a defensive duplicate so the widget renders correctly even
     if it is ever extracted into a standalone component package. */
  .num-value::-webkit-inner-spin-button,
  .num-value::-webkit-outer-spin-button {
    -webkit-appearance: none;
    margin: 0;
  }
  .num-value { -moz-appearance: textfield; appearance: textfield; }

  /* Two-button column on the right.  Each button is half-height of the
     input, separated by a 1 px hairline that matches the outer border
     so the column reads as part of the same control. */
  .num-ctrls {
    display: flex;
    flex-direction: column;
    width: 18px;
    flex-shrink: 0;
    border-left: 1px solid var(--border);
  }
  .num-btn {
    flex: 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: none;
    color: var(--text-muted);
    cursor: pointer;
    padding: 0;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .num-btn + .num-btn { border-top: 1px solid var(--border); }
  .num-btn:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text-primary);
  }
  .num-btn:active:not(:disabled) {
    background: var(--bg-overlay);
  }
  .num-btn:disabled {
    cursor: not-allowed;
    color: var(--text-disabled);
  }
</style>
