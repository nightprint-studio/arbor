<script lang="ts">
  import type { Snippet } from 'svelte';
  import { ChevronDown, Loader2 } from 'lucide-svelte';
  import Dropdown from './Dropdown.svelte';
  import type { DropdownItem } from './Dropdown.svelte';
  import { tooltip as tooltipAction } from '$lib/actions/tooltip';
  import type { TooltipInput } from '$lib/stores/tooltip.svelte';

  export interface SplitOption {
    id: string;
    label: string;
    /** Svelte component (e.g. a Lucide icon) rendered at size 14. */
    icon?: any;
    description?: string;
    disabled?: boolean;
    /** Treat this entry as a non-clickable group header (separator with label). */
    group?: boolean;
  }

  type Variant = 'primary' | 'secondary' | 'ghost' | 'danger';
  type Size    = 'xs' | 'sm' | 'md' | 'lg';

  interface Props {
    label?: string;
    children?: Snippet;
    options?: SplitOption[];
    disabled?: boolean;
    loading?: boolean;
    variant?: Variant;
    size?: Size;
    /**
     * Direction the dropdown menu opens. `'up'`/`'down'` are valid for
     * both `position` modes; `'left'`/`'right'` are only meaningful when
     * `position="fixed"` (typical for narrow vertical toolbars where the
     * menu has to escape the toolbar lane sideways).
     */
    direction?: 'up' | 'down' | 'left' | 'right';
    /**
     * Menu anchor mode (passed straight through to the underlying
     * `<Dropdown>`). Default `"absolute"` keeps the historical inline
     * behaviour. Switch to `"fixed"` when the trigger lives inside an
     * `overflow:hidden` ancestor — the menu then escapes to the viewport.
     */
    position?: 'absolute' | 'fixed';
    /** Forwarded to `<Dropdown width>`. Use it to size the menu. */
    width?: string;
    title?: string;
    /** Rich tooltip; wins over `title` when both are set. */
    tooltip?: TooltipInput;
    /** Optional CSS color override (background for primary, text for others). */
    color?: string;
    /** Extra class on the split-btn root. */
    class?: string;
    onclick?: () => void;
    onselect?: (id: string) => void;
  }

  let {
    label,
    children,
    options = [],
    disabled = false,
    loading = false,
    variant = 'primary',
    size = 'md',
    direction = 'up',
    position = 'absolute',
    width,
    title,
    tooltip,
    color,
    class: rootClass = '',
    onclick,
    onselect,
  }: Props = $props();

  const mainTip = $derived<TooltipInput>(tooltip ?? title ?? '');

  const items = $derived<DropdownItem[]>(
    options.map((opt) =>
      opt.group
        ? ({ kind: 'separator', label: opt.label } as DropdownItem)
        : ({
            kind: 'item',
            id: opt.id,
            label: opt.label,
            icon: opt.icon,
            description: opt.description,
            disabled: opt.disabled,
            onclick: () => onselect?.(opt.id),
          } as DropdownItem),
    ),
  );

  const spinnerSize = $derived(size === 'xs' ? 11 : size === 'sm' ? 12 : size === 'lg' ? 16 : 14);
</script>

<Dropdown
  {position}
  {direction}
  {width}
  {items}
  class="split-dd"
>
  {#snippet trigger({ toggle, close })}
    <div
      class="split-btn split-{variant} split-sz-{size} {rootClass}"
      class:split-has-color={!!color}
      style={color ? `--split-color:${color}` : undefined}
    >
      <button
        type="button"
        class="split-main"
        use:tooltipAction={mainTip}
        aria-busy={loading || undefined}
        disabled={disabled || loading}
        onclick={() => { if (!disabled && !loading) { close(); onclick?.(); } }}
      >
        {#if loading}
          <Loader2 size={spinnerSize} class="split-spin" />
        {/if}
        {#if children}
          {@render children()}
        {:else}
          <span class="split-label">{label}</span>
        {/if}
      </button>

      {#if options.length > 0}
        <button
          type="button"
          class="split-chevron"
          disabled={disabled || loading}
          tabindex="-1"
          aria-label="More options"
          use:tooltipAction={'More options'}
          onclick={toggle}
        >
          <ChevronDown size={11} />
        </button>
      {/if}
    </div>
  {/snippet}
</Dropdown>

<style>
  /* Wrapper from Dropdown becomes the positioning context. The menu is
     anchored to the right edge of the whole split-btn (not to the chevron). */
  :global(.split-dd) {
    display: inline-flex;
    align-items: stretch;
  }
  :global(.split-dd > .dd-menu) {
    left: auto;
    right: 0;
    min-width: 200px;
  }

  .split-btn {
    display: flex;
    align-items: stretch;
    width: 100%;
  }

  /* ---- Main + chevron base ---- */
  .split-main, .split-chevron {
    border: none;
    cursor: pointer;
    display: flex;
    align-items: center;
    transition: background var(--transition-fast), color var(--transition-fast),
                filter var(--transition-fast);
    font-family: var(--font-ui-sans);
    font-weight: 500;
    line-height: 1;
  }
  .split-main {
    flex: 1;
    border-radius: var(--radius-md) 0 0 var(--radius-md);
    gap: 6px;
    white-space: nowrap;
    justify-content: center;
  }
  .split-chevron {
    border-radius: 0 var(--radius-md) var(--radius-md) 0;
    flex-shrink: 0;
    justify-content: center;
  }
  .split-main:disabled, .split-chevron:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  /* ---- Sizes ---- */
  .split-sz-xs .split-main    { padding: 2px 8px;  font-size: var(--font-size-xs); }
  .split-sz-xs .split-chevron { padding: 2px 5px; }
  .split-sz-sm .split-main    { padding: 3px 10px; font-size: var(--font-size-xs); }
  .split-sz-sm .split-chevron { padding: 3px 6px; }
  .split-sz-md .split-main    { padding: 5px 12px; font-size: var(--font-size-sm); }
  .split-sz-md .split-chevron { padding: 5px 7px; }
  .split-sz-lg .split-main    { padding: 7px 16px; font-size: var(--font-size-md); }
  .split-sz-lg .split-chevron { padding: 7px 9px; }

  /* ---- Variants (mirror Button) ---- */
  .split-primary  .split-main,
  .split-primary  .split-chevron {
    background: var(--accent);
    color: var(--text-on-accent);
  }
  .split-primary  .split-chevron { border-left: 1px solid rgba(255,255,255,0.18); }
  .split-primary  .split-main:hover:not(:disabled),
  .split-primary  .split-chevron:hover:not(:disabled) { background: var(--accent-hover); }

  .split-secondary .split-main,
  .split-secondary .split-chevron {
    background: var(--bg-overlay);
    color: var(--text-secondary);
  }
  .split-secondary .split-main    { border: 1px solid var(--border); border-right: none; }
  .split-secondary .split-chevron { border: 1px solid var(--border); border-left: 1px solid var(--border-subtle); }
  .split-secondary .split-main:hover:not(:disabled),
  .split-secondary .split-chevron:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  .split-ghost    .split-main,
  .split-ghost    .split-chevron { background: transparent; color: var(--text-secondary); }
  .split-ghost    .split-chevron { border-left: 1px solid var(--border-subtle); }
  .split-ghost    .split-main:hover:not(:disabled),
  .split-ghost    .split-chevron:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  .split-danger   .split-main,
  .split-danger   .split-chevron { background: transparent; color: var(--error); }
  .split-danger   .split-chevron { border-left: 1px solid var(--border-subtle); }
  .split-danger   .split-main:hover:not(:disabled),
  .split-danger   .split-chevron:hover:not(:disabled) {
    background: var(--error-subtle);
    border-color: var(--error);
  }

  /* ---- Color override (--split-color) ----
     Used to render brand-coloured CTAs (Connect Linear, Connect Jira, …).
     The bg is an absolute brand colour, so the foreground MUST be a fixed
     white — `--text-on-accent` is theme-dependent and would resolve to
     dark in light themes, making the label illegible on the brand fill. */
  .split-has-color.split-primary .split-main,
  .split-has-color.split-primary .split-chevron {
    background: var(--split-color);
    color: #fff;
  }
  .split-has-color.split-primary .split-chevron {
    border-left: 1px solid rgba(255,255,255,0.2);
  }
  .split-has-color.split-primary .split-main:hover:not(:disabled),
  .split-has-color.split-primary .split-chevron:hover:not(:disabled) {
    background: var(--split-color);
    filter: brightness(1.12);
  }

  /* ---- Spinner ---- */
  :global(.split-spin) { animation: split-spin-anim 1s linear infinite; }
  @keyframes split-spin-anim {
    from { transform: rotate(0deg); }
    to   { transform: rotate(360deg); }
  }
</style>
