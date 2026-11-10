<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { appearanceStore } from '$lib/stores/appearance.svelte';
  import { isMac } from '$lib/utils/platform';
  import WindowZoomMenu from './WindowZoomMenu.svelte';
  // Title bar lives at the very top — tooltips fly downward so they don't
  // get clipped by the window edge.
  import { tooltipBottom as tooltip } from '$lib/actions/tooltip';

  interface Props {
    /**
     * Paint our own trio even on macOS. For a window built **frameless on macOS**
     * (`window/mod.rs::frameless_window` — today only Tyto's, which doubles as a
     * full-screen overlay): there are no native traffic lights to stand aside for, so
     * standing aside would leave the window with no controls at all.
     */
    forceCustom?: boolean;
  }
  let { forceCustom = false }: Props = $props();

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

  // `crescenti` draws the mac trio with its glyphs always showing.
  const style = $derived(appearanceStore.windowControlsStyle === 'windows' ? 'windows' : 'mac');
  const glyphsAlways = $derived(appearanceStore.windowControlsStyle === 'crescenti');

  // macOS greys the trio out while the window is in the background.
  let windowFocused = $state(true);

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

<svelte:window
  onresize={() => { if (zoomOpen) closeZoom(); }}
  onfocus={() => (windowFocused = true)}
  onblur={() => (windowFocused = false)}
/>

<!-- On macOS the OS paints the REAL traffic lights over our title bar (native
     Overlay style — see window/mod.rs::native_titlebar), so we render nothing
     here and let the platform own the min/max/close trio. Off macOS we paint our
     own: the `style` setting picks the faux-mac trio (18×18, leading edge, like
     the real thing) or the Windows/IntelliJ trio (full-height, flush to the
     corner). The side swap lives in app.css — see `.window-controls-slot`.
     `forceCustom` opts a macOS window that was built frameless back into ours. -->
{#if !isMac || forceCustom}
<div class="window-controls no-drag" data-style={style} class:zoom-open={zoomOpen} class:inactive={!windowFocused} class:glyphs-always={glyphsAlways}>
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
      <svg class="wc-icon" width="12" height="12" viewBox="0 0 12 12" fill="none" shape-rendering="geometricPrecision" aria-hidden="true">
        <path d="M3.8 3.8l4.4 4.4M8.2 3.8L3.8 8.2" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>
      </svg>
    </button>
    <button class="wc-btn wc-mac wc-minimize" onclick={() => appWindow.minimize()} use:tooltip={'Minimize'} aria-label="Minimize">
      <svg class="wc-icon" width="12" height="12" viewBox="0 0 12 12" fill="none" shape-rendering="geometricPrecision" aria-hidden="true">
        <path d="M3.2 6h5.6" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>
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
        <!-- Collapse: two arrowheads pointing at the centre, their right-angle corners 2.3px apart —
             filled only: an outline would swell each triangle and close the gap. -->
        <svg class="wc-icon" width="12" height="12" viewBox="0 0 12 12" shape-rendering="geometricPrecision" aria-hidden="true">
          <path d="M5.2 5.2H2.2L5.2 2.2Z" fill="currentColor"/>
          <path d="M6.8 6.8H9.8L6.8 9.8Z" fill="currentColor"/>
        </svg>
      {:else}
        <!-- Expand: the platform's outward-pointing pair, hypotenuses 1.4px apart (symmetric about the
             centre of the dot). -->
        <svg class="wc-icon" width="12" height="12" viewBox="0 0 12 12" shape-rendering="geometricPrecision" aria-hidden="true">
          <path d="M3.3 3.3H7.7L3.3 7.7Z" fill="currentColor"/>
          <path d="M8.7 8.7H4.3L8.7 4.3Z" fill="currentColor"/>
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
  /* Real macOS metrics: 12px dots on a 20px pitch, a hairline darker ring and a
     faint top sheen instead of a flat fill. Glyph and ring colours are per
     button, like the system ones. */
  .wc-btn.wc-mac {
    position: relative;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    color: transparent;
    background:
      linear-gradient(180deg, rgba(255, 255, 255, 0.22), rgba(255, 255, 255, 0) 55%),
      var(--wc-mac-bg, transparent);
    box-shadow: inset 0 0 0 0.5px var(--wc-mac-ring, rgba(0, 0, 0, 0.2));
  }
  /* Hit area stays comfortable despite the small dot. */
  .wc-btn.wc-mac::after { content: ''; position: absolute; inset: -4px; border-radius: 50%; }
  .wc-mac.wc-close    { --wc-mac-bg: #ff5f57; --wc-mac-ring: #e2463f; --wc-mac-glyph: #4c0102; }
  .wc-mac.wc-minimize { --wc-mac-bg: #febc2e; --wc-mac-ring: #e1a116; --wc-mac-glyph: #985712; }
  .wc-mac.wc-maximize { --wc-mac-bg: #28c840; --wc-mac-ring: #1aab29; --wc-mac-glyph: #0a6319; }
  .wc-mac:active      { filter: brightness(0.82) saturate(1.1); }
  .wc-mac:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }

  /* Background window: the trio goes neutral grey until the pointer is on it
     (hovering restores the colours, as on macOS). */
  .window-controls[data-style="mac"].inactive:not(:hover):not(.zoom-open) .wc-mac {
    background: rgba(128, 128, 128, 0.35);
    box-shadow: inset 0 0 0 0.5px rgba(128, 128, 128, 0.45);
  }

  /* The glyphs belong to the GROUP, not to the hovered dot: on macOS pointing
     at any one of the three reveals all three. Focus and an open zoom menu
     count as "the user is here" too. */
  .window-controls[data-style="mac"]:hover .wc-mac .wc-icon,
  .window-controls[data-style="mac"]:focus-within .wc-mac .wc-icon,
  .window-controls[data-style="mac"].zoom-open .wc-mac .wc-icon,
  .window-controls[data-style="mac"].glyphs-always .wc-mac .wc-icon {
    color: var(--wc-mac-glyph);
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
