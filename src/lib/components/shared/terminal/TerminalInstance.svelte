<script lang="ts">
  import { onDestroy } from 'svelte';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { Terminal }    from '@xterm/xterm';
  import { FitAddon }    from '@xterm/addon-fit';
  import { WebLinksAddon } from '@xterm/addon-web-links';
  import { terminalWrite, terminalResize, terminalClose } from '$lib/ipc/terminal';
  import { terminalStore } from '$lib/stores/terminal.svelte';
  import '@xterm/xterm/css/xterm.css';

  // ── Props ─────────────────────────────────────────────────────────────────
  let {
    id,
    active = false,
  }: {
    id:     string;
    active?: boolean;
  } = $props();

  // ── Refs ──────────────────────────────────────────────────────────────────
  let container: HTMLDivElement;

  // ── xterm internals ───────────────────────────────────────────────────────
  let term:    Terminal   | null = null;
  let fit:     FitAddon   | null = null;
  let resizeObs: ResizeObserver | null = null;
  let unlistenOutput: UnlistenFn | null = null;
  let unlistenClosed: UnlistenFn | null = null;

  // ── Read terminal theme from CSS variables ────────────────────────────────
  function getTerminalTheme() {
    const s = getComputedStyle(document.documentElement);
    const v = (name: string) => s.getPropertyValue(name).trim();
    return {
      background:    v('--terminal-bg'),
      foreground:    v('--terminal-fg'),
      cursor:        v('--terminal-cursor'),
      cursorAccent:  v('--terminal-bg'),
      selectionBackground: v('--terminal-selection-bg') || 'rgba(107,155,218,0.25)',
      black:         v('--terminal-black'),
      red:           v('--terminal-red'),
      green:         v('--terminal-green'),
      yellow:        v('--terminal-yellow'),
      blue:          v('--terminal-blue'),
      magenta:       v('--terminal-magenta'),
      cyan:          v('--terminal-cyan'),
      white:         v('--terminal-white'),
      brightBlack:   v('--terminal-bright-black'),
      brightRed:     v('--terminal-bright-red'),
      brightGreen:   v('--terminal-bright-green'),
      brightYellow:  v('--terminal-bright-yellow'),
      brightBlue:    v('--terminal-bright-blue'),
      brightMagenta: v('--terminal-bright-magenta'),
      brightCyan:    v('--terminal-bright-cyan'),
      brightWhite:   v('--terminal-bright-white'),
    };
  }

  // ── Initialise once the element is in the DOM ─────────────────────────────
  $effect(() => {
    if (!container || term) return; // already initialised

    term = new Terminal({
      fontFamily:     '"JetBrains Mono", "Cascadia Code", "Fira Code", monospace',
      fontSize:       13,
      lineHeight:     1.2,
      cursorBlink:    true,
      cursorStyle:    'bar',
      scrollback:     5000,
      theme:          getTerminalTheme(),
      allowProposedApi: true,
    });

    fit  = new FitAddon();
    term.loadAddon(fit);
    term.loadAddon(new WebLinksAddon());
    term.open(container);
    fit.fit();

    // ── Send keyboard input to the PTY ─────────────────────────────────────
    term.onData((data) => {
      terminalWrite(id, data).catch(() => {});
    });

    // ── Track dynamic title changes (OSC 0/2) ──────────────────────────────
    term.onTitleChange((title) => {
      if (title) terminalStore.renameTab(id, title);
    });

    // ── Listen for PTY output events ───────────────────────────────────────
    listen<string>(`terminal:output:${id}`, (evt) => {
      if (!term) return;
      // Payload is base64-encoded raw bytes
      const bytes = Uint8Array.from(atob(evt.payload), c => c.charCodeAt(0));
      term.write(bytes);
    }).then(fn => { unlistenOutput = fn; });

    // ── Listen for process-exited event ────────────────────────────────────
    listen<null>(`terminal:closed:${id}`, () => {
      term?.writeln('\r\n\x1b[2m[Process completed — closing…]\x1b[0m');
      // Remove the tab quickly so the user isn't left with a dead terminal
      setTimeout(() => terminalStore.removeTab(id), 400);
    }).then(fn => { unlistenClosed = fn; });

    // ── Auto-resize with ResizeObserver ────────────────────────────────────
    resizeObs = new ResizeObserver(() => {
      if (!fit || !term) return;
      fit.fit();
      const { cols, rows } = term;
      terminalResize(id, cols, rows).catch(() => {});
    });
    resizeObs.observe(container);

    return () => teardown();
  });

  // When the tab becomes active, refit to ensure correct sizing.
  $effect(() => {
    if (active && fit && term) {
      // rAF ensures the element is visible before fitting
      requestAnimationFrame(() => {
        fit!.fit();
        terminalResize(id, term!.cols, term!.rows).catch(() => {});
        term!.focus();
      });
    }
  });

  function teardown() {
    resizeObs?.disconnect();
    unlistenOutput?.();
    unlistenClosed?.();
    term?.dispose();
    term    = null;
    fit     = null;
    resizeObs = null;
  }

  onDestroy(() => {
    teardown();
    // Kill the PTY process when the component is destroyed
    terminalClose(id).catch(() => {});
  });
</script>

<!--
  The container div is always rendered (so xterm keeps its state), but hidden
  when the tab is not active.  Visibility is managed by the parent's CSS.
-->
<div
  class="xterm-container"
  class:active
  bind:this={container}
  aria-label="Terminal"
></div>

<style>
  .xterm-container {
    width:    100%;
    height:   100%;
    display:  none;
    padding:  4px 8px;
    box-sizing: border-box;
    overflow: hidden;
  }
  .xterm-container.active {
    display: block;
  }

  /* Override xterm.js defaults to fit our dark theme */
  :global(.xterm) {
    height: 100%;
  }
  :global(.xterm-viewport) {
    scrollbar-width: thin;
    scrollbar-color: rgba(255,255,255,0.1) transparent;
  }
  :global(.xterm-viewport::-webkit-scrollbar) {
    width: 5px;
  }
  :global(.xterm-viewport::-webkit-scrollbar-thumb) {
    background: rgba(255,255,255,0.12);
    border-radius: var(--radius-sm);
  }
</style>
