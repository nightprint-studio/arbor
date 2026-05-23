<!--
  ColorSwatch — labelled or chip-only colour swatch, optionally editable.

  Two render modes driven by `label`:
    · `label` present  → labelled card row  `[chip] Label   #caption`
                         (Marketplace palette, future theme-preview surfaces)
    · `label` absent   → chip-only          `[chip]`
                         (ThemeEditor row, where the label lives elsewhere)

  Editable: pass `onchange` to overlay an invisible native `<input type="color">`
  on the chip. Fires on every input event so the host can live-update its
  preview. The native picker only accepts true hex values, so we only mount
  the overlay when `color` matches `#rgb`/`#rrggbb`/`#rrggbbaa`.

  Non-colour tokens: pass `glyph` (a single character like "#", "n", "T") to
  render a centred marker instead of a colour fill — useful when the widget
  doubles as a typed-token indicator in a theme variable editor.

  This widget is presentational only. The consumer owns hex extraction (for
  CSS expressions like `color-mix(...)`), the dirty/built-in/read-only logic,
  and the text input that often sits next to the chip.
-->
<script lang="ts">
  import { tooltip as tooltipAction } from '$lib/actions/tooltip';

  interface Props {
    /** Any CSS colour value — hex, rgb(), var(--token), color-mix(...), … */
    color:     string;
    /** Display name. When set, the widget renders as a labelled card row;
     *  when absent, only the chip is rendered (use this for grids where the
     *  label lives outside, e.g. ThemeEditor's `.var-row`). */
    label?:    string;
    /** Right-hand caption in labelled mode. Defaults to the raw `color`. */
    caption?:  string;
    /** Hide the caption in labelled mode. */
    noCaption?: boolean;
    /** Chip width/height in px. Defaults to 18 (labelled) / 22 (chip-only). */
    chipSize?: number;
    /** Tooltip override; defaults to the colour value. */
    tooltip?:  string;
    /** When provided, the chip becomes editable: an invisible native colour
     *  picker is overlaid and this callback fires on every input event with
     *  the new hex. Only honoured for hex `color` values — the native picker
     *  can't represent `var(--…)` / `color-mix(...)` / rgba(). */
    onchange?: (color: string) => void;
    /** Single-character marker shown instead of the colour fill. Used for
     *  non-colour tokens (e.g. "#" for lengths, "n" for numbers, "T" for
     *  typography). When set, the chip background falls back to a neutral
     *  overlay tint instead of `color`. */
    glyph?:    string;
  }

  let {
    color,
    label,
    caption,
    noCaption = false,
    chipSize,
    tooltip,
    onchange,
    glyph,
  }: Props = $props();

  // Default chip size differs per mode: 18px reads well as a labelled row
  // tag, 22px matches ThemeEditor's existing colour-picker trigger.
  const size = $derived(chipSize ?? (label ? 18 : 22));

  const captionText = $derived(caption ?? color);
  const tipText     = $derived(tooltip ?? color);

  // Native <input type="color"> only accepts a literal `#rrggbb` (alpha
  // optional). For anything else (var(--token), rgba, color-mix) we leave
  // the swatch read-only; the host can still let the user edit via a sibling
  // text input.
  const isHex    = $derived(/^#([0-9a-f]{3}|[0-9a-f]{6}|[0-9a-f]{8})$/i.test(color.trim()));
  const editable = $derived(!!onchange && isHex && !glyph);

  function onNativeInput(e: Event) {
    onchange?.((e.target as HTMLInputElement).value);
  }
</script>

{#snippet chip()}
  <span
    class="cs-chip"
    class:cs-chip-glyph={!!glyph}
    class:cs-chip-editable={editable}
    style="width: {size}px; height: {size}px;{glyph ? '' : ` background: ${color};`}"
    use:tooltipAction={label ? '' : tipText}
  >
    {#if glyph}<span class="cs-glyph">{glyph}</span>{/if}
    {#if editable}
      <input
        type="color"
        class="cs-native"
        value={color}
        oninput={onNativeInput}
        tabindex="-1"
        aria-label={label ?? 'Pick colour'}
      />
    {/if}
  </span>
{/snippet}

{#if label}
  <!-- Labelled card row — used by the Marketplace palette and other
       theme-preview surfaces. -->
  <div class="cs-row" use:tooltipAction={tipText}>
    {@render chip()}
    <span class="cs-label">{label}</span>
    {#if !noCaption}<span class="cs-caption">{captionText}</span>{/if}
  </div>
{:else}
  {@render chip()}
{/if}

<style>
  .cs-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
  }

  .cs-chip {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 4px;
    /* Theme-agnostic outline — works against light AND dark host chrome. */
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--text-primary) 14%, transparent);
    flex-shrink: 0;
    overflow: hidden;
    cursor: default;
    transition: box-shadow var(--transition-fast);
  }
  .cs-chip-editable { cursor: pointer; }
  .cs-chip-editable:hover {
    box-shadow: inset 0 0 0 1px var(--border-focus);
  }

  .cs-chip-glyph { background: var(--bg-overlay); }
  .cs-glyph {
    font-family: var(--font-code);
    font-size: 11px;
    font-weight: 600;
    color: var(--text-muted);
    text-transform: lowercase;
    line-height: 1;
  }

  /* Invisible native colour picker overlaid on the chip. Anchored full-bleed
     so clicking anywhere on the chip pops the OS picker. `tabindex="-1"` on
     the input keeps the focus order on the host's text field next to it. */
  .cs-native {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    opacity: 0;
    cursor: pointer;
    padding: 0;
    border: none;
  }

  .cs-label {
    flex: 1;
    font-size: 11px;
    color: var(--text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .cs-caption {
    font-family: var(--font-mono);
    font-size: 10.5px;
    color: var(--text-muted);
    flex-shrink: 0;
  }
</style>
