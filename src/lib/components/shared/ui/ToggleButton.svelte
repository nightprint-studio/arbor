<script lang="ts">
  /**
   * The compact on/off control that lives beside a search field — <b>Aa</b>, <b>W</b>,
   * <b>.*</b>, and the icon ones next to them.
   *
   * Not {@link Toggle}: that is a switch for a settings row, where the label does the talking
   * and the control is a full line high. This is a key on a search bar — one or two glyphs, its
   * own width, and it has to sit in a row of four without turning into a toolbar. IntelliJ's
   * search bars are the reference.
   *
   * `aria-pressed` rather than a checkbox, because it *is* a button whose effect is immediate:
   * flipping it re-runs the search rather than staging a change to be submitted.
   */
  import type { IconComponent } from '$lib/types/icon';
  import { tooltip } from '$lib/actions/tooltip';

  interface Props {
    pressed: boolean;
    /** One or two characters — `Aa`, `W`, `.*`. Rendered in the code face, which is what makes
     *  `.*` read as a regular expression rather than as punctuation. */
    label?: string;
    /** Used instead of {@link label} when the state has no short spelling. */
    icon?: IconComponent;
    /** The tooltip. Say what it *will* do, not what it is: a toggle whose title never changes
     *  makes the user press it to find out which way round it currently is. */
    title: string;
    /** Defaults to {@link title} — override when the title is a sentence about the next press
     *  and the control needs a stable name. */
    ariaLabel?: string;
    onclick: () => void;
    /** Keybinding shown in the tooltip (`'Alt+C'`), beside {@link title}. */
    shortcut?: string;
    /** Borderless 22px key for sitting **inside** a field (the options at the end of a find
     *  input), where a bordered 26px button would make the field a toolbar. */
    compact?: boolean;
  }

  let { pressed, label, icon: Icon, title, ariaLabel, onclick, shortcut, compact = false }: Props = $props();
</script>

<button
  type="button"
  class="tgl"
  class:compact
  class:on={pressed}
  aria-pressed={pressed}
  aria-label={ariaLabel ?? title}
  use:tooltip={shortcut ? { content: title, shortcut } : title}
  {onclick}
>
  {#if Icon}<Icon size={13} />{:else}<span class="tgl-label">{label}</span>{/if}
</button>

<style>
  .tgl {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 26px;
    height: 26px;
    padding: 0 5px;
    font-size: var(--font-size-xs);
    font-weight: 600;
    font-family: var(--font-code);
    color: var(--text-muted);
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    cursor: pointer;
    flex-shrink: 0;
    transition: background var(--transition-fast), border-color var(--transition-fast),
                color var(--transition-fast);
  }
  .tgl:hover { border-color: var(--border); color: var(--text-secondary); }
  .tgl.on { background: var(--accent-subtle); border-color: var(--accent); color: var(--accent); }
  .tgl.compact { min-width: 22px; height: 22px; padding: 0 3px; background: transparent; border-color: transparent; }
  .tgl.compact:hover { background: var(--bg-hover); border-color: transparent; }
  .tgl.compact.on { background: var(--accent-subtle); border-color: transparent; }
  /* A two-character label has to fit the same square an icon does. */
  .tgl-label { font-size: var(--font-size-2xs); line-height: 1; }
</style>
