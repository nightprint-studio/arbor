/**
 * The IntelliJ editing keys, as one list.
 *
 * Arbor has two CodeMirror surfaces — the shared code editor (Bennu, Picus, the
 * previews) and merula's live-coding pane — and both are aimed at the same hand:
 * someone IntelliJ-comfortable, who presses these without looking. They had grown
 * their own near-copies of this list, which is how `Cmd+Backspace` came to delete
 * the whole line in one editor and only the part left of the caret in the other.
 * One list, bound in both places, is the fix that stays fixed.
 *
 * **Where a host splices it matters.** The bindings must come BEFORE `defaultKeymap`
 * and `historyKeymap` in the same `keymap.of([...])` (or at a higher precedence):
 * `Ctrl-y` is the Windows/Linux redo in `historyKeymap`, and `Mod-Backspace` is
 * delete-to-line-start in `defaultKeymap` on macOS. Whoever is tried first wins, so
 * losing that race looks like a broken shortcut rather than a stolen one.
 *
 * Only the keys CodeMirror gets *wrong* for an IDE user, or has no command for, are
 * here. `Mod-/` (toggle comment) and `Alt+↑/↓` (move line) already come from
 * `defaultKeymap` and are not repeated — a binding listed twice is a binding that
 * can drift.
 */

import type { KeyBinding } from '@codemirror/view';
import { deleteLine, moveLineDown, moveLineUp, redo } from '@codemirror/commands';
import { selectNextOccurrence } from '@codemirror/search';

import { duplicateSelection } from './commands';

/**
 * The IntelliJ editing bindings. Returns a fresh array so a host can splice, filter or
 * extend it without touching what the other host gets.
 */
export function intellijEditingKeymap(): KeyBinding[] {
  return [
    // Delete the current line, whole, wherever in it the caret sits. Two spellings of one
    // verb: `Ctrl+Y` is IntelliJ's key on Windows and Linux, `Cmd+Backspace` is its key on
    // macOS — where `defaultKeymap` otherwise runs `deleteLineBoundaryBackward` (line start
    // → caret), which passes for delete-line only while the caret is at the end of the line.
    // The Mac binding is deliberately mac-only: on Windows and Linux `Ctrl+Backspace` is
    // delete-word-backward and must stay that way.
    { key: 'Ctrl-y', run: deleteLine, preventDefault: true },
    { mac: 'Mod-Backspace', run: deleteLine, preventDefault: true },
    // …which leaves the editor with NO redo on Windows: `historyKeymap` binds redo to `Mod-y`
    // there and to `Mod-Shift-z` only on macOS (Linux gets both), so taking `Ctrl-y` for
    // delete-line took the only one Windows had. `Ctrl/Cmd+Shift+Z` is what IntelliJ and
    // VS Code use everywhere.
    { key: 'Mod-Shift-z', run: redo, preventDefault: true },
    // `Mod-d` duplicates: the most-used editing verb after copy and paste, and the one
    // `defaultKeymap` has no command for at all (see `./commands`).
    { key: 'Mod-d', run: duplicateSelection, preventDefault: true },
    // `Alt-j` adds the next occurrence of the selection as a second cursor — IntelliJ's own
    // key for it. VS Code puts this on `Mod-d`; both cannot have it, and duplicating is asked
    // for far more often than multi-select.
    { key: 'Alt-j', run: selectNextOccurrence, preventDefault: true },
    // Moving a line: `Alt+↑/↓` comes from `defaultKeymap`, and these are the IntelliJ spelling
    // of the same thing. An alias, not a second feature.
    { key: 'Mod-Shift-ArrowUp', run: moveLineUp, preventDefault: true },
    { key: 'Mod-Shift-ArrowDown', run: moveLineDown, preventDefault: true },
  ];
}
