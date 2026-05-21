<script lang="ts">
  import type { Snippet } from 'svelte';

  /**
   * variant:
   *   'pill'   — rounded pill, accent bg (e.g. count in sidebar headers)
   *   'status' — square-ish, semantic color (HEAD, local, etc.)
   *   'chip'   — colored chip with custom color (labels, ticket chips)
   *   'count'  — minimal number bubble
   *   'sync'   — ahead/behind with icon
   *   'tone'   — semantic tone (info/success/warning/error/accent)
   */
  type Variant = 'pill' | 'status' | 'chip' | 'count' | 'sync' | 'tone';
  type Size    = 'sm' | 'md';
  type Tone    = 'info' | 'success' | 'warning' | 'error' | 'accent' | 'neutral' | 'tag' | 'stash';

  interface Props {
    variant?: Variant;
    size?: Size;
    /** For variant='tone' — picks a semantic palette. */
    tone?: Tone;
    /** Custom CSS color — applied to text + derived background for chip/status. */
    color?: string;
    bg?: string;
    border?: string;
    /** Show a small leading dot (filled, currentColor). */
    dot?: boolean;
    label?: string;
    icon?: Snippet;
    children?: Snippet;
  }

  let {
    variant = 'pill',
    size    = 'md',
    tone    = 'neutral',
    color,
    bg,
    border,
    dot     = false,
    label,
    icon,
    children,
  }: Props = $props();

  const style = $derived([
    color  ? `color: ${color};` : '',
    bg     ? `background: ${bg};` : '',
    border ? `border-color: ${border};` : '',
  ].filter(Boolean).join(' '));
</script>

<span
  class="badge badge-{variant} sz-{size}"
  class:tone-info={variant === 'tone' && tone === 'info'}
  class:tone-success={variant === 'tone' && tone === 'success'}
  class:tone-warning={variant === 'tone' && tone === 'warning'}
  class:tone-error={variant === 'tone' && tone === 'error'}
  class:tone-accent={variant === 'tone' && tone === 'accent'}
  class:tone-neutral={variant === 'tone' && tone === 'neutral'}
  class:tone-tag={variant === 'tone' && tone === 'tag'}
  class:tone-stash={variant === 'tone' && tone === 'stash'}
  style={style || undefined}
>
  {#if dot}<span class="badge-dot"></span>{/if}
  {#if icon}{@render icon()}{/if}
  {#if children}{@render children()}{:else if label}{label}{/if}
</span>

<style>
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-weight: 600;
    white-space: nowrap;
    flex-shrink: 0;
    border: 1px solid transparent;
    border-radius: 999px;
    line-height: 1;
  }

  .badge-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: currentColor;
    flex-shrink: 0;
  }

  /* ---- Sizes ---- */
  .sz-sm { font-size: 9px;  padding: 0 4px; height: 14px; }
  .sz-md { font-size: 10px; padding: 1px 5px; height: 16px; }

  /* ---- Variants ---- */
  .badge-pill {
    background: rgba(255, 255, 255, 0.06);
    color: var(--text-secondary);
    min-width: 16px;
    justify-content: center;
  }

  .badge-count {
    background: var(--accent);
    color: var(--text-on-accent);
    min-width: 16px;
    justify-content: center;
  }

  .badge-status {
    font-weight: 700;
    font-size: 9px;
    height: 14px;
    padding: 0 4px;
  }

  .badge-chip {
    border-radius: var(--radius-sm);
    padding: 1px 6px;
  }

  .badge-sync {
    background: transparent;
    border: none;
    padding: 0 2px;
    gap: 1px;
  }

  /* ---- Semantic tones (variant='tone') ---- */
  .tone-info    { background: color-mix(in srgb, var(--info)    14%, transparent); color: var(--info);    border-color: color-mix(in srgb, var(--info)    32%, transparent); }
  .tone-success { background: color-mix(in srgb, var(--success) 14%, transparent); color: var(--success); border-color: color-mix(in srgb, var(--success) 32%, transparent); }
  .tone-warning { background: color-mix(in srgb, var(--warning) 14%, transparent); color: var(--warning); border-color: color-mix(in srgb, var(--warning) 32%, transparent); }
  .tone-error   { background: color-mix(in srgb, var(--error)   14%, transparent); color: var(--error);   border-color: color-mix(in srgb, var(--error)   32%, transparent); }
  .tone-accent  { background: color-mix(in srgb, var(--accent)  18%, transparent); color: var(--accent);  border-color: color-mix(in srgb, var(--accent)  35%, transparent); }
  .tone-neutral { background: var(--bg-overlay); color: var(--text-secondary); border-color: var(--border-subtle); }
  .tone-tag     { background: color-mix(in srgb, var(--color-tag)   14%, transparent); color: var(--color-tag);   border-color: color-mix(in srgb, var(--color-tag)   32%, transparent); }
  .tone-stash   { background: color-mix(in srgb, var(--color-stash) 14%, transparent); color: var(--color-stash); border-color: color-mix(in srgb, var(--color-stash) 32%, transparent); }
</style>
