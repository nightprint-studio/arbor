/**
 * The live xterm sessions, kept OUTSIDE the component tree.
 *
 * A terminal is not a view. The process behind it keeps running whether or not you are looking at
 * it, and its scrollback is the record of what it did — so neither can belong to a component that
 * the bottom dock unmounts every time you switch to Stage, Jobs or Pipelines.
 *
 * That is what they used to belong to, and it cost both: switching panels disposed the xterm
 * (the scrollback went with it) and, in the same `onDestroy`, killed the PTY. A `yarn tauri:build`
 * left running in a terminal did not merely become invisible — it was terminated, and the tab that
 * stayed behind was an empty box attached to nothing.
 *
 * So a session owns the terminal, its addons, its PTY event listeners and the element xterm
 * renders into. {@link TerminalInstance} is a viewport onto one: it adopts the element on mount and
 * hands it back on unmount. Between mounts the element waits in {@link limbo} — parked off-screen
 * rather than detached, so it keeps real dimensions and output arriving while the panel is closed
 * lands in a buffer that can still measure itself.
 *
 * A session ends when the terminal does: the user closes the tab, or the process exits.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import { WebLinksAddon } from '@xterm/addon-web-links';
import { terminalWrite, terminalResize } from '$lib/ipc/terminal';
import { terminalStore } from '$lib/stores/terminal.svelte';
import '@xterm/xterm/css/xterm.css';

/** One running terminal. */
export interface TerminalSession {
  id: string;
  term: Terminal;
  fit: FitAddon;
  /** The element xterm renders into. Moved between mount points; never re-created. */
  host: HTMLDivElement;
  /** PTY output + exit listeners, dropped when the session ends. */
  unlisteners: UnlistenFn[];
}

const sessions = new Map<string, TerminalSession>();

/**
 * Where a session's element waits while nothing is showing it.
 *
 * Off-screen rather than `display: none` or detached, both of which measure zero — and a terminal
 * that thinks it is zero rows wide reflows its scrollback to match, so the output of a build that
 * ran while the panel was closed would come back mangled.
 */
let limbo: HTMLDivElement | null = null;

function parkingSpace(): HTMLDivElement {
  if (limbo) return limbo;
  limbo = document.createElement('div');
  limbo.setAttribute('aria-hidden', 'true');
  limbo.style.cssText =
    'position:absolute;left:-10000px;top:0;width:900px;height:600px;overflow:hidden;pointer-events:none';
  document.body.appendChild(limbo);
  return limbo;
}

/** The palette, read from the theme's CSS variables at the moment a terminal is created. */
function terminalTheme() {
  const s = getComputedStyle(document.documentElement);
  const v = (name: string) => s.getPropertyValue(name).trim();
  return {
    background: v('--terminal-bg'),
    foreground: v('--terminal-fg'),
    cursor: v('--terminal-cursor'),
    cursorAccent: v('--terminal-bg'),
    selectionBackground: v('--terminal-selection-bg') || 'rgba(107,155,218,0.25)',
    black: v('--terminal-black'),
    red: v('--terminal-red'),
    green: v('--terminal-green'),
    yellow: v('--terminal-yellow'),
    blue: v('--terminal-blue'),
    magenta: v('--terminal-magenta'),
    cyan: v('--terminal-cyan'),
    white: v('--terminal-white'),
    brightBlack: v('--terminal-bright-black'),
    brightRed: v('--terminal-bright-red'),
    brightGreen: v('--terminal-bright-green'),
    brightYellow: v('--terminal-bright-yellow'),
    brightBlue: v('--terminal-bright-blue'),
    brightMagenta: v('--terminal-bright-magenta'),
    brightCyan: v('--terminal-bright-cyan'),
    brightWhite: v('--terminal-bright-white'),
  };
}

/**
 * The session for `id`, created on first ask.
 *
 * Creation needs a parent with real dimensions — xterm measures a character cell when it opens —
 * so the new element goes into {@link parkingSpace} and is fitted by whoever adopts it.
 */
export function terminalSession(id: string): TerminalSession {
  const existing = sessions.get(id);
  if (existing) return existing;

  const host = document.createElement('div');
  host.className = 'xterm-host';
  host.style.cssText = 'width:100%;height:100%;box-sizing:border-box';
  parkingSpace().appendChild(host);

  const term = new Terminal({
    fontFamily: '"JetBrains Mono", "Cascadia Code", "Fira Code", monospace',
    fontSize: 13,
    lineHeight: 1.2,
    cursorBlink: true,
    cursorStyle: 'bar',
    scrollback: 5000,
    theme: terminalTheme(),
    allowProposedApi: true,
  });
  const fit = new FitAddon();
  term.loadAddon(fit);
  term.loadAddon(new WebLinksAddon());
  term.open(host);
  fit.fit();

  const session: TerminalSession = { id, term, fit, host, unlisteners: [] };
  sessions.set(id, session);

  term.onData((data) => {
    terminalWrite(id, data).catch(() => {});
  });
  term.onTitleChange((title) => {
    if (title) terminalStore.renameTab(id, title);
  });

  // Output keeps arriving — and keeps being written — while no panel is showing this terminal.
  // That is the point: a build you started and walked away from is readable when you come back.
  void listen<string>(`terminal:output:${id}`, (evt) => {
    const bytes = Uint8Array.from(atob(evt.payload), (c) => c.charCodeAt(0));
    session.term.write(bytes);
  }).then((fn) => session.unlisteners.push(fn));

  void listen<null>(`terminal:closed:${id}`, () => {
    session.term.writeln('\r\n\x1b[2m[Process completed — closing…]\x1b[0m');
    // The tab goes shortly after, so the last lines are readable rather than blinked away.
    setTimeout(() => {
      terminalStore.removeTab(id);
      endSession(id);
    }, 400);
  }).then((fn) => session.unlisteners.push(fn));

  return session;
}

/**
 * Tear a session down for good — the terminal, its listeners and its element.
 *
 * Called when the terminal itself ends: the tab is closed, or the process exited. NOT on unmount,
 * which is the mistake this module exists to undo.
 */
export function endSession(id: string): void {
  const session = sessions.get(id);
  if (!session) return;
  sessions.delete(id);
  for (const off of session.unlisteners) off();
  session.term.dispose();
  session.host.remove();
}
