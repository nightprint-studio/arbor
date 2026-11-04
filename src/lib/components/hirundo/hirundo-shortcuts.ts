/**
 * Canonical list of Hirundo's in-window keyboard shortcuts — shared by the shortcuts reference, the docs and
 * the shell's key handler. Keep it in step with the shell's `onKeyDown`.
 *
 * Window-local, like Garrulus's and Picus's: Hirundo is a standalone product window, so its bindings live
 * here rather than in the global (Corvus) keybindings store.
 *
 * Constraints on every choice: never `Ctrl+Alt+<letter>` (AltGr on IT/DE/FR/ES), never `Ctrl+Shift+E` /
 * `Ctrl+Shift+R` (Arbor's OS-global accelerators), and the shell gates on `surfaceStore.hasFocus('hirundo')`
 * so a background tab of the tabbed container never reacts.
 */

export interface HirundoShortcut {
  keys: string[];
  description: string;
}

export interface HirundoShortcutGroup {
  label: string;
  shortcuts: HirundoShortcut[];
}

export const HIRUNDO_SHORTCUTS: HirundoShortcutGroup[] = [
  {
    label: 'Request',
    shortcuts: [
      { keys: ['Ctrl', 'Enter'], description: 'Send the request' },
      { keys: ['Esc'], description: 'Cancel the request in flight' },
      { keys: ['Ctrl', 'S'], description: 'Save the request' },
      { keys: ['Ctrl', 'D'], description: 'Duplicate the request' },
      { keys: ['Ctrl', 'N'], description: 'New request' },
      { keys: ['Ctrl', 'Shift', 'N'], description: 'New collection' },
      { keys: ['Ctrl', 'Shift', 'S'], description: 'Save as…' },
      { keys: ['Ctrl', 'Shift', 'U'], description: 'cURL — paste a curl command, or copy this request as one' },
      { keys: ['Ctrl', 'Shift', 'Y'], description: 'Import an OpenAPI spec (JSON)' },
      { keys: ['Ctrl', 'Shift', 'F'], description: 'Format the JSON body' },
      { keys: ['Ctrl', 'Shift', 'B'], description: 'Bulk edit — the table as text, and back' },
      { keys: ['Ctrl', 'Shift', 'V'], description: 'Resolved variables — what will actually be sent' },
    ],
  },
  {
    label: 'Collections tree',
    shortcuts: [
      { keys: ['↑', '↓'], description: 'Move through the tree; → opens a folder, ← closes it' },
      { keys: ['Enter'], description: 'Open the request in a tab that stays' },
      { keys: ['F2'], description: 'Rename the collection, folder or request' },
      { keys: ['Del'], description: 'Delete it (asks first)' },
      { keys: ['Menu'], description: 'Context menu — also a right click' },
    ],
  },
  {
    label: 'Navigation',
    shortcuts: [
      { keys: ['Ctrl', 'K'], description: 'Command palette' },
      { keys: ['Ctrl', 'P'], description: 'Find a request by name, matched loosely' },
      { keys: ['Ctrl', 'E'], description: 'Environments — switch, edit, define types' },
      { keys: ['Ctrl', 'Shift', 'O'], description: 'Open a project — recent ones, or browse for a folder' },
      { keys: ['Ctrl', 'Shift', 'P'], description: 'New project — a folder with a .hirundo folder inside' },
      { keys: ['Ctrl', 'W'], description: 'Close the tab' },
      { keys: ['F2'], description: 'Rename the request of the tab' },
      { keys: ['Ctrl', 'Shift', 'PageUp'], description: 'Move the tab left (or drag it)' },
      { keys: ['Ctrl', 'Shift', 'PageDown'], description: 'Move the tab right' },
      { keys: ['Ctrl', 'PageDown'], description: 'Next request tab' },
      { keys: ['Ctrl', 'PageUp'], description: 'Previous request tab' },
      { keys: ['Ctrl', '1'], description: 'Collections' },
      { keys: ['Ctrl', '2'], description: 'Detected from code' },
      { keys: ['Ctrl', '3'], description: 'Environments' },
      { keys: ['Ctrl', '4'], description: 'Runner (right sidebar)' },
      { keys: ['Ctrl', '5'], description: 'History (right sidebar)' },
      { keys: ['Ctrl', '6'], description: 'Schedules (right sidebar)' },
      { keys: ['Ctrl', '7'], description: 'Cookies (right sidebar)' },
      { keys: ['Ctrl', '8'], description: 'Event stream (right sidebar)' },
      { keys: ['Ctrl', 'Shift', 'L'], description: 'Listen to the open request as an event stream' },
      { keys: ['Ctrl', 'J'], description: 'Toggle the response panel' },
      { keys: ['Ctrl', 'B'], description: 'Toggle the sidebar' },
      { keys: ['F1'], description: 'Documentation' },
    ],
  },
];
