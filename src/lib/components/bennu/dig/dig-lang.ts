/**
 * Bennu ↔ tree-sitter-geode bridge: the `.dig` {@link LanguageDescriptor}.
 *
 * `.dig` is the scripting language of **geode** (`/games/geode`) — Python-shaped,
 * indentation-delimited, with a closed set of host builtins. Its grammar is
 * `crates/lang/nd-lang-syntax/grammar.js` over there, and the `.wasm` this loads is the
 * same one geode's own `nd-lang-syntax` compiles from it (its crate doc names the Arbor
 * editor as the second consumer). Copied to `static/bennu/tree-sitter-geode.wasm`.
 *
 * ⚠️ The copy is a **snapshot**: a `grammar.js` change over there needs a re-copy here,
 * or new syntax parses as `ERROR` and loses its colour. That is the cost of a vendored
 * grammar and it is the same deal Java and JSP already have. If either `.wasm` is
 * missing the parser factory rejects and the editor stays plain text — graceful, no
 * crash.
 *
 * ## Highlighting mirrors geode's own classifier
 *
 * The token classes follow `nd-lang-syntax/src/highlight.rs` so a `.dig` file reads the
 * same in Bennu as in the game's editor, including the two rules that are not obvious:
 *
 * - **An initial capital is not enough to make a type.** `Tool` and `East` both start with
 *   one, but the first is a namespace and the second is a value — the same value `Tool.Pick`
 *   is, written out. What decides is the **position**, which the grammar already names
 *   (see {@link isTypePosition}): a struct name, a parameter's or a **field's** annotation, a
 *   return type, anything inside a `Fn(Int) -> Bool` signature, the object of a dotted access,
 *   a called constructor, a pattern's constructor. Everything
 *   else capitalised is a symbol, i.e. a value — which is also the compiler's rule, where an
 *   unresolved bare PascalCase becomes a `Sym`.
 * - **A single-token rule carries the rule's name, not the word.** `pass_statement: _ =>
 *   'pass'` makes tree-sitter absorb the anonymous child, so the leaf's type is
 *   `pass_statement`. geode hit this and solved it by falling back to the leaf's *text*
 *   against the keyword list; the same fallback is here, so `pass` / `continue` / `break`
 *   are coloured and a future rule of the same shape is born coloured.
 *
 * ## ⚠️ Comments do not come from the tree at all
 *
 * The scanner that owns indentation consumes `#` lines while producing its newline/indent
 * tokens — it has to, since a comment line must not change a block's indent — so the parser
 * never sees them and there is no `comment` node. `classify`'s comment branch therefore never
 * fired, and `.dig` comments were rendering as plain default text.
 *
 * They are highlighted by a line pass instead ({@link import('./dig-comments').digCommentHighlight},
 * wired through `extraHighlight`), which is the same answer geode's own editor gives — with
 * the same rules and the same four classes, so a file reads the same in both.
 *
 * ## Folding is header-driven, because the blocks are not braced
 *
 * The shared fold service looks for the smallest node that *starts on the queried line*
 * and ends later. In an indentation language the `block` node starts on the **next** line
 * (at the first statement), so it is never that node — the fold owner is the compound
 * statement itself (`fn_definition`, `if_statement`, …), and the range runs from just
 * after the `:` that ends its header to the end of its body.
 *
 * ## Intelligence comes from the language server
 *
 * Completion and hover go through the shared `eagerBackendCompletionSource` /
 * `backendHoverSource`, which the backend answers with whichever engine owns the file —
 * for a `.dig` that is **nd-dig-lsp**, geode's own server (`crates/tools/nd-dig-lsp`
 * over there), wrapping the same analysis service the game's in-game editor calls.
 *
 * It used to be `dig-intel.ts`: 255 lines that resolved completion and hover locally
 * from the generated catalog. That was the right call while `.dig` had no server, and it
 * carried the limits its own doc admitted — no type inference, every `let` in the file
 * offered instead of the ones in scope, `import` lines not completed at all. The server
 * holds the AST, so it answers all three properly, and it knows things Bennu could not
 * invent: which crystals a content pack adds, which functions take how many arguments,
 * what a declared parameter type opens up after a `.`.
 *
 * ⚠️ The trade is **graceful silence**, not a fallback: with no server installed the two
 * hooks answer nothing, where the local path always answered something. That is the same
 * deal every server-backed language here already takes, and it is the honest one — an
 * answer from a stale local copy of geode's vocabulary is worse than no answer. (It is
 * silence and not noise: `IndexService::completion` gates on `understands(file)`, so a
 * `.dig` never falls through to the Java index.)
 *
 * Highlighting and folding stay local and stay here. They come from the grammar, cost no
 * round-trip, and work while the server is down or absent — trading them for semantic
 * tokens would mean a file that opens grey and colours a second later.
 */

import { Parser, Language, type Node } from 'web-tree-sitter';
import type { LanguageDescriptor, TokenClass } from '$lib/components/shared/ui/code-editor';
import { eagerBackendCompletionSource, backendHoverSource } from '../lsp-lang';
import { DIG_CATALOG } from './catalog';
import { digCommentHighlight } from './dig-comments';

const RUNTIME_WASM = '/bennu/tree-sitter.wasm';
const GRAMMAR_WASM = '/bennu/tree-sitter-geode.wasm';

// ── Lazy, once-per-window grammar load ─────────────────────────────────────────

let langPromise: Promise<Language> | null = null;

/** Load the geode grammar (idempotent — the wasm is fetched + compiled once, then every
 *  editor shares the cached {@link Language}). Rejects if either `.wasm` is missing. */
function initDigLang(): Promise<Language> {
  if (!langPromise) {
    langPromise = Parser.init({
      // Tell Emscripten where its runtime core lives (served from static/).
      locateFile: (file: string) => (file.endsWith('tree-sitter.wasm') ? RUNTIME_WASM : file),
    }).then(() => Language.load(GRAMMAR_WASM));
  }
  return langPromise;
}

async function createDigParser(): Promise<Parser> {
  const lang = await initDigLang();
  const parser = new Parser();
  parser.setLanguage(lang);
  return parser;
}

// ── Token classification ───────────────────────────────────────────────────────

/** The grammar's reserved words (`nd_lang_syntax::KEYWORDS`). Also the text-fallback set
 *  for the single-token rules described in the module doc. `true` / `false` / `none` are
 *  reserved too but classify as constants, so they are handled before this is consulted. */
const KEYWORDS = new Set(Object.keys(DIG_CATALOG.keywords));

/** Binary / unary / assignment operators, as the grammar spells them. */
const OPERATORS = new Set([
  '+', '-', '*', '/', '%', '==', '!=', '<', '>', '<=', '>=', '=', '->',
]);

/** Structural punctuation. `wildcard_pattern` is the `_` of a `match` arm: a single-token
 *  rule, so the leaf arrives named (see the module doc). */
const PUNCTUATION = new Set([
  '(', ')', '{', '}', '[', ']', ',', ':', '.', '_', 'wildcard_pattern',
]);

/** Nodes whose whole text is a string literal fragment. */
const STRING_PARTS = new Set(['string_content', 'escape', '"']);

/** The three indentation-delimited body nodes. */
const BLOCK_TYPES = new Set(['block', 'struct_block', 'match_block']);

/** Bracketed literals worth folding when they span lines. */
const BRACKETED = new Set(['list_expression', 'map_expression']);

/** `true` when `node` is (or is the property of) the callee of a call — `dig(…)`,
 *  `log.info(…)`. The property case needs the grandparent, which is why this walks up
 *  rather than reading `parentType`. */
function isCallee(node: Node): boolean {
  const parent = node.parent;
  if (!parent) return false;
  if (parent.type === 'call_expression') {
    return parent.childForFieldName('callee')?.id === node.id;
  }
  // `log.info(…)`: the property of a member_expression that is itself the callee.
  if (parent.type === 'member_expression') {
    const grand = parent.parent;
    return (
      grand?.type === 'call_expression' &&
      grand.childForFieldName('callee')?.id === parent.id
    );
  }
  return false;
}

/** The field each parent names for the child that is a **type**. `enum_pattern` is absent
 *  because it names no fields at all — see {@link isTypePosition}. */
const TYPE_FIELD_BY_PARENT: Record<string, string> = {
  struct_definition: 'name',
  parameter: 'type',
  // A struct field may declare its own type too (`fondo: Int`).
  struct_field: 'type',
  fn_definition: 'ret',
  member_expression: 'object',
  call_expression: 'callee',
  constructor_pattern: 'ctor',
};

/**
 * `true` when the identifier sits where the grammar expects a **type** (or a namespace,
 * which in this language is the same slot).
 *
 * There is nothing to guess: the positions are the ones the grammar gives a field name to,
 * plus the first part of a dotted `match` pattern (`Crystal.Amethyst`), which is a dotted
 * access written without fields.
 */
function isTypePosition(node: Node, field: string | null, parentType: string | null): boolean {
  if (!parentType) return false;
  const wanted = TYPE_FIELD_BY_PARENT[parentType];
  if (wanted) return field === wanted;
  if (parentType === 'enum_pattern') return node.parent?.child(0)?.id === node.id;
  // `Fn(Int) -> Bool`: the type's own name and its return.
  if (parentType === 'fn_type') return field === 'name' || field === 'ret';
  // Its parameters carry no field name — inside a type list *everything* is a type, by
  // construction, so there is nothing to distinguish.
  if (parentType === 'type_list') return true;
  return false;
}

/**
 * Classify a leaf. Mirrors geode's `highlight.rs` classifier, with the extra distinctions
 * Arbor's theme can render that a Bevy-side one could not: a declaration name reads
 * differently from a call site, and a struct field from a local.
 */
function classify(
  node: Node,
  _isNamed: boolean,
  field: string | null,
  parentType: string | null,
): TokenClass | null {
  const type = node.type;

  if (type === 'identifier') {
    // Declarations first — they are named positions, and the name is the point of the line.
    if (field === 'name') {
      if (parentType === 'fn_definition') return 'declaration';
      if (parentType === 'struct_definition') return 'type';
      if (parentType === 'struct_field') return 'field';
      if (parentType === 'keyword_argument') return 'label';
    }
    // A capital says "not a local"; the position says whether it is a type or a value.
    // `Tool.Pick` and `move(East)` pass the same kind of thing, and used to be coloured
    // differently only because one of them was written out.
    const first = node.text[0] ?? '';
    if (first !== first.toLowerCase()) {
      return isTypePosition(node, field, parentType) ? 'type' : 'constant';
    }
    if (isCallee(node)) return 'function';
    if (field === 'property') return 'field';
    // A builtin named as a value (`equip` passed around) still reads as one.
    if (DIG_CATALOG.builtins[node.text]) return 'function';
    return 'ident';
  }

  if (type === 'integer' || type === 'float') return 'number';
  if (type === 'boolean' || type === 'none' || type === 'true' || type === 'false') {
    return 'constant';
  }
  if (STRING_PARTS.has(type)) return 'string';
  // ⚠️ No `comment` branch: the scanner eats comment lines, so one never arrives here.
  // Keeping a dead arm would have kept the wrong model alive — see the module doc.
  if (KEYWORDS.has(type)) return 'keyword';
  if (OPERATORS.has(type)) return 'operator';
  if (PUNCTUATION.has(type)) return 'punctuation';

  // The single-token-rule fallback: the leaf's type is the rule's name
  // (`pass_statement`), so ask the text instead. Keeps the keyword list the only list.
  if (KEYWORDS.has(node.text)) return 'keyword';

  return null;
}

// ── Folding ────────────────────────────────────────────────────────────────────

/** The body of a compound statement, whichever field the grammar names it. `if_statement`
 *  calls its own `then` (and keeps `elif` / `else` as separate clauses, which fold on
 *  their own lines). */
function bodyOf(node: Node): Node | null {
  return node.childForFieldName('body') ?? node.childForFieldName('then');
}

/**
 * Fold a compound statement from the end of its header to the end of its body, or a
 * multi-line list / map from just inside its brackets.
 *
 * The header end is the `:` (or the `->` of a `match` arm) that precedes the body: the
 * body node itself begins on the next line, past the newline and the indentation, so
 * folding from there would put the placeholder in the body's leading whitespace instead
 * of on the header line.
 */
function foldNode(node: Node): { from: number; to: number } | null {
  if (BRACKETED.has(node.type)) {
    const from = node.startIndex + 1;
    const to = node.endIndex - 1;
    return to > from ? { from, to } : null;
  }

  const body = bodyOf(node);
  if (!body || !BLOCK_TYPES.has(body.type)) return null;

  let from = body.startIndex;
  for (let i = 0; i < node.childCount; i++) {
    const child = node.child(i);
    if (!child || child.endIndex > body.startIndex) continue;
    if (child.type === ':' || child.type === '->') from = child.endIndex;
  }
  const to = body.endIndex;
  return to > from ? { from, to } : null;
}

// ── The descriptor ─────────────────────────────────────────────────────────────

/** The `.dig` language: geode's grammar and token classes, geode's server for the rest. */
export const digLanguage: LanguageDescriptor = {
  id: 'dig',
  createParser: createDigParser,
  classify,
  foldNode,
  // Comments never reach the tree — see the module doc and `dig-comments.ts`.
  extraHighlight: digCommentHighlight,
  // `#` to end of line — the grammar's only comment form, so `Ctrl+/` works. A
  // tree-sitter descriptor bypasses CodeMirror's `Language`, so it carries no built-in
  // comment data and this is the only way the toggle learns it.
  commentTokens: { line: '#' },
  // Same bargain as `.dev`: a `.dig` line is words and arguments, not dotted paths, so the
  // server is asked wherever the caret is rather than only after a trigger character.
  intel: { completion: eagerBackendCompletionSource, hover: backendHoverSource },
};
