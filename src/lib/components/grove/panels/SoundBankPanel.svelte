<script lang="ts">
  /**
   * Sound bank — the engine's resolvable voices plus the downloadable sample
   * packs. Driven by the **real registry** (`soundsStore` ← `grove_sounds`), not
   * a static list, so it tracks what's actually installed.
   *
   * Three sections: the built-in synth presets (always present), the resolved
   * sampler voices (filled once any pack is installed), and the **Sample banks**
   * — one card per pack (VSCO 2, Dirt-Samples, drum machines, …) with a
   * description, a download-size estimate, and a job-tracked install + live
   * progress bar (+ Cancel). Downloads are async — the UI never blocks.
   *
   * Each voice row (`SoundBankItem`) copies its name on click and reveals an info
   * panel with the catalogue description + articulations. A filter narrows the
   * (potentially hundreds of) sampler voices by name.
   *
   * Imports only shared/ui (+ the tooltip action) + grove-local.
   */
  import { Music4, Waves, Piano, Download, Check, RefreshCw, Boxes, HardDrive } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import SidebarSection from '$lib/components/shared/ui/SidebarSection.svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import ProgressBar from '$lib/components/shared/ui/ProgressBar.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import SoundBankItem from './SoundBankItem.svelte';
  import { soundsStore } from '../stores/sounds.svelte';
  import { packsStore } from '../stores/packs.svelte';
  import type { GroveInstrument, GrovePack } from '$lib/ipc/grove';

  let query = $state('');
  const q = $derived(query.trim().toLowerCase());
  function match(list: GroveInstrument[]): GroveInstrument[] {
    return q ? list.filter((i) => i.name.toLowerCase().includes(q)) : list;
  }
  const synths   = $derived(match(soundsStore.synths));
  const samplers = $derived(match(soundsStore.samplers));

  let openSynth   = $state(true);
  // Samplers can run to the hundreds (Dirt-Samples), so start collapsed — but a
  // live filter implies the user is hunting a sampler, so auto-open while filtering.
  let openSampler = $state(false);
  let openBanks   = $state(true);
  const showSynth   = $derived(openSynth || (!!q && synths.length > 0));
  const showSampler = $derived(openSampler || (!!q && samplers.length > 0));

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

{#snippet packCard(pack: GrovePack)}
  {@const prog = packsStore.progressOf(pack.id)}
  <div class="pack" class:installed={pack.installed}>
    <div class="pack-head">
      <span class="pack-name">{pack.name}</span>
      {#if pack.installed}
        <Badge variant="tone" tone="success" size="sm"><Check size={9} /> installed</Badge>
      {/if}
    </div>
    {#if pack.description}
      <p class="pack-desc">{pack.description}</p>
    {/if}
    {#if pack.installed}
      <span class="pack-meta">
        <Piano size={11} /> {pack.instrument_count} instruments
        <span class="pack-dot">·</span>
        <HardDrive size={11} /> {formatBytes(pack.size_bytes)} on disk
      </span>
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
      <div class="pack-foot">
        <span class="pack-meta" use:tooltip={'Approximate download size'}>
          <Download size={11} /> ~{formatBytes(pack.approx_bytes)}
        </span>
        <Button size="sm" variant="secondary" onclick={() => packsStore.download(pack.id)}>
          {#snippet iconStart()}<Download size={13} />{/snippet}
          Download
        </Button>
      </div>
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
      <div class="bank-filter">
        <SearchBar bind:query showRegex={false} showCounter={false}
                   placeholder="Filter voices…" ariaLabel="Filter instruments" />
      </div>

      <SidebarSection label="Synth presets" expanded={showSynth} onToggle={() => openSynth = !openSynth} badge={synths.length}>
        {#snippet icon()}<Waves size={13} />{/snippet}
        {#if synths.length}
          {#each synths as inst (inst.name)}<SoundBankItem {inst} />{/each}
        {:else}
          <EmptyState compact message={q ? 'No synth presets match.' : 'No synth presets resolved.'} />
        {/if}
      </SidebarSection>

      <SidebarSection label="Samplers" expanded={showSampler} onToggle={() => openSampler = !openSampler} badge={samplers.length}>
        {#snippet icon()}<Piano size={13} />{/snippet}
        {#if samplers.length}
          {#each samplers as inst (inst.name)}<SoundBankItem {inst} />{/each}
        {:else}
          <EmptyState compact message={q ? 'No sampler voices match.' : 'No sampler voices yet — install a sample bank below.'} />
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
  .bank-filter { padding: 2px 10px 6px; }

  .loading { padding: 24px 12px; }

  /* Sample-bank download cards. */
  .banks { display: flex; flex-direction: column; gap: 8px; padding: 6px 10px 8px; }
  .pack {
    display: flex; flex-direction: column; gap: 6px;
    padding: 9px 10px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
  }
  .pack.installed { border-color: color-mix(in srgb, var(--success) 35%, var(--border-subtle)); }
  .pack-head { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
  .pack-name { font-size: var(--font-size-sm); font-weight: 600; color: var(--text-primary); }
  .pack-desc { margin: 0; font-size: 11px; line-height: 1.5; color: var(--text-secondary); }
  .pack-meta {
    display: inline-flex; align-items: center; gap: 4px;
    font-size: var(--font-size-xs); color: var(--text-muted); font-family: var(--font-code);
  }
  .pack-meta :global(svg) { color: var(--text-disabled); }
  .pack-dot { margin: 0 2px; }
  .pack-foot { display: flex; align-items: center; justify-content: space-between; gap: 8px; }

  .pack-dl { display: flex; flex-direction: column; gap: 5px; }
  .pack-dl-head { display: flex; align-items: baseline; justify-content: space-between; }
  .pack-phase { font-size: var(--font-size-xs); color: var(--text-secondary); }
  .pack-pct { font-size: var(--font-size-xs); color: var(--text-muted); font-family: var(--font-code); }
</style>
