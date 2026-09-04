/**
 * Completion, ghost text and hover for the **markup** languages — XML and JSP.
 *
 * Both ask the same backend the same three questions and differ only in what answers:
 * an XML file is answered by the schema it names, a JSP by the tag libraries it
 * declares. Neither vocabulary lives in the frontend, and neither should: a completion
 * list guessed from the names already in the buffer confidently proposes whatever typo
 * is already there.
 *
 * ## The one thing this file decides
 *
 * Where the token being completed **starts** — CodeMirror needs a `from` to replace, and
 * that is a question about the buffer rather than about the vocabulary. The rule is
 * deliberately shallow and mirrors the backend's: a run of name characters, and what
 * precedes it says which of the four kinds of name it is. Get it wrong and a completion
 * lands at a slightly wrong offset; it can never cause the wrong candidates to be
 * offered, because it does not choose them.
 *
 * ## …except when the backend already decided
 *
 * A candidate may carry its own replacement range, and where it does, that wins. It is not a
 * refinement of the guess above but a different kind of answer: a coordinate completed inside a
 * pom's `<artifactId>` fills the empty `<groupId>` above it in the same edit, so the range spans
 * two elements and no rule about "the token under the caret" could ever produce it.
 */

import { makeByteToU16, makeU16ToByte } from '$lib/components/shared/ui/code-editor';
import type { CompletionContext, CompletionResult } from '@codemirror/autocomplete';
import type { EditorView } from '@codemirror/view';
import { extCompletion, extHover, extInlineHint, type ExtCompletionItem } from '$lib/ipc/bennu/ext';
import { projectStore } from '$lib/stores/bennu/project.svelte';
import type { HoverInfo } from '$lib/ipc/bennu/nav';

/** Markup name characters, including the namespace colon: `context:component-scan` and
 *  `s:iterator` are each ONE token, and splitting them would replace half of a name the
 *  user is in the middle of writing. */
const NAME = /[A-Za-z0-9_.:-]/;

/** The characters an element's **text** can be completed from. Wider than a name: a Maven
 *  coordinate carries `/` in a path and `${…}` in a version, and stopping the token at the `{`
 *  would replace half of a placeholder the user is in the middle of writing. */
const VALUE = /[A-Za-z0-9_.:${}\/-]/;

/** Where the token under the caret begins. `null` where nothing completes. */
export function markupTokenStart(ctx: CompletionContext): number | null {
  const text = ctx.state.doc.sliceString(Math.max(0, ctx.pos - 512), ctx.pos);
  let i = text.length;
  while (i > 0 && NAME.test(text[i - 1])) i--;
  const before = text.slice(0, i);

  // An element name, an attribute value, or an attribute name — in that order, because the
  // first two are recognisable from one character and the third is what is left.
  if (before.endsWith('<') || before.endsWith('</')) return ctx.pos - (text.length - i);
  if (before.endsWith('"') || before.endsWith("'") || before.endsWith('=')) {
    return ctx.pos - (text.length - i);
  }
  // Inside a tag, past its name: an attribute name. Detected by there being an unclosed `<`
  // behind us with no `>` in between — cheap, and wrong only in text that contains a bare `<`,
  // where the backend answers nothing anyway.
  const open = before.lastIndexOf('<');
  if (open >= 0 && !before.slice(open).includes('>')) return ctx.pos - (text.length - i);

  // An element's TEXT — `<groupId>org.spring|</groupId>`. Returning null here is why a pom's
  // values could not be completed at all: every rule above is about a name, and a coordinate, a
  // version, a scope and a module are all content. The token runs over value characters rather
  // than name ones, and the backend decides whether there is anything to say.
  if (open >= 0 && before.slice(open).includes('>')) {
    let j = text.length;
    while (j > 0 && VALUE.test(text[j - 1])) j--;
    return ctx.pos - (text.length - j);
  }
  return null;
}

/** Kind tag → the icon CodeMirror draws beside a candidate. */
function optionType(kind: string): string {
  switch (kind) {
    case 'value': return 'constant';
    case 'attribute': return 'property';
    case 'taglib': return 'namespace';
    default: return 'type';
  }
}

/** The completion source both markup languages install. */
export const markupCompletionSource = async (
  ctx: CompletionContext,
): Promise<CompletionResult | null> => {
  const path = projectStore.activeFilePath;
  if (!path) return null;
  const from = markupTokenStart(ctx);
  if (from === null) return null;

  const src = ctx.state.doc.toString();
  const items = await extCompletion(path, src, makeU16ToByte(src)(ctx.pos)).catch(() => []);
  if (items.length === 0) return null;
  const b2u = makeByteToU16(src);

  return {
    from,
    options: items.map((it) => ({
      label: it.label,
      type: optionType(it.kind),
      detail: it.detail ?? undefined,
      // The provider's order, when it has one — see `ExtCompletionItem.sort_text`.
      boost: it.sort_text ? -Number(it.sort_text) : undefined,
      apply: applyOf(it, b2u),
    })),
    // The backend already filtered by what was typed; letting CodeMirror re-filter against a
    // prefixed label would drop `context:component-scan` — or `s:iterator` — the moment the
    // typed text spans the colon.
    filter: false,
  };
};

/** How a candidate is inserted.
 *
 *  `undefined` for the ordinary one — CodeMirror replaces `[from, caret)` with the label, which is
 *  what every name completion wants. A candidate that carries its own range gets an explicit edit
 *  instead, because the range is the answer: it may start before `from` (filling in an empty
 *  `<groupId>` above the `<artifactId>` being typed) or end after the caret (replacing the rest of
 *  a value that was already there). */
function applyOf(
  it: ExtCompletionItem,
  b2u: (byte: number) => number,
): ((view: EditorView, completion: unknown, from: number, to: number) => void) | undefined {
  if (it.replace_start == null || it.replace_end == null) return undefined;
  const from = b2u(it.replace_start);
  const to = b2u(it.replace_end);
  const insert = it.insert_text ?? it.label;
  return (view) => {
    view.dispatch({
      changes: { from, to: Math.max(from, to), insert },
      selection: { anchor: from + insert.length },
      userEvent: 'input.complete',
    });
  };
}

/** The ghost-text source: what certainly follows, or nothing. */
export const markupInlineHintSource = async (
  view: EditorView,
  pos: number,
): Promise<string | null> => {
  const path = projectStore.activeFilePath;
  if (!path) return null;
  const src = view.state.doc.toString();
  return await extInlineHint(path, src, makeU16ToByte(src)(pos)).catch(() => null);
};

/** The framework extensions' hover, in the editor's card shape. `null` when they have
 *  nothing to say, so a caller can fall back to its own resolver. */
export async function markupExtHover(
  path: string,
  src: string,
  byteOffset: number,
): Promise<HoverInfo | null> {
  const ext = await extHover(path, src, byteOffset).catch(() => null);
  return ext
    ? { signature: ext.signature, kind: ext.title, container: null, doc: ext.doc || null }
    : null;
}
