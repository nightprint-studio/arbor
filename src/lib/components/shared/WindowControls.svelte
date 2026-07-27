<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { appearanceStore } from '$lib/stores/appearance.svelte';
  import { isMac } from '$lib/utils/platform';
  import WindowZoomMenu from './WindowZoomMenu.svelte';
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

  // ── Zoom menu (mac trio only) ──────────────────────────────────────────────
  // macOS pops a "Move & Resize" panel when the pointer dwells on the green
  // button; ours does the same. Hover-intent open + a grace period on leave so
  // the pointer can travel from the dot down into the panel.
  const HOVER_OPEN_MS  = 480;
  const HOVER_CLOSE_MS = 220;

  let zoomBtn    = $state<HTMLButtonElement | undefined>();
  let zoomAnchor = $state<DOMRect | null>(null);
  let openTimer:  ReturnType<typeof setTimeout> | null = null;
  let closeTimer: ReturnType<typeof setTimeout> | null = null;

  const zoomOpen = $derived(zoomAnchor !== null);

  function clearTimers() {
    if (openTimer)  { clearTimeout(openTimer);  openTimer  = null; }
    if (closeTimer) { clearTimeout(closeTimer); closeTimer = null; }
  }

  function openZoom() {
    clearTimers();
    zoomAnchor = zoomBtn?.getBoundingClientRect() ?? null;
  }

  function closeZoom(restoreFocus = false) {
    clearTimers();
    zoomAnchor = null;
    if (restoreFocus) zoomBtn?.focus();
  }

  function armOpen() {
    clearTimers();
    if (zoomOpen) return;
    openTimer = setTimeout(() => { openTimer = null; openZoom(); }, HOVER_OPEN_MS);
  }

  function armClose() {
    clearTimers();
    closeTimer = setTimeout(() => { closeTimer = null; zoomAnchor = null; }, HOVER_CLOSE_MS);
  }

  /** Keyboard path into the menu (the button itself stays Enter = zoom). */
  function onZoomKeydown(e: KeyboardEvent) {
    if (!zoomOpen && e.key === 'ArrowDown') {
      e.preventDefault();
      openZoom();
    }
  }

  function zoomClick() {
    closeZoom();
    void appWindow.toggleMaximize();
  }

  $effect(() => () => clearTimers());
</script>

<svelte:window onresize={() => { if (zoomOpen) closeZoom(); }} />

<!-- On macOS the OS paints the REAL traffic lights over our title bar (native
     Overlay style — see window/mod.rs::native_titlebar), so we render nothing
     here and let the platform own the min/max/close trio. Off macOS we paint our
     own: the `style` setting picks the faux-mac trio (18×18, leading edge, like
     the real thing) or the Windows/IntelliJ trio (full-height, flush to the
     corner). The side swap lives in app.css — see `.window-controls-slot`. -->
{#if !isMac}
<div class="window-controls no-drag" data-style={style} class:zoom-open={zoomOpen}>
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
    <!-- Mac trio: close · minimize · zoom, with the platform's own glyphs. They
         all light up together the moment the pointer enters the group, exactly
         like the real traffic lights. -->
    <button class="wc-btn wc-mac wc-close"    onclick={() => appWindow.close()}    use:tooltip={'Close'}    aria-label="Close window">
      <svg class="wc-icon" width="10" height="10" viewBox="0 0 8 8" fill="none" aria-hidden="true">
        <path d="M2.1 2.1l3.8 3.8M5.9 2.1L2.1 5.9" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
      </svg>
    </button>
    <button class="wc-btn wc-mac wc-minimize" onclick={() => appWindow.minimize()} use:tooltip={'Minimize'} aria-label="Minimize">
      <svg class="wc-icon" width="10" height="10" viewBox="0 0 8 8" fill="none" aria-hidden="true">
        <path d="M1.6 4h4.8" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
      </svg>
    </button>
    <button
      class="wc-btn wc-mac wc-maximize"
      bind:this={zoomBtn}
      onclick={zoomClick}
      onpointerenter={armOpen}
      onpointerleave={armClose}
      onkeydown={onZoomKeydown}
      use:tooltip={zoomOpen ? undefined : (isMaximized ? 'Restore' : 'Zoom')}
      aria-label={isMaximized ? 'Restore' : 'Zoom'}
      aria-haspopup="menu"
      aria-expanded={zoomOpen}
    >
      {#if isMaximized}
        <!-- Collapse: the two arrowheads meet in the centre (tips inward). -->
        <svg class="wc-icon" width="10" height="10" viewBox="0 0 8 8" aria-hidden="true">
          <path d="M4 4L0.9 4L4 0.9Z" fill="currentColor" stroke="currentColor" stroke-width="0.55" stroke-linejoin="round"/>
          <path d="M4 4L7.1 4L4 7.1Z" fill="currentColor" stroke="currentColor" stroke-width="0.55" stroke-linejoin="round"/>
        </svg>
      {:else}
        <!-- Expand: the platform's outward-pointing pair. -->
        <svg class="wc-icon" width="10" height="10" viewBox="0 0 8 8" aria-hidden="true">
          <path d="M1.1 1.1H4.9L1.1 4.9Z" fill="currentColor" stroke="currentColor" stroke-width="0.55" stroke-linejoin="round"/>
          <path d="M6.9 6.9H3.1L6.9 3.1Z" fill="currentColor" stroke="currentColor" stroke-width="0.55" stroke-linejoin="round"/>
        </svg>
      {/if}
    </button>
  {/if}
</div>

{#if zoomAnchor}
  <WindowZoomMenu
    anchor={zoomAnchor}
    onClose={() => closeZoom(true)}
    onHoverIn={clearTimers}
    onHoverOut={armClose}
  />
{/if}
{/if}

<style>
  .window-controls {
    display: flex;
    align-items: center;
    height: 100%;
    flex-shrink: 0;
    -webkit-app-region: no-drag;
  }
  /* Mac trio gets gap + breathing room from the window edge. */
  .window-controls[data-style="mac"] {
    gap: 8px;
    padding: 0 12px;
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

  /* ── Mac variant ───────────────────────────────────────────────────── */
  .wc-btn.wc-mac {
    width: 18px;
    height: 18px;
    border-radius: 50%;
    color: transparent;
    background: var(--wc-mac-bg, transparent);
  }
  .wc-mac.wc-close    { --wc-mac-bg: #ff5f57; }
  .wc-mac.wc-minimize { --wc-mac-bg: #ffbd2e; }
  .wc-mac.wc-maximize { --wc-mac-bg: #28ca41; }
  .wc-mac:active      { filter: brightness(0.8); }
  .wc-mac:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }

  /* The glyphs belong to the GROUP, not to the hovered dot: on macOS pointing
     at any one of the three reveals all three. Focus and an open zoom menu
     count as "the user is here" too. */
  .window-controls[data-style="mac"]:hover .wc-icon,
  .window-controls[data-style="mac"]:focus-within .wc-icon,
  .window-controls[data-style="mac"].zoom-open .wc-icon {
    color: rgba(0, 0, 0, 0.58);
  }

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
