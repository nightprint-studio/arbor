<!--
  MarketplaceDetailEmpty — placeholder shown on the Marketplace right pane
  when nothing is selected. Tab-aware: shows the appropriate copy + icon for
  plugins vs themes, and the three source-classification hints stay visible
  below as a legend.
-->
<script lang="ts">
  import { Package, Palette } from 'lucide-svelte';
  import MarketplaceBadge from './MarketplaceBadge.svelte';
  import type { MarketplaceTab } from '$lib/types/marketplace';

  interface Props {
    tab: MarketplaceTab;
  }

  let { tab }: Props = $props();
</script>

<div class="detail-empty">
  {#if tab === 'plugins'}
    <Package size={48} class="empty-icon" />
    <h3>Select a plugin to see details</h3>
    <p>Browse the list on the left. Each plugin shows the permissions it asks for, its source repository and a description.</p>
  {:else}
    <Palette size={48} class="empty-icon" />
    <h3>Select a theme to preview</h3>
    <p>Each theme ships as a JSON file; you can install several and switch between them from Settings → Appearance.</p>
  {/if}
  <div class="hints">
    <div class="hint">
      <MarketplaceBadge tone="community">Community</MarketplaceBadge>
      Listed on the official <code>arbor-extensions</code> repo.
    </div>
    <div class="hint">
      <MarketplaceBadge tone="custom">Custom source</MarketplaceBadge>
      Third-party git URL — inspect the source before enabling.
    </div>
    <div class="hint">
      <MarketplaceBadge tone="local">Local</MarketplaceBadge>
      Manually installed (zip import or dev folder) — has no marketplace entry to update.
    </div>
  </div>
</div>

<style>
  .detail-empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    padding: 36px;
    gap: 8px;
    color: var(--text-muted);
  }
  .detail-empty :global(.empty-icon) { color: var(--text-disabled); }
  .detail-empty h3 {
    margin: 8px 0 0;
    font-size: var(--font-size-lg);
    color: var(--text-secondary);
    font-weight: 500;
  }
  .detail-empty p {
    margin: 0;
    max-width: 460px;
    line-height: 1.5;
    font-size: var(--font-size-sm);
  }
  .hints {
    margin-top: 20px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: 100%;
    max-width: 460px;
  }
  .hint {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    font-size: var(--font-size-xs);
    color: var(--text-secondary);
    text-align: left;
    line-height: 1.4;
  }
  .hint code {
    font-family: var(--font-mono);
    background: var(--bg-overlay);
    padding: 1px 5px;
    border-radius: var(--radius-sm);
    font-size: 11px;
  }
</style>
