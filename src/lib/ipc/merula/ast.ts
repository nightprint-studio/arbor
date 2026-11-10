/**
 * The Tree-sitter syntax tree of a `.merula` buffer, and the path to one offset in it.
 * Shapes are the shared `arbor-syntax` ones (`$lib/types/syntax`), so the same panel
 * that draws Picus's and Bennu's trees draws this one. Ranges are UTF-8 bytes.
 */

import { merula } from '../rpc';
import type { SyntaxTree, TreeRequest } from '$lib/types/syntax';

export function syntaxTreeOf(text: string, request?: TreeRequest): Promise<SyntaxTree> {
  return merula('merula_syntax_tree_of', { text, request });
}

/** Root-to-leaf ranges holding a byte offset — "reveal what the cursor is in". */
export function syntaxPathAt(text: string, offset: number): Promise<{ start: number; end: number }[]> {
  return merula('merula_syntax_path_at', { text, offset });
}
