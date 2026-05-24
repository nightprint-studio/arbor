<!--
  MarketplaceThemeRow — single theme entry in the Marketplace left-pane list.
  Renders a colour swatch tile (Aa over the theme's bg/fg/accent), name +
  variant, source + installed glyphs, and a short description.
-->
<script lang="ts">
  import { ChevronRight } from 'lucide-svelte';
  import MarketplaceStatusIcons from './MarketplaceStatusIcons.svelte';
  import type { MarketplaceTheme } from '$lib/types/marketplace';

  interface Props {
    theme:         MarketplaceTheme;
    selected:      boolean;
    onSelect:      () => void;
    onContextMenu: (e: MouseEvent) => void;
  }

  let { theme: t, selected, onSelect, onContextMenu }: Props = $props();
</script>

<button class="row theme-row" class:selected
        onclick={onSelect}
        oncontextmenu={onContextMenu}>
  <div class="swatch" style="background: {t.preview.bg}; color: {t.preview.fg};">
    <span class="letter" style="color: {t.preview.fg};">Aa</span>
    <span class="dot" style="background: {t.preview.accent};"></span>
  </div>

  <div class="body">
    <div class="top">
      <span class="name">{t.name}</span>
      {#if t.variant}<span class="version">{t.variant}</span>{/if}
      <MarketplaceStatusIcons
        source={t.source}
        installed={t.installed}
      />
    </div>
    <span class="desc">{t.description}</span>
  </div>

  <ChevronRight size={12} class="row-chev" />
</button>

<style>
  .row {
    display: flex;
    align-items: stretch;
    gap: 10px;
    width: 100%;
    text-align: left;
    padding: 10px;
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius-md);
    cursor: pointer;
    margin-bottom: 2px;
    transition: background var(--transition-fast), border-color var(--transition-fast);
  }
  .row:hover { background: var(--bg-hover); }
  .row.selected {
    background: var(--accent-subtle);
    border-color: var(--accent);
  }

  .body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
    justify-content: center;
  }
  .top {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }
  .name {
    font-size: var(--font-size-sm);
    font-weight: 600;
    color: var(--text-primary);
  }
  .version {
    font-size: 10px;
    color: var(--text-muted);
    font-family: var(--font-mono);
  }
  .desc {
    font-size: 11.5px;
    color: var(--text-secondary);
    line-height: 1.35;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  /* Theme swatch tile */
  .swatch {
    position: relative;
    width: 48px;
    height: 48px;
    border-radius: var(--radius-sm);
    display: flex;
    align-items: center;
    justify-content: center;
    border: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }
  .letter {
    font-size: 16px;
    font-weight: 700;
    font-family: var(--font-mono);
  }
  .dot {
    position: absolute;
    right: 4px;
    bottom: 4px;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    border: 1px solid rgba(255, 255, 255, 0.25);
  }

  .row :global(.row-chev) {
    flex-shrink: 0;
    color: var(--text-disabled);
    align-self: center;
  }
  .row.selected :global(.row-chev) { color: var(--accent); }
</style>
