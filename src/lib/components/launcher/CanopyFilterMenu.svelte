<script lang="ts">
  /**
   * Titlebar filter dropdown — shows the active filter (dot · label · count) and
   * opens a compact menu to pick Tutti / In esecuzione / Da aggiornare. Built on
   * the shared `Dropdown` (open/close, positioning, full keyboard nav); the
   * `--dd-*` overrides give it the launcher's theme-independent "sky" palette.
   */
  import type { FilterKey } from './canopy';
  import Dropdown, { type DropdownItem } from '$lib/components/shared/ui/Dropdown.svelte';

  interface Chip { key: FilterKey; label: string; count: number; color: string; active: boolean; }
  interface Props { chips: Chip[]; onpick: (k: FilterKey) => void; }
  let { chips, onpick }: Props = $props();

  const current = $derived(chips.find(c => c.active) ?? chips[0]);
  const items = $derived<DropdownItem[]>(chips.map(c => ({
    kind: 'item', id: c.key, label: c.label, meta: String(c.count),
    active: c.active, onclick: () => onpick(c.key),
  })));
</script>

<div class="cv-sky">
  <Dropdown {items} position="fixed" direction="down" width="190px">
    {#snippet trigger({ open, toggle })}
      <button class="trigger" class:open onclick={toggle}
              aria-haspopup="menu" aria-expanded={open} title="Filtra">
        <span class="dot" style="background:{current.color};box-shadow:0 0 6px {current.color}"></span>
        <span class="label">{current.label}</span>
        <span class="count">{current.count}</span>
        <svg class="chev" class:flip={open} width="11" height="11" viewBox="0 0 24 24" fill="none">
          <path d="M6 9 L12 15 L18 9" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
      </button>
    {/snippet}
  </Dropdown>
</div>

<style>
  /* Theme-independent "sky" palette for the menu surface (see Dropdown's
     `--dd-*` hooks). Set on the wrapper so it cascades to the menu panel. */
  .cv-sky {
    display: inline-flex;
    --dd-bg: rgba(12, 16, 24, 0.96);
    --dd-border: rgba(255, 255, 255, 0.12);
    --dd-shadow: 0 18px 46px -16px rgba(0, 0, 0, 0.85);
    --dd-text: #c2cad6;
    --dd-text-muted: #9aa3b2;
    --dd-hover-bg: rgba(255, 255, 255, 0.06);
    --dd-active-bg: rgba(255, 255, 255, 0.09);
    --dd-check: #8fce6a;
  }

  .trigger {
    display: flex; align-items: center; gap: 7px; padding: 5px 9px; border-radius: 8px;
    font-family: var(--canopy-sans); font-size: 12px; font-weight: 500; cursor: pointer;
    white-space: nowrap; color: #cfd6e0; background: rgba(255, 255, 255, 0.04);
    border: 1px solid rgba(255, 255, 255, 0.1); transition: background 0.15s, border-color 0.15s;
  }
  .trigger:hover, .trigger.open { background: rgba(255, 255, 255, 0.09); border-color: rgba(255, 255, 255, 0.18); }
  .dot { width: 7px; height: 7px; border-radius: 50%; flex: none; display: inline-block; }
  .label { color: #dfe5ee; }
  .count {
    font-family: var(--canopy-mono); font-size: 10.5px; color: #9aa3b2;
    background: rgba(255, 255, 255, 0.07); border-radius: 9px; padding: 0 6px;
  }
  .chev { color: #7d8696; transition: transform 0.15s; flex: none; }
  .chev.flip { transform: rotate(180deg); }
</style>
