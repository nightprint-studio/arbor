<script lang="ts">
  /**
   * Sound bank — the engine's resolvable voices plus the downloadable sample
   * packs. Driven by the **real registry** (`soundsStore` ← `merula_sounds`), not
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
   * Imports only shared/ui (+ the tooltip action) + merula-local.
   */
  import { Music4, Waves, Piano, Download, Check, RefreshCw, Boxes, HardDrive, Trash2, Star, Clock, Link2, Plus, ArrowRight } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import SidebarSection from '$lib/components/shared/ui/SidebarSection.svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import ProgressBar from '$lib/components/shared/ui/ProgressBar.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import SoundBankItem from './SoundBankItem.svelte';
  import { soundsStore } from '../stores/sounds.svelte';
  import { packsStore } from '../stores/packs.svelte';
  import { workspaceStore } from '../stores/workspace.svelte';
  import { aliasesStore } from '../stores/aliases.svelte';
  import type { MerulaInstrument, MerulaPack } from '$lib/ipc/merula/merula';

  let query = $state('');
  const q = $derived(query.trim().toLowerCase());
  function match(list: MerulaInstrument[]): MerulaInstrument[] {
    return q ? list.filter((i) => i.name.toLowerCase().includes(q)) : list;
  }
  const synths   = $derived(match(soundsStore.synths));
  const samplers = $derived(match(soundsStore.samplers));

  // Favourites + recently-used: resolve the persisted names against the live
  // registry (so a removed pack's voice simply drops out), filtered by the search.
  const byName = $derived(new Map(soundsStore.instruments.map((i) => [i.name, i])));
  const favorites = $derived(match(
    workspaceStore.favoriteSounds.map((n) => byName.get(n)).filter((i): i is MerulaInstrument => !!i),
  ));
  const recents = $derived(match(
    workspaceStore.recentSounds.map((n) => byName.get(n)).filter((i): i is MerulaInstrument => !!i),
  ));
  let openFav    = $state(true);
  let openRecent = $state(true);

  // ── Aliases: global `name → target` renames (e.g. kick → RolandTR808_bd) ──────
  // Driven by the alias store; loaded by MerulaShell. The engine resolves them on
  // the next eval/run, so editing here + Run is enough.
  let openAliases   = $state(false);
  let newAliasName   = $state('');
  let newAliasTarget = $state('');
  const aliasRows = $derived(
    q ? aliasesStore.entries.filter((a) => a.name.toLowerCase().includes(q) || a.target.toLowerCase().includes(q))
      : aliasesStore.entries,
  );
  function addAlias() {
    const name = newAliasName.trim();
    const target = newAliasTarget.trim();
    if (!name || !target) return;
    aliasesStore.set(name, target);
    newAliasName = '';
    newAliasTarget = '';
  }
  function onNewAliasKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') { e.preventDefault(); addAlias(); }
  }

  // Samplers grouped by their origin pack (Dirt-Samples, drum machines, …) so the
  // bank isn't one flat list of hundreds of voices. Ordered to match the pack
  // display order; an "Other" bucket catches any voice without a known pack.
  interface SamplerGroup { id: string; name: string; items: MerulaInstrument[]; }
  const samplerGroups = $derived.by<SamplerGroup[]>(() => {
    const groups = new Map<string, SamplerGroup>();
    for (const inst of samplers) {
      const id = inst.pack ?? 'other';
      let g = groups.get(id);
      if (!g) { g = { id, name: inst.pack_name ?? 'Other', items: [] }; groups.set(id, g); }
      g.items.push(inst);
    }
    const order = packsStore.packs.map((p) => p.id);
    const rank = (id: string) => { const i = order.indexOf(id); return i < 0 ? order.length : i; };
    return [...groups.values()].sort((a, b) => rank(a.id) - rank(b.id));
  });

  let openSynth   = $state(true);
  let openBanks   = $state(true);
  const showSynth = $derived(openSynth || (!!q && synths.length > 0));
  // Per-pack expand state. Sampler banks can run to the hundreds (Dirt-Samples),
  // so start collapsed — but a live filter implies the user is hunting a voice,
  // so auto-open any group that still has matches while filtering.
  let openPacks = $state<Record<string, boolean>>({});
  function togglePack(id: string) { openPacks = { ...openPacks, [id]: !(openPacks[id] ?? false) }; }
  function packShown(id: string): boolean { return (openPacks[id] ?? false) || !!q; }

  // Re-index: rebuild an installed pack's registry from the files on disk (fixes
  // a pack that indexed to zero instruments, e.g. an older VSCO install).
  let reindexError = $state<Record<string, string>>({});
  async function doReindex(pack: MerulaPack) {
    reindexError = { ...reindexError, [pack.id]: '' };
    try {
      await packsStore.reindex(pack.id);
    } catch (e) {
      reindexError = { ...reindexError, [pack.id]: e instanceof Error ? e.message : String(e) };
    }
  }

  // Per-profile active toggle: enable/disable an installed pack's voices for the
  // active profile. Drops/re-adds them live (the store refreshes packs + sounds).
  let activeError = $state<Record<string, string>>({});
  async function setActive(pack: MerulaPack, on: boolean) {
    activeError = { ...activeError, [pack.id]: '' };
    try {
      await packsStore.setActive(pack.id, on);
    } catch (e) {
      activeError = { ...activeError, [pack.id]: e instanceof Error ? e.message : String(e) };
    }
  }

  // Delete-pack confirmation (an installed pack's files are removed from disk).
  let confirmDelete = $state<MerulaPack | null>(null);
  let deleting      = $state(false);
  let deleteError   = $state<string | null>(null);
  function askDelete(pack: MerulaPack) { deleteError = null; confirmDelete = pack; }
  async function doDelete() {
    if (!confirmDelete) return;
    deleting = true;
    deleteError = null;
    try {
      await packsStore.remove(confirmDelete.id);
      confirmDelete = null;
    } catch (e) {
      deleteError = e instanceof Error ? e.message : String(e);
    } finally {
      deleting = false;
    }
  }

  // The pack subscription is owned by the MerulaShell; here we just (re)read the
  // registry on mount and again whenever the pack set changes (an install adds
  // sampler voices to the registry only after extraction).
  $effect(() => {
    void packsStore.packs; // dep: re-read after an install completes
    void soundsStore.refresh();
  });

  // Delayed spinner: only surface it if the first load actually takes a while
  // (merula_sounds is a fast registry read — usually no spinner at all).
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

{#snippet packCard(pack: MerulaPack)}
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
      <div class="pack-foot">
        <span class="pack-meta">
          <Piano size={11} /> {pack.instrument_count} instruments
          <span class="pack-dot">·</span>
          <HardDrive size={11} /> {formatBytes(pack.size_bytes)} on disk
        </span>
        <div class="pack-actions">
          <Toggle size="sm" checked={packsStore.activeOf(pack.id)}
                  ariaLabel={`${packsStore.activeOf(pack.id) ? 'Disable' : 'Enable'} ${pack.name} for this profile`}
                  onchange={(v) => setActive(pack, v)} />
          <span class="pack-active" class:on={packsStore.activeOf(pack.id)}>
            {packsStore.activeOf(pack.id) ? 'Active' : 'Inactive'}
          </span>
          <button class="pack-del" use:tooltip={'Rebuild this pack’s instruments from the files on disk'}
                  aria-label={`Re-index ${pack.name}`} disabled={packsStore.reindexingOf(pack.id)}
                  onclick={() => doReindex(pack)}>
            {#if packsStore.reindexingOf(pack.id)}<Spinner size={13} />{:else}<RefreshCw size={13} />{/if}
          </button>
          <button class="pack-del" use:tooltip={'Delete pack'}
                  aria-label={`Delete ${pack.name}`} onclick={() => askDelete(pack)}>
            <Trash2 size={13} />
          </button>
        </div>
      </div>
      <p class="pack-hint">Enabling/disabling a pack for this profile applies on the next run.</p>
      {#if activeError[pack.id]}
        <p class="pack-err">{activeError[pack.id]}</p>
      {/if}
      {#if pack.instrument_count === 0}
        <p class="pack-hint">No instruments indexed — try <strong>Re-index</strong> to rebuild from the downloaded files.</p>
      {/if}
      {#if reindexError[pack.id]}
        <p class="pack-err">{reindexError[pack.id]}</p>
      {/if}
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

      <SidebarSection label="Aliases" expanded={openAliases || (!!q && aliasRows.length > 0)} onToggle={() => openAliases = !openAliases} badge={aliasesStore.count}>
        {#snippet icon()}<Link2 size={13} />{/snippet}
        <div class="aliases">
          <p class="alias-hint">Your own names for any voice, usable in <code>s(…)</code> / <code>inst(…)</code>. Global; applies on the next run.</p>
          {#each aliasRows as a (a.name)}
            <div class="alias-row">
              <input class="alias-in name" value={a.name} aria-label="Alias name"
                     onchange={(e) => aliasesStore.rename(a.name, e.currentTarget.value)} />
              <ArrowRight size={12} class="alias-arrow" />
              <input class="alias-in target" value={a.target} aria-label="Alias target" list="alias-targets"
                     onchange={(e) => aliasesStore.set(a.name, e.currentTarget.value)} />
              <button class="alias-del" use:tooltip={'Remove alias'} aria-label="Remove alias" onclick={() => aliasesStore.remove(a.name)}><Trash2 size={12} /></button>
            </div>
          {/each}
          <div class="alias-row add">
            <input class="alias-in name" bind:value={newAliasName} placeholder="alias" aria-label="New alias name" onkeydown={onNewAliasKeydown} />
            <ArrowRight size={12} class="alias-arrow" />
            <input class="alias-in target" bind:value={newAliasTarget} placeholder="target voice" aria-label="New alias target" list="alias-targets" onkeydown={onNewAliasKeydown} />
            <button class="alias-add" use:tooltip={'Add alias'} aria-label="Add alias" disabled={!newAliasName.trim() || !newAliasTarget.trim()} onclick={addAlias}><Plus size={13} /></button>
          </div>
        </div>
        <datalist id="alias-targets">
          {#each soundsStore.instruments as inst (inst.name)}<option value={inst.name}></option>{/each}
        </datalist>
      </SidebarSection>

      {#if favorites.length}
        <SidebarSection label="Favourites" expanded={openFav} onToggle={() => openFav = !openFav} badge={favorites.length}>
          {#snippet icon()}<Star size={13} />{/snippet}
          {#each favorites as inst (inst.name)}<SoundBankItem {inst} />{/each}
        </SidebarSection>
      {/if}

      {#if recents.length}
        <SidebarSection label="Recently used" expanded={openRecent} onToggle={() => openRecent = !openRecent} badge={recents.length}>
          {#snippet icon()}<Clock size={13} />{/snippet}
          {#each recents as inst (inst.name)}<SoundBankItem {inst} />{/each}
        </SidebarSection>
      {/if}

      <SidebarSection label="Synth presets" expanded={showSynth} onToggle={() => openSynth = !openSynth} badge={synths.length}>
        {#snippet icon()}<Waves size={13} />{/snippet}
        {#if synths.length}
          {#each synths as inst (inst.name)}<SoundBankItem {inst} />{/each}
        {:else}
          <EmptyState compact message={q ? 'No synth presets match.' : 'No synth presets resolved.'} />
        {/if}
      </SidebarSection>

      {#if samplerGroups.length}
        {#each samplerGroups as group (group.id)}
          <SidebarSection label={group.name} expanded={packShown(group.id)}
                          onToggle={() => togglePack(group.id)} badge={group.items.length}>
            {#snippet icon()}<Piano size={13} />{/snippet}
            {#each group.items as inst (inst.name)}<SoundBankItem {inst} />{/each}
          </SidebarSection>
        {/each}
      {:else}
        <SidebarSection label="Samplers" expanded={true} onToggle={() => {}} badge={0}>
          {#snippet icon()}<Piano size={13} />{/snippet}
          <EmptyState compact message={q ? 'No sampler voices match.' : 'No sampler voices yet — install a sample bank below.'} />
        </SidebarSection>
      {/if}

      <SidebarSection label="Sample banks" bind:expanded={openBanks} badge={packsStore.packs.length}>
        {#snippet icon()}<Boxes size={13} />{/snippet}
        <div class="banks">
          {#each packsStore.packs as pack (pack.id)}{@render packCard(pack)}{/each}
        </div>
      </SidebarSection>
    </div>
  {/if}
</PanelShell>

{#if confirmDelete}
  <ConfirmModal
    variant="danger"
    title="Delete sample pack"
    message={`Delete “${confirmDelete.name}” and all its samples from disk?`}
    detail={deleteError ?? 'You can re-download it any time from the sound bank.'}
    confirmLabel="Delete"
    busy={deleting}
    onConfirm={doDelete}
    onCancel={() => { if (!deleting) confirmDelete = null; }}
  />
{/if}

<style>
  .bank { padding: 4px 0; }
  .bank-filter { padding: 2px 10px 6px; }

  /* Aliases editor */
  .aliases { display: flex; flex-direction: column; gap: 5px; padding: 4px 10px 8px; }
  .alias-hint { margin: 0 0 2px; font-size: var(--font-size-xs); line-height: 1.5; color: var(--text-muted); }
  .alias-hint code { font-family: var(--font-code); font-size: var(--font-size-2xs); }
  .alias-row { display: flex; align-items: center; gap: 5px; }
  .alias-row :global(.alias-arrow) { color: var(--text-disabled); flex-shrink: 0; }
  .alias-in {
    flex: 1; min-width: 0;
    height: 24px; padding: 0 7px;
    background: var(--bg-input); border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm); color: var(--text-primary);
    font-family: var(--font-code); font-size: var(--font-size-xs);
  }
  .alias-in::placeholder { color: var(--text-disabled); }
  .alias-in:focus { outline: none; border-color: var(--border-focus); }
  .alias-del, .alias-add {
    display: inline-flex; align-items: center; justify-content: center;
    width: 24px; height: 24px; flex-shrink: 0;
    background: transparent; border: none; border-radius: var(--radius-sm);
    color: var(--text-muted); cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .alias-del:hover { background: var(--error-subtle); color: var(--error); }
  .alias-add:hover:not(:disabled) { background: var(--accent-subtle); color: var(--accent); }
  .alias-add:disabled { opacity: 0.45; cursor: default; }

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
  .pack-desc { margin: 0; font-size: var(--font-size-xs); line-height: 1.5; color: var(--text-secondary); }
  .pack-meta {
    display: inline-flex; align-items: center; gap: 4px;
    font-size: var(--font-size-xs); color: var(--text-muted); font-family: var(--font-code);
  }
  .pack-meta :global(svg) { color: var(--text-disabled); }
  .pack-dot { margin: 0 2px; }
  .pack-foot { display: flex; align-items: center; justify-content: space-between; gap: 8px; }

  .pack-del {
    display: inline-flex; align-items: center; justify-content: center;
    flex-shrink: 0; padding: 3px; border-radius: var(--radius-sm);
    background: transparent; border: none; cursor: pointer;
    color: var(--text-disabled);
    transition: color var(--transition-fast), background var(--transition-fast);
  }
  .pack-del:hover:not(:disabled) { color: var(--error); background: var(--error-subtle); }
  .pack-del:focus-visible { outline: none; box-shadow: inset 0 0 0 1px var(--error); }
  .pack-del:disabled { opacity: 0.5; cursor: default; }

  .pack-actions { display: inline-flex; align-items: center; gap: 6px; flex-shrink: 0; }
  /* Active/Inactive affordance next to the per-profile toggle. */
  .pack-active {
    font-size: var(--font-size-xs); color: var(--text-disabled);
    font-variant-numeric: tabular-nums; min-width: 44px;
    transition: color var(--transition-fast);
  }
  .pack-active.on { color: var(--accent); }
  /* The re-index button is the first `.pack-del` in `.pack-actions`; tint it
     accent on hover (it rebuilds, it doesn't destroy) rather than the delete red. */
  .pack-actions .pack-del:nth-of-type(1):hover:not(:disabled) { color: var(--accent); background: var(--accent-subtle); }
  .pack-actions .pack-del:nth-of-type(1):focus-visible { box-shadow: inset 0 0 0 1px var(--accent); }

  .pack-hint { margin: 2px 0 0; font-size: var(--font-size-xs); line-height: 1.5; color: var(--text-muted); }
  .pack-hint strong { color: var(--text-secondary); font-weight: 600; }
  .pack-err { margin: 2px 0 0; font-size: var(--font-size-xs); line-height: 1.5; color: var(--error); }

  .pack-dl { display: flex; flex-direction: column; gap: 5px; }
  .pack-dl-head { display: flex; align-items: baseline; justify-content: space-between; }
  .pack-phase { font-size: var(--font-size-xs); color: var(--text-secondary); }
  .pack-pct { font-size: var(--font-size-xs); color: var(--text-muted); font-family: var(--font-code); }
</style>
