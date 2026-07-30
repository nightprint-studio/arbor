<script lang="ts">
  /**
   * Clip launcher — an Ableton-style session grid over the source's `scene(...)`
   * declarations. Rows are scenes, columns are the base tracks (mixer order). A
   * populated cell fires that scene's clip on that track; a scene-launch button
   * fires the whole row. Firing is quantized to the next cycle boundary by the
   * backend (`merula_launch`), and `launcherStore` owns the live selection.
   *
   * Read/launch only — clips are authored in code (`scene(...)`); this panel is
   * the live performance surface, not an editor.
   */
  import { LayoutGrid, Play, Square, AlertTriangle } from 'lucide-svelte';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { merulaStore } from '../merula-store.svelte';
  import { launcherStore } from '../stores/launcher.svelte';
  import { laneColor } from '../palette';

  const tracks = $derived(launcherStore.tracks);
  const scenes = $derived(launcherStore.scenes);

  // How many tracks are currently playing a clip (for the header meta).
  const liveCount = $derived(tracks.reduce((n, _t, i) => n + (launcherStore.activeOf(i) ? 1 : 0), 0));

  /** The set of base-track indices a scene declares a (resolvable) clip for. */
  function clipTracks(name: string): Set<number> {
    const s = scenes.find((x) => x.name === name);
    const out = new Set<number>();
    if (s) for (const c of s.clips) if (c.track_index != null) out.add(c.track_index);
    return out;
  }
  /** How many of a scene's clips target a track that doesn't exist (inert). */
  function inertCount(name: string): number {
    const s = scenes.find((x) => x.name === name);
    return s ? s.clips.filter((c) => c.track_index == null).length : 0;
  }

  /** Toggle a cell: stop the track if it's already playing this clip, else fire it. */
  function toggleCell(track: number, scene: string): void {
    if (launcherStore.isActive(track, scene)) launcherStore.stopTrack(track);
    else launcherStore.launchClip(track, scene);
  }
</script>

<div class="lp">
  <BottomPanelHeader title="Clip launcher" onClose={() => merulaStore.toggleBottom('launcher')}>
    {#snippet icon()}<LayoutGrid size={13} />{/snippet}
    {#snippet children()}
      <span class="lp-meta">{scenes.length} {scenes.length === 1 ? 'scene' : 'scenes'}{#if liveCount > 0} · <span class="lp-playing">{liveCount} playing</span>{/if}</span>
    {/snippet}
    {#snippet actions()}
      <div class="lp-quant" title="Launch quantization — clips fire on this cycle grid">
        <span class="lp-quant-label">Quantize</span>
        {#each [1, 2, 4] as q (q)}
          <button
            class="ps-btn"
            class:ps-btn-active={launcherStore.quantum === q}
            onclick={() => launcherStore.setQuantum(q)}
          >{q}</button>
        {/each}
      </div>
    {/snippet}
  </BottomPanelHeader>

  <div class="lp-body">
    {#if !launcherStore.hasScenes}
      <EmptyState message={'Declare scenes in the source to launch clip variations live — scene("chorus", track("drums", s(bd bd sn bd))).'} />
    {:else}
      <div class="grid-scroll">
        <div class="grid" style="--cols: {tracks.length};">
          <!-- Header row: stop-all corner + track names -->
          <button
            class="corner"
            title="Stop all clips"
            disabled={!launcherStore.anyActive}
            onclick={() => launcherStore.stopAll()}
          >
            <Square size={11} /> Stop
          </button>
          {#each tracks as name, ti (ti)}
            {@const act = launcherStore.activeOf(ti)}
            {@const queued = launcherStore.isQueued(ti)}
            <div
              class="col-head"
              class:playing={act != null}
              style:--c={laneColor(ti)}
              title={act != null ? `${name || `#${ti + 1}`} — playing ${act}${queued ? ' (queued)' : ''}` : (name || `track ${ti + 1}`)}
            >
              <span class="dot" style:background={laneColor(ti)}></span>
              <span class="col-name">{name || `#${ti + 1}`}</span>
              {#if act != null}
                <span class="col-scene" class:armed={queued}>{act}</span>
              {/if}
            </div>
          {/each}

          <!-- One row per scene -->
          {#each scenes as s (s.name)}
            {@const present = clipTracks(s.name)}
            {@const inert = inertCount(s.name)}
            <button
              class="scene-launch"
              class:active={launcherStore.isSceneActive(s.name)}
              title={`Launch scene "${s.name}"`}
              onclick={() => launcherStore.launchScene(s.name)}
            >
              <Play size={11} class="tri" />
              <span class="scene-name">{s.name}</span>
              {#if inert > 0}
                <span class="warn" title={`${inert} clip${inert === 1 ? '' : 's'} target a track that doesn't exist`}>
                  <AlertTriangle size={11} />
                </span>
              {/if}
            </button>
            {#each tracks as _name, ti (ti)}
              {@const on = launcherStore.isActive(ti, s.name)}
              {@const queued = on && launcherStore.isQueued(ti)}
              {#if present.has(ti)}
                <button
                  class="cell filled"
                  class:active={on && !queued}
                  class:armed={queued}
                  style:--c={laneColor(ti)}
                  title={on ? 'Stop' : `Launch ${s.name} on ${tracks[ti] || `#${ti + 1}`}`}
                  onclick={() => toggleCell(ti, s.name)}
                >
                  {#if on && !queued}<Square size={10} />{:else}<Play size={10} class="tri" />{/if}
                </button>
              {:else}
                <div class="cell empty"></div>
              {/if}
            {/each}
          {/each}
        </div>
      </div>
    {/if}
  </div>
</div>

<style>
  .lp { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .lp-meta { color: var(--text-muted); font-variant-numeric: tabular-nums; }
  .lp-playing { color: var(--accent); font-weight: 600; }

  /* Launch-quantization selector in the header. */
  .lp-quant { display: inline-flex; align-items: center; gap: 3px; }
  .lp-quant-label {
    font-size: var(--font-size-2xs);
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.3px;
    margin-right: 2px;
  }
  .lp-quant .ps-btn { min-width: 22px; justify-content: center; font-variant-numeric: tabular-nums; }

  /* Armed: queued to fire on the next grid line — pulse until it lands. */
  @keyframes lp-pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.45; } }

  .lp-body { flex: 1; min-height: 0; display: flex; }
  .grid-scroll { flex: 1; min-width: 0; overflow: auto; padding: 8px; }

  /* First column sized to the scene labels; tracks share the rest. */
  .grid {
    display: grid;
    grid-template-columns: minmax(118px, max-content) repeat(var(--cols), minmax(64px, 1fr));
    gap: 4px;
    align-content: start;
    min-width: max-content;
  }

  /* ── Header row ─────────────────────────────────────────────────────────── */
  .corner,
  .scene-launch {
    position: sticky;
    left: 0;
    z-index: 1;
  }

  .corner {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 28px;
    padding: 0 10px;
    font-size: var(--font-size-xs);
    font-weight: 600;
    color: var(--text-muted);
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: color var(--transition-fast), border-color var(--transition-fast);
  }
  .corner:hover:not(:disabled) { color: var(--danger); border-color: var(--danger); }
  .corner:disabled { opacity: 0.4; cursor: default; }

  .col-head {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 8px;
    min-width: 0;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    transition: border-color var(--transition-fast), background var(--transition-fast);
  }
  /* A track playing a clip lights its header in the track colour. */
  .col-head.playing {
    border-color: var(--c);
    background: color-mix(in srgb, var(--c) 12%, var(--bg-elevated));
  }
  .dot { width: 8px; height: 8px; border-radius: 50%; flex: none; }
  .col-name {
    font-size: var(--font-size-xs);
    font-weight: 600;
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  /* Which scene this track is currently sounding (or queued to). */
  .col-scene {
    margin-left: auto;
    flex: none;
    max-width: 50%;
    padding: 1px 6px;
    border-radius: 999px;
    font-size: var(--font-size-2xs);
    font-weight: 600;
    color: color-mix(in srgb, #000 55%, var(--c));
    background: var(--c);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  /* Queued (armed): hollow + pulsing until the grid line is reached. */
  .col-scene.armed {
    color: var(--c);
    background: color-mix(in srgb, var(--c) 18%, var(--bg-elevated));
    box-shadow: inset 0 0 0 1px var(--c);
    animation: lp-pulse 0.6s ease-in-out infinite;
  }

  /* ── Scene rows ─────────────────────────────────────────────────────────── */
  .scene-launch {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 28px;
    padding: 0 10px;
    font-size: var(--font-size-sm);
    color: var(--text);
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: background var(--transition-fast), border-color var(--transition-fast), color var(--transition-fast);
  }
  .scene-launch :global(.tri) { color: var(--text-muted); flex: none; }
  .scene-launch:hover { border-color: var(--accent); }
  .scene-launch:hover :global(.tri) { color: var(--accent); }
  .scene-launch.active {
    background: color-mix(in srgb, var(--accent) 16%, var(--bg-elevated));
    border-color: var(--accent);
    color: var(--accent);
  }
  .scene-launch.active :global(.tri) { color: var(--accent); }
  .scene-name { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .warn { margin-left: auto; color: var(--warning); display: inline-flex; }

  /* ── Cells ──────────────────────────────────────────────────────────────── */
  .cell {
    height: 28px;
    border-radius: var(--radius-sm);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .cell.empty {
    border: 1px dashed var(--border-subtle);
    opacity: 0.5;
  }
  .cell.filled {
    border: 1px solid var(--border);
    background: var(--bg-elevated);
    color: var(--text-muted);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast), box-shadow var(--transition-fast), border-color var(--transition-fast);
  }
  .cell.filled :global(.tri) { color: color-mix(in srgb, var(--c) 70%, var(--text-muted)); }
  .cell.filled:hover {
    border-color: var(--c);
    color: var(--c);
    background: color-mix(in srgb, var(--c) 12%, var(--bg-elevated));
  }
  .cell.filled.active {
    background: var(--c);
    border-color: var(--c);
    color: color-mix(in srgb, #000 60%, var(--c));
    box-shadow: 0 0 10px color-mix(in srgb, var(--c) 55%, transparent);
  }
  /* Queued (armed): the target colour, pulsing, until the grid line is reached. */
  .cell.filled.armed {
    border-color: var(--c);
    color: var(--c);
    background: color-mix(in srgb, var(--c) 18%, var(--bg-elevated));
    animation: lp-pulse 0.6s ease-in-out infinite;
  }
  .cell.filled.armed :global(.tri) { color: var(--c); }
</style>
