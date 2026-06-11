<script lang="ts">
  /**
   * Sound bank — the engine's resolvable voices, grouped: the built-in synth
   * presets (always present) and the VSCO 2 samplers (present once the bank is
   * installed). Driven by the **real registry** (`soundsStore` ← `grove_sounds`),
   * not a static list, so it tracks what's actually installed.
   *
   * The VSCO block manages the sample bank: install status (count + size) when
   * present, or a Download button that kicks off the job-tracked install with a
   * live progress bar (+ Cancel) while it runs. The download is async (a job) —
   * the UI never blocks.
   *
   * Imports only shared/ui (+ the tooltip action) + grove-local stores.
   */
  import { Music4, Waves, Piano, Download, Check, RefreshCw } from 'lucide-svelte';
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
  import { vscoStore } from '../stores/vsco.svelte';
  import type { GroveInstrument } from '$lib/ipc/grove';

  const synths   = $derived(soundsStore.synths);
  const samplers = $derived(soundsStore.samplers);
  let openSynth   = $state(true);
  let openSampler = $state(true);

  // The VSCO subscription is owned by the GroveShell; here we just (re)read the
  // registry on mount and again whenever an install completes (the registry
  // gains the sampler voices only after extraction).
  $effect(() => {
    void vscoStore.installed; // dep: flips true when an install finishes
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

  const progress  = $derived(vscoStore.progress);
  const phaseLabel = $derived(progress?.phase === 'extracting' ? 'Extracting…' : 'Downloading…');

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

      <SidebarSection label="VSCO 2 samplers" bind:expanded={openSampler} badge={samplers.length}>
        {#snippet icon()}<Piano size={13} />{/snippet}

        <!-- Bank management: status / download / progress. -->
        <div class="vsco">
          {#if vscoStore.installed}
            <div class="vsco-status">
              <Badge variant="tone" tone="success" size="sm"><Check size={9} /> installed</Badge>
              <span class="vsco-meta">{vscoStore.instrumentCount} instruments · {formatBytes(vscoStore.sizeBytes)}</span>
            </div>
          {:else if vscoStore.downloading}
            <div class="vsco-dl">
              <div class="vsco-dl-head">
                <span class="vsco-phase">{phaseLabel}</span>
                {#if progress && progress.pct >= 0}<span class="vsco-pct">{Math.round(progress.pct)}%</span>{/if}
              </div>
              <ProgressBar pct={progress && progress.pct >= 0 ? progress.pct : undefined}
                           indeterminate={!progress || progress.pct < 0}
                           ariaLabel="VSCO 2 download progress" />
              <Button size="xs" variant="ghost" block onclick={() => vscoStore.cancel()}>Cancel</Button>
            </div>
          {:else}
            <div class="vsco-empty">
              <p class="vsco-hint">The VSCO 2 orchestral bank isn't installed. Download it to unlock the sampler voices.</p>
              <Button size="sm" variant="secondary" block onclick={() => vscoStore.download()}>
                {#snippet iconStart()}<Download size={13} />{/snippet}
                Download VSCO 2
              </Button>
            </div>
          {/if}
        </div>

        <!-- Resolved sampler voices (present only once installed). -->
        {#if samplers.length}
          {#each samplers as inst (inst.name)}{@render voiceRow(inst)}{/each}
        {:else if vscoStore.installed}
          <EmptyState compact message="No sampler voices in the registry." />
        {/if}
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

  .vsco { padding: 4px 10px 8px; display: flex; flex-direction: column; gap: 6px; }

  .vsco-status { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
  .vsco-meta { font-size: var(--font-size-xs); color: var(--text-muted); font-family: var(--font-code); }

  .vsco-empty { display: flex; flex-direction: column; gap: 7px; }
  .vsco-hint { margin: 0; font-size: var(--font-size-xs); color: var(--text-muted); line-height: 1.4; }

  .vsco-dl { display: flex; flex-direction: column; gap: 5px; }
  .vsco-dl-head { display: flex; align-items: baseline; justify-content: space-between; }
  .vsco-phase { font-size: var(--font-size-xs); color: var(--text-secondary); }
  .vsco-pct { font-size: var(--font-size-xs); color: var(--text-muted); font-family: var(--font-code); }
</style>
