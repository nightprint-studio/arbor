/**
 * Bennu's `.merula` outline — the Structure / Outline panel rows.
 *
 * The tree is Merula's own: `extractOutlineTree` walks the same Tree-sitter parse the Merula
 * window's Outline panel uses (tracks with their `section(…)` bands, `let` constants, `fn`s,
 * imports). Nothing is re-derived here; this module only renames the kinds onto the glyph table
 * that every Bennu symbol list shares, so a track reads as a module and a constant as a constant.
 *
 * It parses in the WebView with the grammar wasm the editor already loaded for highlighting, so
 * an outline never spawns `merula-be` (which is what the shared syntax-tree RPC would cost).
 */

import {
  parseMerula, extractOutlineTree, type MerulaOutlineNode, type MerulaSymbolKind,
} from '$lib/components/merula/editor/merula-lang';

/** One outline row, in the shape `BennuSymbolList` renders. */
export interface MerulaOutlineRow {
  kind: string;
  name: string;
  detail?: string;
  line: number;
  /** UTF-16 offset of the name token — unique per row, so it keys the tree. */
  offset: number;
  children?: MerulaOutlineRow[];
}

/** Merula symbol kind → the glyph-table kind (`symbol-kind-glyph.ts`). */
const GLYPH_KIND: Record<MerulaSymbolKind, string> = {
  track: 'module',
  section: 'enummember',
  fn: 'function',
  let: 'constant',
  import: 'namespace',
};

/** Merula's outline nodes → panel rows. Pure, so it is unit-tested without a parser. */
export function merulaOutlineRows(nodes: readonly MerulaOutlineNode[]): MerulaOutlineRow[] {
  return nodes.map((n) => ({
    kind: GLYPH_KIND[n.kind],
    // A `fn` label already carries its parameters (`bassline(root)`); the others are bare names.
    name: n.label,
    detail: n.kind === 'import' ? 'import' : undefined,
    line: n.line,
    offset: n.offset,
    children: n.children?.length ? merulaOutlineRows(n.children) : undefined,
  }));
}

/** Outline rows of a `.merula` source. Empty when the grammar wasm is unavailable. */
export async function merulaOutline(src: string): Promise<MerulaOutlineRow[]> {
  try {
    const tree = await parseMerula(src);
    return tree ? merulaOutlineRows(extractOutlineTree(tree)) : [];
  } catch {
    return [];
  }
}
