/**
 * Canonical list of Garrulus's in-window keyboard shortcuts — the single source
 * of truth shared by the shortcuts reference, the docs panel and the shell's key
 * handler. Keep it in sync with the matching cases in the shell's `onKeyDown`.
 *
 * Window-local, like Picus's and Tyto's: Garrulus is a standalone product window,
 * so its bindings live here rather than in the global (Corvus) keybindings store.
 * That store is Corvus's own — putting a note verb in it would show the binding in
 * Corvus's settings and in its shortcut sheet, where it can never fire.
 *
 * **The gate that makes window-local bindings actually local.** Products can run
 * as tabs of one window, and a background tab stays mounted (`SurfaceHost` hides
 * it, it is not destroyed), so its `window` key listener still receives every
 * chord. Corvus's handler returns early unless `surfaceStore.hasFocus('corvus')`;
 * Garrulus's must do the same for `'garrulus'`. Without that gate `Ctrl+Shift+S`
 * here and *Toggle stage area* in Corvus fire together in the tabbed container.
 *
 * Constraints on every choice below:
 *  • never `Ctrl+Alt+<letter>` — Chromium drops those on IT/DE/FR/ES layouts to
 *    preserve AltGr;
 *  • never `Ctrl+Shift+E` or `Ctrl+Shift+R` — those are Arbor's opt-in OS-global
 *    accelerators (File Explorer, Tyto record) and would never reach this window;
 *  • bare `Alt+<letter>` carries the inline formatting family by decision
 *    (`docs/garrulus-design.md` §12.7): one modifier, learnable as a group, and
 *    `Ctrl+B` stays the sidebar across the whole suite.
 */

export interface GarrulusShortcut {
  /** Chord parts, e.g. ['Ctrl', 'Shift', 'S']. Rendered with the shared Kbd widget. */
  keys: string[];
  description: string;
}

export interface GarrulusShortcutGroup {
  label: string;
  shortcuts: GarrulusShortcut[];
}

export const GARRULUS_SHORTCUTS: GarrulusShortcutGroup[] = [
  {
    label: 'Navigation',
    shortcuts: [
      { keys: ['Ctrl', 'K'], description: 'Command palette' },
      // Two boxes, two questions: Ctrl+K is "what can I do", Ctrl+O is "where is
      // the note called…". Merging them ranks a verb against a title and answers
      // neither well.
      { keys: ['Ctrl', 'O'], description: 'Quick switcher — open a note by title, matched loosely' },
      // The vault picker is the "open variant" of Ctrl+O, which is why it takes
      // the Shift. Everything on screen belongs to one vault, so choosing which
      // is a rarer and larger act than choosing a note.
      { keys: ['Ctrl', 'Shift', 'O'], description: 'Vaults — open, switch or create the folder of notes this window shows' },
      { keys: ['Ctrl', 'Shift', 'F'], description: 'Search the vault — full text plus type: and field: filters' },
      { keys: ['Ctrl', '1'], description: 'Notes — the vault tree, pinned notes and recents' },
      { keys: ['Ctrl', '2'], description: 'Search' },
      { keys: ['Ctrl', '3'], description: 'Tags and frontmatter fields' },
      { keys: ['Ctrl', '4'], description: 'Note types' },
      { keys: ['Ctrl', 'B'], description: 'Toggle the sidebar' },
      { keys: ['Ctrl', 'J'], description: 'Toggle the bottom dock — tasks, problems, conflicts, history' },
      { keys: ['Alt', '←'], description: 'Back through the notes you visited' },
      { keys: ['Alt', '→'], description: 'Forward' },
      { keys: ['Ctrl', 'Tab'], description: 'Next note tab' },
      { keys: ['Ctrl', 'Shift', 'Tab'], description: 'Previous note tab' },
      { keys: ['Ctrl', 'W'], description: 'Close the current note tab' },
      { keys: ['Ctrl', 'Click'], description: 'Follow the link under the pointer — Enter follows the one at the caret' },
      { keys: ['Ctrl', 'Shift', 'Click'], description: 'Follow the link in a split beside this note' },
    ],
  },
  {
    label: 'Notes',
    shortcuts: [
      { keys: ['Ctrl', 'N'], description: 'New note' },
      { keys: ['Ctrl', 'Shift', 'N'], description: 'New note of a type — pick the type first, its template fills the note' },
      { keys: ['Ctrl', 'D'], description: "Open today's daily note, creating it if this is the first line of the day" },
      { keys: ['Ctrl', 'S'], description: 'Save now — the editor also saves on its own; this makes the moment explicit' },
      // F2 rather than a Ctrl+Shift letter: it is the IDE verb for rename, and
      // this rename is the refactoring one — every link that pointed at the old
      // title is rewritten with it.
      { keys: ['F2'], description: 'Rename this note and update every link to it' },
      { keys: ['Alt', 'Enter'], description: 'Intentions on the selection or the line — extract to a new note, promote to a type, turn into a task' },
      { keys: ['Ctrl', 'Shift', 'Space'], description: 'Inbox scratch — an unnamed, unfiled buffer that asks nothing' },
      { keys: ['Alt', 'Shift', 'E'], description: 'Export this note — the palette carries the format' },
    ],
  },
  {
    // The editor's own bindings: they apply while the caret is in a note. Listed
    // here because the reference is the user's map of the keyboard, not of the code.
    label: 'Editing',
    shortcuts: [
      { keys: ['Ctrl', 'E'], description: 'Rendered ↔ source for the whole note — the line at the caret always shows its markdown' },
      { keys: ['Alt', 'B'], description: 'Bold' },
      { keys: ['Alt', 'I'], description: 'Italic' },
      { keys: ['Alt', 'C'], description: 'Inline code' },
      { keys: ['Alt', 'S'], description: 'Strikethrough' },
      { keys: ['Alt', 'H'], description: 'Highlight' },
      // Alt+L rather than Alt+K so it is not read as a cousin of Ctrl+K, the palette.
      { keys: ['Alt', 'L'], description: 'Link — wraps the selection' },
      { keys: ['Alt', 'Q'], description: 'Quote, and a second press makes it a callout' },
      { keys: ['Alt', 'T'], description: 'Task checkbox' },
      { keys: ['Ctrl', 'Shift', 'K'], description: 'Insert a link to a note — the same picker typing [[ opens' },
      { keys: ['Ctrl', 'Shift', 'T'], description: 'Insert a table' },
      { keys: ['Tab'], description: 'Accept the completion at the caret — a note title, a tag, a heading inside a note' },
      { keys: ['Esc'], description: 'Dismiss the completion, or close the panel in front' },
    ],
  },
  {
    label: 'Sync',
    shortcuts: [
      // The one binding that writes bytes to the remote, and it is a keystroke
      // rather than a timer on purpose: nothing in Garrulus syncs without a
      // deliberate action (`docs/garrulus-design.md` §4.2).
      { keys: ['Ctrl', 'Shift', 'S'], description: 'Sync now — commit what changed, pull, push' },
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
