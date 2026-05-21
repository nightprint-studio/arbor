<script lang="ts">
  /**
   * Floating "Show keyboard inputs" overlay.
   *
   * Renders the live key/chord queue from `keystrokesStore` as a stack of
   * IDE-quality keystroke cards: rendered key caps with a soft accent
   * glow, paired with the human-readable name of the action they trigger
   * (when one is bound).  Used for demos, screencasts and
   * pair-programming.
   *
   * The component is pure presentation — `keystrokesStore` owns the
   * window-level capture listeners.  `pointer-events: none` on the
   * wrapper means the overlay never steals clicks from the app
   * underneath, even at full opacity.
   */
  import { flip } from 'svelte/animate';
  import { fly, fade } from 'svelte/transition';
  import { cubicOut, expoOut } from 'svelte/easing';
  import { MousePointerClick, ArrowBigUp } from 'lucide-svelte';
  import { keystrokesStore, TONE_COLORS } from '$lib/stores/keystrokes.svelte';
  import { animStore } from '$lib/stores/animations.svelte';

  const items       = $derived(keystrokesStore.entries);
  const position    = $derived(keystrokesStore.position);
  const size        = $derived(keystrokesStore.size);
  const opacity     = $derived(keystrokesStore.opacity);
  const tone        = $derived(keystrokesStore.tone);
  const compact     = $derived(keystrokesStore.compact);
  const showAction  = $derived(keystrokesStore.showAction);
  const edgeOffset  = $derived(keystrokesStore.edgeOffset);

  // Resolve the CSS colour for the selected tone — empty string means
  // fall back to the theme accent. Mono is special-cased to use the
  // text-primary token so it visually de-emphasises the colour stripe.
  const toneOverride = $derived(
    tone === 'mono'   ? 'var(--text-primary)'
    : TONE_COLORS[tone] || 'var(--accent)'
  );

  // Used by the position picker for visual anchor logic. The store keeps
  // newest entries at index 0; CSS flex-direction below puts the newest
  // entry CLOSEST to the anchored edge for every position.
  const isCentered = $derived(position === 'bottom-center');

  // Slide-in direction: from below for bottom anchors, from above for
  // top anchors, lateral for center.
  const flyY = $derived(
    position === 'top-left' || position === 'top-right' ? -22
    : 22,
  );

  function isModifier(part: string): boolean {
    return part === 'Ctrl' || part === 'Alt' || part === 'Shift' || part === 'Meta';
  }
</script>

{#if keystrokesStore.enabled && items.length > 0}
  <div
    class="ks-root pos-{position} sz-{size} tone-{tone}"
    class:centered={isCentered}
    class:compact
    style="--ks-opacity: {opacity}; --ks-edge: {edgeOffset}px; --ks-key-tone: {toneOverride};"
    aria-hidden="true"
  >
    {#each items as entry (entry.id)}
      {@const hasAction = showAction && !!entry.action}
      <div
        class="ks-card"
        class:mouse={entry.isMouse}
        class:has-action={hasAction}
        animate:flip={{ duration: animStore.dBase }}
        in:fly={{ y: flyY, duration: animStore.dPanel, easing: expoOut }}
        out:fade={{ duration: animStore.dBase, easing: cubicOut }}
      >
        <!-- Accent gradient stripe — adapts to mouse vs keyboard tone. -->
        <span class="ks-stripe" aria-hidden="true"></span>

        <div class="ks-body">
          <div class="ks-chord-row">
            {#if entry.isMouse}
              <span class="ks-leading-icon">
                <MousePointerClick
                  size={size === 'sm' ? 12 : size === 'lg' ? 18 : 14}
                  strokeWidth={2.2}
                />
              </span>
            {/if}

            <span class="ks-chord">
              {#each entry.parts as part, i}
                {#if i > 0}<span class="ks-plus" aria-hidden="true">+</span>{/if}
                <kbd class="ks-key" class:mod={isModifier(part)}>
                  {#if part === 'Shift'}
                    <ArrowBigUp
                      size={size === 'sm' ? 11 : size === 'lg' ? 16 : 13}
                      strokeWidth={2.2}
                    />
                    <span class="ks-key-text">Shift</span>
                  {:else}
                    {part}
                  {/if}
                </kbd>
              {/each}
            </span>

            {#if entry.count > 1}
              <span class="ks-count">×{entry.count}</span>
            {/if}
          </div>

          {#if hasAction}
            <div class="ks-action">{entry.action}</div>
          {/if}
        </div>
      </div>
    {/each}
  </div>
{/if}

<style>
  /* ── Root ─────────────────────────────────────────────────────────────── */
  .ks-root {
    position: fixed;
    z-index: 100000;
    display: flex;
    gap: 8px;
    pointer-events: none;
    opacity: var(--ks-opacity, 1);
    /* Drop-shadow lives at root level so the card border + accent glow
       compose with it on the GPU instead of layering twice. */
    filter: drop-shadow(0 8px 22px rgba(0, 0, 0, 0.42));
  }

  /* For bottom anchors the newest entry should sit CLOSEST to the edge
     — column-reverse renders index 0 at the visual bottom.
     For top anchors the column flow already puts index 0 at the top edge.
     `--ks-edge` is user-configurable distance from the anchored edge. */
  .pos-bottom-right  { right: 18px; bottom: var(--ks-edge, 44px); flex-direction: column-reverse; align-items: flex-end;   }
  .pos-bottom-left   { left:  18px; bottom: var(--ks-edge, 44px); flex-direction: column-reverse; align-items: flex-start; }
  .pos-bottom-center { left:  50%;  bottom: var(--ks-edge, 44px); flex-direction: column-reverse; align-items: center; transform: translateX(-50%); }
  .pos-top-right     { right: 18px; top:    var(--ks-edge, 48px); flex-direction: column;         align-items: flex-end;   }
  .pos-top-left      { left:  18px; top:    var(--ks-edge, 48px); flex-direction: column;         align-items: flex-start; }

  /* ── Card ─────────────────────────────────────────────────────────────── */
  .ks-card {
    position: relative;
    display: inline-flex;
    align-items: stretch;
    min-width: 0;
    max-width: 90vw;
    /* SOLID background — never `color-mix(...transparent)` or `backdrop-filter`.
       Backdrop blur was deliberately removed: it has nasty per-frame cost
       on Tauri's WebView (especially over a busy CommitGraph) and the
       previous translucent stack let underlying commit rows bleed through
       the card.  Two layers of defence:
         • `background-color` is a guaranteed-opaque fallback that paints
           even if a custom theme set `--bg-elevated` to a transparent
           token.
         • `background-image` is the gradient on top for the soft tint.
       The user's opacity slider on the root remains the ONE translucency
       knob — at 100% nothing bleeds through. */
    background-color: #2b2d30;
    background-image: linear-gradient(180deg, var(--bg-elevated), var(--bg-base));
    border: 1px solid var(--border);
    border-radius: 14px;
    overflow: hidden;
    color: var(--text-primary);
    font-family: var(--font-sans);
    /* Subtle inner highlight on the top edge + accent ring around it. */
    box-shadow:
      0 1px 0 color-mix(in srgb, white 10%, transparent) inset,
      0 0 0 1px color-mix(in srgb, var(--accent) 10%, transparent),
      0 12px 30px -10px rgba(0, 0, 0, 0.55);
    transform-origin: center;
    /* Faint accent glow on freshly-inserted cards — fades after 1s. */
    animation: ks-pop var(--anim-dur-panel, 200ms) cubic-bezier(0.16, 1, 0.3, 1) both;
  }
  .ks-card.mouse {
    /* Mouse events keep their own cooler blue tone regardless of the
       user's keyboard tone — preserves the visual distinction in the
       stack between input modalities. */
    --ks-tone: var(--info, #4aa8ff);
  }
  .ks-card:not(.mouse) {
    /* Inherit the user-picked tone (overridden via `--ks-key-tone` on
       the root); falls back to the theme accent if nothing was set. */
    --ks-tone: var(--ks-key-tone, var(--accent));
  }
  /* Mono tone: soft, achromatic — drop the saturation on the modifier
     keys and the stripe so the look is purely typographic. */
  .ks-root.tone-mono .ks-card:not(.mouse) {
    --ks-tone: color-mix(in srgb, var(--text-primary) 75%, transparent);
  }

  /* Left accent stripe — pure gradient, draws the eye to the newest entry. */
  .ks-stripe {
    width: 3px;
    flex-shrink: 0;
    background: linear-gradient(
      180deg,
      var(--ks-tone),
      color-mix(in srgb, var(--ks-tone) 30%, transparent)
    );
    box-shadow: 0 0 12px color-mix(in srgb, var(--ks-tone) 60%, transparent);
  }

  .ks-body {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 4px;
    padding: 9px 14px 10px 12px;
    min-width: 0;
  }

  /* ── Chord row ────────────────────────────────────────────────────────── */
  .ks-chord-row {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .ks-leading-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--ks-tone);
    flex-shrink: 0;
  }

  .ks-chord {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    min-width: 0;
    flex-wrap: wrap;
  }

  .ks-plus {
    color: var(--text-disabled);
    font-size: 0.78em;
    font-weight: 700;
    user-select: none;
    letter-spacing: 0.5px;
  }

  /* Key cap — physical-looking pill with top highlight + bottom shadow line.
     Modifier keys (Ctrl/Alt/Shift/Meta) get the accent treatment so the
     terminal printable key stands out as the "main" key visually. */
  .ks-key {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 3px;
    min-width: 1.85em;
    padding: 0 0.62em;
    height: 1.85em;
    background-color: #3c3f41;
    background-image: linear-gradient(180deg, var(--bg-overlay), var(--bg-hover));
    border: 1px solid var(--border);
    border-bottom-width: 2px;
    border-radius: 7px;
    color: var(--text-primary);
    font-family: var(--font-code);
    font-weight: 600;
    letter-spacing: 0.3px;
    line-height: 1;
    box-shadow:
      0 1px 0 color-mix(in srgb, white 8%, transparent) inset,
      0 1px 2px rgba(0, 0, 0, 0.18);
    transition: transform var(--anim-dur-fast, 80ms) ease;
  }

  .ks-key.mod {
    background-color: var(--bg-overlay);
    background-image: linear-gradient(
      180deg,
      color-mix(in srgb, var(--ks-tone) 22%, var(--bg-overlay)),
      color-mix(in srgb, var(--ks-tone) 12%, var(--bg-hover))
    );
    border-color: color-mix(in srgb, var(--ks-tone) 55%, var(--border));
    color: color-mix(in srgb, var(--ks-tone) 92%, var(--text-primary));
    text-shadow: 0 0 8px color-mix(in srgb, var(--ks-tone) 30%, transparent);
  }

  /* Hide the inline "Shift" label when the icon already conveys it on
     small / medium sizes — keeps the cap compact. */
  .sz-sm .ks-key-text,
  .sz-md .ks-key-text { display: none; }

  /* ── Repeat counter ──────────────────────────────────────────────────── */
  .ks-count {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 1px 7px;
    background: color-mix(in srgb, var(--ks-tone) 18%, transparent);
    border: 1px solid color-mix(in srgb, var(--ks-tone) 45%, var(--border));
    border-radius: 999px;
    color: color-mix(in srgb, var(--ks-tone) 92%, var(--text-primary));
    font-family: var(--font-code);
    font-size: 0.78em;
    font-weight: 700;
    letter-spacing: 0.4px;
    margin-left: 2px;
    flex-shrink: 0;
  }

  /* ── Action description ──────────────────────────────────────────────── */
  .ks-action {
    color: var(--text-muted);
    font-size: 0.82em;
    font-weight: 500;
    letter-spacing: 0.15px;
    line-height: 1.25;
    text-transform: none;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 36ch;
    margin-left: 2px;
    /* Very subtle gradient text — gives a "label" feel without competing
       with the chord visually. */
    background: linear-gradient(
      90deg,
      var(--text-secondary, var(--text-muted)),
      color-mix(in srgb, var(--ks-tone) 35%, var(--text-secondary, var(--text-muted)))
    );
    -webkit-background-clip: text;
            background-clip: text;
    -webkit-text-fill-color: transparent;
            color: transparent;
  }
  /* Slightly tighten spacing when no action label is rendered — the card
     becomes a pure chord pill. */
  .ks-card:not(.has-action) .ks-body { padding: 10px 14px 11px 12px; }

  /* ── Size variants ────────────────────────────────────────────────────── */
  .sz-sm .ks-body    { padding: 7px 11px 8px 10px; gap: 3px; }
  .sz-sm .ks-stripe  { width: 2px; }
  .sz-sm .ks-key     { font-size: 10px; min-width: 1.6em; height: 1.6em; padding: 0 0.5em; border-radius: 5px; }
  .sz-sm .ks-action  { font-size: 10px; max-width: 30ch; }
  .sz-sm .ks-card    { border-radius: 11px; }

  .sz-md .ks-key     { font-size: 12px; }
  .sz-md .ks-action  { font-size: 11px; }

  .sz-lg .ks-body    { padding: 12px 18px 13px 14px; gap: 6px; }
  .sz-lg .ks-stripe  { width: 4px; }
  .sz-lg .ks-key     { font-size: 15px; min-width: 2.1em; height: 2.1em; padding: 0 0.75em; border-radius: 9px; border-bottom-width: 3px; }
  .sz-lg .ks-action  { font-size: 13px; max-width: 44ch; }
  .sz-lg .ks-card    { border-radius: 16px; }
  .sz-lg .ks-count   { font-size: 0.85em; padding: 2px 9px; }

  /* ── Entrance animation ─────────────────────────────────────────────── */
  @keyframes ks-pop {
    0% {
      transform: scale(0.92);
      box-shadow:
        0 1px 0 color-mix(in srgb, white 10%, transparent) inset,
        0 0 0 2px color-mix(in srgb, var(--ks-tone) 45%, transparent),
        0 12px 30px -10px rgba(0, 0, 0, 0.55);
    }
    60% {
      transform: scale(1.015);
    }
    100% {
      transform: scale(1);
      box-shadow:
        0 1px 0 color-mix(in srgb, white 10%, transparent) inset,
        0 0 0 1px color-mix(in srgb, var(--accent) 10%, transparent),
        0 12px 30px -10px rgba(0, 0, 0, 0.55);
    }
  }

  /* ── Compact (single-line) layout ─────────────────────────────────────
     Chord and action sit on the same row, separated by a thin dot.
     Removes the vertical body stack and pulls everything inline. */
  .ks-root.compact .ks-body {
    flex-direction: row;
    align-items: center;
    gap: 10px;
    padding: 7px 14px 7px 12px;
  }
  .ks-root.compact .ks-card.has-action .ks-action::before {
    content: '·';
    display: inline-block;
    margin-right: 8px;
    color: var(--text-disabled);
    font-weight: 700;
  }
  .ks-root.compact .ks-action { max-width: 28ch; }
  .ks-root.compact.sz-lg .ks-action { max-width: 36ch; }

  /* Reduced-motion users skip the bounce and the entrance fly. */
  @media (prefers-reduced-motion: reduce) {
    .ks-card { animation: none; }
  }
</style>
