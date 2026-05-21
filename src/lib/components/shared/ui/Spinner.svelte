<script lang="ts">
  import { Loader2 } from 'lucide-svelte';

  type Size = 'xs' | 'sm' | 'md' | 'lg' | 'xl';
  type Variant = 'spin' | 'dots' | 'bars';

  interface Props {
    /** Visual size — maps to a numeric pixel size for the icon. */
    size?: Size | number;
    /** Animation variant. 'spin' = rotating Loader icon (default), 'dots' = three pulsing dots, 'bars' = three bouncing bars. */
    variant?: Variant;
    /** CSS color value — defaults to currentColor. */
    color?: string;
    /** Show inline (default) vs block-centered. */
    block?: boolean;
    /** Optional label rendered next to the spinner. */
    label?: string;
    /** Accessibility: announce to screen readers. */
    ariaLabel?: string;
  }

  let {
    size = 'md',
    variant = 'spin',
    color,
    block = false,
    label,
    ariaLabel = 'Loading',
  }: Props = $props();

  const px = $derived(
    typeof size === 'number'
      ? size
      : size === 'xs' ? 11
      : size === 'sm' ? 14
      : size === 'md' ? 16
      : size === 'lg' ? 22
      : /* xl */        32,
  );
</script>

<span
  class="spinner v-{variant}"
  class:block
  style={color ? `color: ${color};` : undefined}
  role="status"
  aria-label={ariaLabel}
  aria-live="polite"
>
  {#if variant === 'spin'}
    <Loader2 size={px} class="spinner-icon" />
  {:else if variant === 'dots'}
    <span class="dots" style="--dot-size: {Math.max(3, Math.round(px / 4))}px">
      <span class="dot"></span>
      <span class="dot"></span>
      <span class="dot"></span>
    </span>
  {:else}
    <span class="bars" style="--bar-h: {px}px; --bar-w: {Math.max(2, Math.round(px / 6))}px">
      <span class="bar"></span>
      <span class="bar"></span>
      <span class="bar"></span>
    </span>
  {/if}

  {#if label}
    <span class="spinner-label">{label}</span>
  {/if}
</span>

<style>
  .spinner {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--text-muted);
    line-height: 1;
  }
  .spinner.block {
    display: flex;
    justify-content: center;
    width: 100%;
  }

  .spinner-label {
    font-size: var(--font-size-xs);
    color: var(--text-muted);
  }

  /* ---- Spin ---- */
  /* The icon class is :global(...) because lucide-svelte forwards `class`
     to the SVG element (which lives outside this component's scoped class
     hashes). The keyframes MUST be `-global-…` so Svelte does not rename
     them — otherwise the :global rule references a name that no longer
     exists and the icon stays still. Same reasoning for pulse/bounce. */
  :global(.spinner-icon) { animation: spinner-rotate 1s linear infinite; }
  @keyframes -global-spinner-rotate {
    from { transform: rotate(0deg); }
    to   { transform: rotate(360deg); }
  }

  /* ---- Dots ---- */
  .dots { display: inline-flex; gap: calc(var(--dot-size) * 0.75); align-items: center; }
  .dot {
    width: var(--dot-size);
    height: var(--dot-size);
    background: currentColor;
    border-radius: 50%;
    animation: spinner-pulse 1.2s ease-in-out infinite;
  }
  .dot:nth-child(1) { animation-delay: 0s; }
  .dot:nth-child(2) { animation-delay: 0.2s; }
  .dot:nth-child(3) { animation-delay: 0.4s; }
  @keyframes -global-spinner-pulse {
    0%, 80%, 100% { opacity: 0.3; transform: scale(0.85); }
    40%           { opacity: 1;   transform: scale(1.1);  }
  }

  /* ---- Bars ---- */
  .bars { display: inline-flex; gap: calc(var(--bar-w) * 0.7); align-items: flex-end; height: var(--bar-h); }
  .bar {
    width: var(--bar-w);
    height: 60%;
    background: currentColor;
    border-radius: 1px;
    animation: spinner-bounce 1s ease-in-out infinite;
  }
  .bar:nth-child(1) { animation-delay: 0s; }
  .bar:nth-child(2) { animation-delay: 0.15s; }
  .bar:nth-child(3) { animation-delay: 0.3s; }
  @keyframes -global-spinner-bounce {
    0%, 100% { height: 30%; }
    50%      { height: 100%; }
  }
</style>
