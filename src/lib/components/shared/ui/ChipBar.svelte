<!--
  ChipBar — horizontal pill selector with optional counts.

  Two main use cases:
    · "Filter chips" above a list/grid (single-select, picks an active
      category; first chip is typically `all`).
    · "Tag chips" for multi-faceted filtering (multi-select).

  Each item ships a stable `id`, a `label`, optional `count` (rendered as
  a small number bubble), optional `tone` (override the colour), and an
  optional `icon` (Lucide name resolved through PLUGIN_ICONS).
-->
<script module lang="ts">
  // ── Public types (must live in a module-scope script so consumers can
  //    `import type { ChipItem } from '…/ChipBar.svelte'`. Svelte 5
  //    disallows `export type/interface` from a regular <script>). ──
  export type ChipTone = 'accent' | 'info' | 'success' | 'warning' | 'error' | 'muted' | 'neutral';

  export interface ChipItem {
    id:     string;
    label:  string;
    count?: number;
    tone?:  ChipTone;
    icon?:  string;
    /** Tooltip shown on hover. */
    tooltip?: string;
    /** Disable the chip — rendered dim, no click. */
    disabled?: boolean;
  }
</script>

<script lang="ts">
  import { PLUGIN_ICONS } from '$lib/utils/plugin-icons';

  interface Props {
    items:     ChipItem[];
    /** Currently selected id(s). `string` in single mode, `string[]` in multi. */
    selected:  string | string[];
    /** Multi-select mode (toggling a chip adds/removes it). */
    multi?:    boolean;
    /** Visual density. Default: `md`. */
    size?:     'sm' | 'md';
    /** When set, the chip's count badge uses the chip's own tone instead of neutral. */
    tintCount?: boolean;
    onSelect:  (sel: string | string[]) => void;
  }

  let { items, selected, multi = false, size = 'md', tintCount = true, onSelect }: Props = $props();

  function isActive(id: string): boolean {
    if (multi) return Array.isArray(selected) && selected.includes(id);
    return selected === id;
  }

  function handleClick(id: string) {
    if (!multi) {
      onSelect(id);
      return;
    }
    const cur = Array.isArray(selected) ? selected : [];
    if (cur.includes(id)) onSelect(cur.filter(x => x !== id));
    else                  onSelect([...cur, id]);
  }
</script>

<div class="chip-bar sz-{size}" role="toolbar">
  {#each items as it (it.id)}
    {@const Icon = it.icon ? PLUGIN_ICONS[it.icon] : null}
    {@const active = isActive(it.id)}
    <button
      type="button"
      class="chip"
      class:active
      class:disabled={it.disabled}
      data-tone={it.tone ?? 'neutral'}
      title={it.tooltip ?? undefined}
      disabled={!!it.disabled}
      onclick={() => handleClick(it.id)}
    >
      {#if Icon}<Icon size={size === 'sm' ? 10 : 11} class="chip-icon" />{/if}
      <span class="chip-label">{it.label}</span>
      {#if it.count !== undefined && it.count !== null}
        <span class="chip-count" class:tinted={tintCount}>{it.count}</span>
      {/if}
    </button>
  {/each}
</div>

<style>
  .chip-bar {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 2px 9px 2px 9px;
    border-radius: 999px;
    background: transparent;
    border: 1px solid var(--border);
    color: var(--text-secondary);
    cursor: pointer;
    font-family: var(--font-ui-sans);
    font-weight: 500;
    transition: background var(--transition-fast),
                color var(--transition-fast),
                border-color var(--transition-fast);
    min-height: 22px;
  }
  .sz-md .chip { font-size: 11px; padding: 2px 10px; min-height: 22px; }
  .sz-sm .chip { font-size: 10px; padding: 1px 7px;  min-height: 18px; }

  .chip:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text-primary);
    border-color: var(--border-strong, var(--border));
  }

  .chip.disabled,
  .chip:disabled {
    cursor: default;
    opacity: 0.5;
  }

  /* ── Active state — color picked from data-tone ──────────────────────── */
  .chip.active {
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    border-color: color-mix(in srgb, var(--accent) 36%, transparent);
  }

  .chip.active[data-tone="info"]    { color: var(--info);    background: color-mix(in srgb, var(--info)    14%, transparent); border-color: color-mix(in srgb, var(--info)    36%, transparent); }
  .chip.active[data-tone="success"] { color: var(--success); background: color-mix(in srgb, var(--success) 14%, transparent); border-color: color-mix(in srgb, var(--success) 36%, transparent); }
  .chip.active[data-tone="warning"] { color: var(--warning); background: color-mix(in srgb, var(--warning) 14%, transparent); border-color: color-mix(in srgb, var(--warning) 36%, transparent); }
  .chip.active[data-tone="error"]   { color: var(--error);   background: color-mix(in srgb, var(--error)   14%, transparent); border-color: color-mix(in srgb, var(--error)   36%, transparent); }
  .chip.active[data-tone="muted"]   { color: var(--text-secondary); background: var(--bg-overlay); border-color: var(--border); }
  .chip.active[data-tone="neutral"] { color: var(--text-primary);   background: var(--bg-overlay); border-color: var(--border-strong, var(--border)); }

  :global(.chip-icon) { opacity: 0.9; flex-shrink: 0; }

  .chip-label { white-space: nowrap; }

  .chip-count {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 16px;
    padding: 0 5px;
    height: 14px;
    border-radius: 999px;
    background: var(--bg-overlay);
    color: var(--text-disabled);
    font-size: 9px;
    font-weight: 700;
    line-height: 1;
  }
  .chip.active .chip-count.tinted {
    background: color-mix(in srgb, currentColor 20%, transparent);
    color: currentColor;
  }
</style>
