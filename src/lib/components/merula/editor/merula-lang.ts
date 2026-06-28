/**
 * merula ↔ Tree-sitter bridge (pure, CodeMirror-free).
 *
 * Loads the SAME `merula-lang` tree-sitter grammar the backend uses,
 * compiled to WebAssembly, into the WebView via `web-tree-sitter`. This is the
 * single seam that turns `.merula` text into a concrete syntax tree the editor
 * walks for syntax highlighting, the symbol table (go-to-declaration), and the
 * outline. Keeping it CM-agnostic means the same parser feeds any consumer.
 *
 * Two `.wasm` files ship under `static/merula/` (served at `/merula/…`):
 *   - `tree-sitter.wasm`        — the web-tree-sitter Emscripten runtime core.
 *   - `tree-sitter-merula.wasm`  — the merula grammar (built from the committed
 *                                 `parser.c` + `scanner.c`; see the README there).
 *
 * Offset model: web-tree-sitter reports `startIndex`/`endIndex` in **UTF-16 code
 * units** (it parses the JS string in UTF-16), which is exactly CodeMirror's
 * document coordinate — so tree offsets drop straight into CM with no mapping.
 * The backend, by contrast, reports diagnostics/active-hap spans in **UTF-8 byte
 * offsets**; {@link makeByteToU16} converts those (identity on pure-ASCII source).
 */

import { Parser, Language, type Tree, type Node, type TreeCursor } from 'web-tree-sitter';

const RUNTIME_WASM = '/merula/tree-sitter.wasm';
const GRAMMAR_WASM = '/merula/tree-sitter-merula.wasm';

// ── Lazy, once-per-window grammar load ─────────────────────────────────────────

let langPromise: Promise<Language> | null = null;

/** Load the merula grammar (idempotent — the wasm is fetched + compiled once,
 *  then every editor shares the cached {@link Language}). Rejects if either
 *  `.wasm` is missing (e.g. the grammar wasn't built yet — see static/merula). */
export function initMerulaLang(): Promise<Language> {
  if (!langPromise) {
    langPromise = Parser.init({
      // Tell Emscripten where its runtime core lives (served from static/).
      locateFile: (file: string) =>
        file.endsWith('tree-sitter.wasm') ? RUNTIME_WASM : file,
    }).then(() => Language.load(GRAMMAR_WASM));
  }
  return langPromise;
}

/** A parser bound to the merula grammar. One per editor instance is fine —
 *  parsers are cheap; the heavy `Language` is shared. */
export async function createMerulaParser(): Promise<Parser> {
  const lang = await initMerulaLang();
  const parser = new Parser();
  parser.setLanguage(lang);
  return parser;
}

// ── Token classification (CST node → highlight class) ──────────────────────────
//
// Maps a node to a CSS class for syntax highlighting. Only **leaf** tokens are
// classified (containers recurse); the editor emits one decoration per match.
// `parentType` disambiguates `identifier` (a name binding vs a call vs a splice
// reference all share the `name`/`function` fields but want different colours).

/** Grammar keyword literals (anonymous tokens). */
const KEYWORDS = new Set(['let', 'fn', 'import', 'from']);

/** Mini-notation + host operator/punctuation literals (anonymous tokens). */
const OPERATORS = new Set([
  '&', '*', '/', '!', '@', ':', "'", '%', '+', '-', '=', '=>', '$',
  '..', '..=', '[', ']', '<', '>', '{', '}', '(', ')', ',',
]);

/** External operator tokens (named in the grammar, but lexically punctuation). */
const OP_TYPES = new Set(['dot', 'range_op', 'range_inclusive_op']);

export type MerulaTokenClass =
  | 'comment' | 'string' | 'number' | 'note' | 'chord' | 'sound'
  | 'island' | 'mininote' | 'keyword' | 'fn' | 'def' | 'splice'
  | 'ident' | 'op';

/** Classify a leaf node into a highlight class, or `null` to leave it plain. */
export function classifyToken(
  type: string,
  named: boolean,
  field: string | null,
  parentType: string | null,
): MerulaTokenClass | null {
  switch (type) {
    case 'comment':       return 'comment';
    case 'string':        return 'string';
    case 'integer':
    case 'float':         return 'number';
    case 'note_literal':
    case 'note_name':     return 'note';
    case 'chord_name':    return 'chord';
    case 'sound_name':    return 'sound';
    case 'island_start':
    case 'island_end':    return 'island';
    case 'rest':
    case 'extend':        return 'mininote'; // ~  _
  }
  if (OP_TYPES.has(type)) return 'op';
  if (type === 'identifier') {
    if (field === 'function' || field === 'method') return 'fn';
    if (field === 'name') {
      // `name` is shared by declarations and splice references — split by parent.
      if (parentType === 'splice') return 'splice';
      return 'def'; // let / fn / import binding name
    }
    return 'ident';
  }
  if (!named) {
    if (KEYWORDS.has(type)) return 'keyword';
    if (OPERATORS.has(type)) return 'op';
  }
  return null;
}

// ── Byte (UTF-8) → UTF-16 offset mapping ───────────────────────────────────────

/** Build a converter from a UTF-8 byte offset (backend span coordinate) to a
 *  UTF-16 code-unit offset (CodeMirror/tree-sitter coordinate). Identity on
 *  pure-ASCII source (the common case); otherwise a binary search over
 *  code-point boundaries. Offsets are clamped into `[0, src.length]`. */
export function makeByteToU16(src: string): (byte: number) => number {
  let ascii = true;
  for (let i = 0; i < src.length; i++) {
    if (src.charCodeAt(i) > 0x7f) { ascii = false; break; }
  }
  if (ascii) return (b) => (b < 0 ? 0 : b > src.length ? src.length : b);

  const bytePos: number[] = [0];
  const u16Pos: number[] = [0];
  let byte = 0;
  for (let i = 0; i < src.length; ) {
    const cp = src.codePointAt(i)!;
    const u16len = cp > 0xffff ? 2 : 1;
    const blen = cp <= 0x7f ? 1 : cp <= 0x7ff ? 2 : cp <= 0xffff ? 3 : 4;
    byte += blen;
    i += u16len;
    bytePos.push(byte);
    u16Pos.push(i);
  }
  const total = byte;
  return (b) => {
    if (b <= 0) return 0;
    if (b >= total) return src.length;
    // Largest index whose byte position is ≤ b.
    let lo = 0, hi = bytePos.length - 1;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if (bytePos[mid] <= b) lo = mid; else hi = mid - 1;
    }
    return u16Pos[lo];
  };
}

// ── Symbols + outline (one shared tree walk) ──────────────────────────────────

/** Kind of an outline / declaration symbol. `section` only appears nested under
 *  a track in the tree outline (never in the flat declaration list). */
export type MerulaSymbolKind = 'track' | 'fn' | 'let' | 'import' | 'section';

/** A declared symbol the editor can jump to (go-to-decl) and the Outline panel
 *  lists. Carries a UTF-16 `offset` so the jump is exact, not line-rounded. */
export interface MerulaSymbol {
  id: string;
  kind: MerulaSymbolKind;
  /** Display label (e.g. `bassline(root)`, `INTRO`, a track name). */
  label: string;
  /** Bare name used to resolve a reference (e.g. `bassline`). */
  name: string;
  /** 1-based line of the declaration. */
  line: number;
  /** UTF-16 offset of the declaration's name token (exact jump target). */
  offset: number;
}

/** The resolvable surface of one `.merula` file. `defs` are local declarations
 *  (let / fn / track); `imports` map an imported name → the source path it came
 *  from (`import { drumGroove } from "lib/drums.merula"`), so cross-file
 *  go-to-decl can open the right file. */
export interface MerulaSymbols {
  defs: Map<string, MerulaSymbol>;
  imports: Map<string, string>;
  /** Flat ordered list for the Outline (declarations + tracks, source order). */
  outline: MerulaSymbol[];
}

function nodeLine(node: Node): number {
  return node.startPosition.row + 1;
}

function unquote(s: string): string {
  return s.length >= 2 && s.startsWith('"') && s.endsWith('"') ? s.slice(1, -1) : s;
}

/** Recursively collect `track("name", …)` calls anywhere under `node` (they
 *  nest inside `tracks(…)` / `arrange(…)`). The track name is the first string
 *  argument of a call whose function identifier is `track`. */
function collectTracks(node: Node, out: MerulaSymbol[]): void {
  if (node.type === 'call_expression') {
    const fn = node.childForFieldName('function');
    if (fn?.type === 'identifier' && fn.text === 'track') {
      const args = node.childForFieldName('arguments');
      const firstStr = args?.namedChildren.find((c) => c?.type === 'string');
      if (firstStr) {
        const name = unquote(firstStr.text);
        out.push({
          id: `track:${name}:${node.startIndex}`,
          kind: 'track', label: name, name,
          line: nodeLine(node), offset: node.startIndex,
        });
      }
    }
  }
  for (const child of node.namedChildren) if (child) collectTracks(child, out);
}

/** Walk the top-level items, building the symbol table + outline in one pass. */
export function extractSymbols(tree: Tree): MerulaSymbols {
  const defs = new Map<string, MerulaSymbol>();
  const imports = new Map<string, string>();
  const decls: MerulaSymbol[] = [];
  const root = tree.rootNode;

  for (const item of root.namedChildren) {
    if (!item) continue;
    if (item.type === 'let_binding') {
      const nameNode = item.childForFieldName('name');
      if (nameNode) {
        const sym: MerulaSymbol = {
          id: `let:${nameNode.text}`, kind: 'let', label: nameNode.text,
          name: nameNode.text, line: nodeLine(item), offset: nameNode.startIndex,
        };
        defs.set(sym.name, sym);
        decls.push(sym);
      }
    } else if (item.type === 'fn_definition') {
      const nameNode = item.childForFieldName('name');
      const paramsNode = item.childForFieldName('params');
      if (nameNode) {
        const params = paramsNode ? paramsNode.text : '';
        const sym: MerulaSymbol = {
          id: `fn:${nameNode.text}`, kind: 'fn', label: `${nameNode.text}(${params})`,
          name: nameNode.text, line: nodeLine(item), offset: nameNode.startIndex,
        };
        defs.set(sym.name, sym);
        decls.push(sym);
      }
    } else if (item.type === 'import_statement') {
      const path = item.childForFieldName('path');
      const from = path ? unquote(path.text) : '';
      // All `name`-field identifiers are imported bindings.
      for (const child of item.namedChildren) {
        if (child?.type === 'identifier') {
          imports.set(child.text, from);
          decls.push({
            id: `import:${child.text}`, kind: 'import', label: child.text,
            name: child.text, line: nodeLine(item), offset: child.startIndex,
          });
        }
      }
    }
  }

  // Tracks live inside a top-level `tracks(…)` expression (and arrange blocks).
  const tracks: MerulaSymbol[] = [];
  collectTracks(root, tracks);

  // Outline order: imports + declarations as written, then tracks (timeline).
  const outline = [...decls.filter((d) => d.kind !== 'track'), ...tracks];
  return { defs, imports, outline };
}

/** Outline symbol list from an already-parsed tree (the editor reuses its live
 *  tree). Same extraction the highlighter walk feeds — no drift. */
export function extractOutline(tree: Tree): MerulaSymbol[] {
  return extractSymbols(tree).outline;
}

/** A track outline node that may carry its named `section(…)` blocks as
 *  children (the expandable Outline rows). Non-track symbols never have
 *  children. */
export interface MerulaOutlineNode extends MerulaSymbol {
  children?: MerulaSymbol[];
  /** A self-contained snippet that plays just this declaration in isolation:
   *  the file's `let`/`fn`/`import` preamble (so dependencies resolve) plus this
   *  node as the sole top-level output. Set for playable nodes (`track` / `let`);
   *  `undefined` for `fn` (needs args) / `import`. Drives the Outline Play button. */
  playSource?: string;
}

/** Like {@link collectTracks} but keeps each track's tree node so we can scan
 *  its subtree for sections. */
function collectTrackNodes(node: Node, out: { sym: MerulaSymbol; node: Node }[]): void {
  if (node.type === 'call_expression') {
    const fn = node.childForFieldName('function');
    if (fn?.type === 'identifier' && fn.text === 'track') {
      const args = node.childForFieldName('arguments');
      const firstStr = args?.namedChildren.find((c) => c?.type === 'string');
      if (firstStr) {
        const name = unquote(firstStr.text);
        out.push({
          sym: { id: `track:${name}:${node.startIndex}`, kind: 'track', label: name, name,
            line: nodeLine(node), offset: node.startIndex },
          node,
        });
      }
    }
  }
  for (const child of node.namedChildren) if (child) collectTrackNodes(child, out);
}

/** Collect `section("name", …)` calls within a subtree — the arrangement's
 *  named bands, in source order. The jump target is the name string token. */
function collectSections(node: Node, out: MerulaSymbol[]): void {
  if (node.type === 'call_expression') {
    const fn = node.childForFieldName('function');
    if (fn?.type === 'identifier' && fn.text === 'section') {
      const args = node.childForFieldName('arguments');
      const firstStr = args?.namedChildren.find((c) => c?.type === 'string');
      if (firstStr) {
        const name = unquote(firstStr.text);
        out.push({
          id: `section:${name}:${node.startIndex}`,
          kind: 'section', label: name, name,
          line: nodeLine(node), offset: firstStr.startIndex,
        });
      }
    }
  }
  for (const child of node.namedChildren) if (child) collectSections(child, out);
}

/** Outline with each track's `section(…)` blocks nested as children — drives the
 *  expandable Outline panel. The flat declaration order is preserved; only
 *  tracks gain a `children` array (when they declare sections). */
export function extractOutlineTree(tree: Tree): MerulaOutlineNode[] {
  const flat = extractSymbols(tree).outline;
  const trackNodes: { sym: MerulaSymbol; node: Node }[] = [];
  collectTrackNodes(tree.rootNode, trackNodes);

  const sectionsByOffset = new Map<number, MerulaSymbol[]>();
  const trackTextByOffset = new Map<number, string>();
  for (const { sym, node } of trackNodes) {
    const secs: MerulaSymbol[] = [];
    collectSections(node, secs);
    if (secs.length) sectionsByOffset.set(sym.offset, secs);
    trackTextByOffset.set(sym.offset, node.text);
  }

  // The file's `let`/`fn`/`import` preamble — prepended to a played declaration so
  // its dependencies (other lets, imports) resolve when evaluated in isolation.
  const preamble = topLevelPreamble(tree);

  // The played top-level output: a track is a bare `track(...)` call (a Track
  // value); a let plays its bound name (the value becomes the output track). Both
  // are accepted as top-level outputs by the evaluator.
  function playSourceFor(s: MerulaSymbol): string | undefined {
    let target: string | undefined;
    if (s.kind === 'track') target = trackTextByOffset.get(s.offset);
    else if (s.kind === 'let') target = s.name;
    if (!target) return undefined;
    return preamble ? `${preamble}\n${target}` : target;
  }

  return flat.map((s) => {
    const node: MerulaOutlineNode = { ...s };
    const play = playSourceFor(s);
    if (play) node.playSource = play;
    if (s.kind === 'track' && sectionsByOffset.has(s.offset)) node.children = sectionsByOffset.get(s.offset);
    return node;
  });
}

/** The file's top-level `let` / `fn` / `import` declarations, concatenated in
 *  source order — the preamble prepended to a played fragment so its dependencies
 *  (other constants, functions, imports) resolve when it's evaluated in isolation. */
export function topLevelPreamble(tree: Tree): string {
  return tree.rootNode.namedChildren
    .filter((c): c is Node => !!c &&
      (c.type === 'let_binding' || c.type === 'fn_definition' || c.type === 'import_statement'))
    .map((c) => c.text)
    .join('\n');
}

// ── Standalone parse (for consumers without an editor, e.g. the Outline panel) ─

let sharedParser: Parser | null = null;

/** Parse a `.merula` source with a shared, lazily-created parser. For one-off
 *  parses outside an editor (no incremental tree to maintain). */
export async function parseMerula(src: string): Promise<Tree | null> {
  if (!sharedParser) sharedParser = await createMerulaParser();
  return sharedParser.parse(src);
}

/** Outline directly from source — the helper Step 4's Outline panel mounts: it
 *  parses + extracts in one call, so the panel needs no Tree-sitter knowledge.
 *  Returns `[]` if the grammar wasm hasn't been built / fails to load. */
export async function outlineFromSource(src: string): Promise<MerulaSymbol[]> {
  try {
    const tree = await parseMerula(src);
    return tree ? extractOutline(tree) : [];
  } catch {
    return [];
  }
}

/** Resolve a played `fragment` against the active file `src`: prepend the file's
 *  `let`/`fn`/`import` preamble so a bare variable (or any expression referencing
 *  file-level bindings) evaluates in isolation. A self-contained fragment is left
 *  effectively unchanged (the unused preamble is harmless). Falls back to the raw
 *  fragment if the grammar isn't available. */
export async function withFileDeps(src: string, fragment: string): Promise<string> {
  if (!fragment.trim()) return fragment;
  try {
    const tree = await parseMerula(src);
    if (!tree) return fragment;
    const pre = topLevelPreamble(tree);
    // A bare nullary `fn` name isn't a pattern on its own — invoke it so it plays.
    let frag = fragment;
    const lone = fragment.trim();
    if (/^[A-Za-z_]\w*$/.test(lone) && fnArity(tree, lone) === 0) frag = `${lone}()`;
    return pre ? `${pre}\n${frag}` : frag;
  } catch {
    return fragment;
  }
}

/** Nested outline (tracks → sections) from source — the Outline panel's source.
 *  Returns `[]` if the grammar wasm hasn't been built / fails to load. */
export async function outlineTreeFromSource(src: string): Promise<MerulaOutlineNode[]> {
  try {
    const tree = await parseMerula(src);
    return tree ? extractOutlineTree(tree) : [];
  } catch {
    return [];
  }
}

// ── Identifier-at-offset (Ctrl+Click target) ──────────────────────────────────

/** The smallest `identifier` node covering `offset` (UTF-16), or null. Used by
 *  go-to-declaration to read the word under the cursor straight from the tree
 *  (so `n(c4)` vs the `n` island head are disambiguated by the grammar). */
export function identifierAt(tree: Tree, offset: number): string | null {
  const node = tree.rootNode.descendantForIndex(offset);
  if (!node) return null;
  let cur: Node | null = node;
  while (cur) {
    if (cur.type === 'identifier') return cur.text;
    cur = cur.parent;
  }
  return null;
}

/** The body expression node of a `fn_definition` — the last named child that isn't
 *  the name or the parameter list (the grammar is `fn IDENT(params) = EXPR`). */
function fnBodyNode(fn: Node): Node | null {
  const name = fn.childForFieldName('name');
  const params = fn.childForFieldName('params');
  const named = fn.namedChildren.filter((c): c is Node => !!c);
  for (let i = named.length - 1; i >= 0; i--) {
    const c = named[i];
    if (name && c.id === name.id) continue;
    if (params && c.id === params.id) continue;
    return c;
  }
  return null;
}

/** Parameter count of a top-level `fn` named `name`, or `null` when there's no
 *  such function. Used to decide whether a bare fn name is directly playable. */
export function fnArity(tree: Tree, name: string): number | null {
  for (const item of tree.rootNode.namedChildren) {
    if (item?.type === 'fn_definition' && item.childForFieldName('name')?.text === name) {
      const params = item.childForFieldName('params');
      return params ? params.namedChildren.filter((c) => c?.type === 'identifier').length : 0;
    }
  }
  return null;
}

/** When the selection `[from, to)` is exactly a single `identifier` that names a
 *  top-level `let` or `fn`, return that declaration's **body** range (UTF-16) —
 *  where its note literals live. Used by the editor→DAW link: selecting a variable
 *  or a function boxes every hap it produces (their source spans point into the
 *  declaration's body, not the use site). Returns null when the selection isn't a
 *  clean declaration reference. */
export function declBodyRangeForSelection(
  tree: Tree, from: number, to: number,
): { from: number; to: number } | null {
  if (to <= from) return null;
  let id: Node | null = tree.rootNode.descendantForIndex(from);
  while (id && id.type !== 'identifier') id = id.parent;
  // Require the selection to cover exactly the identifier token (a deliberate
  // "select the symbol", not an incidental overlap).
  if (!id || id.startIndex !== from || id.endIndex !== to) return null;
  const name = id.text;
  for (const item of tree.rootNode.namedChildren) {
    if (!item || item.childForFieldName('name')?.text !== name) continue;
    if (item.type === 'let_binding') {
      const v = item.childForFieldName('value');
      if (v) return { from: v.startIndex, to: v.endIndex };
    } else if (item.type === 'fn_definition') {
      const body = item.childForFieldName('body') ?? fnBodyNode(item);
      if (body) return { from: body.startIndex, to: body.endIndex };
    }
  }
  return null;
}

// ── Find usages (every occurrence of an identifier) ────────────────────────────

/** Every `identifier` node in the tree whose text equals `name`, as UTF-16 doc
 *  ranges (declaration + references, incl. `$splice` and call/method positions),
 *  in source order. Powers "find usages": a flat, file-scoped name match — the
 *  DSL has no shadowing to disambiguate, so same-name == same symbol here. */
export function identifierUsages(tree: Tree, name: string): { from: number; to: number }[] {
  const out: { from: number; to: number }[] = [];
  const walk = (node: Node) => {
    if (node.type === 'identifier' && node.text === name) {
      out.push({ from: node.startIndex, to: node.endIndex });
    }
    for (let i = 0; i < node.childCount; i++) {
      const child = node.child(i);
      if (child) walk(child);
    }
  };
  walk(tree.rootNode);
  out.sort((a, b) => a.from - b.from);
  return out;
}

// ── Tracks referencing a symbol (editor ↔ arrangement link) ────────────────────

/** Indices (declaration order) of the `track("name", …)` calls whose subtree
 *  references the identifier `name` — i.e. the arrangement lanes a `let` / `fn`
 *  phrase feeds. A track matches when an `identifier` equal to `name` (a plain
 *  use or a `$splice` reference) appears anywhere inside its `track(...)` call.
 *  The index is the same stable key the mixer / arrangement use. */
export function tracksReferencing(tree: Tree, name: string): number[] {
  const out: number[] = [];
  let index = -1;
  const subtreeUses = (node: Node): boolean => {
    if (node.type === 'identifier' && node.text === name) return true;
    for (let i = 0; i < node.childCount; i++) {
      const c = node.child(i);
      if (c && subtreeUses(c)) return true;
    }
    return false;
  };
  const visit = (node: Node) => {
    if (node.type === 'call_expression') {
      const fn = node.childForFieldName('function');
      if (fn?.type === 'identifier' && fn.text === 'track') {
        index++; // track calls don't nest — this matches `collectTracks` ordering
        if (subtreeUses(node)) out.push(index);
        return;
      }
    }
    for (const child of node.namedChildren) if (child) visit(child);
  };
  visit(tree.rootNode);
  return out;
}

// ── String-argument call context (scoped value completion) ─────────────────────

/** When `offset` sits inside a string-literal argument, the callee name of the
 *  innermost call/method wrapping that string plus `from` — the offset just past
 *  the opening quote, where a replacement completion should start. Lets the
 *  editor offer value completions scoped to a builtin (e.g. instrument names in
 *  `inst("…")`, and to *suppress* language completions inside any other string).
 *  Null when the cursor isn't inside a call's string argument. */
export function stringArgCallAt(tree: Tree, offset: number): { from: number; to: number; fn: string } | null {
  const at = tree.rootNode.descendantForIndex(offset);
  if (!at) return null;
  // Nearest enclosing string literal (the argument being typed).
  let str: Node | null = at;
  while (str && str.type !== 'string') str = str.parent;
  if (!str || offset <= str.startIndex) return null; // not past the opening quote
  // The innermost call whose argument list contains this string decides the scope.
  for (let cur: Node | null = str.parent; cur; cur = cur.parent) {
    const args = cur.childForFieldName('arguments');
    if (!args) continue;
    if (str.startIndex < args.startIndex || str.endIndex > args.endIndex) continue;
    const callee = cur.childForFieldName('function') ?? cur.childForFieldName('method');
    if (callee?.type !== 'identifier') return null;
    // `from`/`to` bracket the string *content* (between the quotes), the range a
    // value completion / quick-fix replaces.
    return { from: str.startIndex + 1, to: str.endIndex - 1, fn: callee.text };
  }
  return null;
}

/** The instrument / sound name when `offset` sits inside an `inst("…")` or
 *  `s("…")` string argument, else null. Powers Ctrl/Cmd+click → instrument
 *  preview: it recognises the two functions that name a registry voice and
 *  returns the bare name (unquoted), so the editor can open the audition modal
 *  instead of go-to-declaration. */
export function instrumentNameAt(tree: Tree, offset: number): string | null {
  const at = tree.rootNode.descendantForIndex(offset);
  if (!at) return null;
  let str: Node | null = at;
  while (str && str.type !== 'string') str = str.parent;
  if (!str) return null;
  for (let cur: Node | null = str.parent; cur; cur = cur.parent) {
    const args = cur.childForFieldName('arguments');
    if (!args) continue;
    if (str.startIndex < args.startIndex || str.endIndex > args.endIndex) continue;
    const callee = cur.childForFieldName('function') ?? cur.childForFieldName('method');
    if (callee?.type !== 'identifier') return null;
    return callee.text === 'inst' || callee.text === 's' ? unquote(str.text) : null;
  }
  return null;
}

export type { Parser, Tree, Node, TreeCursor };
