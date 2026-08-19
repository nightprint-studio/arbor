/**
 * Canonical list of Tyto's in-window keyboard shortcuts — the single source of
 * truth shared by the titlebar hints, the shortcuts reference modal, the docs
 * "Shortcuts" section and the shell's key handler. Keep this in sync with the
 * matching cases in TytoShell's `onKeyDown`.
 *
 * These are window-local (not the global keybindings store, which is Corvus's):
 * Tyto is a standalone product window, so its bindings live here, close to the
 * only code that consumes them.
 */

export interface TytoShortcut {
  /** Chord parts, e.g. ['Ctrl', '1']. Rendered with the shared Kbd widget. */
  keys: string[];
  description: string;
}

export interface TytoShortcutGroup {
  label: string;
  shortcuts: TytoShortcut[];
}

export const TYTO_SHORTCUTS: TytoShortcutGroup[] = [
  {
    label: 'Capture',
    shortcuts: [
      { keys: ['Ctrl', '1'], description: 'Switch to Recording mode' },
      { keys: ['Ctrl', '2'], description: 'Switch to Screenshot mode' },
      { keys: ['Ctrl', 'Enter'], description: 'Primary action — record / stop, or take a screenshot' },
      { keys: ['Ctrl', 'Shift', 'A'], description: 'Toggle system audio (recording)' },
      { keys: ['Ctrl', 'Shift', 'F'], description: 'Recording output: video ↔ frame sequence' },
    ],
  },
  {
    label: 'Source',
    shortcuts: [
      { keys: ['Ctrl', 'Shift', 'S'], description: 'Cycle source: Monitor → Window → Region' },
      { keys: ['Ctrl', 'Shift', '1'], description: 'Source: Monitor' },
      { keys: ['Ctrl', 'Shift', '2'], description: 'Source: Window' },
      { keys: ['Ctrl', 'Shift', '3'], description: 'Source: Region' },
      { keys: ['Ctrl', 'Shift', 'D'], description: 'Reselect the capture region' },
    ],
  },
  {
    label: 'View',
    shortcuts: [
      { keys: ['Ctrl', 'Shift', 'B'], description: 'Toggle the captures library' },
      { keys: ['Ctrl', 'Shift', 'O'], description: 'Reveal the output folder' },
      { keys: ['Ctrl', 'Shift', 'C'], description: 'Snip capture (in-window selector)' },
    ],
  },
  {
    label: 'Frame sequence player',
    shortcuts: [
      { keys: ['Space'], description: 'Play / pause' },
      { keys: ['←'], description: 'Previous frame' },
      { keys: ['→'], description: 'Next frame' },
      { keys: ['Shift', '←'], description: 'Back 10 frames' },
      { keys: ['Shift', '→'], description: 'Forward 10 frames' },
      { keys: ['Home'], description: 'First frame' },
      { keys: ['End'], description: 'Last frame' },
      { keys: ['L'], description: 'Toggle loop' },
    ],
  },
  {
    label: 'Help & app',
    shortcuts: [
      { keys: ['Ctrl', ','], description: 'Settings' },
      { keys: ['F1'], description: 'Documentation' },
      { keys: ['Shift', 'F1'], description: 'Keyboard shortcuts' },
      { keys: ['Esc'], description: 'Close panel · cancel region selection' },
    ],
  },
];
