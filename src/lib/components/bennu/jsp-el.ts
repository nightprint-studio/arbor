/**
 * A tiny **EL / OGNL** tokenizer for JSP expression bodies (`${…}`, `#{…}`, `%{…}`).
 *
 * The tree-sitter-jsp grammar emits each expression as a SINGLE leaf token
 * (`el_expression` / `ognl_expression`), so without this the whole `${user.name == 'x'}`
 * paints one flat colour — an unreadable wall of purple. We plug this into the shared
 * code-editor's {@link LanguageDescriptor.injections} exactly like the JS/CSS injection
 * for `<script>`/`<style>` bodies: the highlighter runs it over the leaf's raw text and
 * emits per-token `cm-tok-*` marks.
 *
 * It's a CodeMirror legacy-mode {@link StreamParser} (the interface the injection host
 * drives). Token type strings map through the host's `mapStreamToken` onto our editor
 * classes, so identifiers, property accesses, strings, numbers, keywords, operators and
 * the `${`/`}` delimiters each get their own colour — no nested grammar, no new wasm.
 *
 * Scope note: this is highlight-only (no autocomplete), and deliberately lenient — EL and
 * OGNL differ in details (OGNL adds `#context` vars, `@Class@member` statics, `%{…}`),
 * but a single tolerant lexer covers both well enough to read. It never throws; the host
 * has a no-progress guard, but we also always consume at least one char.
 */

import type { StreamParser } from '@codemirror/language';

/** Per-region lexer state: whether the previous significant token was a `.` (so the next
 *  identifier is a property access, coloured as a field rather than a bare variable). */
interface ElState {
  afterDot: boolean;
}

/** OGNL / EL infix + unary word operators and constructors (lower-cased match). */
const KEYWORDS = new Set([
  'and', 'or', 'not', 'eq', 'ne', 'neq', 'lt', 'gt', 'le', 'ge', 'lte', 'gte',
  'div', 'mod', 'instanceof', 'in', 'new', 'empty',
]);

/** Literal constants — coloured as constants, not identifiers. */
const ATOMS = new Set(['true', 'false', 'null']);

/** The EL/OGNL expression tokenizer handed to the editor's injection mechanism. */
export const elOgnlStream: StreamParser<ElState> = {
  startState: () => ({ afterDot: false }),

  token(stream, state) {
    // Opening (`${` / `%{` / `#{`) and closing (`}`) delimiters — punctuation.
    if (stream.match(/^[$%#]\{/)) {
      state.afterDot = false;
      return 'bracket';
    }
    if (stream.match(/^\}/)) {
      state.afterDot = false;
      return 'bracket';
    }

    // Whitespace: keep `afterDot` so `user . name` still reads `name` as a property.
    if (stream.eatSpace()) return null;

    const dot = state.afterDot;
    state.afterDot = false;

    // String literal (`'…'` or `"…"`), backslash-escape aware.
    const quote = stream.peek();
    if (quote === '"' || quote === "'") {
      stream.next();
      let escaped = false;
      let c: string | void;
      while ((c = stream.next()) != null) {
        if (c === quote && !escaped) break;
        escaped = !escaped && c === '\\';
      }
      return 'string';
    }

    // Numbers (int / decimal).
    if (stream.match(/^\d+(?:\.\d+)?/)) return 'number';

    // OGNL context-variable marker `#` (as in `#session`, `#request`, `#parameters`, `#attr`).
    // Emit the `#` as its OWN token in a distinct colour (annotation) so the sigil stands out
    // from the variable name — the identifier rule colours the name next. (`#{ … }` deferred-EL
    // / map is a delimiter, already caught above.)
    if (stream.peek() === '#') {
      stream.next();
      return 'meta';
    }

    // OGNL static access marker `@com.x.Foo@bar` — the `@`s are operators; the dotted
    // class + member fall through to the identifier / dot rules.
    if (stream.match(/^@/)) return 'operator';

    // Property access dot — flags the next identifier as a property.
    if (stream.match(/^\./)) {
      state.afterDot = true;
      return 'operator';
    }

    // Identifier / keyword / constant.
    const id = stream.match(/^[A-Za-z_$][\w$]*/) as RegExpMatchArray | null;
    if (id) {
      const word = id[0];
      if (ATOMS.has(word)) return 'atom';
      if (KEYWORDS.has(word.toLowerCase())) return 'keyword';
      return dot ? 'property' : 'variable';
    }

    // Brackets / grouping.
    if (stream.match(/^[[\]()]/)) return 'bracket';
    if (stream.match(/^,/)) return 'punctuation';

    // Multi- and single-char operators.
    if (stream.match(/^(?:==|!=|<=|>=|&&|\|\||[-+*/%=<>!?:])/)) return 'operator';

    // Anything else: consume one char so the host never stalls.
    stream.next();
    return null;
  },
};
