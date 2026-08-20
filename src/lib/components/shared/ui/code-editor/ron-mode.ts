/**
 * RON (Rusty Object Notation) highlighting — the CodeMirror stream mode, and nothing else.
 *
 * ## Why not just use the Rust mode
 *
 * Because it is wrong in the one place a RON file is read. RON borrows Rust's *syntax* but
 * none of its vocabulary: a `.ron` file has no `fn`, no `let`, no `impl` — but it very
 * plausibly has fields called `type:`, `mod:`, `ref:`, `box:` or `move:`, and the Rust mode
 * paints every one of them as a keyword. Worse, it has nothing to say about the thing RON is
 * mostly made of — **field names** — so a config file comes out as an undifferentiated run
 * of identifiers, which is exactly the structure you were looking for.
 *
 * So the two distinctions this mode exists to draw are:
 *
 *   * a **field name** (`name:`) apart from a value, because a RON file is read by scanning
 *     the left-hand column;
 *   * a **struct or variant name** (`Name(`) apart from a bare identifier, because that is
 *     what tells you which shape you are inside.
 *
 * ## Lenient by construction
 *
 * A `.ron` is opened *because* somebody wants to read it, frequently one a generator wrote
 * and nobody has validated. An unparseable byte is consumed one character at a time rather
 * than turning the rest of the file into one long error.
 *
 * Token names are the legacy-mode vocabulary the editor's injection host already maps onto
 * its own classes; nothing here needs to know what colour anything ends up.
 */

import { StreamLanguage, type StreamParser } from '@codemirror/language';
import type { Extension } from '@codemirror/state';

/** The whole of RON's reserved vocabulary. It really is this short — everything else in a
 *  RON file is a name somebody chose. */
const ATOMS = new Set(['true', 'false', 'None', 'Some', 'inf', 'nan']);

interface RonState {
  /** Depth of the `/* … *\/` nesting. RON, like Rust, nests block comments — and a config
   *  file with a commented-out block containing a commented-out block is exactly how you
   *  meet that. Counted rather than flagged, or the inner `*\/` ends both. */
  comment: number;
}

export const ronMode: StreamParser<RonState> = {
  name: 'ron',

  startState: () => ({ comment: 0 }),

  token(stream, state) {
    if (state.comment > 0) {
      while (!stream.eol()) {
        if (stream.match(/^\/\*/)) { state.comment += 1; continue; }
        if (stream.match(/^\*\//)) {
          state.comment -= 1;
          if (state.comment === 0) break;
          continue;
        }
        stream.next();
      }
      return 'comment';
    }
    if (stream.eatSpace()) return null;

    if (stream.match(/^\/\//)) { stream.skipToEnd(); return 'comment'; }
    if (stream.match(/^\/\*/)) { state.comment = 1; return 'comment'; }

    // `#![enable(implicit_some)]` — the extension header, which changes how the rest of the
    // file is read and so should not look like the data below it.
    if (stream.match(/^#!?\[[^\]]*\]?/)) return 'meta';

    // Raw strings first: `r#"…"#` may contain a bare `"` and the plain rule would stop there.
    if (stream.match(/^r(#*)"/)) {
      const hashes = (stream.current().match(/#/g) ?? []).length;
      const close = new RegExp(`^[\\s\\S]*?"#{${hashes}}`);
      if (!stream.match(close)) stream.skipToEnd();
      return 'string';
    }
    if (stream.match(/^"(?:[^"\\]|\\.)*"?/)) return 'string';
    // A char literal, and the byte forms RON inherits.
    if (stream.match(/^'(?:[^'\\]|\\.)'/)) return 'string-2';

    // Numbers: decimal, hex, octal, binary, with the underscores and the sign RON allows.
    if (stream.match(/^[+-]?0[xX][0-9a-fA-F_]+/)) return 'number';
    if (stream.match(/^[+-]?0[oO][0-7_]+/)) return 'number';
    if (stream.match(/^[+-]?0[bB][01_]+/)) return 'number';
    if (stream.match(/^[+-]?\d[\d_]*(\.\d[\d_]*)?([eE][+-]?\d+)?/)) return 'number';

    if (stream.match(/^[()[\]{}]/)) return 'bracket';
    if (stream.match(/^[:,]/)) return 'punctuation';

    const word = stream.match(/^[A-Za-z_][A-Za-z0-9_]*/) as RegExpMatchArray | null;
    if (word) {
      if (ATOMS.has(word[0])) return 'atom';
      // What comes NEXT decides what this was. `peek`-based rather than state-based because
      // RON is not line-oriented: `name:` and `Name(` can both appear anywhere, and a mode
      // that remembered "we are in a struct" would have to track a stack to be right about
      // where it ends.
      const after = stream.string.slice(stream.pos);
      // A field name: an identifier whose next non-space character is `:`. This is the token
      // the whole mode is for.
      if (/^\s*:/.test(after)) return 'property';
      // A struct, an enum variant or a tuple-struct: an identifier applied to something.
      //
      // `type`, not the CM5 name `variable-3` this used to return. CodeMirror 6's legacy
      // token table has no entry for `variable-3` and no tag of that name, so it resolved to
      // nothing at all — which is why a RON file rendered as a wall of white: every
      // constructor in it, which is most of the words on the page, was unstyled.
      if (/^\s*[([]/.test(after)) return 'type';
      // A bare capitalised identifier is a **unit variant** (`blend: Additive`). Nothing
      // follows it to give it away, but in RON a capitalised name is a type name — there are
      // no variables to confuse it with.
      if (/^[A-Z]/.test(word[0])) return 'type';
      return 'variable';
    }

    // Anything unrecognised: one character, so the host never stalls on a malformed file.
    stream.next();
    return null;
  },

  languageData: {
    commentTokens: { line: '//', block: { open: '/*', close: '*/' } },
    closeBrackets: { brackets: ['(', '[', '{', '"'] },
    indentOnInput: /^\s*[)\]}]$/,
  },
};

/** The RON language extension, allocated once — a fresh `StreamLanguage` per mount would
 *  reconfigure the editor for nothing. */
export const ronLanguageExtension: Extension = StreamLanguage.define(ronMode);
