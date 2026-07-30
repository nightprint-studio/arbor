<script lang="ts">
  /**
   * The connectable git hosts, one generic card each.
   *
   * Shared because connecting a git host is not a Corvus-only concern: the File
   * Explorer performs git operations too, and a token that expires there must be
   * fixable without opening the Git window. Corvus's Settings ▸ Git and the
   * cross-product Credentials modal both render this.
   *
   * Fully descriptor-driven: the shell self-describes its providers, so there is
   * no per-provider code here — adding a provider is a backend change only.
   */
  import { onMount } from 'svelte';
  import BrandTile from './BrandTile.svelte';
  import ProviderConnectionCard from './ProviderConnectionCard.svelte';
  import { gitProviders } from '$lib/ipc/corvus/providers';
  import type { ProviderDescriptor } from '$lib/types/corvus/providers';

  interface Props {
    /** Notified after a successful connect/disconnect, to refresh dependent UI. */
    onchange?: () => void;
  }

  let { onchange }: Props = $props();

  let descriptors = $state<ProviderDescriptor[]>([]);

  onMount(async () => {
    try { descriptors = await gitProviders.list(); } catch { descriptors = []; }
  });
</script>

{#each descriptors as d (d.id)}
  <div class="provider-slot">
    <ProviderConnectionCard descriptor={d} service={gitProviders} {onchange} />
  </div>
{/each}

<!-- Bitbucket (coming soon) — static placeholder until a provider ships. -->
<div class="provider-card provider-disabled">
  <BrandTile brand="bitbucket" />
  <div class="provider-main">
    <div class="provider-top">
      <div class="provider-info">
        <span class="provider-name">Bitbucket</span>
        <span class="provider-desc">Atlassian Git hosting</span>
      </div>
      <span class="badge-soon">Coming soon</span>
    </div>
  </div>
</div>

<style>
  .provider-slot { margin-bottom: 12px; }

  .provider-card {
    display: flex; align-items: flex-start; gap: 13px;
    padding: 13px 14px;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
  }
  .provider-disabled { opacity: 0.45; pointer-events: none; }

  .provider-main { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 10px; }
  .provider-top  { display: flex; align-items: center; gap: 10px; }
  .provider-info { flex: 1; display: flex; flex-direction: column; gap: 2px; }
  .provider-name { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }
  .provider-desc { font-size: var(--font-size-xs); color: var(--text-muted); }

  .badge-soon {
    font-size: var(--font-size-2xs); font-weight: 600; padding: 2px 7px;
    background: var(--bg-overlay); border: 1px solid var(--border-subtle);
    border-radius: 99px; color: var(--text-muted);
  }
</style>
