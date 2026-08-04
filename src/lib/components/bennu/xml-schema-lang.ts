/**
 * The editor language for **XML with a schema behind it**.
 *
 * Colouring is the stock CodeMirror mode; everything else here comes from outside the file —
 * the DTD or XSD the document names, found in the project or inside a dependency jar:
 *
 * - **completion** over the elements a parent may contain, the attributes an element may carry,
 *   and the values an attribute accepts where the schema closes the set;
 * - **ghost text** where exactly one thing can follow;
 * - **hover** with the schema's own documentation, the required attributes, and which grammar
 *   the answer came from.
 *
 * All three are backend calls, and all three are silent when no schema resolves. Nothing about
 * the vocabulary lives here — a frontend that guessed tag names from the ones already in the
 * file would confidently propose whatever typo is already there.
 *
 * ## The one thing this file decides
 *
 * Where the token being completed **starts** — CodeMirror needs a `from` to replace, and that is
 * a question about the buffer rather than about the schema. The rule is deliberately shallow and
 * mirrors the backend's: a run of name characters, and what precedes it says which of the three
 * kinds of name it is. Get it wrong and a completion lands at a slightly wrong offset; it can
 * never cause the wrong candidates to be offered, because it does not choose them.
 */

import type { LanguageDescriptor } from '$lib/components/shared/ui/code-editor';
import { makeU16ToByte } from '$lib/components/shared/ui/code-editor';
import { StreamLanguage, type StreamParser } from '@codemirror/language';
import type { CompletionContext, CompletionResult } from '@codemirror/autocomplete';
import type { EditorView } from '@codemirror/view';
import { extCompletion, extHover, extInlineHint } from '$lib/ipc/bennu/ext';
import { projectStore } from '$lib/stores/bennu/project.svelte';
import { makeHoverSource } from './bennu-hover';

/** XML name characters, including the namespace colon: `context:component-scan` is one token,
 *  and splitting it would replace half of a name the user is in the middle of writing. */
const NAME = /[A-Za-z0-9_.:-]/;

/** Where the token under the caret begins. `null` where nothing completes. */
function tokenStart(ctx: CompletionContext): number | null {
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
  return null;
}

const xmlCompletionSource = async (ctx: CompletionContext): Promise<CompletionResult | null> => {
  const path = projectStore.activeFilePath;
  if (!path) return null;
  const from = tokenStart(ctx);
  if (from === null) return null;

  const src = ctx.state.doc.toString();
  const items = await extCompletion(path, src, makeU16ToByte(src)(ctx.pos)).catch(() => []);
  if (items.length === 0) return null;

  return {
    from,
    options: items.map((it) => ({
      label: it.label,
      type: it.kind === 'value' ? 'constant' : it.kind === 'attribute' ? 'property' : 'type',
      detail: it.detail ?? undefined,
    })),
    // The backend already filtered by what was typed; letting CodeMirror re-filter against a
    // prefixed label would drop `context:component-scan` the moment the typed text spans the colon.
    filter: false,
  };
};

const xmlInlineSource = async (view: EditorView, pos: number): Promise<string | null> => {
  const path = projectStore.activeFilePath;
  if (!path) return null;
  const src = view.state.doc.toString();
  return await extInlineHint(path, src, makeU16ToByte(src)(pos)).catch(() => null);
};

// Straight to the framework extensions: an XML file has no language server of its own, so
// unlike Java there is no first answer to fall back from.
const xmlHoverSource = makeHoverSource(async (path, src, byteOffset) => {
  const ext = await extHover(path, src, byteOffset).catch(() => null);
  return ext
    ? { signature: ext.signature, kind: ext.title, container: null, doc: ext.doc || null }
    : null;
});

const intel = {
  completion: xmlCompletionSource,
  hover: xmlHoverSource,
  inlineCompletion: xmlInlineSource,
};

/** Build the descriptor. Called once at module load — the identity has to be stable, because
 *  `CodeEditor` rebuilds its extensions when the descriptor changes and a fresh object per read
 *  would remount the editor on every keystroke. */
export function xmlSchemaLang(id: string, parser: StreamParser<unknown>): LanguageDescriptor {
  return {
    id,
    createParser: () => Promise.reject(new Error(`cm-language:${id} has no tree-sitter parser`)),
    classify: () => null,
    cmExtension: StreamLanguage.define(parser),
    intel,
  };
}
