/**
 * The syntax tree of a script, and the path to one offset in it.
 *
 * Thin over `arbor-syntax`, which knows no SQL: everything here is the shape that
 * crate produces, so the same panel will read Bennu's Java trees without a second
 * client.
 */

import { picus } from '../rpc';

// The shapes live in `$lib/types/syntax`: the crate that builds them (`arbor-syntax`) knows no
// language, so Picus and Bennu get the same thing back and one panel draws both. Re-exported
// here so a Picus call site still reads as one import.
export type { SyntaxNode, SyntaxTree, TreeRequest } from '$lib/types/syntax';
import type { SyntaxTree, TreeRequest } from '$lib/types/syntax';


/** The tree of a saved script, from the text the repository was read with. */
export function syntaxTree(
  root: string,
  path: string,
  request?: TreeRequest,
): Promise<SyntaxTree> {
  return picus('picus_syntax_tree', { root, path, request });
}

/**
 * The tree of a buffer the user is editing.
 *
 * A separate call rather than a flag, because the two answer different questions:
 * this one is about text that has no path to be stale against.
 */
export function syntaxTreeOf(text: string, request?: TreeRequest): Promise<SyntaxTree> {
  return picus('picus_syntax_tree_of', { text, request });
}

/** One thing the grammar could not read, in UTF-8 byte offsets. */
export interface ParseFault {
  start: number;
  end: number;
  message: string;
}

/**
 * What the parser could not read in this text.
 *
 * The syntax-tree panel has always known these; the editor did not, so a statement
 * that could not possibly run looked perfectly fine until it was run. This is the
 * one question the semantic diagnostics cannot answer — "is this SQL at all" is a
 * question only the grammar can be asked.
 */
export function parseFaults(text: string, engine?: string): Promise<ParseFault[]> {
  return picus('picus_parse_faults', { text, engine });
}

/** Root-to-leaf ranges holding a byte offset — "reveal what the cursor is in". */
export function syntaxPathAt(
  text: string,
  offset: number,
): Promise<{ start: number; end: number }[]> {
  return picus('picus_syntax_path_at', { text, offset });
}
