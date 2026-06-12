/**
 * Nemus keybindings (Step 0). The first, intentionally small set from ui.md —
 * pane-scoped where it matters. Never `Ctrl+Alt+<letter>` (Arbor hard rule):
 * Chromium drops those on IT/DE/FR/ES layouts to preserve AltGr.
 *
 * Ctrl+Click (go-to-declaration) is handled inside the NemusEditor CodeMirror
 * mousedown handler (it needs the click target), so it isn't a keydown binding here.
 */

export interface NemusBinding {
  id:    string;
  key:   string;
  ctrl?: boolean;
  shift?: boolean;
  /** `Alt+Shift+<letter>` is AltGr-safe (Arbor hard rule) — used where Ctrl
   *  combos collide (e.g. commit, which mustn't clash with Save/devtools). */
  alt?:  boolean;
  /** Where the binding is active: 'editor' = only when the tab pane has focus. */
  scope: 'global' | 'editor';
  description: string;
}

export const NEMUS_BINDINGS: NemusBinding[] = [
  // Editor-scoped (only when the tab pane has focus).
  { id: 'goto_line',    key: 'g', ctrl: true,  scope: 'editor', description: 'Go to line' },
  { id: 'new_file',     key: 'n', ctrl: true,  scope: 'editor', description: 'New .nemus in the tab pane' },
  // Transport. `Shift+F9` (IntelliJ-style) leaves `Ctrl+Space` free for the
  // editor's autocomplete (the universal completion trigger).
  { id: 'run_stop',       key: 'F9', shift: true, scope: 'global', description: 'Toggle Run / Stop' },
  { id: 'seek_to_start',  key: '[', ctrl: true, shift: true, scope: 'global', description: 'Skip to start (cycle 0)' },
  { id: 'seek_to_end',    key: ']', ctrl: true, shift: true, scope: 'global', description: 'Skip to end of arrangement' },
  // Window — discovery + layout.
  { id: 'command_palette', key: 'p', ctrl: true, shift: true, scope: 'global', description: 'Open the Command Palette' },
  { id: 'shortcuts',    key: 'F1', scope: 'global', description: 'Show the keyboard shortcuts' },
  { id: 'settings',     key: ',', ctrl: true,  scope: 'global', description: 'Open Settings' },
  { id: 'zen',          key: 'z', ctrl: true, shift: true, scope: 'global', description: 'Toggle Zen mode' },
  { id: 'find',         key: 'f', ctrl: true,  scope: 'global', description: 'Find in file (editor) · search the Console / Problems' },
  // Project / file.
  { id: 'new_project',  key: 'n', ctrl: true, shift: true, scope: 'global', description: 'New project' },
  { id: 'open_project', key: 'o', ctrl: true,  scope: 'global', description: 'Open project' },
  { id: 'open_file',    key: 'o', ctrl: true, shift: true, scope: 'global', description: 'Open file' },
  { id: 'save',         key: 's', ctrl: true,  scope: 'global', description: 'Save the active file' },
  { id: 'render_wav',   key: 'r', ctrl: true, shift: true, scope: 'global', description: 'Export / render to WAV' },
  // `Alt+Shift+I` (AltGr-safe): `Ctrl+Shift+I` would collide with the webview's
  // devtools toggle.
  { id: 'import_audio', key: 'i', alt: true, shift: true, scope: 'global', description: 'Import audio / MIDI as .nemus' },
  // Mixer.
  { id: 'commit_overrides', key: 'c', alt: true, shift: true, scope: 'global', description: 'Commit mixer gain/pan overrides to source' },
];

/** Physical-key codes for punctuation bindings whose printed char shifts with
 *  the modifier/layout (e.g. Shift+`[` → `{`). Matching on `e.code` keeps the
 *  binding stable regardless of Shift state or keyboard layout. */
const PUNCT_CODES: Record<string, string> = {
  '[': 'BracketLeft',
  ']': 'BracketRight',
};

/** True when the event matches the binding (layout-tolerant for letters). */
export function matchesNemus(e: KeyboardEvent, b: NemusBinding): boolean {
  if (!!b.ctrl !== (e.ctrlKey || e.metaKey)) return false;
  if (!!b.shift !== e.shiftKey) return false;
  if (!!b.alt !== e.altKey) return false;
  if (e.key.toLowerCase() === b.key.toLowerCase()) return true;
  if (b.key.length === 1 && /[a-z]/i.test(b.key) && e.code === `Key${b.key.toUpperCase()}`) return true;
  if (PUNCT_CODES[b.key] && e.code === PUNCT_CODES[b.key]) return true;
  return false;
}
