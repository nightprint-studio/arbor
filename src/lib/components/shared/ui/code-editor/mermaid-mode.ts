/**
 * Mermaid highlighting — the CodeMirror stream mode.
 *
 * Two places read mermaid source: a `.mmd` file opened as itself, and a ```mermaid fence with the
 * caret in it (the block renders as a picture until you go to edit it, and then it is code again).
 * The fence is coloured by Prism, which ships a mermaid grammar; this is the other half, for the
 * editor's own buffers.
 *
 * ## What a mermaid file is actually made of
 *
 * Almost none of it is vocabulary. A flowchart is a handful of reserved words — `flowchart`,
 * `subgraph`, `end` — and then a page of **node ids, arrows and labels**, which is why colouring
 * only the keywords leaves the file looking untouched. So the three distinctions this mode draws
 * are the three a reader is actually scanning for:
 *
 *   * the **arrows** (`-->`, `-.->`, `==>`, `--x`, `<-->`), because they are the edges and the
 *     direction is the meaning;
 *   * the **labels** — a bracketed `[…]`, `(…)`, `{…}` node text, and the `|…|` on an edge —
 *     because that is the prose, and it reads as prose rather than as identifiers;
 *   * the **diagram type on the first line**, because it decides what every line under it means.
 *
 * ## Lenient by construction
 *
 * Mermaid's dialects differ per diagram type (a `sequenceDiagram` has `participant` and `loop`, a
 * `gantt` has `section` and dates) and this mode does not switch on the header: it recognises the
 * union of the reserved words and treats everything else as a name. Being approximate is the right
 * trade for a highlighter — an unknown word coloured as a name is invisible, a mode that guessed
 * the diagram type wrong would mis-colour every line after it.
 *
 * Token names are the legacy-mode vocabulary the highlight host already maps onto its classes.
 */

import { StreamLanguage, type StreamParser } from '@codemirror/language';
import type { Extension } from '@codemirror/state';

/** The word that opens a diagram — the one line whose meaning is structural. */
const DIAGRAMS = new Set([
  'graph', 'flowchart', 'sequenceDiagram', 'classDiagram', 'stateDiagram',
  'erDiagram', 'journey', 'gantt', 'pie', 'gitGraph', 'mindmap', 'timeline', 'quadrantChart',
  'requirementDiagram', 'C4Context', 'C4Container', 'C4Component', 'C4Dynamic', 'C4Deployment',
  'zenuml',
]);

/** Everything else mermaid reserves, across its dialects. One set rather than one per diagram:
 *  see the note about not switching on the header. */
const KEYWORDS = new Set([
  // structure
  'subgraph', 'end', 'direction', 'namespace',
  // sequence
  'participant', 'actor', 'activate', 'deactivate', 'loop', 'alt', 'else', 'opt', 'par', 'and',
  'critical', 'option', 'break', 'rect', 'autonumber', 'box',
  // class / state / er
  'class', 'state', 'note', 'link', 'callback', 'call', 'href',
  // gantt / timeline / journey
  'title', 'section', 'dateFormat', 'axisFormat', 'excludes', 'includes', 'todayMarker',
  'tickInterval', 'weekday', 'milestone',
  // styling + interaction
  'style', 'classDef', 'linkStyle', 'click', 'accTitle', 'accDescr', 'default',
  // pie / quadrant / xy
  'showData',
]);

/**
 * Every arrow, longest first — and the order is the whole of the correctness here.
 *
 * `->>` must not be cut down to `->` (a sequence message would lose its head, and the `>` left
 * behind opens a node shape that eats the rest of the line); `<-->` must not lose its head to
 * `<--`; and `.->`, the closing half of a labelled dotted link (`A -. text .-> B`), has to be an
 * arrow rather than a dot followed by a `>`.
 */
const ARROW =
  /^(?:<<-{1,2}>>|<<-{1,2}|-{1,2}>>|<-{2,3}>|<\|?-{2,3}\|?>|x-{2,3}x|o-{2,3}o|-{1,3}[>xo)]|-\.{1,3}-?[>xo]?|\.{1,3}-+[>xo]?|={2,3}[>xo]?|-{2,3}|\.{2,3}>|~{3}|<\||\|>|\*--|--\*)/;

/** The reserved words with a hyphen in them. Matched whole, because the identifier rule cannot
 *  accept a hyphen without stealing the arrows'. */
const HYPHENATED =
  /^(?:stateDiagram-v2|sankey-beta|xychart-beta|block-beta|packet-beta|architecture-beta|quadrant-[1-4]|[xy]-axis)\b/;

/** Directions, which are values rather than statements (`flowchart LR`). */
const ATOMS = new Set(['TB', 'TD', 'BT', 'RL', 'LR', 'v', '^']);

interface MermaidState {
  /** Inside a `%%{ … }%%` directive, which spans lines and is configuration rather than diagram. */
  directive: boolean;
  /** The bracket that opened the label being read, or `''`. A label is free text: `[Order (paid)]`
   *  is one label and not a nested anything, so it is consumed to its closer rather than
   *  tokenised. */
  label: string;
  /** Nothing has been seen yet on this line — how the diagram header is told from a node called
   *  `graph` further down. */
  lineStart: boolean;
}

/** The closer that ends a label opened by `open`. */
const CLOSERS: Record<string, string> = { '[': ']', '(': ')', '{': '}', '|': '|', '>': ']' };

export const mermaidMode: StreamParser<MermaidState> = {
  name: 'mermaid',

  startState: () => ({ directive: false, label: '', lineStart: true }),

  token(stream, state) {
    if (stream.sol()) {
      state.lineStart = true;
      // ⚠️ A label never survives a newline. Without this, one unterminated `[` — which is what
      // a half-typed node is — turns every line below it into label text, and the file goes
      // grey from the caret down while you are still typing the word.
      state.label = '';
    }

    if (state.directive) {
      if (stream.match(/^[\s\S]*?}%%/)) state.directive = false;
      else stream.skipToEnd();
      return 'meta';
    }

    if (state.label) {
      const close = CLOSERS[state.label] ?? ']';
      // ⚠️ The closer is CONSUMED here, and that is the whole reason this branch owns it: `|` is
      // both the opener and the closer of an edge label, so a closer left on the stream is read
      // as the start of the next label and the rest of the line disappears into it.
      if (stream.peek() === close) {
        stream.next();
        // `]]`, `))`, `}}`, `)]` — the second half of a shaped node's closer belongs to the shape.
        stream.match(/^[\])}]/);
        state.label = '';
        return 'bracket';
      }
      // The inside of a label is prose, and mermaid lets it contain the brackets and arrows that
      // would otherwise re-enter this tokenizer.
      while (!stream.eol() && stream.peek() !== close) stream.next();
      return 'string';
    }

    if (stream.eatSpace()) return null;

    // `%%{ init: … }%%` is a directive; a bare `%%` is a comment to end of line.
    if (stream.match(/^%%\{/)) { state.directive = true; return 'meta'; }
    if (stream.match(/^%%/)) { stream.skipToEnd(); return 'comment'; }

    if (stream.match(/^"(?:[^"\\]|\\.)*"?/)) return 'string';

    // The arrows, longest first — `-.->` must not be read as `-` then `.` then `->`, and `<-->`
    // must not lose its head to `<--`.
    //
    // ⚠️ The `\.-+` alternative is the second half of a **labelled dotted link**: mermaid writes
    // one as `A -. text .-> B`, so the closing piece begins with a dot and would otherwise fall
    // through to the `>` rule below — which is a node shape (`A>text]`), and the rest of the line
    // would be eaten as that node's label.
    if (stream.match(ARROW)) return 'operator';
    // `:::className` attaches a class to a node — an operator followed by a name, and the name is
    // the interesting half.
    if (stream.match(/^:::/)) return 'operator';

    // A label opens here and everything to its closer is text.
    const open = stream.match(/^(\[|\(|\{|\||>)/) as RegExpMatchArray | null;
    if (open) {
      // `[[`, `((`, `{{`, `([`, `[(` — the shaped nodes. The extra bracket belongs to the shape,
      // not to the text, so it is eaten with the opener.
      stream.match(/^[[({]/);
      state.label = open[0];
      state.lineStart = false;
      return 'bracket';
    }
    if (stream.match(/^[\])}]/)) { state.lineStart = false; return 'bracket'; }

    if (stream.match(/^[:;,&]/)) { state.lineStart = false; return 'punctuation'; }
    if (stream.match(/^\d+(\.\d+)?/)) { state.lineStart = false; return 'number'; }

    // The reserved words that contain a hyphen, before the identifier rule — which does NOT
    // accept one, because in `Alice->>John` the hyphen is the arrow's and an identifier that
    // swallowed it would take the arrow's head with it.
    if (stream.match(HYPHENATED)) { state.lineStart = false; return 'keyword'; }

    const word = stream.match(/^[A-Za-z_]\w*/) as RegExpMatchArray | null;
    if (word) {
      const wasLineStart = state.lineStart;
      state.lineStart = false;
      // The diagram type only counts as one when it opens a line. Further down, `graph` is
      // somebody's node.
      if (wasLineStart && DIAGRAMS.has(word[0])) return 'keyword';
      if (KEYWORDS.has(word[0])) return 'keyword';
      if (ATOMS.has(word[0])) return 'atom';
      return 'variable';
    }

    stream.next();
    state.lineStart = false;
    return null;
  },

  languageData: {
    commentTokens: { line: '%%' },
    closeBrackets: { brackets: ['[', '(', '{', '"'] },
  },
};

/** The mermaid language extension, allocated once — a fresh `StreamLanguage` per mount would
 *  reconfigure the editor for nothing. */
export const mermaidLanguageExtension: Extension = StreamLanguage.define(mermaidMode);
