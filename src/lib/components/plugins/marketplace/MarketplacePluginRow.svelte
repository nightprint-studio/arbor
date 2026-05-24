<!--
  MarketplacePluginRow — single plugin entry in the Marketplace left-pane
  list. Renders the icon (custom SVG / image / monogram fallback), name +
  version, status glyphs (source, installed, update, experimental), short
  description and author. Selection / context-menu wiring sits on the host.
-->
<script lang="ts">
  import { ChevronRight } from 'lucide-svelte';
  import Monogram from '$lib/components/shared/ui/Monogram.svelte';
  import MarketplaceStatusIcons from './MarketplaceStatusIcons.svelte';
  import { isInlineSvg } from '$lib/marketplace/ui-helpers';
  import type { MarketplacePlugin } from '$lib/types/marketplace';

  interface Props {
    plugin:        MarketplacePlugin;
    selected:      boolean;
    onSelect:      () => void;
    onContextMenu: (e: MouseEvent) => void;
  }

  let { plugin: p, selected, onSelect, onContextMenu }: Props = $props();

  const installedDisabled = $derived(p.installed && p.enabled === false);
  /** Dim the icon when the plugin isn't installed OR is installed-but-off —
   *  mirrors PluginPanel's monogram tinting so the two surfaces feel like one. */
  const dimIcon = $derived(!p.installed || installedDisabled);
</script>

<button class="row" class:selected class:installed-disabled={installedDisabled}
        onclick={onSelect}
        oncontextmenu={onContextMenu}>
  {#if p.icon}
    {#if isInlineSvg(p.icon)}
      <span class="icon-art icon-art-sm" class:dim={dimIcon} aria-hidden="true">{@html p.icon}</span>
    {:else}
      <img class="icon-art icon-art-sm" class:dim={dimIcon} src={p.icon} alt="" />
    {/if}
  {:else}
    <!-- Monogram fallback — dimmed when installed-disabled so the row reads
         as inactive at a glance, matching the Plugin Manager's treatment. -->
    <span class="monogram-wrap" class:dim={installedDisabled}>
      <Monogram name={p.name} initials={p.name[0].toUpperCase()}
                color="var(--accent-subtle)"
                fg="var(--accent)"
                size={42}
                disabled={installedDisabled} />
    </span>
  {/if}

  <div class="body">
    <div class="top">
      <span class="name">{p.name}</span>
      <span class="version">v{p.version}</span>
      <MarketplaceStatusIcons
        source={p.source}
        installed={p.installed}
        enabledState={p.installed ? (p.enabled ? 'enabled' : 'disabled') : undefined}
        updateAvailable={p.update_available}
        installedVersion={p.installed_version}
        experimental={p.experimental}
      />
    </div>
    <span class="desc">{p.description}</span>
    <div class="foot">
      <span class="author">by {p.author}</span>
    </div>
  </div>

  <ChevronRight size={12} class="row-chev" />
</button>

<style>
  /* ── Row chrome ──────────────────────────────────────────────────────── */
  .row {
    display: flex;
    align-items: center;
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

  /* Installed-but-disabled: subtle text/name dim so the user can immediately
     tell at a glance which plugins are inert. Keeping it subtle — the
     dedicated PowerOff glyph in the status cluster does the heavy lifting. */
  .row.installed-disabled .name,
  .row.installed-disabled .desc {
    color: var(--text-muted);
  }
  .row.installed-disabled .version { opacity: 0.7; }

  /* ── Body ────────────────────────────────────────────────────────────── */
  .body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
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
  .foot {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    margin-top: 2px;
  }
  .author {
    font-size: 10px;
    color: var(--text-muted);
    margin-left: auto;
    font-style: italic;
  }

  /* ── Icon art (custom SVG / image) ──────────────────────────────────── */
  .icon-art {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    background: var(--accent-subtle);
    color: var(--accent);
    border-radius: var(--radius-md);
    overflow: hidden;
  }
  .icon-art-sm { width: 36px; height: 36px; }
  .icon-art-sm :global(svg) { width: 22px; height: 22px; display: block; }
  /* Dim icon for not-yet-installed (or installed-disabled) entries. */
  .icon-art.dim {
    background: var(--bg-overlay);
    color: var(--text-secondary);
  }
  .monogram-wrap.dim {
    opacity: 0.55;
    filter: saturate(0.55);
    transition: opacity var(--transition-fast), filter var(--transition-fast);
  }

  /* Chevron — lucide icon needs :global() to receive the tint. */
  .row :global(.row-chev) {
    flex-shrink: 0;
    color: var(--text-disabled);
  }
  .row.selected :global(.row-chev) { color: var(--accent); }
</style>
