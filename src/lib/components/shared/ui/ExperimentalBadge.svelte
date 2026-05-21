<!--
  Small "Experimental" pill — used in modal headers and on plugin rows to
  flag features that are still in active iteration. Soft amber→coral
  gradient with a flask icon; reads as a status flag without overwhelming
  the row.
-->
<script lang="ts">
  import { FlaskConical } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';

  interface Props {
    /** Tooltip title. Falls back to "Experimental". */
    title?:       string;
    /** Longer description shown under the tooltip title. */
    description?: string;
    /** Visual scale. `sm` is for list rows, `md` (default) for modal headers. */
    size?:        'sm' | 'md';
    /** Override the visible label (default: "Experimental"). */
    label?:       string;
  }

  let {
    title       = 'Experimental',
    description = 'This feature is still being shaped — behaviour, hooks and storage may change between releases.',
    size        = 'md',
    label       = 'Experimental',
  }: Props = $props();

  // Icon scales with the badge so the chip stays balanced at both sizes.
  const iconSize = $derived(size === 'sm' ? 11 : 13);
</script>

<span
  class="experimental-badge size-{size}"
  use:tooltip={{ content: title, description }}
>
  <FlaskConical class="exp-icon" size={iconSize} strokeWidth={2.25} />
  <span class="exp-label">{label}</span>
</span>

<style>
  .experimental-badge {
    position: relative;
    display: inline-flex;
    align-items: center;
    /* Squared-off chip rather than a full pill — reads as a tag/label
       instead of a status bubble.  Small corner radius keeps the
       silhouette friendly without going full pill. */
    border-radius: 5px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.07em;
    /* Two-stop diagonal wash: amber on the left, a hint of coral on the
       right.  Both stops are pulled from the theme (`--warning` /
       `--error`) so the chip stays in-palette while feeling alive.
       Text + icon ride on `--warning` at full saturation, which has
       enough contrast against the low-opacity background on every
       preset. */
    color: var(--warning);
    background: linear-gradient(
      135deg,
      color-mix(in srgb, var(--warning) 22%, transparent) 0%,
      color-mix(in srgb, var(--warning) 12%, transparent) 55%,
      color-mix(in srgb, var(--error)   16%, transparent) 100%
    );
    border: 1px solid color-mix(in srgb, var(--warning) 48%, transparent);
    /* Outer: tiny halo that hints at "active/live".
       Inner: 1px top highlight gives the chip a touch of dimensionality
       so it doesn't read as flat tape. */
    box-shadow:
      0 0 0 1px color-mix(in srgb, var(--warning) 8%, transparent),
      inset 0 1px 0 color-mix(in srgb, var(--warning) 35%, transparent),
      0 1px 2px rgba(0, 0, 0, 0.18);
    flex-shrink: 0;
    cursor: help;
    user-select: none;
    white-space: nowrap;
    line-height: 1;
    overflow: hidden;
    transition:
      background    var(--transition-fast),
      border-color  var(--transition-fast),
      box-shadow    var(--transition-fast),
      transform     var(--transition-fast);
  }

  /* Specular sheen — diagonal highlight stripe that animates across the
     chip on hover.  Pseudo-element rather than a background-position
     animation so the underlying gradient stays untouched. */
  .experimental-badge::before {
    content: '';
    position: absolute;
    inset: 0;
    border-radius: inherit;
    background: linear-gradient(
      115deg,
      transparent 30%,
      color-mix(in srgb, var(--warning) 28%, transparent) 50%,
      transparent 70%
    );
    transform: translateX(-110%);
    transition: transform 650ms cubic-bezier(0.22, 1, 0.36, 1);
    pointer-events: none;
  }

  .experimental-badge:hover {
    background: linear-gradient(
      135deg,
      color-mix(in srgb, var(--warning) 30%, transparent) 0%,
      color-mix(in srgb, var(--warning) 18%, transparent) 55%,
      color-mix(in srgb, var(--error)   24%, transparent) 100%
    );
    border-color: color-mix(in srgb, var(--warning) 68%, transparent);
    box-shadow:
      0 0 0 2px color-mix(in srgb, var(--warning) 14%, transparent),
      inset 0 1px 0 color-mix(in srgb, var(--warning) 45%, transparent),
      0 2px 6px rgba(0, 0, 0, 0.22);
    transform: translateY(-0.5px);
  }
  .experimental-badge:hover::before { transform: translateX(110%); }

  /* Icon inherits via currentColor — same hue as the label, slightly
     dimmed so the glyph doesn't out-shout the wordmark next to it. */
  :global(.experimental-badge .exp-icon) {
    color: color-mix(in srgb, var(--warning) 88%, transparent);
    flex-shrink: 0;
    /* Position above the sheen pseudo-element so the icon stays crisp
       during the hover shimmer. */
    position: relative;
    z-index: 1;
    filter: drop-shadow(0 0 2px color-mix(in srgb, var(--warning) 35%, transparent));
  }

  .exp-label {
    display: inline;
    position: relative;
    z-index: 1;
  }

  .size-md {
    padding: 4px 10px 4px 8px;
    gap: 6px;
    font-size: 11px;
  }
  .size-sm {
    padding: 2px 8px 2px 6px;
    gap: 5px;
    font-size: 10px;
    letter-spacing: 0.06em;
  }
</style>
