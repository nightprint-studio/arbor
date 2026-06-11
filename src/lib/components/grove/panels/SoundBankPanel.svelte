<script lang="ts">
  /**
   * Sound bank — the engine's resolvable voices plus the downloadable sample
   * packs. Driven by the **real registry** (`soundsStore` ← `grove_sounds`), not
   * a static list, so it tracks what's actually installed.
   *
   * Three sections: the built-in synth presets (always present), the resolved
   * sampler voices (filled once any pack is installed), and the **Sample banks**
   * — one download card per pack (VSCO 2, Dirt-Samples, drum machines, …) with a
   * job-tracked install + live progress bar (+ Cancel). Downloads are async — the
   * UI never blocks.
   *
   * Imports only shared/ui (+ the tooltip action) + grove-local stores.
   */
  import { Music4, Waves, Piano, Download, Check, RefreshCw, Boxes } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import SidebarSection from '$lib/components/shared/ui/SidebarSection.svelte';
  import SidebarItem from '$lib/components/shared/ui/SidebarItem.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import ProgressBar from '$lib/components/shared/ui/ProgressBar.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { soundsStore } from '../stores/sounds.svelte';
  import { packsStore } from '../stores/packs.svelte';
  import type { GroveInstrument, GrovePack } from '$lib/ipc/grove';

  const synths   = $derived(soundsStore.synths);
  const samplers = $derived(soundsStore.samplers);
  let openSynth   = $state(true);
  // Samplers can run to the hundreds (Dirt-Samples), so start collapsed.
  let openSampler = $state(false);
  let openBanks   = $state(true);

  // The pack subscription is owned by the GroveShell; here we just (re)read the
  // registry on mount and again whenever the pack set changes (an install adds
  // sampler voices to the registry only after extraction).
  $effect(() => {
    void packsStore.packs; // dep: re-read after an install completes
    void soundsStore.refresh();
  });

  // Delayed spinner: only surface it if the first load actually takes a while
  // (grove_sounds is a fast registry read — usually no spinner at all).
  let slowLoad = $state(false);
  $effect(() => {
    if (soundsStore.loading && !soundsStore.loaded) {
      const t = setTimeout(() => { slowLoad = true; }, 250);
      return () => clearTimeout(t);
    }
    slowLoad = false;
  });

  function formatBytes(n: number): string {
    if (n <= 0) return '—';
    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    let v = n, i = 0;
    while (v >= 1024 && i < units.length - 1) { v /= 1024; i++; }
    return `${v < 10 && i > 0 ? v.toFixed(1) : Math.round(v)} ${units[i]}`;
  }
</script>

{#snippet voiceRow(inst: GroveInstrument)}
  <SidebarItem>
    {#snippet icon()}
      {#if inst.kind === 'synth'}<Waves size={13} />{:else}<Piano size={13} />{/if}
    {/snippet}
    <span class="bank-name">{inst.name}</span>
  </SidebarItem>
  {#if inst.articulations.length}
    <div class="arts" use:tooltip={'Articulations — use .art("…") on this instrument'}>
      {#each inst.articulations as a (a)}<span class="art-chip">{a}</span>{/each}
    </div>
  {/if}
{/snippet}

{#snippet packCard(pack: GrovePack)}
  {@const prog = packsStore.progressOf(pack.id)}
  <div class="pack">
    <div class="pack-head">
      <span class="pack-name">{pack.name}</span>
      {#if pack.installed}
        <Badge variant="tone" tone="success" size="sm"><Check size={9} /> installed</Badge>
      {/if}
    </div>
    {#if pack.installed}
      <span class="pack-meta">{pack.instrument_count} instruments · {formatBytes(pack.size_bytes)}</span>
    {:else if packsStore.downloadingOf(pack.id)}
      <div class="pack-dl">
        <div class="pack-dl-head">
          <span class="pack-phase">{prog?.phase === 'extracting' ? 'Extracting…' : 'Downloading…'}</span>
          {#if prog && prog.pct >= 0}<span class="pack-pct">{Math.round(prog.pct)}%</span>{/if}
        </div>
        <ProgressBar pct={prog && prog.pct >= 0 ? prog.pct : undefined}
                     indeterminate={!prog || prog.pct < 0}
                     ariaLabel={`${pack.name} download progress`} />
        <Button size="xs" variant="ghost" block onclick={() => packsStore.cancel(pack.id)}>Cancel</Button>
      </div>
    {:else}
      <Button size="sm" variant="secondary" block onclick={() => packsStore.download(pack.id)}>
        {#snippet iconStart()}<Download size={13} />{/snippet}
        Download
      </Button>
    {/if}
  </div>
{/snippet}

<PanelShell title="Sound bank" count={soundsStore.instruments.length}>
  {#snippet icon()}<Music4 size={13} />{/snippet}
  {#snippet actions()}
    <button class="ps-btn" use:tooltip={'Refresh sound list'} aria-label="Refresh sound list"
            onclick={() => soundsStore.refresh()}><RefreshCw size={13} /></button>
  {/snippet}

  {#if !soundsStore.loaded && slowLoad}
    <div class="loading"><Spinner block label="Loading sounds…" /></div>
  {:else}
    <div class="bank">
      <SidebarSection label="Synth presets" bind:expanded={openSynth} badge={synths.length}>
        {#snippet icon()}<Waves size={13} />{/snippet}
        {#if synths.length}
          {#each synths as inst (inst.name)}{@render voiceRow(inst)}{/each}
        {:else}
          <EmptyState compact message="No synth presets resolved." />
        {/if}
      </SidebarSection>

      <SidebarSection label="Samplers" bind:expanded={openSampler} badge={samplers.length}>
        {#snippet icon()}<Piano size={13} />{/snippet}
        {#if samplers.length}
          {#each samplers as inst (inst.name)}{@render voiceRow(inst)}{/each}
        {:else}
          <EmptyState compact message="No sampler voices yet — install a sample bank below." />
        {/if}
      </SidebarSection>

      <SidebarSection label="Sample banks" bind:expanded={openBanks} badge={packsStore.packs.length}>
        {#snippet icon()}<Boxes size={13} />{/snippet}
        <div class="banks">
          {#each packsStore.packs as pack (pack.id)}{@render packCard(pack)}{/each}
        </div>
      </SidebarSection>
    </div>
  {/if}
</PanelShell>

<style>
  .bank { padding: 4px 0; }
  .bank-name { font-family: var(--font-code); font-size: 11.5px; }

  /* Articulation chips under an SFZ voice (legato / staccato / …). */
  .arts { display: flex; flex-wrap: wrap; gap: 3px; padding: 0 10px 5px 30px; }
  .art-chip {
    font-family: var(--font-code); font-size: 9px; line-height: 1.5;
    padding: 0 5px; border-radius: var(--radius-sm);
    color: var(--text-muted); background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
  }

  .loading { padding: 24px 12px; }

  /* Sample-bank download cards. */
  .banks { display: flex; flex-direction: column; gap: 8px; padding: 6px 10px 8px; }
  .pack { display: flex; flex-direction: column; gap: 6px; }
  .pack-head { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
  .pack-name { font-size: var(--font-size-xs); font-weight: 600; color: var(--text-primary); }
  .pack-meta { font-size: var(--font-size-xs); color: var(--text-muted); font-family: var(--font-code); }

  .pack-dl { display: flex; flex-direction: column; gap: 5px; }
  .pack-dl-head { display: flex; align-items: baseline; justify-content: space-between; }
  .pack-phase { font-size: var(--font-size-xs); color: var(--text-secondary); }
  .pack-pct { font-size: var(--font-size-xs); color: var(--text-muted); font-family: var(--font-code); }
</style>
