<script lang="ts">
  import { tooltip } from '$lib/actions/tooltip';
  /**
   * BennuProjectSwitcher — the Alt+Tab-style overlay over your open projects.
   *
   * Drawn like the dictation overlay: no card, the editor dimmed behind it, the selected project large in the middle
   * with a ring and two dots circling it, its name and path under it, the other projects as a row of small marks, and
   * the keys at the foot. A task switcher is something you glance at and release, so what is selected is what the eye
   * lands on; the path is there because it is the only way to tell two checkouts of one project apart.
   *
   * Purely presentational: the keys (Ctrl+Tab to open and walk, release Ctrl to commit, Esc to
   * cancel, arrows too) are owned by BennuWindow, because they have to run before everything else
   * on the window. Tiles are also clickable, so the overlay works from the palette with a mouse.
   *
   * It lists only what is loaded — on screen, visited this session, or with an index / language
   * server in memory (asked of the backend). Everything else, including a workspace member that
   * was only restored in the background, is "Go to project…", which searches.
   *
   * Motion is opacity and transform only — no blur, which WebView2 pays for on every frame.
   */
  import Monogram from '$lib/components/shared/ui/Monogram.svelte';
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { projectSwitcherStore, type SwitcherEntry } from '$lib/stores/bennu/project-switcher.svelte';
  import { workspacesStore, wsColorVar } from '$lib/stores/bennu/workspaces.svelte';

  let { onPick }: { onPick: (entry: SwitcherEntry) => void } = $props();

  const activeRoot = $derived(projectStore.project?.root ?? null);
  const activeWorkspace = $derived(workspacesStore.activeId);
  const showWorkspace = $derived(workspacesStore.hasMany);
  const selected = $derived(projectSwitcherStore.entries[projectSwitcherStore.index]);

  function isCurrent(e: SwitcherEntry): boolean {
    return e.root === activeRoot && e.workspaceId === activeWorkspace;
  }

  // The selection is one highlight that SLIDES to the tile under the selection, rather than a style
  // toggled on each tile: a Tab press then reads as movement, and a wrap from last to first as a
  // sweep across the strip. Transform only (no layout, no blur).
  let strip = $state<HTMLElement | null>(null);
  let tiles = $state<HTMLElement[]>([]);
  let glow = $state<{ x: number; y: number; w: number; h: number } | null>(null);
  /** False for the first placement, so the highlight appears in place instead of flying in from 0,0. */
  let glowAnimated = $state(false);

  $effect(() => {
    const i = projectSwitcherStore.index;
    const open = projectSwitcherStore.open;
    void projectSwitcherStore.entries.length;
    if (!open) {
      glow = null;
      glowAnimated = false;
      return;
    }
    // Measured now and once more after the frame: the tiles pop in with a transform and the stage
    // rises, and a halo placed from a half-laid-out strip is an empty box on the wrong row that only
    // the next selection corrects.
    const place = () => {
      const tile = tiles[i];
      if (!tile || !tile.isConnected || tile.offsetWidth === 0) return;
      tile.scrollIntoView({ block: 'nearest', inline: 'nearest' });
      glow = { x: tile.offsetLeft, y: tile.offsetTop, w: tile.offsetWidth, h: tile.offsetHeight };
    };
    place();
    // Switched on a frame later: the transition must not run for the placement that creates it.
    requestAnimationFrame(() => {
      place();
      glowAnimated = true;
    });
  });
</script>

{#if projectSwitcherStore.open}
  <div class="ps-scrim">
    <div class="ps-stage" role="dialog" aria-label="Switch project" tabindex="-1">
      <!-- The selected project, large. Keyed by the selection so each Tab lands it with a small pop. -->
      {#key projectSwitcherStore.index}
        <div class="ps-hero" style:--ws={showWorkspace && selected ? wsColorVar(selected.colorIdx) : 'var(--accent)'}>
          <span class="ps-ring" aria-hidden="true"></span>
          <span class="ps-ring ps-ring-outer" aria-hidden="true"></span>
          <span class="ps-orbit" aria-hidden="true"><span class="ps-orbit-dot"></span></span>
          <span class="ps-orbit ps-orbit-slow" aria-hidden="true"><span class="ps-orbit-dot small"></span></span>
          <span class="ps-hero-mark">
            {#if selected}
              <Monogram name={selected.name} size={64} color={showWorkspace ? wsColorVar(selected.colorIdx) : undefined} />
            {/if}
          </span>
        </div>
      {/key}

      <span class="ps-dots" aria-hidden="true"></span>

      {#key projectSwitcherStore.index}
        <div class="ps-title">{selected?.name ?? ''}</div>
        {#if showWorkspace && selected}<div class="ps-subtitle">{selected.workspaceName}</div>{/if}
        <div class="ps-path" use:tooltip={selected?.root}>{selected?.root ?? ''}</div>
      {/key}

      <div class="ps-strip" role="listbox" aria-label="Open projects" bind:this={strip}>
        {#if glow}
          <span
            class="ps-glow"
            class:animated={glowAnimated}
            style:width="{glow.w}px"
            style:height="{glow.h}px"
            style:transform="translate({glow.x}px, {glow.y}px)"
            style:--ws={showWorkspace && selected ? wsColorVar(selected.colorIdx) : 'var(--accent)'}
          ></span>
        {/if}
        {#each projectSwitcherStore.entries as p, i (p.workspaceId + '|' + p.root)}
          <button
            bind:this={tiles[i]}
            type="button"
            class="ps-tile"
            class:active={i === projectSwitcherStore.index}
            role="option"
            aria-selected={i === projectSwitcherStore.index}
            style:--i={Math.min(i, 8)}
            style:--ws={showWorkspace ? wsColorVar(p.colorIdx) : 'var(--accent)'}
            onmousemove={() => projectSwitcherStore.select(i)}
            onclick={() => onPick(p)}
          >
            <span class="ps-mark">
              <Monogram
                name={p.name}
                size={32}
                color={showWorkspace ? wsColorVar(p.colorIdx) : undefined}
              />
              {#if isCurrent(p)}<span class="ps-dot" use:tooltip={'Current project'}></span>{/if}
            </span>
            <span class="ps-name">{p.name}</span>
          </button>
        {/each}
      </div>

      <div class="ps-foot">
        <span class="ps-hint"><Kbd keys={['Tab']} size="sm" /> next</span>
        <span class="ps-hint"><Kbd keys={['Shift', 'Tab']} size="sm" /> previous</span>
        <span class="ps-hint"><Kbd keys={['Esc']} size="sm" /> cancel</span>
        <span class="ps-hint">release <Kbd keys={['Ctrl']} size="sm" /> to switch</span>
      </div>
    </div>
  </div>
{/if}

<style>
  .ps-scrim {
    position: fixed; inset: 0; z-index: 1000;
    display: flex; align-items: center; justify-content: center;
    /* A dimmed vignette, not a blur: blurring what is behind costs the compositor a pass per frame on
       WebView2. The editor stays readable at the edges and goes quiet where the stage is. */
    background: radial-gradient(ellipse at center, rgba(0, 0, 0, 0.78) 0%, rgba(0, 0, 0, 0.58) 70%, rgba(0, 0, 0, 0.46) 100%);
    animation: ps-fade 160ms ease both;
  }
  .ps-stage {
    max-width: min(760px, calc(100vw - 48px)); max-height: 90vh;
    display: flex; flex-direction: column; align-items: center;
    animation: ps-rise 260ms cubic-bezier(0.18, 0.9, 0.25, 1.05) both;
  }

  /* The selected project: a large mark, a ring around it, two dots circling. Transform and opacity only. */
  .ps-hero {
    position: relative; width: 200px; height: 200px; display: flex; align-items: center; justify-content: center;
    animation: ps-land 280ms cubic-bezier(0.2, 0.9, 0.3, 1.15) both;
  }
  .ps-hero-mark {
    position: relative; z-index: 2; width: 104px; height: 104px; border-radius: 28px;
    display: flex; align-items: center; justify-content: center;
    background: linear-gradient(160deg, color-mix(in srgb, var(--ws) 90%, white 10%), color-mix(in srgb, var(--ws) 65%, black 35%));
    box-shadow: 0 18px 44px color-mix(in srgb, var(--ws) 45%, transparent), 0 0 0 1px color-mix(in srgb, var(--ws) 60%, transparent) inset;
  }
  .ps-ring {
    position: absolute; inset: 26px; border-radius: 50%; z-index: 1;
    border: 1px solid color-mix(in srgb, var(--ws) 35%, transparent);
    animation: ps-breathe 2.6s ease-in-out infinite;
  }
  .ps-ring-outer { inset: 0; border-color: color-mix(in srgb, var(--ws) 18%, transparent); animation-delay: 0.6s; }
  .ps-orbit {
    position: absolute; inset: 0; z-index: 1; animation: ps-spin 5.5s linear infinite;
  }
  .ps-orbit-slow { animation-duration: 9s; animation-direction: reverse; inset: 26px; }
  .ps-orbit-dot {
    position: absolute; top: 0; left: 50%; width: 9px; height: 9px; margin-left: -4.5px; border-radius: 50%;
    background: var(--ws); box-shadow: 0 0 12px color-mix(in srgb, var(--ws) 70%, transparent);
  }
  .ps-orbit-dot.small { width: 6px; height: 6px; margin-left: -3px; opacity: 0.8; }

  /* A dotted line that drifts, like the level line of the dictation overlay. */
  .ps-dots {
    width: 200px; height: 2px; margin: 14px 0 12px; opacity: 0.8;
    background-image: radial-gradient(circle, var(--accent) 1px, transparent 1.5px);
    background-size: 8px 2px; animation: ps-drift 1.2s linear infinite;
  }
  .ps-title { font-size: 20px; font-weight: 600; color: var(--text-primary); text-align: center; animation: ps-fade 160ms ease both; }
  .ps-subtitle { margin-top: 2px; font-size: var(--font-size-xs); color: var(--text-muted); animation: ps-fade 160ms ease both; }
  .ps-path {
    max-width: 100%; margin: 6px 4px 0; padding: 0 4px; min-height: 18px; text-align: center;
    font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; direction: rtl;
    animation: ps-fade 160ms ease both;
  }

  /* The other projects: small marks in a row, the selected one lifted by a soft halo that slides to it. */
  .ps-strip {
    position: relative; margin-top: 22px;
    display: flex; flex-wrap: wrap; justify-content: center; gap: 4px;
    padding: 4px; overflow-y: auto; max-height: 30vh;
  }
  .ps-glow {
    position: absolute; left: 0; top: 0; z-index: 0; pointer-events: none;
    border-radius: var(--radius-md, 10px);
    background: radial-gradient(circle at 50% 40%, color-mix(in srgb, var(--ws) 28%, transparent), color-mix(in srgb, var(--ws) 6%, transparent) 70%);
    border: 1px solid color-mix(in srgb, var(--ws) 45%, transparent);
    will-change: transform;
  }
  .ps-glow.animated {
    transition: transform 240ms cubic-bezier(0.22, 1, 0.36, 1), width 240ms cubic-bezier(0.22, 1, 0.36, 1),
      height 240ms cubic-bezier(0.22, 1, 0.36, 1);
  }
  .ps-tile {
    position: relative; z-index: 1; width: 92px; padding: 10px 6px 8px;
    display: flex; flex-direction: column; align-items: center; gap: 5px;
    background: transparent; border: 1px solid transparent; border-radius: var(--radius-md, 10px);
    color: var(--text-muted); font-family: var(--font-ui-sans); cursor: pointer;
    transition: color var(--transition-fast);
    animation: ps-tile-in 260ms cubic-bezier(0.2, 0.9, 0.3, 1.1) backwards;
    animation-delay: calc(var(--i) * 24ms);
  }
  .ps-mark { position: relative; display: inline-flex; transition: transform 220ms cubic-bezier(0.2, 0.9, 0.3, 1.3); }
  .ps-tile.active { color: var(--text-primary); }
  .ps-tile.active .ps-mark { transform: translateY(-2px) scale(1.12); }
  .ps-dot {
    position: absolute; right: -3px; bottom: -3px; width: 9px; height: 9px; border-radius: 50%;
    background: var(--success); border: 2px solid rgba(0, 0, 0, 0.6);
  }
  .ps-name {
    max-width: 100%; text-align: center; font-size: var(--font-size-2xs); font-weight: 600; line-height: 1.25;
    display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden;
    word-break: break-word;
  }

  .ps-foot { display: flex; flex-wrap: wrap; justify-content: center; gap: 6px 18px; margin-top: 22px; font-size: var(--font-size-2xs); color: var(--text-muted); }
  .ps-hint { display: inline-flex; align-items: center; gap: 6px; }

  @keyframes ps-fade { from { opacity: 0; } to { opacity: 1; } }
  @keyframes ps-rise {
    from { opacity: 0; transform: translateY(14px) scale(0.97); }
    to   { opacity: 1; transform: none; }
  }
  @keyframes ps-land {
    from { opacity: 0.4; transform: scale(0.9); }
    to   { opacity: 1; transform: none; }
  }
  @keyframes ps-tile-in {
    from { opacity: 0; transform: translateY(10px) scale(0.92); }
    to   { opacity: 1; transform: none; }
  }
  @keyframes ps-spin { to { transform: rotate(360deg); } }
  @keyframes ps-breathe { 0%, 100% { transform: scale(1); opacity: 0.9; } 50% { transform: scale(1.05); opacity: 0.45; } }
  @keyframes ps-drift { to { background-position: 8px 0; } }
  @media (prefers-reduced-motion: reduce) {
    .ps-scrim, .ps-stage, .ps-hero, .ps-tile, .ps-path, .ps-title, .ps-subtitle, .ps-ring, .ps-orbit, .ps-dots { animation: none; }
    .ps-tile, .ps-mark, .ps-glow.animated { transition: none; }
  }
</style>
