<script lang="ts">
  /**
   * SourcePicker — renders the per-kind target chooser for the active source
   * (monitor tiles, filtered window list, or the region CTA/preview). The kind
   * switch itself lives in the CapturePanel section header; the region option
   * opens the region selector via the recorder store.
   */
  import { Monitor, AppWindow, Crop, Check, RefreshCw, X } from 'lucide-svelte';
  import { convertFileSrc } from '@tauri-apps/api/core';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import Monogram from '$lib/components/shared/ui/Monogram.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { recorderStore } from '$lib/stores/tyto/recorder.svelte';

  /** Stable per-app colour (hashed hue) so each window reads distinctly. */
  function appColor(app: string): string {
    let h = 0;
    for (let i = 0; i < app.length; i++) h = (h * 31 + app.charCodeAt(i)) % 360;
    return `hsl(${h} 52% 52%)`;
  }

  // With potentially dozens of open windows, filter by title / app. Sorted A→Z by
  // title — a flat, scannable grid (no app grouping / sort controls).
  let winQuery = $state('');
  const sortedWindows = $derived(
    recorderStore.windows
      .filter((w) => {
        const q = winQuery.trim().toLowerCase();
        return !q || `${w.title} ${w.app}`.toLowerCase().includes(q);
      })
      .slice()
      .sort((a, b) => a.title.localeCompare(b.title)),
  );

  // Live preview of the selected window: re-grabbed whenever the pick changes.
  // Debounced so type-ahead / quick clicks don't spam xcap.
  let previewUrl = $state<string | null>(null);
  let previewLoading = $state(false);
  $effect(() => {
    void recorderStore.selectedWindowId; // re-run when the selected window changes
    if (recorderStore.targetKind !== 'window' || !recorderStore.backendUp) {
      previewUrl = null;
      return;
    }
    let cancelled = false;
    previewLoading = true;
    const t = setTimeout(() => {
      void recorderStore.capturePreview().then((path) => {
        if (cancelled) return;
        previewUrl = path ? convertFileSrc(path) : null;
        previewLoading = false;
      });
    }, 160);
    return () => { cancelled = true; clearTimeout(t); previewLoading = false; };
  });
</script>

<div class="source">
  {#if recorderStore.targetKind === 'monitor'}
    <div class="grid">
      {#each recorderStore.monitors as mon (mon.id)}
        <button
          type="button"
          class="tile"
          class:selected={mon.id === recorderStore.selectedMonitorId}
          onclick={() => recorderStore.selectMonitor(mon.id)}
          use:tooltip={`${mon.name} · ${mon.resolution} · ${mon.scale}× scale`}
          aria-pressed={mon.id === recorderStore.selectedMonitorId}
        >
          <div class="tile-thumb"><Monitor size={22} /></div>
          <div class="tile-body">
            <div class="tile-title">
              {mon.name}
              {#if mon.primary}<span class="pill">Primary</span>{/if}
            </div>
            <div class="tile-sub">{mon.resolution} · {mon.scale}× scale</div>
          </div>
          {#if mon.id === recorderStore.selectedMonitorId}<Check size={15} class="tile-check" />{/if}
        </button>
      {/each}
    </div>
  {:else if recorderStore.targetKind === 'window'}
    {#if previewUrl || previewLoading}
      <div class="win-preview">
        {#if previewUrl}
          <img src={previewUrl} alt="Live preview of the selected window" />
        {:else}
          <div class="win-preview-loading"><Spinner size={16} /></div>
        {/if}
      </div>
    {/if}

    <SearchBar
      bind:query={winQuery}
      showRegex={false}
      showCounter={false}
      placeholder="Filter open windows…"
      ariaLabel="Filter windows"
    />

    {#if sortedWindows.length === 0}
      <StateBlock tone="neutral" fill={false}>
        {#snippet icon()}<AppWindow size={20} />{/snippet}
        {recorderStore.windows.length === 0 ? 'No capturable windows open' : `No windows match “${winQuery}”`}
      </StateBlock>
    {:else}
      <div class="win-grid">
        {#each sortedWindows as win (win.id)}
          <button
            type="button"
            class="win-tile"
            class:selected={win.id === recorderStore.selectedWindowId}
            onclick={() => recorderStore.selectWindow(win.id)}
            use:tooltip={`${win.title} — ${win.app}`}
            aria-pressed={win.id === recorderStore.selectedWindowId}
          >
            <Monogram name={win.app} color={appColor(win.app)} size={30} variant="square" />
            <div class="win-body">
              <div class="win-title">{win.title}</div>
              <div class="win-app">{win.app}</div>
            </div>
            {#if win.id === recorderStore.selectedWindowId}<span class="win-check"><Check size={12} /></span>{/if}
          </button>
        {/each}
      </div>
    {/if}
  {:else}
    {#if recorderStore.region}
      <div class="region-card">
        <div class="region-preview" style={`aspect-ratio: ${recorderStore.region.physical.w} / ${recorderStore.region.physical.h};`}>
          <span class="region-dims">
            {recorderStore.region.physical.w} × {recorderStore.region.physical.h}
          </span>
        </div>
        <div class="region-meta">
          <div class="region-title">Region selected</div>
          <div class="region-sub">
            {recorderStore.region.physical.w}×{recorderStore.region.physical.h} px
            · at ({recorderStore.region.physical.x}, {recorderStore.region.physical.y})
          </div>
          <div class="region-actions">
            <button type="button" class="link" onclick={() => recorderStore.openScreenRegion()}>
              <RefreshCw size={12} /> Reselect
            </button>
            <button type="button" class="link danger" onclick={() => recorderStore.setRegion(null)}>
              <X size={12} /> Clear
            </button>
          </div>
        </div>
      </div>
    {:else}
      <button type="button" class="region-cta" onclick={() => recorderStore.openScreenRegion()} use:tooltip={'Drag a rectangle to define a region'}>
        <Crop size={20} />
        <span class="region-cta-title">Select a region…</span>
        <span class="region-cta-sub">Dims one monitor and lets you drag a capture rectangle</span>
      </button>
    {/if}
  {/if}
</div>

<style>
  .source { display: flex; flex-direction: column; gap: 12px; }

  /* ── Monitor tiles ── */
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(220px, 1fr)); gap: 8px; }
  .tile {
    display: flex; align-items: center; gap: 11px;
    padding: 10px 12px; text-align: left;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    color: var(--text-secondary);
    cursor: pointer;
    transition: background var(--transition-fast), border-color var(--transition-fast),
                transform var(--transition-fast), box-shadow var(--transition-fast);
  }
  .tile:hover { background: var(--bg-hover); border-color: var(--border-subtle); transform: translateY(-1px); box-shadow: 0 4px 12px rgba(0,0,0,0.14); }
  .tile:active { transform: translateY(0); }
  .tile.selected { border-color: var(--accent); background: var(--accent-subtle); }
  .tile-thumb {
    display: flex; align-items: center; justify-content: center;
    width: 46px; height: 34px; flex-shrink: 0;
    border-radius: var(--radius-sm);
    color: #fff;
    background: linear-gradient(140deg,
      color-mix(in srgb, var(--accent) 90%, #fff),
      color-mix(in srgb, var(--accent) 72%, #000));
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.14), 0 2px 8px color-mix(in srgb, var(--accent) 32%, transparent);
  }
  .tile.selected .tile-thumb { box-shadow: inset 0 0 0 1px rgba(255,255,255,0.2), 0 3px 12px color-mix(in srgb, var(--accent) 45%, transparent); }
  .tile-body { min-width: 0; flex: 1; }
  .tile-title { display: flex; align-items: center; gap: 6px; font-size: 12.5px; font-weight: 550; color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tile-sub { font-size: 11px; color: var(--text-muted); margin-top: 2px; }
  :global(.tile-check) { color: var(--accent); flex-shrink: 0; }

  .pill {
    font-size: 9.5px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.4px;
    padding: 1px 5px; border-radius: 999px;
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    color: var(--accent);
  }

  /* ── Window picker (tile grid, same language as the monitor tiles) ── */
  .win-preview {
    display: flex; align-items: center; justify-content: center;
    max-height: 150px; overflow: hidden;
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 4px;
  }
  .win-preview img {
    max-width: 100%; max-height: 142px;
    object-fit: contain;
    border-radius: var(--radius-sm);
    display: block;
  }
  .win-preview-loading { height: 88px; display: flex; align-items: center; justify-content: center; }

  .win-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(230px, 1fr));
    gap: 8px;
    max-height: 300px; overflow: auto;
    padding: 1px;
  }
  .win-tile {
    display: flex; align-items: center; gap: 10px;
    padding: 9px 11px; text-align: left;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    color: var(--text-secondary);
    cursor: pointer;
    transition: background var(--transition-fast), border-color var(--transition-fast),
                transform var(--transition-fast), box-shadow var(--transition-fast);
  }
  .win-tile:hover { background: var(--bg-hover); border-color: var(--border-subtle); transform: translateY(-1px); box-shadow: 0 4px 12px rgba(0,0,0,0.14); }
  .win-tile:active { transform: translateY(0); }
  .win-tile.selected { border-color: var(--accent); background: var(--accent-subtle); }
  .win-body { flex: 1; min-width: 0; }
  .win-title { font-size: 12.5px; font-weight: 550; color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .win-app { font-size: 10.5px; color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; margin-top: 2px; }
  .win-check {
    display: inline-flex; align-items: center; justify-content: center;
    width: 18px; height: 18px; flex-shrink: 0;
    border-radius: 50%; background: var(--accent); color: var(--text-on-accent, #fff);
  }

  /* ── Region ── */
  .region-cta {
    display: flex; flex-direction: column; align-items: center; gap: 4px;
    padding: 20px; text-align: center;
    background: var(--bg-input);
    border: 1px dashed var(--border);
    border-radius: var(--radius-md);
    color: var(--text-secondary);
    cursor: pointer;
    transition: background var(--transition-fast), border-color var(--transition-fast),
                color var(--transition-fast), transform var(--transition-fast);
  }
  .region-cta:hover { background: var(--bg-hover); border-color: var(--accent); color: var(--text-primary); transform: translateY(-1px); }
  .region-cta:active { transform: translateY(0); }
  .region-cta > :global(svg:first-child) { color: var(--accent); }
  .region-cta-title { font-size: 13px; font-weight: 550; }
  .region-cta-sub { font-size: 11px; color: var(--text-muted); }

  .region-card { display: flex; gap: 12px; padding: 12px; background: var(--bg-input); border: 1px solid var(--accent); border-radius: var(--radius-md); }
  .region-preview {
    flex-shrink: 0; width: 120px; max-height: 90px;
    display: flex; align-items: center; justify-content: center;
    background: repeating-conic-gradient(var(--bg-base) 0% 25%, var(--bg-overlay) 0% 50%) 0 0 / 14px 14px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
  }
  .region-dims { font-size: 11px; font-weight: 600; color: var(--text-primary); background: var(--bg-elevated); padding: 2px 6px; border-radius: var(--radius-sm); font-variant-numeric: tabular-nums; }
  .region-meta { display: flex; flex-direction: column; gap: 3px; min-width: 0; justify-content: center; }
  .region-title { font-size: 13px; font-weight: 600; color: var(--text-primary); }
  .region-sub { font-size: 11px; color: var(--text-muted); font-variant-numeric: tabular-nums; }
  .region-actions { display: flex; gap: 12px; margin-top: 6px; }
  .link {
    display: inline-flex; align-items: center; gap: 4px;
    background: none; border: none; padding: 0; cursor: pointer;
    font-size: 11.5px; color: var(--accent);
  }
  .link:hover { text-decoration: underline; }
  .link.danger { color: var(--error); }
</style>
