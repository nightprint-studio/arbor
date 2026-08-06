<script lang="ts">
  import { ChevronDown } from 'lucide-svelte';
  import Dropdown from './Dropdown.svelte';
  import type { DropdownItem } from './Dropdown.svelte';

  interface Option {
    value: string | number;
    label: string;
    disabled?: boolean;
  }

  interface Props {
    value: string | number;
    options: Option[];
    disabled?: boolean;
    narrow?: boolean;
    /**
     * `sm` is the **chrome-row** size: the header of a search overlay, a panel toolbar — anywhere
     * the picker sits above or beside the thing it acts on rather than in a form.
     *
     * It is a size and not a second component because the behaviour is identical, and the one
     * thing a control on a chrome row must not do is weigh as much as the field under it.
     */
    size?: 'sm' | 'md';
    /**
     * Draw the trigger in the accent — for a value that is **narrowing something**, as opposed to
     * the one that means "all of them".
     *
     * A filter you cannot see at a glance is how an empty result gets blamed on the search.
     */
    highlight?: boolean;
    /**
     * Cap on the menu's visual height in pixels. Passed straight to the
     * underlying `Dropdown` — important when the dropdown might open
     * *upward* (`flipUp`), because Dropdown's positioning formula uses
     * this value as the worst-case menu height. Leaving it unset means
     * 420 is assumed, which leaves a visible gap above the trigger when
     * the actual menu is much shorter.
     *
     * If omitted, Select auto-derives a tight value from `options.length`
     * (32px per item + 16px padding, capped at 420). Pass an explicit
     * value when items render taller (subtitle, avatar, …).
     */
    maxHeight?: number;
    /**
     * Put a filter field at the top of the menu.
     *
     * Worth setting the moment the list can hold more than a screenful and the
     * user knows the name of what they want — a schema's tables, a font, a
     * timezone. Scrolling a hundred entries to find `VERSIONE_DB` is not a list,
     * it is a haystack.
     */
    searchable?: boolean;
    searchPlaceholder?: string;
    /**
     * Take the full width of the container instead of sizing to the trigger.
     *
     * The default is `inline-block`, which collapses to the *selected label* —
     * fine for `CRLF / LF`, and unreadable for anything whose values are long
     * enough to matter. In a form row, this is almost always what you want.
     */
    fill?: boolean;
    /** Shown when nothing is selected at all. Not shown for a value whose label
     *  is merely unknown — that renders the value itself. */
    placeholder?: string;
    /** Shown inside the menu when there is nothing to choose from — far more
     *  useful than an empty box, which reads as a broken control. */
    emptyMessage?: string;
    /** The control's accessible name. Needed wherever the picker stands on its own — a chrome
     *  row has no `<label>` to be associated with, and "combobox, Project" is all a screen
     *  reader can say without one. */
    ariaLabel?: string;
    onchange?: (value: string) => void;
  }

  let {
    value = $bindable(),
    options,
    disabled  = false,
    narrow    = false,
    size      = 'md',
    highlight = false,
    maxHeight,
    searchable = false,
    searchPlaceholder = 'Search…',
    fill = false,
    placeholder = '',
    emptyMessage,
    ariaLabel,
    onchange,
  }: Props = $props();

  /**
   * The menu never gets narrower than this, however narrow the trigger is.
   *
   * A select's menu holds the things you are choosing between, and it is aligned
   * to the trigger for tidiness — not because the trigger's width says anything
   * about how long an option's label is. Without a floor, a picker squeezed into
   * a flex row opens a strip you cannot read a single character in.
   *
   * `narrow` opts out: at 120px it is a deliberate, self-describing size (a
   * grouping switch, a unit), and widening its menu would misalign it for no gain.
   */
  const MENU_FLOOR = $derived(narrow ? 0 : 240);

  /** Tight bound on the menu height so the upward-flip placement doesn't
   *  reserve room for a phantom 420px menu. Caller can override. */
  const derivedMaxHeight = $derived(
    // The filter field lives inside the menu, so it has to be in the height the
    // placement reserves — otherwise a searchable menu opening upward is cut off
    // by exactly the height of its own search box.
    maxHeight ?? Math.min(420, options.length * 32 + 16 + (searchable ? 44 : 0)),
  );

  const items = $derived<DropdownItem[]>(
    options.map(o => ({
      kind:     'item',
      id:       String(o.value),
      label:    o.label,
      active:   String(o.value) === String(value),
      disabled: o.disabled,
      onclick:  () => {
        value = o.value;
        onchange?.(String(o.value));
      },
    })),
  );

  /**
   * What the trigger shows.
   *
   * The fallback to the raw value is the important half. A select whose options
   * are loaded from somewhere — a live catalogue, a directory listing — has a
   * window in which it holds a perfectly good value and knows no option matching
   * it, and rendering the empty string there tells the user their setting is
   * gone. It is not gone; it is just not in this list yet.
   */
  const matched = $derived(options.find(o => String(o.value) === String(value)) ?? null);
  const selectedLabel = $derived(matched ? matched.label : String(value ?? ''));

  /**
   * True when there is genuinely nothing selected.
   *
   * That means **no option matches**, not "the value is the empty string": the
   * empty string is a perfectly ordinary key, and it is the one every "all of
   * them" entry in this codebase uses — `'' → Every folder`, `'' → Both`,
   * `'' → Any`. Testing the value alone made those three render as blank boxes
   * with a chevron, which reads as a control that failed to load rather than as
   * one that is set to its default.
   */
  const isEmpty = $derived(!matched && String(value ?? '') === '');
</script>

<div class="select-wrap" class:narrow class:fill class:sm={size === 'sm'}>
  <Dropdown
    position="fixed"
    direction="down"
    matchTriggerWidth
    maxHeight={derivedMaxHeight}
    minMenuWidth={MENU_FLOOR}
    {searchable}
    {searchPlaceholder}
    {emptyMessage}
    {items}
  >
    {#snippet trigger({ open, toggle })}
      <button
        class="select-input"
        class:narrow
        class:sm={size === 'sm'}
        class:hl={highlight}
        onclick={toggle}
        {disabled}
        type="button"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={ariaLabel}
      >
        <span class="select-input-label" class:select-placeholder={isEmpty}>
          {isEmpty ? placeholder : selectedLabel}
        </span>
        <ChevronDown size={11} />
      </button>
    {/snippet}
  </Dropdown>
</div>

<style>
  .select-wrap { display: inline-block; }
  .select-wrap.narrow { width: 120px; }
  /* Sizes to the container rather than to the selected label.
     `flex` and `min-width` are both load-bearing, not belt-and-braces: this
     usually sits in a flex row next to a button, and there `width: 100%` alone
     is only a *preferred* size — a flex item still shrinks to its content, so a
     picker whose label happened to be empty collapsed to the width of its own
     chevron. `min-width: 0` then lets it shrink below the label's length instead
     of pushing the button off the row. */
  .select-wrap.fill { display: block; width: 100%; flex: 1 1 auto; min-width: 0; }
  .select-wrap :global(.dd-root) { width: 100%; }

  .select-input {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    width: 100%;
    box-sizing: border-box;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    padding: 5px 8px;
    cursor: pointer;
    outline: none;
    text-align: left;
    transition: border-color var(--transition-fast);
  }
  .select-input:focus,
  .select-input[aria-expanded='true'] { border-color: var(--border-focus); }
  .select-input:disabled { opacity: 0.45; cursor: not-allowed; }
  .select-input-label {
    flex: 1;
    /* `min-width: 0` so a long label truncates instead of forcing the trigger
       wider than the row that holds it. */
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: left;
  }
  /* Nothing chosen — as opposed to a value whose label has not loaded, which is
     rendered as the value itself and in the ordinary colour. */
  .select-placeholder { color: var(--text-disabled); }

  /* ── size: sm — the chrome-row variant ──────────────────────────────────────
     Sits on a header strip beside a count or a tab strip, so it takes the
     elevated background those rows already have rather than the input well,
     and it stops growing where a long label would start pushing the row
     around. The menu it opens is the same one. */
  .select-wrap.sm { max-width: 200px; }
  .select-input.sm {
    padding: 3px 6px;
    gap: 4px;
    background: var(--bg-elevated);
    border-color: var(--border-subtle);
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    font-size: var(--font-size-2xs);
  }
  .select-input.sm:hover { border-color: var(--border); color: var(--text-primary); }
  /* Narrowing something — see the `highlight` prop. */
  .select-input.hl { border-color: var(--accent); color: var(--accent); }
</style>
