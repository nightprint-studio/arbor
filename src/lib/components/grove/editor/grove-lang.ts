/**
 * grove ↔ Tree-sitter bridge (pure, CodeMirror-free).
 *
 * Loads the SAME `arbor-grove-lang` tree-sitter grammar the backend uses,
 * compiled to WebAssembly, into the WebView via `web-tree-sitter`. This is the
 * single seam that turns `.grove` text into a concrete syntax tree the editor
 * walks for syntax highlighting, the symbol table (go-to-declaration), and the
 * outline. Keeping it CM-agnostic means the same parser feeds any consumer.
 *
 * Two `.wasm` files ship under `static/grove/` (served at `/grove/…`):
 *   - `tree-sitter.wasm`        — the web-tree-sitter Emscripten runtime core.
 *   - `tree-sitter-grove.wasm`  — the grove grammar (built from the committed
 *                                 `parser.c` + `scanner.c`; see the README there).
 *
 * Offset model: web-tree-sitter reports `startIndex`/`endIndex` in **UTF-16 code
 * units** (it parses the JS string in UTF-16), which is exactly CodeMirror's
 * document coordinate — so tree offsets drop straight into CM with no mapping.
 * The backend, by contrast, reports diagnostics/active-hap spans in **UTF-8 byte
 * offsets**; {@link makeByteToU16} converts those (identity on pure-ASCII source).
 */

import { Parser, Language, type Tree, type Node, type TreeCursor } from 'web-tree-sitter';

const RUNTIME_WASM = '/grove/tree-sitter.wasm';
const GRAMMAR_WASM = '/grove/tree-sitter-grove.wasm';

// ── Lazy, once-per-window grammar load ─────────────────────────────────────────

let langPromise: Promise<Language> | null = null;

/** Load the grove grammar (idempotent — the wasm is fetched + compiled once,
 *  then every editor shares the cached {@link Language}). Rejects if either
 *  `.wasm` is missing (e.g. the grammar wasn't built yet — see static/grove). */
export function initGroveLang(): Promise<Language> {
  if (!langPromise) {
    langPromise = Parser.init({
      // Tell Emscripten where its runtime core lives (served from static/).
      locateFile: (file: string) =>
        file.endsWith('tree-sitter.wasm') ? RUNTIME_WASM : file,
    }).then(() => Language.load(GRAMMAR_WASM));
  }
  return langPromise;
}

/** A parser bound to the grove grammar. One per editor instance is fine —
 *  parsers are cheap; the heavy `Language` is shared. */
export async function createGroveParser(): Promise<Parser> {
  const lang = await initGroveLang();
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

export type GroveTokenClass =
  | 'comment' | 'string' | 'number' | 'note' | 'chord' | 'sound'
  | 'island' | 'mininote' | 'keyword' | 'fn' | 'def' | 'splice'
  | 'ident' | 'op';

/** Classify a leaf node into a highlight class, or `null` to leave it plain. */
export function classifyToken(
  type: string,
  named: boolean,
  field: string | null,
  parentType: string | null,
): GroveTokenClass | null {
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

/** Kind of an outline / declaration symbol. Mirrors the mock `OutlineEntry`. */
export type GroveSymbolKind = 'track' | 'fn' | 'let' | 'import';

/** A declared symbol the editor can jump to (go-to-decl) and Step 4 lists in
 *  the Outline. Superset of the mock `OutlineEntry` (adds the UTF-16 `offset`
 *  so the jump is exact, not line-rounded). */
export interface GroveSymbol {
  id: string;
  kind: GroveSymbolKind;
  /** Display label (e.g. `bassline(root)`, `INTRO`, a track name). */
  label: string;
  /** Bare name used to resolve a reference (e.g. `bassline`). */
  name: string;
  /** 1-based line of the declaration. */
  line: number;
  /** UTF-16 offset of the declaration's name token (exact jump target). */
  offset: number;
}

/** The resolvable surface of one `.grove` file. `defs` are local declarations
 *  (let / fn / track); `imports` map an imported name → the source path it came
 *  from (`import { drumGroove } from "lib/drums.grove"`), so cross-file
 *  go-to-decl can open the right file. */
export interface GroveSymbols {
  defs: Map<string, GroveSymbol>;
  imports: Map<string, string>;
  /** Flat ordered list for the Outline (declarations + tracks, source order). */
  outline: GroveSymbol[];
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
function collectTracks(node: Node, out: GroveSymbol[]): void {
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
export function extractSymbols(tree: Tree): GroveSymbols {
  const defs = new Map<string, GroveSymbol>();
  const imports = new Map<string, string>();
  const decls: GroveSymbol[] = [];
  const root = tree.rootNode;

  for (const item of root.namedChildren) {
    if (!item) continue;
    if (item.type === 'let_binding') {
      const nameNode = item.childForFieldName('name');
      if (nameNode) {
        const sym: GroveSymbol = {
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
        const sym: GroveSymbol = {
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
  const tracks: GroveSymbol[] = [];
  collectTracks(root, tracks);

  // Outline order: imports + declarations as written, then tracks (timeline).
  const outline = [...decls.filter((d) => d.kind !== 'track'), ...tracks];
  return { defs, imports, outline };
}

/** Outline symbol list from an already-parsed tree (the editor reuses its live
 *  tree). Same extraction the highlighter walk feeds — no drift. */
export function extractOutline(tree: Tree): GroveSymbol[] {
  return extractSymbols(tree).outline;
}

// ── Standalone parse (for consumers without an editor, e.g. the Outline panel) ─

let sharedParser: Parser | null = null;

/** Parse a `.grove` source with a shared, lazily-created parser. For one-off
 *  parses outside an editor (no incremental tree to maintain). */
export async function parseGrove(src: string): Promise<Tree | null> {
  if (!sharedParser) sharedParser = await createGroveParser();
  return sharedParser.parse(src);
}

/** Outline directly from source — the helper Step 4's Outline panel mounts: it
 *  parses + extracts in one call, so the panel needs no Tree-sitter knowledge.
 *  Returns `[]` if the grammar wasm hasn't been built / fails to load. */
export async function outlineFromSource(src: string): Promise<GroveSymbol[]> {
  try {
    const tree = await parseGrove(src);
    return tree ? extractOutline(tree) : [];
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

export type { Parser, Tree, Node, TreeCursor };
