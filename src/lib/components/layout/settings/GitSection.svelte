<script lang="ts">
  import { onMount } from 'svelte';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import BrandTile from '$lib/components/shared/internal/BrandTile.svelte';
  import ProviderConnectionCard from '$lib/components/shared/internal/ProviderConnectionCard.svelte';
  import { gitProviders } from '$lib/ipc/providers';
  import type { ProviderDescriptor } from '$lib/types/providers';

  // Fully generic: fetch the self-describing git-host descriptors and render one
  // generic card per provider. No per-provider code, no hardcoded github/gitlab.
  let descriptors = $state<ProviderDescriptor[]>([]);

  onMount(async () => {
    try { descriptors = await gitProviders.list(); } catch { descriptors = []; }
  });
</script>

<SectionHeader title="Git" description="Connect to Git hosting providers. Credentials are stored in the OS keychain." />

{#each descriptors as d (d.id)}
  <div class="provider-slot">
    <ProviderConnectionCard descriptor={d} service={gitProviders} />
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
  .provider-name { font-size: 13px; font-weight: 600; color: var(--text-primary); }
  .provider-desc { font-size: 11px; color: var(--text-muted); }

  .badge-soon {
    font-size: 10px; font-weight: 600; padding: 2px 7px;
    background: var(--bg-overlay); border: 1px solid var(--border-subtle);
    border-radius: 99px; color: var(--text-muted);
  }
</style>
