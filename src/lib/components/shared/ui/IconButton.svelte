<script lang="ts">
  /**
   * IconButton — a square button that is only an icon.
   *
   * The widget every toolbar and every table header in the app had hand-rolled: a bare
   * `<button>` with three lines of the same CSS, a `use:tooltip`, and — about half the time —
   * no `aria-label`, because the label lives in the tooltip and the tooltip is not read by a
   * screen reader.
   *
   * So the tooltip is **required** here, and it doubles as the accessible name. An icon with no
   * words attached is only usable if hovering it says what it does; making that impossible to
   * forget is most of the point of having the widget at all.
   *
   * Distinct from `Button` on purpose. `Button` is a labelled control that may also carry an
   * icon; this is a control whose *whole* content is the icon, which changes the sizing, the
   * hit area, the focus ring and the accessibility contract — enough that one component doing
   * both would be a component with a mode.
   */
  import type { Snippet } from 'svelte';
  import { tooltip as tooltipAction } from '$lib/actions/tooltip';
  import type { TooltipInput } from '$lib/stores/tooltip.svelte';

  interface Props {
    /** The icon. Sized by the caller, so a caller with a 12px toolbar and one with a 16px
     *  panel header both look right without a size table here. */
    children: Snippet;
    /** What it does, in a few words. Shown on hover AND used as the accessible name — an
     *  icon-only control with neither is unusable, so this is not optional. Also accepts the
     *  full tooltip object (description, shortcut, placement…); the accessible name is then its
     *  `content`. */
    tooltip: TooltipInput;
    /** Keybinding rendered in the tooltip (`'Alt+Ins'`). A `shortcut` already inside a
     *  `tooltip` object wins. */
    shortcut?: string;
    /** Square edge in pixels. */
    size?: number;
    /** Pressed / open state — for a toggle, or a trigger whose menu is showing. */
    active?: boolean;
    disabled?: boolean;
    /** `ghost` blends into a toolbar; `accent` is the one action in a group worth pointing at;
     *  `success` confirms / adds; `danger` is destructive. */
    variant?: 'ghost' | 'accent' | 'success' | 'danger';
    /** Whether the variant's colour shows at rest (`'rest'`) or only under the pointer
     *  (`'hover'`). Default: `accent` and `success` at rest, `danger` on hover only. */
    tone?: 'rest' | 'hover';
    /** Hover fill. `overlay` / `elevated` are for a button that sits on a surface close to the
     *  default `--bg-hover` (a hovered row, an overlay header), where it would vanish. */
    hoverBg?: 'default' | 'overlay' | 'elevated';
    /** A filled chip with a 1px border (`--bg-overlay` / `--border-subtle`) — the icon button
     *  that sits next to an input. */
    bordered?: boolean;
    /** Extra class on the button, for layout concerns of the caller (margin, alignment). */
    class?: string;
    /** ARIA for a trigger that opens a menu. */
    ariaHasPopup?: boolean;
    ariaExpanded?: boolean;
    onclick?: (event: MouseEvent) => void;
  }

  let {
    children,
    tooltip,
    shortcut,
    size = 24,
    active = false,
    disabled = false,
    variant = 'ghost',
    tone,
    hoverBg = 'default',
    bordered = false,
    class: extraClass = '',
    ariaHasPopup = false,
    ariaExpanded,
    onclick,
  }: Props = $props();

  const label = $derived(typeof tooltip === 'string' ? tooltip : tooltip.content);

  const tooltipInput = $derived<TooltipInput>(
    typeof tooltip === 'string'
      ? (shortcut ? { content: tooltip, shortcut } : tooltip)
      : (shortcut && !tooltip.shortcut ? { ...tooltip, shortcut } : tooltip),
  );

  const colouredAtRest = $derived((tone ?? (variant === 'danger' ? 'hover' : 'rest')) === 'rest');
</script>

<button
  type="button"
  class="ib v-{variant} {extraClass}"
  class:rest-accent={variant === 'accent' && colouredAtRest}
  class:rest-success={variant === 'success' && colouredAtRest}
  class:rest-danger={variant === 'danger' && colouredAtRest}
  class:hb-overlay={hoverBg === 'overlay'}
  class:hb-elevated={hoverBg === 'elevated'}
  class:bordered
  class:active
  style="--ib-size: {size}px"
  {disabled}
  aria-label={label}
  aria-pressed={active && !ariaHasPopup ? true : undefined}
  aria-haspopup={ariaHasPopup ? 'menu' : undefined}
  aria-expanded={ariaExpanded}
  use:tooltipAction={tooltipInput}
  {onclick}
>
  {@render children()}
</button>

<style>
  .ib {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: 0 0 auto;
    width: var(--ib-size);
    height: var(--ib-size);
    padding: 0;
    border: none;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--text-muted);
    cursor: pointer;
    transition: background-color 0.12s ease, color 0.12s ease;
  }
  .ib:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text-primary);
  }
  .ib:focus-visible {
    outline: 1px solid var(--accent);
    outline-offset: -1px;
  }
  .ib:disabled {
    opacity: 0.35;
    cursor: default;
  }
  /* Inside a row that is already hovered: the default fill would disappear into it. Same
     specificity as `.ib:hover`, declared after it, before the variants (which may still repaint). */
  .hb-overlay:hover:not(:disabled) { background: var(--bg-overlay); }
  .hb-elevated:hover:not(:disabled) { background: var(--bg-elevated); }

  /* Chip with a border, for a button sitting beside an input. Declared before the variants so
     a coloured variant still overrides the resting colour. */
  .bordered {
    background: var(--bg-overlay);
    color: var(--text-secondary);
    border: 1px solid var(--border-subtle);
  }
  .bordered:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text-primary);
    border-color: var(--accent);
  }

  /* Open / pressed reads as a filled chip, so a dropdown trigger looks held-down while its
     menu is up rather than merely hovered. */
  .ib.active {
    background: color-mix(in srgb, var(--accent) 20%, transparent);
    color: var(--text-primary);
  }

  /* Resting colour is its own single-class rule (not `.v-accent`) so that it keeps the same
     specificity as `.ib` and still loses to `.ib.active`, exactly as before `tone` existed. */
  .rest-accent { color: var(--accent); }
  .v-accent:hover:not(:disabled) { color: var(--accent); }
  .rest-success { color: var(--success); }
  .ib.bordered.rest-success { border-color: color-mix(in srgb, var(--success) 35%, var(--border-subtle)); }
  .v-success:hover:not(:disabled) {
    background: color-mix(in srgb, var(--success) 15%, transparent);
    color: var(--success);
  }
  .ib.bordered.v-success:hover:not(:disabled) {
    border-color: color-mix(in srgb, var(--success) 35%, var(--border-subtle));
  }
  .rest-danger { color: var(--error); }
  .v-danger:hover:not(:disabled) {
    background: color-mix(in srgb, var(--error) 18%, transparent);
    color: var(--error);
  }
</style>
