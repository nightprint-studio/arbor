<script lang="ts">
  /** Sound bank — registry voices (default synths + VSCO 2 samplers), grouped. */
  import { Music4, Waves, Piano, Download, Check, Play } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import SidebarSection from '$lib/components/shared/ui/SidebarSection.svelte';
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { MOCK_VOICES } from '../mock/data';
  import type { Voice } from '../mock/types';

  const synths   = $derived(MOCK_VOICES.filter(v => v.kind === 'synth'));
  const samplers = $derived(MOCK_VOICES.filter(v => v.kind === 'sampler'));
  let openSynth = $state(true);
  let openSampler = $state(true);
</script>

{#snippet voiceRow(v: Voice)}
  <SidebarItem>
    {#snippet icon()}
      {#if v.kind === 'synth'}<Waves size={13} />{:else}<Piano size={13} />{/if}
    {/snippet}
    <span class="bank-name">{v.name}</span>
    {#snippet badges()}
      {#if v.kind === 'sampler'}
        {#if v.installed}
          <Badge variant="tone" tone="success" size="sm"><Check size={9} /> ready</Badge>
        {:else}
          <span use:tooltip={'Not installed — download the VSCO 2 bank'}><Badge variant="tone" tone="neutral" size="sm"><Download size={9} /> get</Badge></span>
        {/if}
      {/if}
    {/snippet}
    {#snippet actions()}
      <button use:tooltip={'Preview'} aria-label="Preview voice"><Play size={12} /></button>
    {/snippet}
  </SidebarItem>
{/snippet}

<PanelShell title="Sound bank" count={MOCK_VOICES.length}>
  {#snippet icon()}<Music4 size={13} />{/snippet}

  <div class="bank">
    <SidebarSection label="Synth presets" bind:expanded={openSynth} badge={synths.length}>
      {#snippet icon()}<Waves size={13} />{/snippet}
      {#each synths as v (v.id)}{@render voiceRow(v)}{/each}
    </SidebarSection>

    <SidebarSection label="VSCO 2 samplers" bind:expanded={openSampler} badge={samplers.length}>
      {#snippet icon()}<Piano size={13} />{/snippet}
      {#each samplers as v (v.id)}{@render voiceRow(v)}{/each}
    </SidebarSection>
  </div>
</PanelShell>

<style>
  .bank { padding: 4px 0; }
  .bank-name { font-family: var(--font-code); font-size: 11.5px; }
</style>
