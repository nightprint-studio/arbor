/**
 * Bennu ↔ tree-sitter-java bridge: the Java {@link LanguageDescriptor} for the shared
 * code-editor core.
 *
 * Loads `web-tree-sitter` (the Emscripten runtime core) + the Java grammar compiled
 * to WebAssembly into the WebView, mirroring merula-lang's `createParser`. Two
 * `.wasm` files ship under `static/bennu/` (served at `/bennu/…`):
 *   - `tree-sitter.wasm`       — the web-tree-sitter runtime core (copied from
 *                                static/merula, it's the same runtime).
 *   - `tree-sitter-java.wasm`  — the tree-sitter-java grammar (prebuilt).
 *
 * If either wasm is missing the parser factory rejects and the editor stays plain
 * text (graceful — no crash), exactly like merula.
 *
 * Beyond highlighting, the descriptor wires:
 *   - **Folding** (`foldNode`) — braced blocks + block comments collapse, head
 *     line kept visible (IntelliJ-style). Purely tree-driven, no backend.
 *   - **Completion** (`intel.completion`) — a CodeMirror source that calls
 *     `bennu_completion` on `.`/identifier typing, mapping CompletionItem → CM
 *     Completion. Returns [] gracefully until the BE index is warm.
 *
 * Offset model: web-tree-sitter reports UTF-16 code-unit offsets — CodeMirror's
 * document coordinate — so tree offsets drop straight in with no mapping. The BE
 * completion command wants a **UTF-8 byte** offset, so the source maps the caret
 * with the shared byte↔UTF-16 helper before the call.
 */

import { Parser, Language, type Node } from 'web-tree-sitter';
import {
  makeU16ToByte, makeByteToU16,
  type LanguageDescriptor, type TokenClass, type CompletionSource,
} from '$lib/components/shared/ui/code-editor';
import {
  insertCompletionText,
  type Completion, type CompletionContext, type CompletionResult,
} from '@codemirror/autocomplete';
import type { EditorView } from '@codemirror/view';
import { completion as ipcCompletion, importEdit as ipcImportEdit } from '$lib/ipc/bennu';
import { hover as ipcHover } from '$lib/ipc/bennu/nav';
import { projectStore } from '$lib/stores/bennu/project.svelte';
import { bennuSettingsStore } from '$lib/stores/bennu/settings.svelte';
import { makeHoverSource } from './bennu-hover';

const RUNTIME_WASM = '/bennu/tree-sitter.wasm';
const GRAMMAR_WASM = '/bennu/tree-sitter-java.wasm';

// ── Lazy, once-per-window grammar load ─────────────────────────────────────────

let langPromise: Promise<Language> | null = null;

/** Load the Java grammar (idempotent — the wasm is fetched + compiled once, then
 *  every editor shares the cached {@link Language}). Rejects if either `.wasm` is
 *  missing. */
function initJavaLang(): Promise<Language> {
  if (!langPromise) {
    langPromise = Parser.init({
      // Tell Emscripten where its runtime core lives (served from static/).
      locateFile: (file: string) =>
        file.endsWith('tree-sitter.wasm') ? RUNTIME_WASM : file,
    }).then(() => Language.load(GRAMMAR_WASM));
  }
  return langPromise;
}

/** A parser bound to the Java grammar. One per editor is fine — parsers are cheap;
 *  the heavy {@link Language} is shared. */
async function createJavaParser(): Promise<Parser> {
  const lang = await initJavaLang();
  const parser = new Parser();
  parser.setLanguage(lang);
  return parser;
}

// ── Token classification (tree-sitter-java CST node → highlight class) ──────────
//
// Only **leaf** tokens are classified (the highlighter recurses into containers and
// calls this per leaf). `field` is the parent's field name for this child, and
// `parentType` the parent's node type — used to disambiguate a bare `identifier`
// (a method call name vs a field vs a plain reference).
//
// tree-sitter-java surfaces keywords/operators as *anonymous* leaf tokens whose
// `type` is the literal text (`class`, `public`, `void`, `+`, `;`, …). Named leaves
// (`identifier`, `type_identifier`, `string_literal`, `decimal_integer_literal`, …)
// carry a stable `type`.

/** Java keyword literals (anonymous tokens). `this`/`super` handled separately. */
const KEYWORDS = new Set([
  'abstract', 'assert', 'break', 'case', 'catch', 'class', 'const', 'continue',
  'default', 'do', 'else', 'enum', 'extends', 'final', 'finally', 'for', 'goto',
  'if', 'implements', 'import', 'instanceof', 'interface', 'native', 'new',
  'package', 'private', 'protected', 'public', 'return', 'static', 'strictfp',
  'switch', 'synchronized', 'throw', 'throws', 'transient', 'try',
  'volatile', 'while', 'yield', 'record', 'sealed', 'permits', 'non-sealed', 'var',
  'open', 'module', 'requires', 'exports', 'opens', 'uses', 'provides', 'with', 'to',
]);

/** Primitive type keywords — coloured as types, not plain keywords, so a `int`
 *  reads with the same blue as `String` (IntelliJ does this too). */
const PRIMITIVES = new Set([
  'boolean', 'byte', 'char', 'double', 'float', 'int', 'long', 'short', 'void',
]);

/** Boolean / null literal keywords → highlighted as constants. */
const CONSTANTS = new Set(['true', 'false', 'null']);

/** Language self-references. */
const SELF = new Set(['this', 'super']);

/** Punctuation / bracket literals (anonymous tokens) — muted. */
const PUNCTUATION = new Set(['(', ')', '{', '}', '[', ']', ';', ',', '.', '::', '...', '@', '->']);

/** Named string-ish literals + their nested fragments (text blocks span children
 *  in newer grammars: `string_fragment`, `escape_sequence`). */
const STRINGY = new Set([
  'string_literal', 'character_literal', 'text_block',
  'string_fragment', 'multiline_string_fragment', 'escape_sequence',
]);

/** Named number literals across all bases. */
const NUMERIC = new Set([
  'decimal_integer_literal', 'hex_integer_literal', 'octal_integer_literal',
  'binary_integer_literal', 'decimal_floating_point_literal', 'hex_floating_point_literal',
]);

/** Classify a leaf node into a highlight class, or `null` to leave it plain. */
function classify(
  node: Node,
  named: boolean,
  field: string | null,
  parentType: string | null,
): TokenClass | null {
  const type = node.type;

  // Named leaves with a stable type first.
  if (type === 'line_comment' || type === 'block_comment') return 'comment';
  if (STRINGY.has(type)) return 'string';
  if (NUMERIC.has(type)) return 'number';
  if (type === 'null_literal') return 'constant';
  // `true`/`false` are anonymous in some grammar builds, named in others.
  if (type === 'true' || type === 'false') return 'constant';

  // A `type_identifier` is a class/interface/enum name reference.
  if (type === 'type_identifier') return 'type';

  // `identifier` — disambiguate by field / parent.
  if (type === 'identifier') {
    // Method *declaration* name → declaration (bolder gold).
    if (field === 'name' && parentType === 'method_declaration') return 'declaration';
    // Constructor declaration name reads as a type (it IS the class name).
    if (field === 'name' && parentType === 'constructor_declaration') return 'type';
    // Method *invocation* name → function (call site gold).
    if (field === 'name' && parentType === 'method_invocation') return 'function';
    // Annotation name (`@Override`) — parent is `marker_annotation` / `annotation`.
    if (parentType === 'marker_annotation' || parentType === 'annotation') return 'annotation';
    // Field access target / field declaration name → field violet.
    if (parentType === 'field_access' && field === 'field') return 'field';
    if (parentType === 'variable_declarator' && field === 'name'
        && isFieldDeclarator(node)) return 'field';
    // A parameter / catch / lambda formal name stays a plain identifier — locals
    // read as text-primary so instance state (`field`) pops against them.
    // Constant-style ALL_CAPS identifiers read as constants (a common convention).
    if (/^[A-Z][A-Z0-9_]*$/.test(node.text) && node.text.length > 1) return 'constant';
    // A leading-uppercase identifier is very likely a type reference the grammar
    // didn't tag as `type_identifier` (a scoped name segment, an enum member).
    if (/^[A-Z]/.test(node.text)) return 'type';
    return 'ident';
  }

  // Anonymous leaf tokens (keywords / operators / punctuation) carry their literal
  // text as `type`.
  if (!named) {
    if (SELF.has(type)) return 'self';
    if (CONSTANTS.has(type)) return 'constant';
    if (PRIMITIVES.has(type)) return 'type';
    if (KEYWORDS.has(type)) return 'keyword';
    if (PUNCTUATION.has(type)) return 'punctuation';
    // Everything else anonymous that's non-alphanumeric is an operator (`+`, `=`,
    // `&&`, `<`, `>`, `?`, `:`, …). Alphanumeric leftovers stay plain.
    if (/^[^\w\s]+$/.test(type)) return 'operator';
  }

  return null;
}

/** True when a `variable_declarator`'s ancestor is a `field_declaration` (an
 *  instance/static field) rather than a local variable declaration. */
function isFieldDeclarator(node: Node): boolean {
  let cur: Node | null = node.parent; // variable_declarator
  cur = cur?.parent ?? null;          // field_declaration | local_variable_declaration
  return cur?.type === 'field_declaration';
}

// ── Folding ────────────────────────────────────────────────────────────────────
//
// Fold the *inside* of braced blocks (class/interface/enum/method bodies, blocks,
// switch bodies, array initialisers, lambda blocks) and the body of block
// comments. The head line (`class Foo {`, `void m() {`, `/**`) stays visible.

const BLOCK_TYPES = new Set([
  'class_body', 'interface_body', 'enum_body', 'block', 'constructor_body',
  'switch_block', 'array_initializer', 'annotation_type_body', 'element_value_array_initializer',
]);

/** For a `{ … }`-delimited node fold from just after the `{` to just before the
 *  `}`; for a block comment fold from the end of its first line to its end. */
function foldNode(node: Node): { from: number; to: number } | null {
  if (BLOCK_TYPES.has(node.type)) {
    // Fold after the opening brace to before the closing one, so both braces
    // stay on-screen (`{ … }` collapses to `{…}`).
    const from = node.startIndex + 1;
    const to = node.endIndex - 1;
    return to > from ? { from, to } : null;
  }
  if (node.type === 'block_comment') {
    // Keep `/*` visible; fold the rest.
    const from = node.startIndex + 2;
    const to = node.endIndex;
    return to > from ? { from, to } : null;
  }
  return null;
}

// ── Completion source (member-access + identifier) ──────────────────────────────
//
// A CodeMirror completion source backed by `bennu_completion`. It fires on `.`
// (member access) and while typing an identifier, debounced. The BE wants a UTF-8
// byte offset; we map the caret with the shared U16→byte helper against the live
// buffer. Until the BE index is warm it returns [] → the popup just doesn't show,
// which is the desired graceful degradation (the mock returns none too).

/** Map a CompletionItem `kind` string to a CodeMirror completion `type` (drives
 *  the little kind icon in the popup). */
function kindToType(kind: string): string {
  switch (kind) {
    case 'method':
    case 'function':   return 'method';
    case 'field':
    case 'property':   return 'property';
    case 'class':
    case 'interface':
    case 'enum':
    case 'type':       return 'class';
    case 'variable':
    case 'parameter':  return 'variable';
    case 'keyword':    return 'keyword';
    case 'constant':   return 'constant';
    case 'package':    return 'namespace';
    default:           return 'text';
  }
}

/** After accepting a type-name completion, add its import (gated by the auto-import setting). Runs
 *  against the POST-insertion doc — the import region is above the caret, so the just-inserted name
 *  doesn't shift its offsets. The BE returns a byte-offset edit (or nothing when no import is needed);
 *  we map it to UTF-16 and dispatch it as a second, small change. Fire-and-forget. */
async function applyAutoImport(view: EditorView, fqn: string): Promise<void> {
  if (!bennuSettingsStore.autoImport) return;
  const path = projectStore.activeFilePath;
  if (!path) return;
  const src = view.state.doc.toString();
  let edit;
  try {
    edit = await ipcImportEdit(src, fqn);
  } catch {
    return; // BE absent — the name is inserted; import with Alt+Enter
  }
  if (!edit) return; // no import needed (same package / already imported / java.lang)
  const b2u = makeByteToU16(src);
  view.dispatch({ changes: { from: b2u(edit.start), to: b2u(edit.end), insert: edit.replacement } });
}

let completionSeq = 0;

// Java keyword/primitive/constant labels offered as completion fallback (identifier-
// shaped only — `non-sealed` & co. are dropped). Built once.
const KEYWORD_COMPLETION_LABELS: string[] = [...new Set([...KEYWORDS, ...PRIMITIVES, ...CONSTANTS])]
  .filter((k) => /^[A-Za-z][A-Za-z0-9_$]*$/.test(k))
  .sort();

const BUFFER_WORD_RE = /[A-Za-z_$][A-Za-z0-9_$]{2,}/g;
const MAX_FALLBACK = 400;

/** Enrich `out` with Java keywords + identifiers already present in the buffer that
 *  match `prefix` — the FE fallback so completion is useful even when the BE (member-
 *  access only, for now) returns nothing. Dedupes against `seen` (the BE labels). */
function appendFallbackCompletions(
  ctx: CompletionContext, prefix: string, seen: Set<string>, out: Completion[],
): void {
  const pl = prefix.toLowerCase();
  for (const k of KEYWORD_COMPLETION_LABELS) {
    if (seen.has(k) || (pl && !k.startsWith(pl))) continue;
    seen.add(k);
    out.push({ label: k, type: 'keyword' });
  }
  const src = ctx.state.doc.toString();
  BUFFER_WORD_RE.lastIndex = 0;
  let m: RegExpExecArray | null;
  let scanned = 0;
  while ((m = BUFFER_WORD_RE.exec(src)) !== null) {
    if (++scanned > 20000 || out.length >= MAX_FALLBACK) break;
    const w = m[0];
    if (seen.has(w) || w.toLowerCase() === pl) continue;
    if (pl && !w.toLowerCase().startsWith(pl)) continue;
    seen.add(w);
    out.push({ label: w, type: 'variable' });
  }
}

const javaCompletionSource: CompletionSource = async (
  ctx: CompletionContext,
): Promise<CompletionResult | null> => {
  // Trigger on `.` explicitly, or on an in-progress identifier word. Bail on an
  // empty word unless the completion was explicitly requested (Ctrl+Space) or we
  // just typed a `.`.
  const before = ctx.matchBefore(/\.?[\w$]*$/);
  const dotTrigger = ctx.matchBefore(/\.$/) != null;
  if (!ctx.explicit && !dotTrigger && (!before || before.from === before.to)) return null;

  const path = projectStore.activeFilePath;
  if (!path) return null;

  // The token start for CM to replace: after the `.` if any, else the word start.
  const word = ctx.matchBefore(/[\w$]*$/);
  const from = word ? word.from : ctx.pos;

  // Map the caret (UTF-16 doc offset) to a UTF-8 byte offset for the BE.
  const src = ctx.state.doc.toString();
  const u2b = makeU16ToByte(src);
  const byteOffset = u2b(ctx.pos);

  // Debounce: only the latest request resolves into a popup (a stale earlier
  // response is dropped). CM already coalesces, but the async IPC can race.
  const seq = ++completionSeq;
  let items: Awaited<ReturnType<typeof ipcCompletion>>;
  try {
    items = await ipcCompletion(path, byteOffset, src);
  } catch {
    items = []; // BE absent / not indexed yet — fall back to keywords + buffer words.
  }
  if (seq !== completionSeq) return null; // superseded by a newer keystroke

  const options: Completion[] = (items ?? []).map((it) => {
    const c: Completion = { label: it.label, detail: it.detail, type: kindToType(it.kind) };
    // A type-name completion with a single importable class carries its FQN — on accept, insert the
    // name AND (when auto-import is on) add its import in the same gesture ("IntelliJ-style").
    if (it.auto_import) {
      const fqn = it.auto_import;
      const label = it.label;
      c.apply = (view, _completion, from, to) => {
        view.dispatch(insertCompletionText(view.state, label, from, to));
        void applyAutoImport(view, fqn);
      };
    }
    return c;
  });

  // After a `.` only the BE's member list makes sense; elsewhere (identifier typing or
  // explicit Ctrl+Space) enrich with Java keywords + buffer identifiers so completion
  // is useful even while the BE member-access index is cold or the caret isn't after a
  // dot.
  if (!dotTrigger) {
    appendFallbackCompletions(ctx, word ? word.text : '', new Set(options.map((o) => o.label)), options);
  }

  if (options.length === 0) return null;
  return { from, options, validFor: /^[\w$]*$/ };
};

// ── Hover source (symbol signature + `var`/`val` inferred type) ──────────────────
//
// A CodeMirror `hoverTooltip` source backed by `bennu_hover` — a symbol's signature + kind +
// container (+ Javadoc), and for a local `var`/`val` (or any local / parameter) its resolved type.
// The shared factory owns the word-finding + DOM; this just supplies the fetch.

const javaHoverSource = makeHoverSource((path, src, byteOffset) => ipcHover(path, src, byteOffset));

/** The Java {@link LanguageDescriptor} handed to the shared `CodeEditor`. */
export const javaLanguage: LanguageDescriptor = {
  id: 'java',
  createParser: createJavaParser,
  classify,
  foldNode,
  commentTokens: { line: '//', block: { open: '/*', close: '*/' } },
  intel: { completion: javaCompletionSource, hover: javaHoverSource },
  // resolveGoto: reserved for when the symbol index / language service lands.
};
