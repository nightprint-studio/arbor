/**
 * merula editor ergonomics — the IntelliJ-comfortable editing layer that sits on
 * top of the highlight/lint glue in `merula-cm.ts`. Kept in its own module so the
 * glue file stays focused on parsing/highlight/theme:
 *
 *   - **Comment tokens** — `languageData` advertising `//` (line) and `/* *​/`
 *     (block), so `Ctrl/Cmd+/` toggles comments via `toggleComment`.
 *   - **Autoclose** — bracket + quote pairing, but ONLY the pairs merula actually
 *     balances: `()`, `[]`, `""`. `'`, `<`, `{` are mini-notation / host operators
 *     (see `OPERATORS` in `merula-lang`), so autoclosing them would fight the user.
 *     With a selection active, typing an opener wraps the selection (open before,
 *     close after) — the standard `closeBrackets` behaviour.
 *   - **The IntelliJ editing keys** — delete-line (`Ctrl+Y`, `Cmd+⌫` on a Mac),
 *     duplicate, add-next-occurrence, move-line — from the shared
 *     {@link intellijEditingKeymap}, which also re-binds redo to `Ctrl/Cmd+Shift+Z`
 *     since delete-line takes the `Ctrl+Y` that was redo on Windows.
 *   - **Soft wrap** — long mini-notation phrases wrap instead of scrolling off.
 *   - **Code folding** — collapse multi-line `( … )` / `[ … ]` / `{ … }` blocks,
 *     driven by a self-contained bracket scanner (no Lezer tree needed; it skips
 *     strings + `//` / `/* *​/` comments).
 *
 * Everything is bundled by {@link merulaEditingExtensions}, dropped into the
 * editor's extension list AHEAD of the base keymap so the IntelliJ keys / `Mod-/`
 * win over the history and default bindings that claim the same chords.
 */

import { EditorView, keymap } from '@codemirror/view';
import { EditorState, type Extension } from '@codemirror/state';
import { toggleComment } from '@codemirror/commands';
import { closeBrackets, closeBracketsKeymap } from '@codemirror/autocomplete';
import { codeFolding, foldGutter, foldKeymap, foldService } from '@codemirror/language';

import { intellijEditingKeymap } from '$lib/components/shared/ui/code-editor/intellij-keymap';

// ── Language data (comment + autoclose config) ─────────────────────────────────

/** Advertise merula' comment syntax + the (restricted) autoclose pair set to the
 *  CodeMirror commands that read `languageData`. `'` / `<` / `{` are deliberately
 *  excluded from autoclose — they're operators, not balanced delimiters. */
const merulaLanguageData = EditorState.languageData.of(() => [{
  commentTokens: { line: '//', block: { open: '/*', close: '*/' } },
  closeBrackets: { brackets: ['(', '[', '"'] },
}]);

// ── Code folding (bracket scanner) ─────────────────────────────────────────────

/** Opening delimiter → its closing delimiter. `<` is excluded: it's the splice /
 *  comparison operator, rarely a multi-line block, and matching it would misfire. */
const FOLD_OPENERS: Record<string, string> = { '(': ')', '[': ']', '{': '}' };

/** Scan forward from an opening bracket at `from` to its matching close, skipping
 *  strings (`"…"`) and `//` / `/* *​/` comments. Returns the absolute offset of the
 *  matching close, or -1 if unbalanced within the scan window. */
function matchingClose(doc: EditorState['doc'], from: number): number {
  const open = doc.sliceString(from, from + 1);
  const close = FOLD_OPENERS[open];
  if (!close) return -1;
  // Bound the scan so a stray unbalanced bracket can't walk a huge buffer.
  const LIMIT = 50_000;
  const s = doc.sliceString(from, Math.min(doc.length, from + LIMIT));
  let depth = 0, inStr = false, inLine = false, inBlock = false;
  for (let i = 0; i < s.length; i++) {
    const c = s[i], n = s[i + 1];
    if (inLine) { if (c === '\n') inLine = false; continue; }
    if (inBlock) { if (c === '*' && n === '/') { inBlock = false; i++; } continue; }
    if (inStr) { if (c === '\\') { i++; continue; } if (c === '"') inStr = false; continue; }
    if (c === '/' && n === '/') { inLine = true; i++; continue; }
    if (c === '/' && n === '*') { inBlock = true; i++; continue; }
    if (c === '"') { inStr = true; continue; }
    if (c === open) depth++;
    else if (c === close) { depth--; if (depth === 0) return from + i; }
  }
  return -1;
}

/** Fold service: if a line opens a bracket block that closes on a later line,
 *  fold the inner region (`tracks( … )` collapses to `tracks( … )`). The first
 *  qualifying opener on the line wins, so the outermost block folds. */
const merulaFold = foldService.of((state, lineStart, lineEnd) => {
  const line = state.doc.sliceString(lineStart, lineEnd);
  let inStr = false, inBlock = false;
  for (let i = 0; i < line.length; i++) {
    const c = line[i], n = line[i + 1];
    if (inBlock) { if (c === '*' && n === '/') { inBlock = false; i++; } continue; }
    if (inStr) { if (c === '\\') { i++; continue; } if (c === '"') inStr = false; continue; }
    if (c === '/' && n === '/') break; // rest of the line is a comment
    if (c === '/' && n === '*') { inBlock = true; i++; continue; }
    if (c === '"') { inStr = true; continue; }
    if (FOLD_OPENERS[c]) {
      const close = matchingClose(state.doc, lineStart + i);
      if (close > lineEnd) return { from: lineStart + i + 1, to: close };
    }
  }
  return null;
});

// ── Bundle ─────────────────────────────────────────────────────────────────────

/** Editing ergonomics: comments, autoclose, the IntelliJ editing keys, soft wrap, folding.
 *  Returned as a single `Extension` so the editor factory can place it ahead of the base
 *  keymap (giving `Ctrl-y` / `Cmd-⌫` / `Mod-/` precedence over history/defaults). */
export function merulaEditingExtensions(): Extension {
  return [
    merulaLanguageData,
    EditorView.lineWrapping,
    closeBrackets(),
    codeFolding(),
    foldGutter(),
    merulaFold,
    keymap.of([
      { key: 'Mod-/', run: toggleComment },
      // Delete-line, duplicate, multi-cursor, move-line, redo — the same list the shared
      // code editor binds, so a key cannot mean one thing in a `.merula` buffer and another
      // in a `.java` one. It also brings the redo Windows had lost: delete-line takes
      // `Ctrl+Y`, which is the only redo `historyKeymap` gives that platform.
      ...intellijEditingKeymap(),
      ...closeBracketsKeymap,
      ...foldKeymap,
    ]),
  ];
}
