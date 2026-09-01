/**
 * `.dig` comment highlighting — and why it cannot go through the grammar.
 *
 * ## The grammar never hands us a comment
 *
 * `grammar.js` does declare `comment` in `extras`, so the obvious wiring is
 * `LanguageDescriptor.injections` on a `comment` leaf. It does not work, and the reason is
 * one level below the grammar: geode's **external scanner** owns indentation, and to decide
 * a line's indent it has to treat a comment line as blank — so it consumes `#` to
 * end-of-line itself, while producing the newline/indent tokens. The parser never sees the
 * text, and the tree has no `comment` node in it. (Dumping the CST for
 * `# head\nlet x = 1  # tail\n` yields a `source_file` whose only child is the
 * `let_statement`: both comments are simply absent.)
 *
 * ⚠️ Which means `classify`'s `type === 'comment'` branch has **never** fired, and `.dig`
 * comments in Bennu have never been highlighted at all — they rendered as default text.
 * The tell is the italic: `.cm-tok-comment` is italic, and they were upright.
 *
 * geode's own editor hit this first and answered it the same way — a line pass, next to the
 * tree walk (`nd_lang_syntax::highlight::comment_spans`). This is that pass, with the same
 * rules and the same four classes, so a `.dig` reads the same in both.
 *
 * ## What a comment holds
 *
 * Three things, and two are not prose:
 *
 * 1. **The directive** in the first line — `@title:`, `@category:`, `@summary:`,
 *    `@requires:`. geode *reads* these: they index the example browser, feed the wiki, and
 *    `@requires` names the language features a sample needs. A comment a machine interprets
 *    and a comment that explains are different things.
 * 2. **Inline code** in backticks, naming the language itself.
 * 3. **Emphasis** in `**doubled asterisks**`, which carries the argument through a
 *    twenty-line doc block.
 *
 * Across geode's own scripts that is 19 directives, 126 backtick spans and 216 emphasised
 * ones — enough to decide whether a comment block gets read or skipped.
 *
 * ## The two rules worth stating
 *
 * The directive counts **only at the head** of the comment: an `@name:` further along is an
 * at-sign in a sentence. And ⚠️ **an unclosed marker is prose** — a lone backtick or a
 * dangling `**` colours nothing, because swallowing the rest of the line is the quickest way
 * to make a hastily written comment unreadable.
 */

import { Decoration, EditorView, ViewPlugin } from '@codemirror/view';
import type { DecorationSet, ViewUpdate } from '@codemirror/view';
import { RangeSetBuilder } from '@codemirror/state';

/** One coloured piece of a comment: offsets are **columns within its line**. */
export interface CommentToken {
  from: number;
  to: number;
  /** Rendered as `cm-tok-<cls>`; the theme styles all four. */
  cls: 'comment' | 'comment-directive' | 'comment-code' | 'comment-strong';
}

/** Where the comment starts on this line, or `-1`. A `#` inside a string does not open one
 *  (`print("a # b")`), which is why this tracks quotes and their escapes rather than using
 *  `indexOf`. Strings in `.dig` do not span lines, so one line is the whole context. */
export function commentStart(line: string): number {
  let inString = false;
  let escaped = false;
  for (let i = 0; i < line.length; i++) {
    const c = line[i];
    if (inString) {
      if (escaped) escaped = false;
      else if (c === '\\') escaped = true;
      else if (c === '"') inString = false;
    } else if (c === '"') {
      inString = true;
    } else if (c === '#') {
      return i;
    }
  }
  return -1;
}

/** The closing index of `marker … marker` opening at `s[at]`, marker included, or `-1` when
 *  it does not close on this line or the middle is empty (```` `` ```` and `****` are
 *  punctuation, not spans). */
function closes(s: string, at: number, marker: string): number {
  if (!s.startsWith(marker, at)) return -1;
  const close = s.indexOf(marker, at + marker.length);
  if (close < 0 || close === at + marker.length) return -1;
  return close + marker.length;
}

/** The head `@word:` length at `s[at]`, colon included, or `0`. The colon is part of it:
 *  `@title:` reads as one thing, and the word without it would read as a word. */
function directiveLength(s: string, at: number): number {
  if (s[at] !== '@') return 0;
  let i = at + 1;
  while (i < s.length && /[A-Za-z0-9_]/.test(s[i])) i++;
  return i > at + 1 && s[i] === ':' ? i + 1 - at : 0;
}

/**
 * Split a line's comment into **consecutive, non-overlapping** pieces. `[]` when the line
 * has no comment.
 *
 * Consecutive rather than "a base span plus overlays": overlapping ranges only work if
 * whoever paints them applies them in the right order, and that is a dependency on someone
 * else's implementation. These just work.
 */
export function digCommentTokens(line: string): CommentToken[] {
  const start = commentStart(line);
  if (start < 0) return [];

  const out: CommentToken[] = [];
  const push = (from: number, to: number, cls: CommentToken['cls']) => {
    if (to > from) out.push({ from, to, cls });
  };

  // The `#` and the blank after it: comment like the rest, but skipped over so a directive
  // can be its own piece.
  let i = start + 1;
  while (i < line.length && (line[i] === ' ' || line[i] === '\t')) i++;

  // ⚠️ `plain` starts at the `#`, not after it: the hash and its spaces are part of the
  // comment, and starting after them dropped them from the output entirely — a comment
  // without its hash. geode's port of this had the same bug, caught by an older test.
  let plain = start;

  const len = directiveLength(line, i);
  if (len > 0) {
    push(plain, i, 'comment');
    push(i, i + len, 'comment-directive');
    i += len;
    plain = i;
  }

  while (i < line.length) {
    let end = closes(line, i, '`');
    let cls: CommentToken['cls'] = 'comment-code';
    if (end < 0) {
      end = closes(line, i, '**');
      cls = 'comment-strong';
    }
    if (end < 0) {
      i++;
      continue;
    }
    push(plain, i, 'comment');
    push(i, end, cls);
    i = end;
    plain = i;
  }
  push(plain, line.length, 'comment');
  return out;
}

// ── The extension ──────────────────────────────────────────────────────────────

const MARKS = new Map<string, Decoration>();
function mark(cls: string): Decoration {
  let m = MARKS.get(cls);
  if (!m) {
    m = Decoration.mark({ class: `cm-tok-${cls}` });
    MARKS.set(cls, m);
  }
  return m;
}

/** Decorations for the comments in the **visible** lines. Viewport-scoped because a comment
 *  is a line-local fact: there is nothing to carry across, and a thousand-line file would
 *  otherwise be rescanned in full on every keystroke. */
function build(view: EditorView): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  const doc = view.state.doc;
  for (const { from, to } of view.visibleRanges) {
    let pos = from;
    while (pos <= to) {
      const line = doc.lineAt(pos);
      for (const tok of digCommentTokens(line.text)) {
        builder.add(line.from + tok.from, line.from + tok.to, mark(tok.cls));
      }
      if (line.to >= doc.length) break;
      pos = line.to + 1;
    }
  }
  return builder.finish();
}

/** The `.dig` comment layer, for {@link LanguageDescriptor.extraHighlight}. */
export const digCommentHighlight = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;

    constructor(view: EditorView) {
      this.decorations = build(view);
    }

    update(u: ViewUpdate) {
      if (u.docChanged || u.viewportChanged) this.decorations = build(u.view);
    }
  },
  { decorations: (v) => v.decorations },
);
