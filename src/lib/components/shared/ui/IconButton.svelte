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

  interface Props {
    /** The icon. Sized by the caller, so a caller with a 12px toolbar and one with a 16px
     *  panel header both look right without a size table here. */
    children: Snippet;
    /** What it does, in a few words. Shown on hover AND used as the accessible name — an
     *  icon-only control with neither is unusable, so this is not optional. */
    tooltip: string;
    /** Keybinding rendered in the tooltip (`'Alt+Ins'`). */
    shortcut?: string;
    /** Square edge in pixels. */
    size?: number;
    /** Pressed / open state — for a toggle, or a trigger whose menu is showing. */
    active?: boolean;
    disabled?: boolean;
    /** `ghost` blends into a toolbar; `accent` is the one action in a group worth pointing at;
     *  `danger` is destructive. */
    variant?: 'ghost' | 'accent' | 'danger';
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
    ariaHasPopup = false,
    ariaExpanded,
    onclick,
  }: Props = $props();
</script>

<button
  type="button"
  class="ib v-{variant}"
  class:active
  style="--ib-size: {size}px"
  {disabled}
  aria-label={tooltip}
  aria-pressed={active && !ariaHasPopup ? true : undefined}
  aria-haspopup={ariaHasPopup ? 'menu' : undefined}
  aria-expanded={ariaExpanded}
  use:tooltipAction={shortcut ? { content: tooltip, shortcut } : tooltip}
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
  /* Open / pressed reads as a filled chip, so a dropdown trigger looks held-down while its
     menu is up rather than merely hovered. */
  .ib.active {
    background: color-mix(in srgb, var(--accent) 20%, transparent);
    color: var(--text-primary);
  }

  .v-accent { color: var(--accent); }
  .v-accent:hover:not(:disabled) { color: var(--accent); }
  .v-danger:hover:not(:disabled) {
    background: color-mix(in srgb, var(--error) 18%, transparent);
    color: var(--error);
  }
</style>
