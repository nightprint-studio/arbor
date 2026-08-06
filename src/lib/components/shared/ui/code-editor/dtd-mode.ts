/**
 * DTD highlighting — the CodeMirror stream mode, and nothing else.
 *
 * Lives in `code-editor/` for the same reason `sql-modes.ts` does: it is language
 * data with no Arbor concept in it. CodeMirror ships no DTD mode and none of the
 * neighbouring ones fit — XML reads `<!ELEMENT` as a malformed tag, so a real DTD
 * comes out mostly one flat colour with the odd stretch of red.
 *
 * A DTD is small enough to lex properly rather than approximate: four declaration
 * forms, one reference form, and a content model made of punctuation. What that buys
 * over "colour the quotes and hope" is the two distinctions that carry the meaning —
 * the **declared name** apart from the words around it (so the file reads as a list
 * of definitions, which is what it is), and the **parameter-entity references** apart
 * from everything else, since a real DTD is written almost entirely in them and they
 * are the reason a declaration says less than it appears to.
 *
 * Token names are the legacy-mode vocabulary the editor's injection host already maps
 * onto its own classes; nothing here needs to know what colour anything ends up.
 */

import { StreamLanguage, type StreamParser } from '@codemirror/language';
import type { Extension } from '@codemirror/state';

/** The four declaration keywords, without their `<!`. */
const DECLARATIONS = new Set(['ELEMENT', 'ATTLIST', 'ENTITY', 'NOTATION']);

/** Attribute types (`<!ATTLIST el id ID #REQUIRED>`) and the two content-model constants. */
const TYPES = new Set([
  'CDATA', 'ID', 'IDREF', 'IDREFS', 'NMTOKEN', 'NMTOKENS', 'ENTITY', 'ENTITIES',
  'NOTATION', 'EMPTY', 'ANY', 'SYSTEM', 'PUBLIC',
]);

/** What the lexer is in the middle of, which is all the state a DTD needs. */
interface DtdState {
  /** Inside a `<!-- … -->`, which is the only construct that spans lines by design. */
  comment: boolean;
  /** The declaration being read (`ELEMENT`, `ATTLIST`, …), or null between declarations. */
  decl: string | null;
  /** How many bare names have been seen in this declaration. The first is what is being
   *  declared; in an `<!ATTLIST` the ones after it are attribute names. */
  names: number;
}

/**
 * The DTD tokenizer.
 *
 * Deliberately lenient — a DTD is opened *because* an editor wants to read it, and a
 * declaration it cannot parse is skipped a character at a time rather than turning the
 * rest of the file into one long error.
 */
export const dtdMode: StreamParser<DtdState> = {
  name: 'dtd',

  startState: () => ({ comment: false, decl: null, names: 0 }),

  token(stream, state) {
    if (state.comment) {
      while (!stream.eol()) {
        if (stream.match(/^-->/)) {
          state.comment = false;
          break;
        }
        stream.next();
      }
      return 'comment';
    }
    if (stream.eatSpace()) return null;

    if (stream.match(/^<!--/)) {
      state.comment = true;
      return 'comment';
    }
    // `<?xml version="1.0"?>` and the conditional-section brackets `<![INCLUDE[` / `]]>`.
    if (stream.match(/^<\?[^?]*\?>/) || stream.match(/^<!\[|^\]\]>/)) return 'meta';

    // `<!ELEMENT` / `<!ATTLIST` / `<!ENTITY` / `<!NOTATION` — one token with its `<!`, so the
    // declaration reads as a single word rather than as punctuation followed by a name.
    const decl = stream.match(/^<!([A-Z]+)/) as RegExpMatchArray | null;
    if (decl) {
      const word = decl[1];
      state.decl = DECLARATIONS.has(word) ? word : null;
      state.names = 0;
      return state.decl ? 'keyword' : 'meta';
    }
    if (stream.match(/^>/)) {
      state.decl = null;
      state.names = 0;
      return 'bracket';
    }

    // A parameter-entity reference (`%common;`) or a general one (`&lt;`). Highlighted apart
    // from the names around it because it stands for text that is not here.
    if (stream.match(/^[%&][A-Za-z_:][\w.:-]*;?/)) return 'meta';
    // A lone `%` introducing a parameter-entity DECLARATION (`<!ENTITY % name "…">`).
    if (stream.match(/^%/)) return 'operator';

    if (stream.match(/^"[^"]*"?/) || stream.match(/^'[^']*'?/)) return 'string';

    // `#PCDATA`, `#REQUIRED`, `#IMPLIED`, `#FIXED`.
    if (stream.match(/^#[A-Za-z]+/)) return 'atom';

    // The content model: grouping, alternation, sequence, and the occurrence indicators.
    if (stream.match(/^[()]/)) return 'bracket';
    if (stream.match(/^[|,?*+]/)) return 'operator';

    const name = stream.match(/^[A-Za-z_:][\w.:-]*/) as RegExpMatchArray | null;
    if (name) {
      if (TYPES.has(name[0])) return 'variable-3';
      state.names += 1;
      // The first name in a declaration is the thing being declared; in an `<!ATTLIST` the
      // rest are attribute names, and everything else is an element name in a content model.
      if (state.decl && state.names === 1) return 'def';
      if (state.decl === 'ATTLIST') return 'attribute';
      return 'variable';
    }

    // Anything unrecognised: consume one character so the host never stalls.
    stream.next();
    return null;
  },
};

/** The DTD language extension, allocated once — a fresh `StreamLanguage` per mount would
 *  reconfigure the editor for nothing. */
export const dtdLanguage: Extension = StreamLanguage.define(dtdMode);
