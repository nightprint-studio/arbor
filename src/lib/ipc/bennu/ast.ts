/**
 * The syntax tree of the buffer in front of the user, and the path to one offset in it.
 *
 * Thin over `arbor-syntax`, which knows no Java: everything here is the shape that crate
 * produces — the same one Picus's SQL trees arrive in, which is why one panel draws both.
 *
 * Same convention as the rest of the bennu IPC: one `args` object, snake_case fields.
 */

import { bennu } from '../rpc';
import type { SyntaxTree, TreeRequest } from '$lib/types/syntax';

export type { SyntaxNode, SyntaxTree, TreeRequest } from '$lib/types/syntax';

/** What the backend answers: which language, and its tree when there is a grammar for it. */
export interface AstAnswer {
  /**
   * What the file is in — `Java`, `XML`, `JSP`. Always present, **including** when there is no
   * grammar, so the panel can say which language it cannot read rather than showing a blank.
   */
  language: string;
  /** The tree, or `null` when no grammar reads that language yet. */
  tree: SyntaxTree | null;
}

/**
 * The tree of a buffer.
 *
 * The text is the caller's, not the file's: a tree of what is on disk would be wrong from the
 * first keystroke, and wrong exactly when it matters — the moment you want the tree is the
 * moment you have typed something the parser read differently than you expected. `path` is used
 * only to choose the grammar.
 *
 * Wire: `bennu_syntax_tree_of` — `{ text, path, request }`.
 */
export function syntaxTreeOf(
  text: string,
  path: string,
  request?: TreeRequest,
): Promise<AstAnswer> {
  return bennu('bennu_syntax_tree_of', { args: { text, path, request } });
}

/**
 * The **declaration model** of a buffer — what Bennu understood, rather than what the grammar
 * built.
 *
 * The same shape as {@link syntaxTreeOf} on purpose: one panel draws both, and the difference
 * between the two answers is a tab rather than a second component. `tree` is `null` for a
 * language Bennu has no model of, which the panel reports as a fact about the tool.
 *
 * There is no `pathAt` companion: this tree is never truncated, so "what am I in" is a
 * containment walk over the tree already in hand — a round trip would buy nothing.
 *
 * Wire: `bennu_symbol_tree_of` — `{ text, path }`.
 */
export function symbolTreeOf(text: string, path: string): Promise<AstAnswer> {
  return bennu('bennu_symbol_tree_of', { args: { text, path } });
}

/**
 * Root-to-leaf ranges holding a byte offset — "reveal what the caret is in".
 *
 * Asked of the backend rather than searched in the tree the panel holds, because that tree may
 * have been truncated: "reveal what I am in" must not depend on how far the walk happened to go.
 *
 * Empty for a language with no grammar. Wire: `bennu_syntax_path_at`.
 */
export function syntaxPathAt(
  text: string,
  path: string,
  offset: number,
): Promise<{ start: number; end: number }[]> {
  return bennu('bennu_syntax_path_at', { args: { text, path, offset } });
}
