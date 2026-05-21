<script lang="ts">
  import { workspaceColorVar, workspaceInitials } from '$lib/types/workspace';
  import { tooltip } from '$lib/actions/tooltip';

  interface Props {
    name:      string;
    colorIdx:  number;
    /** Pixel size of the square's shorter edge.  Works well from 12 up to 26. */
    size?:     number;
    /** 'monogram' (default) = rounded square with initials,
     *  'dot'      = solid filled circle, no text,
     *  'outline'  = monogram without fill (transparent bg, coloured border + initials). */
    variant?:  'monogram' | 'dot' | 'outline';
  }

  let { name, colorIdx, size = 18, variant = 'monogram' }: Props = $props();

  const initials = $derived(workspaceInitials(name));
  const bg       = $derived(workspaceColorVar(colorIdx));
</script>

<span
  class="ws-dot"
  class:monogram={variant === 'monogram'}
  class:dot={variant === 'dot'}
  class:outline={variant === 'outline'}
  style="--dot-size: {size}px; --dot-bg: {bg};"
  use:tooltip={name}
  aria-hidden="true"
>
  {#if variant !== 'dot'}
    <span class="ws-dot-text">{initials}</span>
  {/if}
</span>

<style>
  .ws-dot {
    /* Flexbox centres the text block so typography stays crisp regardless
       of the dot size — no positioning tricks. */
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--dot-size);
    height: var(--dot-size);
    flex-shrink: 0;
    user-select: none;
    /* Subtle tonal border that adapts to the background colour so the
       monogram stays readable against both the titlebar and menu bg.  A
       hard ring would clash with every palette hue. */
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--dot-bg) 38%, transparent);
  }

  /* ── Monogram: rounded square with 2-letter initials (default) ─────── */
  .ws-dot.monogram {
    /* IntelliJ-style project tile: slightly rounded square, solid fill,
       bright text.  Radius scales with size so 12px and 26px both look
       proportional.  The background layers a subtle top-down sheen on
       top of a solid base fill — the `var(--dot-bg)` base guarantees the
       tile is never translucent, the gradient only adds depth. */
    border-radius: calc(var(--dot-size) * 0.22);
    background:
      linear-gradient(
        180deg,
        color-mix(in srgb, white   14%, transparent) 0%,
        color-mix(in srgb, black   18%, transparent) 100%
      ),
      var(--dot-bg);
  }
  .ws-dot.monogram .ws-dot-text {
    /* Foreground colour for initials follows the active theme via
       `--ws-color-fg` (white in dark themes, near-black in light ones). The
       drop-shadow keeps a touch of legibility regardless of the underlying
       hue. */
    color: var(--ws-color-fg, #ffffff);
    font-family: var(--font-ui-sans);
    font-weight: 600;
    font-size: calc(var(--dot-size) * 0.44);
    letter-spacing: 0.01em;
    line-height: 1;
    text-shadow: 0 1px 0 rgba(0, 0, 0, 0.35);
  }

  /* ── Dot: bare coloured disc, no glyph ─────────────────────────────── */
  .ws-dot.dot {
    border-radius: 50%;
    background: var(--dot-bg);
  }

  /* ── Outline: same tile shape but hollow — used for sub-components
        where the monogram's hue would otherwise compete with its row. ── */
  .ws-dot.outline {
    border-radius: calc(var(--dot-size) * 0.22);
    background: transparent;
    box-shadow: inset 0 0 0 1.5px var(--dot-bg);
  }
  .ws-dot.outline .ws-dot-text {
    color: var(--dot-bg);
    font-family: var(--font-ui-sans);
    font-weight: 600;
    font-size: calc(var(--dot-size) * 0.44);
    letter-spacing: 0.02em;
    line-height: 1;
  }
</style>
