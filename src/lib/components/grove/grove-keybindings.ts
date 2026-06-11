/**
 * Grove keybindings (Step 0). The first, intentionally small set from ui.md —
 * pane-scoped where it matters. Never `Ctrl+Alt+<letter>` (Arbor hard rule):
 * Chromium drops those on IT/DE/FR/ES layouts to preserve AltGr.
 *
 * Ctrl+Click (go-to-declaration) is handled inside the GroveEditor CodeMirror
 * mousedown handler (it needs the click target), so it isn't a keydown binding here.
 */

export interface GroveBinding {
  id:    string;
  key:   string;
  ctrl?: boolean;
  shift?: boolean;
  /** Where the binding is active: 'editor' = only when the tab pane has focus. */
  scope: 'global' | 'editor';
  description: string;
}

export const GROVE_BINDINGS: GroveBinding[] = [
  // Editor-scoped (only when the tab pane has focus).
  { id: 'goto_line',    key: 'g', ctrl: true,  scope: 'editor', description: 'Go to line' },
  { id: 'new_file',     key: 'n', ctrl: true,  scope: 'editor', description: 'New .grove in the tab pane' },
  // Transport.
  { id: 'run_stop',     key: ' ', ctrl: true,  scope: 'global', description: 'Toggle Run / Stop' },
  // Window — discovery + layout.
  { id: 'command_palette', key: 'p', ctrl: true, shift: true, scope: 'global', description: 'Open the Command Palette' },
  { id: 'shortcuts',    key: 'F1', scope: 'global', description: 'Show the keyboard shortcuts' },
  { id: 'settings',     key: ',', ctrl: true,  scope: 'global', description: 'Open Settings' },
  { id: 'zen',          key: 'z', ctrl: true, shift: true, scope: 'global', description: 'Toggle Zen mode' },
  { id: 'find',         key: 'f', ctrl: true,  scope: 'global', description: 'Search in the Console / Problems' },
  // Project / file.
  { id: 'new_project',  key: 'n', ctrl: true, shift: true, scope: 'global', description: 'New project' },
  { id: 'open_project', key: 'o', ctrl: true,  scope: 'global', description: 'Open project' },
  { id: 'open_file',    key: 'o', ctrl: true, shift: true, scope: 'global', description: 'Open file' },
  { id: 'save',         key: 's', ctrl: true,  scope: 'global', description: 'Save the active file' },
  { id: 'render_wav',   key: 'r', ctrl: true, shift: true, scope: 'global', description: 'Export / render to WAV' },
];

/** True when the event matches the binding (layout-tolerant for letters). */
export function matchesGrove(e: KeyboardEvent, b: GroveBinding): boolean {
  if (!!b.ctrl !== (e.ctrlKey || e.metaKey)) return false;
  if (!!b.shift !== e.shiftKey) return false;
  if (e.altKey) return false;
  if (e.key.toLowerCase() === b.key.toLowerCase()) return true;
  if (b.key.length === 1 && /[a-z]/i.test(b.key) && e.code === `Key${b.key.toUpperCase()}`) return true;
  return false;
}
