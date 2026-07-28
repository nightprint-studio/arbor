/**
 * Canonical list of Picus's in-window keyboard shortcuts — the single source of
 * truth shared by the shortcuts reference modal, the docs panel and the shell's
 * key handler. Keep it in sync with the matching cases in `PicusShell`'s
 * `onKeyDown`.
 *
 * Window-local, like Tyto's: Picus is a standalone product window, so its
 * bindings live here rather than in the global (Corvus) keybindings store.
 *
 * Two constraints on every choice below:
 *  • never `Ctrl+Alt+<letter>` — Chromium drops those on IT/DE/FR/ES layouts to
 *    preserve AltGr;
 *  • never bare `Alt+Shift+<letter>` — Windows uses it to switch keyboard layout.
 */

export interface PicusShortcut {
  /** Chord parts, e.g. ['Ctrl', 'Enter']. Rendered with the shared Kbd widget. */
  keys: string[];
  description: string;
}

export interface PicusShortcutGroup {
  label: string;
  shortcuts: PicusShortcut[];
}

export const PICUS_SHORTCUTS: PicusShortcutGroup[] = [
  {
    label: 'Navigation',
    shortcuts: [
      { keys: ['Ctrl', 'K'], description: 'Command palette' },
      { keys: ['Ctrl', '1'], description: 'Connections' },
      { keys: ['Ctrl', '2'], description: 'Scripts on disk' },
      { keys: ['Ctrl', '3'], description: 'Generate DML' },
      { keys: ['Ctrl', '4'], description: 'Inventory' },
      { keys: ['Ctrl', 'B'], description: 'Toggle the sidebar' },
      { keys: ['Ctrl', 'J'], description: 'Toggle the bottom panel' },
      { keys: ['Ctrl', 'Tab'], description: 'Next tab' },
      { keys: ['Ctrl', 'Shift', 'Tab'], description: 'Previous tab' },
      { keys: ['Ctrl', 'W'], description: 'Close the current tab' },
    ],
  },
  {
    label: 'Database',
    shortcuts: [
      { keys: ['Ctrl', 'Enter'], description: 'Run the statement under the cursor (or the selection)' },
      { keys: ['Ctrl', 'Shift', 'Enter'], description: 'Run the whole script' },
      { keys: ['Ctrl', 'Shift', 'C'], description: 'Cancel the running query' },
      { keys: ['Ctrl', 'T'], description: 'New query tab on the active connection' },
      { keys: ['Ctrl', 'Shift', 'D'], description: 'Cycle the active connection' },
      { keys: ['Ctrl', 'Shift', 'N'], description: 'New connection' },
      // F4 rather than a Ctrl+Shift letter: it is the IDE verb for "properties of
      // the thing selected", and `Ctrl+Shift+E` is already Arbor's opt-in
      // OS-global accelerator for the File Explorer — a window-local binding
      // underneath it would simply never fire for anyone who enabled it.
      { keys: ['F4'], description: 'Edit the active connection' },
    ],
  },
  {
    // These are the editor's own bindings (CodeMirror), not the shell's — they only
    // apply while the caret is in a query tab or a script file. Listed here anyway
    // because the reference is the user's map of the keyboard, not of the code.
    label: 'SQL editor',
    shortcuts: [
      { keys: ['Ctrl', 'Space'], description: 'Completion — tables, columns, keywords for the tab’s dialect' },
      { keys: ['Tab'], description: 'Accept the completion, or the greyed continuation at the caret' },
      { keys: ['Esc'], description: 'Dismiss the completion or the greyed continuation' },
      { keys: ['Ctrl', '/'], description: 'Comment or uncomment the selected lines' },
      { keys: ['Ctrl', 'Y'], description: 'Delete the current line' },
    ],
  },
  {
    label: 'Scripts on disk',
    shortcuts: [
      // F5 rather than a Ctrl+Shift letter: "re-read from disk" is what F5 means
      // everywhere, and the two obvious alternatives are taken elsewhere in Arbor
      // (Ctrl+Shift+E is the global File Explorer, Ctrl+Shift+R is Tyto's global
      // record) — a window-local binding underneath either would never fire.
      { keys: ['F5'], description: 'Re-read the script repository from disk' },
    ],
  },
  {
    label: 'Generation',
    shortcuts: [
      { keys: ['Ctrl', 'G'], description: 'Generate — build the SQL for every enabled target' },
      { keys: ['Ctrl', 'Shift', 'W'], description: 'Write the generated SQL to the scripts (asks first)' },
      { keys: ['Alt', '1'], description: 'Source: guided form' },
      { keys: ['Alt', '2'], description: 'Source: paste SQL' },
      { keys: ['Alt', '3'], description: 'Source: CSV' },
      { keys: ['Alt', 'ArrowRight'], description: 'Preview the next target' },
      { keys: ['Alt', 'ArrowLeft'], description: 'Preview the previous target' },
    ],
  },
  {
    label: 'Consistency',
    shortcuts: [
      { keys: ['Ctrl', 'Shift', 'K'], description: 'Re-run the consistency check' },
      { keys: ['F8'], description: 'Go to the next finding — opens the file at its line' },
      { keys: ['Shift', 'F8'], description: 'Go to the previous finding' },
    ],
  },
  {
    label: 'Help & app',
    shortcuts: [
      { keys: ['Ctrl', ','], description: 'Settings' },
      { keys: ['F1'], description: 'Documentation' },
      { keys: ['Shift', 'F1'], description: 'Keyboard shortcuts' },
      { keys: ['Esc'], description: 'Close the panel or dialog in front' },
    ],
  },
];
