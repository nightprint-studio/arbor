/**
 * Postfix completion — write the expression first, then say what to do with it.
 *
 * `order.getTotal().nn` becomes `if (order.getTotal() != null) { … }`, with the caret in the body.
 * The point is not the keystrokes saved; it is that you get to think in the order you actually
 * think — the value, then the control flow around it — instead of committing to an `if (` before
 * you know what goes in it, and then arrowing back out to close it.
 *
 * ## What is shared and what is not
 *
 * Everything mechanical is here: finding the expression the dot was written on, matching what you
 * typed against the available templates, and replacing the whole expression with the expansion as
 * one undo step. What each template *means* is per language, and arrives as a
 * {@link PostfixTemplate} list — so a language gets postfix by supplying a table, not by
 * reimplementing any of this.
 *
 * A language served by a language server usually needs no table at all: rust-analyzer, for one,
 * ships its own postfix completions and they arrive through the ordinary LSP completion source.
 * This exists for the languages Arbor is itself the engine for.
 *
 * ## Caret stops
 *
 * A template marks where the caret should land with `$|`. Several marks make a tab-through, using
 * the same {@link import('./snippet-stops').insertWithStops} machinery a server snippet does — so
 * `.for` can drop you on the loop variable with Tab waiting on the body. No mark at all leaves the
 * caret at the end of the expansion.
 *
 * ## Why it is a completion source rather than a keymap
 *
 * Because it *is* completion: you type a dot and a name, you see a list, and the list is filtered as
 * you type. Making it a command with its own trigger would mean discovering it from documentation
 * rather than from the editor.
 */

import { type Completion, type CompletionContext, type CompletionResult } from '@codemirror/autocomplete';
import { getIndentUnit } from '@codemirror/language';
import type { EditorView } from '@codemirror/view';

import { insertWithStops } from './snippet-stops';
import { boostForRank, TEMPLATE, type RankBand } from './completion-rank';

/** The mark a template puts where the caret should land. Two or more make a tab-through. */
export const CARET = '$|';

/** One postfix template: `<expr>.<name>` → something built out of `<expr>`. */
export interface PostfixTemplate {
  /** What you type after the dot — `if`, `nn`, `sout`. */
  name: string;
  /** The one-line description shown beside the name in the list. */
  detail: string;
  /**
   * Build the expansion.
   *
   * `expr` is the source text of the expression the dot was written on, verbatim. `indent` is the
   * indentation of the line it starts on, and `unit` one further level — templates that open a
   * block need both, and computing them per template would be the same three lines each time.
   *
   * Mark caret positions with {@link CARET}.
   */
  expand(expr: string, indent: string, unit: string): string;
}

/** How postfix items rank against everything else the completion list is offering. */
export interface PostfixOptions {
  /**
   * The band postfix items are boosted within. Defaults to {@link TEMPLATE} — below every resolved
   * member, above the guesses — which is the ordering the band exists to express.
   */
  band?: RankBand;
}

/** Characters that continue an expression when scanning backwards through a bare identifier. */
const IDENT = /[A-Za-z0-9_$]/;

/**
 * The start offset of the expression ending just before `dot`, or `null` when there isn't one.
 *
 * Scans backwards balancing `()` / `[]` and skipping over string and character literals, so
 * `repo.find(a, b[i]).nn` takes the whole call and `"a.b".sout` takes the whole literal. It stops at
 * the first character that cannot be part of a postfix subject — an operator, a separator, a brace —
 * which is what keeps `x = y.nn` from swallowing the `x =`.
 *
 * Deliberately syntactic rather than tree-based: the buffer is mid-edit by definition here (the
 * template name is not valid in any grammar yet), so a parse of it is a parse of something broken.
 */
export function expressionStart(doc: string, dot: number): number | null {
  let i = dot - 1;
  let end = dot; // one past the last character accepted so far
  while (i >= 0) {
    const ch = doc[i];
    if (ch === ')' || ch === ']') {
      const open = matchBackwards(doc, i);
      if (open === null) return null;
      i = open - 1;
      end = open;
      continue;
    }
    if (ch === '"' || ch === "'") {
      const open = literalStart(doc, i);
      if (open === null) return null;
      i = open - 1;
      end = open;
      continue;
    }
    if (IDENT.test(ch) || ch === '.') {
      i -= 1;
      end = i + 1;
      continue;
    }
    break;
  }
  if (end >= dot) return null; // nothing before the dot
  // A leading `.` would mean the chain starts with one — not an expression.
  return doc[end] === '.' ? null : end;
}

/** The offset of the bracket matching the closer at `close`, or `null` if unbalanced. */
function matchBackwards(doc: string, close: number): number | null {
  const opener = doc[close] === ')' ? '(' : '[';
  const closer = doc[close];
  let depth = 0;
  for (let i = close; i >= 0; i--) {
    const ch = doc[i];
    if (ch === '"' || ch === "'") {
      const open = literalStart(doc, i);
      if (open === null) return null;
      i = open;
      continue;
    }
    if (ch === closer) depth += 1;
    else if (ch === opener) {
      depth -= 1;
      if (depth === 0) return i;
    }
  }
  return null;
}

/** The offset of the quote opening the literal that ends at `close`, or `null` if unterminated. */
function literalStart(doc: string, close: number): number | null {
  const quote = doc[close];
  for (let i = close - 1; i >= 0; i--) {
    if (doc[i] !== quote) continue;
    // Count the backslashes immediately before it: an odd number means this quote is escaped.
    let slashes = 0;
    while (i - 1 - slashes >= 0 && doc[i - 1 - slashes] === '\\') slashes += 1;
    if (slashes % 2 === 0) return i;
  }
  return null;
}

/** Strip the {@link CARET} marks out of `text`, returning the plain text and where they were. */
export function extractStops(text: string): { text: string; stops: { start: number; end: number }[] } {
  const stops: { start: number; end: number }[] = [];
  let out = '';
  let i = 0;
  while (i < text.length) {
    if (text.startsWith(CARET, i)) {
      stops.push({ start: out.length, end: out.length });
      i += CARET.length;
      continue;
    }
    out += text[i];
    i += 1;
  }
  return { text: out, stops };
}

/**
 * A CodeMirror completion source offering `templates` after a dot.
 *
 * The completion range starts *after* the dot, so CodeMirror filters on the template name alone —
 * matching it against the whole expression would rank every template last. Replacing the expression
 * is the template's own job, in `apply`.
 */
export function postfixCompletion(
  templates: readonly PostfixTemplate[],
  options: PostfixOptions = {},
): (ctx: CompletionContext) => CompletionResult | null {
  return (ctx: CompletionContext): CompletionResult | null => {
    const typed = ctx.matchBefore(/\.[A-Za-z]*/);
    if (!typed) return null;
    // An explicit Ctrl+Space on a bare dot should list them; an implicit trigger on one should not
    // (you have just typed `order.` and want members, not a wall of templates).
    const prefix = typed.text.slice(1);
    if (!prefix && !ctx.explicit) return null;

    const doc = ctx.state.doc.toString();
    const dot = typed.from;
    const start = expressionStart(doc, dot);
    if (start === null) return null;
    const expr = doc.slice(start, dot);

    const line = ctx.state.doc.lineAt(start);
    const indent = /^[ \t]*/.exec(line.text)?.[0] ?? '';
    const unit = ' '.repeat(getIndentUnit(ctx.state));

    const matching = templates.filter((t) => t.name.startsWith(prefix));
    if (matching.length === 0) return null;

    const band = options.band ?? TEMPLATE;
    const items: Completion[] = matching.map((t, rank) => ({
      label: t.name,
      detail: t.detail,
      type: 'keyword',
      boost: boostForRank(rank, band),
      apply: (view: EditorView, _c: Completion, _from: number, to: number) => {
        const { text, stops } = extractStops(t.expand(expr, indent, unit));
        // Byte offsets and UTF-16 offsets coincide here: the stops were counted in the very string
        // being inserted, so the identity converter is the correct one rather than a shortcut.
        insertWithStops(view, start, to, text, stops, (n) => n);
      },
    }));

    return { from: dot + 1, options: items, validFor: /^[A-Za-z]*$/ };
  };
}
