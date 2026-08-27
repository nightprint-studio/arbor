<script lang="ts">
  /**
   * A viewport onto one terminal.
   *
   * It owns no terminal and no process: both live in {@link terminalSession}, outside the
   * component tree, because the bottom dock unmounts this every time you switch to Stage, Jobs or
   * Pipelines — and a build running in a terminal must survive that. This mounts the session's
   * element, keeps it fitted to the space it has, and hands it back on unmount.
   */
  import { terminalResize } from '$lib/ipc/terminal';
  import { terminalSession } from './session';

  // ── Props ─────────────────────────────────────────────────────────────────
  let {
    id,
    active = false,
  }: {
    id:     string;
    active?: boolean;
  } = $props();

  let container: HTMLDivElement;

  /**
   * Adopt the session's element, and give it back when this viewport goes away.
   *
   * `appendChild` MOVES the element, so there is never a second copy and never a re-created one:
   * the same xterm, with the same scrollback, simply changes parent. The teardown does not dispose
   * anything — see the module docs on `session.ts` for what that used to cost.
   */
  $effect(() => {
    if (!container) return;
    const session = terminalSession(id);
    container.appendChild(session.host);
    refit();

    const observer = new ResizeObserver(refit);
    observer.observe(container);
    return () => {
      observer.disconnect();
      // Back to the parking space, still running, still holding everything it has printed.
      session.host.remove();
    };
  });

  /** When this tab becomes the visible one, it finally has a size — fit to it and take focus. */
  $effect(() => {
    if (!active) return;
    requestAnimationFrame(() => {
      refit();
      terminalSession(id).term.focus();
    });
  });

  /** Fit the terminal to the space it currently has, and tell the PTY the new geometry. A hidden
   *  or parked element measures nothing useful, and reflowing to it would mangle the scrollback. */
  function refit() {
    if (!container?.clientWidth || !container.clientHeight) return;
    const { term, fit } = terminalSession(id);
    fit.fit();
    terminalResize(id, term.cols, term.rows).catch(() => {});
  }
</script>

<!--
  Always rendered while the panel is open, hidden by CSS when this is not the active tab — the
  session's element stays inside it either way.
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
