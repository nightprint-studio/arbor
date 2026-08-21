/**
 * Bennu ↔ merula: the `.merula` {@link LanguageDescriptor}.
 *
 * There is **no second grammar here**. `.merula` already has a real tree-sitter grammar —
 * `crates/products/merula/merula-lang/grammar.js`, the same one `merula-be` parses with —
 * compiled to `static/merula/tree-sitter-merula.wasm` and loaded by
 * `merula/editor/merula-lang.ts`, which is deliberately CodeMirror-free for exactly this
 * reason ("Keeping it CM-agnostic means the same parser feeds any consumer"). This module
 * is that consumer: it adapts the bridge's `classifyToken` onto the shared editor's token
 * vocabulary and adds the two things Merula's own editor gets elsewhere — folding and the
 * comment tokens for `Ctrl+/`.
 *
 * So a `.merula` file reads the same in Bennu as in Merula because it *is* the same parse,
 * and a `grammar.js` change reaches both by rebuilding one `.wasm`. A second tokenizer here
 * would have been a copy that silently drifts the first time the mini-notation grows a
 * token — and mini-notation is most of what makes the language legible.
 *
 * The wasm needs no copy under `static/bennu/`: Bennu and Merula are windows of one
 * SvelteKit app, so `/merula/*.wasm` is served to both, and the two `tree-sitter.wasm`
 * runtime cores under `static/` are byte-identical.
 *
 * **Not here: completion and hover.** Merula's (`merula-intel.ts`) are driven by the
 * canonical DSL catalogue, which arrives over RPC from `merula-be`. Offering them in Bennu
 * means Bennu spawning that backend to open a text file, which is a decision about process
 * lifetime rather than about syntax — see the note in `languages.ts`.
 */

import type { Node } from 'web-tree-sitter';
import type { LanguageDescriptor, TokenClassName } from '$lib/components/shared/ui/code-editor';
import {
  createMerulaParser, classifyToken, type MerulaTokenClass,
} from '$lib/components/merula/editor/merula-lang';

/**
 * merula's token classes onto the shared editor's vocabulary.
 *
 * The seven that have an equivalent are mapped to it; the six that do not keep their own
 * name and are styled in the shared theme, which is the documented way a grammar grows a
 * token kind. `def` becomes `type` rather than `declaration` because that is the colour
 * Merula's own editor gives a `let` / `fn` binding name — the point is that the two editors
 * agree, not that the class name is the prettiest one available.
 */
const CLASSES: Record<MerulaTokenClass, TokenClassName> = {
  comment: 'comment',
  string:  'string',
  number:  'number',
  keyword: 'keyword',
  ident:   'ident',
  fn:      'function',
  op:      'operator',
  def:     'type',
  // Mini-notation and islands — merula's own, styled as `cm-tok-<name>` in the theme.
  note:     'note',
  chord:    'chord',
  sound:    'sound',
  island:   'island',
  mininote: 'mininote',
  splice:   'splice',
};

function classify(
  node: Node,
  isNamed: boolean,
  field: string | null,
  parentType: string | null,
): TokenClassName | null {
  const cls = classifyToken(node.type, isNamed, field, parentType);
  return cls ? CLASSES[cls] : null;
}

/** The range between the first `open` child and the last `close` child, or null when either
 *  delimiter is missing (an incomplete edit mid-typing, which must not offer a fold). */
function between(node: Node, open: string, close: string): { from: number; to: number } | null {
  const first = node.children.find((c) => c?.text === open);
  const last = node.lastChild;
  if (!first || !last || last.text !== close) return null;
  return first.endIndex < last.startIndex ? { from: first.endIndex, to: last.startIndex } : null;
}

/**
 * What folds.
 *
 * `arguments` is the one that matters: a merula file's shape is one long `tracks( … )` at
 * the top level, and folding a call's argument list is what collapses a track. `meta_block`
 * is the file's front matter, which is read once and then in the way. Block comments fold
 * for the same reason they do in every other language here.
 *
 * The shared fold service asks for the smallest node *starting on* the queried line, so a
 * fold is offered on the `tracks(` line itself rather than on the line below it.
 */
function foldNode(node: Node): { from: number; to: number } | null {
  switch (node.type) {
    case 'arguments':  return between(node, '(', ')');
    case 'meta_block': return between(node, '{', '}');
    case 'comment':
      return node.text.startsWith('/*')
        ? { from: node.startIndex + 2, to: node.endIndex - 2 }
        : null;
    default: return null;
  }
}

export const merulaLanguage: LanguageDescriptor = {
  id: 'merula',
  createParser: createMerulaParser,
  classify,
  foldNode,
  // A tree-sitter descriptor bypasses CodeMirror's `Language` and so carries no built-in
  // comment data; without this `Ctrl+/` does nothing. Both forms, as the grammar has both.
  commentTokens: { line: '//', block: { open: '/*', close: '*/' } },
};
