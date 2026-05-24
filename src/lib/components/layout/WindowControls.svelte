<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { appearanceStore } from '$lib/stores/appearance.svelte';
  // Title bar lives at the very top — tooltips fly downward so they don't
  // get clipped by the window edge.
  import { tooltipBottom as tooltip } from '$lib/actions/tooltip';

  const appWindow = getCurrentWindow();
  let isMaximized = $state(false);

  $effect(() => {
    let active = true;
    let unlisten: (() => void) | null = null;
    appWindow.isMaximized().then(m => { if (active) isMaximized = m; });
    appWindow.onResized(async () => {
      const m = await appWindow.isMaximized();
      if (active) isMaximized = m;
    }).then(fn => { if (active) unlisten = fn; else fn(); });
    return () => { active = false; unlisten?.(); };
  });

  const style = $derived(appearanceStore.windowControlsStyle);
</script>

<!-- Mac and Windows variants share the same outer wrapper but differ in
     dimensions: mac keeps the original 18×18 trio with breathing room, while
     windows goes IntelliJ-style — full title-bar height, no gap, flush to
     the right edge so the close button reaches the corner. -->
<div class="window-controls no-drag" data-style={style}>
  {#if style === 'windows'}
    <button class="wc-btn wc-win wc-minimize" onclick={() => appWindow.minimize()} use:tooltip={'Minimize'} aria-label="Minimize">
      <svg class="wc-icon" width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
        <path d="M2 8h12" stroke="currentColor" stroke-width="1.25" stroke-linecap="square"/>
      </svg>
    </button>
    <button class="wc-btn wc-win wc-maximize" onclick={() => appWindow.toggleMaximize()} use:tooltip={isMaximized ? 'Restore' : 'Maximize'} aria-label={isMaximized ? 'Restore' : 'Maximize'}>
      {#if isMaximized}
        <svg class="wc-icon" width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
          <rect x="2.5" y="5" width="8.5" height="8.5" stroke="currentColor" stroke-width="1.25" fill="none"/>
          <path d="M5 5V2.5h8.5V11H11" stroke="currentColor" stroke-width="1.25" fill="none"/>
        </svg>
      {:else}
        <svg class="wc-icon" width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
          <rect x="2.5" y="2.5" width="11" height="11" stroke="currentColor" stroke-width="1.25" fill="none"/>
        </svg>
      {/if}
    </button>
    <button class="wc-btn wc-win wc-close" onclick={() => appWindow.close()} use:tooltip={'Close'} aria-label="Close window">
      <svg class="wc-icon" width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
        <path d="M2.5 2.5l11 11M13.5 2.5l-11 11" stroke="currentColor" stroke-width="1.25" stroke-linecap="square"/>
      </svg>
    </button>
  {:else}
    <!-- Mac-inspired (default): coloured trio, dimensions 18×18, X / − / □ glyphs -->
    <button class="wc-btn wc-mac wc-close"    onclick={() => appWindow.close()}          use:tooltip={'Close'}    aria-label="Close window">
      <svg class="wc-icon" width="7" height="7" viewBox="0 0 7 7" fill="none" aria-hidden="true">
        <path d="M1 1l5 5M6 1L1 6" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/>
      </svg>
    </button>
    <button class="wc-btn wc-mac wc-minimize" onclick={() => appWindow.minimize()}       use:tooltip={'Minimize'} aria-label="Minimize">
      <svg class="wc-icon" width="7" height="7" viewBox="0 0 7 7" fill="none" aria-hidden="true">
        <path d="M1 3.5h5" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/>
      </svg>
    </button>
    <button class="wc-btn wc-mac wc-maximize" onclick={() => appWindow.toggleMaximize()} use:tooltip={isMaximized ? 'Restore' : 'Maximize'} aria-label={isMaximized ? 'Restore' : 'Maximize'}>
      {#if isMaximized}
        <svg class="wc-icon" width="7" height="7" viewBox="0 0 7 7" fill="none" aria-hidden="true">
          <path d="M2.5 1H6v3.5M1 2.5V6h3.5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
      {:else}
        <svg class="wc-icon" width="7" height="7" viewBox="0 0 7 7" fill="none" aria-hidden="true">
          <path d="M1 3.5h5M3.5 1v5" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/>
        </svg>
      {/if}
    </button>
  {/if}
</div>

<style>
  .window-controls {
    display: flex;
    align-items: center;
    height: 100%;
    flex-shrink: 0;
    -webkit-app-region: no-drag;
  }
  /* Mac trio gets gap + breathing room from the right edge. */
  .window-controls[data-style="mac"] {
    gap: 7px;
    padding: 0 14px;
  }
  /* Windows / IntelliJ trio: glued together, flush to the right corner so
     the close button hugs the edge of the window. */
  .window-controls[data-style="windows"] {
    gap: 0;
    padding: 0;
  }

  .wc-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    border: none;
    cursor: pointer;
    transition: color var(--transition-fast),
                background var(--transition-fast),
                filter var(--transition-fast);
    flex-shrink: 0;
    padding: 0;
    -webkit-app-region: no-drag;
  }

  /* ── Mac-inspired variant ──────────────────────────────────────────── */
  .wc-btn.wc-mac {
    width: 18px;
    height: 18px;
    border-radius: 50%;
    color: transparent;
    background: var(--wc-mac-bg, transparent);
  }
  .wc-btn.wc-mac:hover .wc-icon { color: rgba(0,0,0,0.6); }
  .wc-mac.wc-close    { --wc-mac-bg: #ff5f57; }
  .wc-mac.wc-minimize { --wc-mac-bg: #ffbd2e; }
  .wc-mac.wc-maximize { --wc-mac-bg: #28ca41; }
  .wc-mac:hover       { filter: brightness(0.82); }

  /* ── Windows / IntelliJ variant ────────────────────────────────────── */
  /* Wider rectangular buttons that take the full title-bar height — the
     close button gets the conventional red flash on hover, the others a
     subtle hover background that matches the rest of the icon-btn row. */
  .wc-btn.wc-win {
    width: 46px;
    height: 100%;
    border-radius: 0;
    background: transparent;
    color: var(--text-secondary);
  }
  .wc-btn.wc-win:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }
  .wc-win.wc-close:hover {
    background: #e81123;
    color: #ffffff;
  }

  .wc-icon { display: block; pointer-events: none; }
</style>
